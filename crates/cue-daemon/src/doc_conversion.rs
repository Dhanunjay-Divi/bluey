use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use cue_core::ContextKind;
use tracing::debug;

const MAX_DIRECT_TEXT_BYTES: u64 = 1_000_000;
const MAX_MARKITDOWN_INPUT_BYTES: u64 = 25_000_000;
const MAX_MARKITDOWN_OUTPUT_BYTES: u64 = 2_000_000;
const MARKITDOWN_TIMEOUT: Duration = Duration::from_secs(20);
const NATIVE_CONVERTER_TIMEOUT: Duration = Duration::from_secs(15);
const PREVIEW_CHARS: usize = 16_000;

#[cfg(unix)]
const DOCUMENT_CHILD_ENV_ALLOWLIST: &[&str] = &["HOME", "TMPDIR", "LANG", "LC_ALL"];
#[cfg(windows)]
const DOCUMENT_CHILD_ENV_ALLOWLIST: &[&str] = &["USERPROFILE", "HOME", "TEMP", "TMP"];
#[cfg(not(any(unix, windows)))]
const DOCUMENT_CHILD_ENV_ALLOWLIST: &[&str] = &[];

pub(crate) fn supported_context_formats_message() -> &'static str {
    "Supported formats: PDF, Word, PowerPoint, Excel/ODS, CSV/TSV, text, Markdown, code/data files, and PNG/JPEG/WebP/GIF/HEIC/BMP/TIFF images. Video files are not readable context yet."
}

#[derive(Debug, Clone)]
struct ConverterCommand {
    program: PathBuf,
}

pub(crate) fn classify_context_path(path: &Path) -> ContextKind {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" | "tif" => {
            if path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.to_ascii_lowercase().contains("diagram"))
            {
                ContextKind::Diagram
            } else {
                ContextKind::Image
            }
        }
        "rs" | "swift" | "c" | "h" | "cpp" | "hpp" | "js" | "jsx" | "ts" | "tsx" | "py" | "go"
        | "java" | "kt" | "kts" | "cs" | "rb" | "php" | "sql" | "sh" | "ps1" | "toml" | "yaml"
        | "yml" | "json" | "html" | "css" | "scss" => ContextKind::Code,
        "pdf" | "doc" | "docx" | "rtf" | "ppt" | "pptx" | "xls" | "xlsx" | "xlsm" | "xlsb"
        | "ods" => ContextKind::Document,
        "txt" | "log" | "csv" | "tsv" | "md" | "markdown" | "rst" | "adoc" => ContextKind::Text,
        _ => ContextKind::Other,
    }
}

pub(crate) fn is_supported_context_file(path: &Path) -> bool {
    match classify_context_path(path) {
        ContextKind::Code | ContextKind::Document | ContextKind::Text => true,
        ContextKind::Image | ContextKind::Diagram => matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" | "tif"
        ),
        ContextKind::Other => false,
    }
}

pub(crate) fn convert_context_file_to_markdown(
    path: &Path,
    kind: ContextKind,
    size_bytes: u64,
) -> Result<String> {
    match kind {
        ContextKind::Code | ContextKind::Text | ContextKind::Document => {}
        ContextKind::Image | ContextKind::Diagram => {
            return Err(anyhow!("image context is handled by the vision pipeline"));
        }
        ContextKind::Other => {
            return Err(anyhow!(
                "unsupported context file type. {}",
                supported_context_formats_message()
            ));
        }
    }

    if size_bytes > MAX_MARKITDOWN_INPUT_BYTES {
        return Err(anyhow!(
            "file is over {} MB; split it before attaching so Bluey cannot stall on conversion",
            MAX_MARKITDOWN_INPUT_BYTES / 1_000_000
        ));
    }

    for converter in discover_markitdown_commands() {
        match run_markitdown(&converter, path) {
            Ok(markdown) if !markdown.trim().is_empty() => {
                return Ok(markdown.trim().to_string());
            }
            Ok(_) => {
                debug!(
                    file_kind = ?kind,
                    "MarkItDown returned empty markdown; falling back to native parser"
                );
            }
            Err(error) => {
                debug!(
                    file_kind = ?kind,
                    error = %error,
                    "MarkItDown conversion failed; falling back to native parser"
                );
            }
        }
    }

    native_markdown_fallback(path, kind, size_bytes)
}

pub(crate) fn build_markdown_preview(markdown: &str) -> String {
    build_text_preview(markdown, PREVIEW_CHARS)
}

fn discover_markitdown_commands() -> Vec<ConverterCommand> {
    let mut commands = Vec::new();
    let mut push = |program: PathBuf| {
        if !commands
            .iter()
            .any(|command: &ConverterCommand| command.program == program)
        {
            commands.push(ConverterCommand { program });
        }
    };

    // Arbitrary executable overrides are available only in development and
    // tests. Release daemons resolve converters exclusively inside the
    // canonical package root established by their own executable path.
    #[cfg(debug_assertions)]
    for name in ["BLUEY_DOC_CONVERTER_BIN", "BLUEY_MARKITDOWN_BIN"] {
        if let Some(value) = env::var_os(name).filter(|value| !value.is_empty()) {
            let path = PathBuf::from(value);
            push(canonical_executable(&path).unwrap_or(path));
        }
    }

    for root in packaged_install_roots() {
        for candidate in bluey_doc_converter_candidates_for_roots([root.clone()]) {
            if let Some(program) = canonical_packaged_executable(&candidate, &root) {
                push(program);
            }
        }
    }

    #[cfg(debug_assertions)]
    for candidate in bluey_home_doc_converter_candidates() {
        if candidate.is_file() {
            push(canonical_executable(&candidate).unwrap_or(candidate));
        }
    }

    // Development builds retain PATH-based commands for source-tree workflows.
    // This block is absent from production binaries.
    #[cfg(debug_assertions)]
    {
        push(PathBuf::from("bluey-doc-converter"));
        push(PathBuf::from("markitdown"));
    }

    commands
}

fn packaged_install_roots() -> Vec<PathBuf> {
    let Some(executable) = env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
    else {
        return Vec::new();
    };
    let Some(executable_dir) = executable.parent() else {
        return Vec::new();
    };
    let root = if executable_dir.file_name().is_some_and(|name| name == "bin") {
        executable_dir.parent().unwrap_or(executable_dir)
    } else {
        executable_dir
    };
    vec![root.to_path_buf()]
}

fn canonical_packaged_executable(candidate: &Path, root: &Path) -> Option<PathBuf> {
    let canonical_root = root.canonicalize().ok()?;
    let canonical = canonical_executable(candidate)?;
    canonical.starts_with(canonical_root).then_some(canonical)
}

fn canonical_executable(candidate: &Path) -> Option<PathBuf> {
    let canonical = candidate.canonicalize().ok()?;
    let metadata = canonical.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(canonical)
}

#[cfg(debug_assertions)]
fn bluey_home_doc_converter_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for name in ["BLUEY_INSTALL_ROOT", "HOME", "USERPROFILE"] {
        if let Some(value) = env::var_os(name).filter(|value| !value.is_empty()) {
            let root = PathBuf::from(value);
            let install_root = if name == "BLUEY_INSTALL_ROOT" {
                root
            } else {
                root.join(".bluey")
            };
            if !roots
                .iter()
                .any(|existing: &PathBuf| existing == &install_root)
            {
                roots.push(install_root);
            }
        }
    }

    bluey_doc_converter_candidates_for_roots(roots)
}

fn bluey_doc_converter_candidates_for_roots<I>(roots: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut candidates = Vec::new();
    for root in roots {
        candidates.extend([
            root.join("tools/doc-converter/.venv/bin/markitdown"),
            root.join("bin/bluey-doc-converter"),
            root.join("tools/doc-converter/bin/bluey-doc-converter"),
        ]);

        #[cfg(target_os = "windows")]
        candidates.extend([
            root.join("tools/doc-converter/.venv/Scripts/markitdown.exe"),
            root.join("bin/bluey-doc-converter.cmd"),
            root.join("tools/doc-converter/bin/bluey-doc-converter.cmd"),
        ]);
    }
    candidates
}

fn run_markitdown(converter: &ConverterCommand, path: &Path) -> Result<String> {
    let (output, file) = PrivateCommandOutput::new("markitdown", "md")?;
    drop(file);

    let mut command = Command::new(&converter.program);
    command.arg(path).arg("-o").arg(output.path());
    configure_document_command(&mut command);
    command.stdout(Stdio::null());
    let child = command
        .spawn()
        .map_err(|error| anyhow!("document converter could not start ({:?})", error.kind()))?;
    wait_for_bounded_child(
        child,
        output.path(),
        MARKITDOWN_TIMEOUT,
        MAX_MARKITDOWN_OUTPUT_BYTES,
        "document converter",
    )?;
    cue_core::app_paths::validate_private_file(output.path())
        .context("document converter output lost its private permissions")?;
    read_utf8_bounded(output.path(), MAX_MARKITDOWN_OUTPUT_BYTES)
}

struct PrivateCommandOutput {
    directory: PathBuf,
    path: PathBuf,
}

impl PrivateCommandOutput {
    fn new(label: &str, extension: &str) -> Result<(Self, File)> {
        let directory = env::temp_dir().join(format!(
            "bluey-{label}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        cue_core::app_paths::create_private_dir(&directory)
            .context("failed to create private converter workspace")?;
        let path = directory.join(format!("output.{extension}"));
        let file = cue_core::app_paths::create_private_file_new(&path)
            .context("failed to create private converter output")?;
        Ok((Self { directory, path }, file))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateCommandOutput {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn configure_document_command(command: &mut Command) {
    command
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    for name in DOCUMENT_CHILD_ENV_ALLOWLIST {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }

    #[cfg(debug_assertions)]
    if let Some(path) = env::var_os("PATH") {
        command.env("PATH", path);
    }

    #[cfg(all(not(debug_assertions), unix))]
    command.env("PATH", "/usr/bin:/bin");

    #[cfg(windows)]
    if let Some(root) = windows_system_root() {
        let system32 = root.join("System32");
        command
            .env("SystemRoot", &root)
            .env("WINDIR", &root)
            .env("ComSpec", system32.join("cmd.exe"));
        #[cfg(not(debug_assertions))]
        command.env(
            "PATH",
            format!("{};{}", system32.to_string_lossy(), root.to_string_lossy()),
        );
    }
}

fn wait_for_bounded_child(
    mut child: std::process::Child,
    output_path: &Path,
    timeout: Duration,
    max_output_bytes: u64,
    label: &str,
) -> Result<()> {
    let started = Instant::now();
    loop {
        if fs::metadata(output_path)
            .map(|metadata| metadata.len() > max_output_bytes)
            .unwrap_or(false)
        {
            terminate_child(&mut child);
            return Err(anyhow!("{label} exceeded its output limit"));
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_child(&mut child);
                return Err(error).context("failed to poll converter");
            }
        };
        if let Some(status) = status {
            if !status.success() {
                return Err(anyhow!("{label} exited unsuccessfully"));
            }
            let bytes = fs::metadata(output_path)
                .context("converter output is missing")?
                .len();
            if bytes > max_output_bytes {
                return Err(anyhow!("{label} exceeded its output limit"));
            }
            return Ok(());
        }
        if started.elapsed() > timeout {
            terminate_child(&mut child);
            return Err(anyhow!("{label} exceeded its time limit"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn run_bounded_stdout_command<I, S>(program: &Path, args: I, label: &str) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let (output, file) = PrivateCommandOutput::new("native-converter", "txt")?;
    let mut command = Command::new(program);
    command.args(args);
    configure_document_command(&mut command);
    command.stdout(Stdio::from(file));
    let child = command
        .spawn()
        .map_err(|error| anyhow!("{label} could not start ({:?})", error.kind()))?;
    wait_for_bounded_child(
        child,
        output.path(),
        NATIVE_CONVERTER_TIMEOUT,
        MAX_MARKITDOWN_OUTPUT_BYTES,
        label,
    )?;
    cue_core::app_paths::validate_private_file(output.path())
        .context("native converter output lost its private permissions")?;
    read_utf8_bounded(output.path(), MAX_MARKITDOWN_OUTPUT_BYTES)
}

fn native_markdown_fallback(path: &Path, kind: ContextKind, size_bytes: u64) -> Result<String> {
    match kind {
        ContextKind::Code => {
            if size_bytes > MAX_DIRECT_TEXT_BYTES {
                return Err(anyhow!(
                    "code file is over 1 MB; attach a smaller file or use the bundled document converter"
                ));
            }
            let text = fs::read_to_string(path).context("failed to read code file")?;
            let language = markdown_language_for_path(path);
            Ok(format!("```{language}\n{}\n```", text.trim_end()))
        }
        ContextKind::Text => {
            if size_bytes > MAX_DIRECT_TEXT_BYTES {
                return Err(anyhow!(
                    "text file is over 1 MB; attach a smaller file or use the bundled document converter"
                ));
            }
            fs::read_to_string(path).context("failed to read text file")
        }
        ContextKind::Document => extract_document_text_preview(path, size_bytes),
        _ => Err(anyhow!("unsupported context file kind")),
    }
}

fn extract_document_text_preview(path: &Path, size_bytes: u64) -> Result<String> {
    if size_bytes > 10_000_000 {
        return Err(anyhow!(
            "document is over 10 MB; use the bundled MarkItDown converter or split it before attaching"
        ));
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = match extension.as_str() {
        "pdf" => extract_pdf_text(path)?,
        "doc" | "docx" | "rtf" => extract_word_text(path)?,
        "ppt" | "pptx" => {
            return Err(anyhow!(
                "PowerPoint conversion needs the bundled MarkItDown converter"
            ));
        }
        "xls" | "xlsx" | "xlsm" | "xlsb" | "ods" => {
            return Err(anyhow!(
                "spreadsheet conversion needs the bundled MarkItDown converter"
            ));
        }
        _ => {
            return Err(anyhow!(
                "no parser is registered for .{} documents",
                extension
            ));
        }
    };

    Ok(text)
}

fn extract_pdf_text(path: &Path) -> Result<String> {
    let program = resolve_pdf_text_extractor().ok_or_else(|| {
        anyhow!(
            "PDF text extraction needs the bundled MarkItDown converter or a packaged PDF extractor"
        )
    })?;
    run_bounded_stdout_command(
        &program,
        [
            OsString::from("-layout"),
            path.as_os_str().to_os_string(),
            OsString::from("-"),
        ],
        "PDF text extractor",
    )
}

fn resolve_pdf_text_extractor() -> Option<PathBuf> {
    for root in packaged_install_roots() {
        for candidate in [
            root.join("tools/doc-converter/bin/pdftotext"),
            root.join("bin/pdftotext"),
        ] {
            if let Some(program) = canonical_packaged_executable(&candidate, &root) {
                return Some(program);
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        return Some(PathBuf::from("pdftotext"));
    }
    #[cfg(not(debug_assertions))]
    None
}

#[cfg(target_os = "macos")]
fn extract_word_text(path: &Path) -> Result<String> {
    let program = canonical_executable(Path::new("/usr/bin/textutil")).ok_or_else(|| {
        anyhow!("DOC/DOCX text extraction needs macOS textutil or the bundled converter")
    })?;
    run_bounded_stdout_command(
        &program,
        [
            OsString::from("-convert"),
            OsString::from("txt"),
            OsString::from("-stdout"),
            path.as_os_str().to_os_string(),
        ],
        "document text extractor",
    )
}

#[cfg(target_os = "windows")]
fn extract_word_text(path: &Path) -> Result<String> {
    let script = format!(
        r#"
$path = {path}
$ext = [IO.Path]::GetExtension($path).ToLowerInvariant()
if ($ext -eq '.docx') {{
  $dest = Join-Path ([IO.Path]::GetTempPath()) ('bluey-docx-' + [guid]::NewGuid().ToString())
  New-Item -ItemType Directory -Path $dest | Out-Null
  try {{
    Expand-Archive -LiteralPath $path -DestinationPath $dest -Force
    $xmlPath = Join-Path $dest 'word/document.xml'
    if (Test-Path $xmlPath) {{
      [xml]$xml = Get-Content -LiteralPath $xmlPath -Raw
      $nsm = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
      $nsm.AddNamespace('w', 'http://schemas.openxmlformats.org/wordprocessingml/2006/main')
      ($xml.SelectNodes('//w:t', $nsm) | ForEach-Object {{ $_.InnerText }}) -join ' '
    }}
  }} finally {{
    Remove-Item -LiteralPath $dest -Recurse -Force -ErrorAction SilentlyContinue
  }}
}} else {{
  Write-Error 'Legacy .doc/.rtf parsing needs the bundled MarkItDown converter on Windows.'
  exit 2
}}
"#,
        path = powershell_single_quoted(path)
    );
    let program = windows_powershell_path().ok_or_else(|| {
        anyhow!("DOCX text extraction needs Windows PowerShell or the bundled converter")
    })?;
    run_bounded_stdout_command(
        &program,
        [
            OsString::from("-NoLogo"),
            OsString::from("-NoProfile"),
            OsString::from("-NonInteractive"),
            OsString::from("-ExecutionPolicy"),
            OsString::from("Bypass"),
            OsString::from("-Command"),
            OsString::from(script),
        ],
        "Windows document parser",
    )
}

#[cfg(windows)]
fn windows_system_root() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;

    let mut buffer = vec![0_u16; 260];
    loop {
        let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
        if length == 0 {
            return None;
        }
        if (length as usize) < buffer.len() {
            buffer.truncate(length as usize);
            return Some(PathBuf::from(OsString::from_wide(&buffer)));
        }
        buffer.resize(length as usize + 1, 0);
    }
}

#[cfg(windows)]
fn windows_powershell_path() -> Option<PathBuf> {
    let root = windows_system_root()?;
    let canonical_root = root.canonicalize().ok()?;
    let candidate = root.join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let canonical = canonical_executable(&candidate)?;
    canonical.starts_with(canonical_root).then_some(canonical)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn extract_word_text(_path: &Path) -> Result<String> {
    Err(anyhow!(
        "DOC/DOCX text extraction needs the bundled MarkItDown converter on this platform"
    ))
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn read_utf8_bounded(path: &Path, max_bytes: u64) -> Result<String> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(anyhow!("converter output exceeded its size limit"));
    }
    String::from_utf8(bytes).context("converter output was not valid UTF-8")
}

fn markdown_language_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "hpp" => "cpp",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "sql" => "sql",
        "sh" => "bash",
        "ps1" => "powershell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "html" => "html",
        "css" | "scss" => "css",
        _ => "",
    }
}

fn build_text_preview(text: &str, max_chars: usize) -> String {
    let mut preview = String::new();
    let mut previous_blank = false;

    for line in text.lines() {
        let line = line.trim_end();
        let is_blank = line.trim().is_empty();
        if is_blank && previous_blank {
            continue;
        }
        previous_blank = is_blank;

        let next_len = preview.chars().count() + line.chars().count() + 1;
        if next_len > max_chars {
            let remaining = max_chars.saturating_sub(preview.chars().count());
            if remaining > 0 {
                preview.extend(line.chars().take(remaining));
            }
            if !preview.ends_with('\n') {
                preview.push('\n');
            }
            preview.push_str("...");
            break;
        }

        preview.push_str(line);
        preview.push('\n');
    }

    preview.trim().to_string()
}

pub(crate) fn write_markdown_artifact(
    data_dir: &Path,
    artifact_id: uuid::Uuid,
    markdown: &str,
) -> Result<PathBuf> {
    let dir = data_dir.join("context-markdown");
    cue_core::app_paths::create_private_dir(&dir)?;
    let path = dir.join(format!("{artifact_id}.md"));
    let mut file = cue_core::app_paths::create_private_file_new(&path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(markdown.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_context_filter_rejects_video_and_key_material() {
        assert!(is_supported_context_file(Path::new("plan.md")));
        assert!(is_supported_context_file(Path::new("architecture.pdf")));
        assert!(is_supported_context_file(Path::new("deck.pptx")));
        assert!(is_supported_context_file(Path::new("budget.xlsx")));
        assert!(is_supported_context_file(Path::new("forecast.xlsm")));
        assert!(is_supported_context_file(Path::new("main.rs")));
        assert!(is_supported_context_file(Path::new("diagram.png")));
        assert!(is_supported_context_file(Path::new("iphone-photo.heic")));
        assert!(is_supported_context_file(Path::new("scan.tiff")));
        assert!(!is_supported_context_file(Path::new("clip.mp4")));
        assert!(!is_supported_context_file(Path::new("backup.p12")));
    }

    #[test]
    fn code_fallback_wraps_markdown_fence() {
        let base = env::temp_dir().join(format!("bluey-doc-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base).expect("temp dir");
        let path = base.join("main.rs");
        fs::write(&path, "fn main() {}\n").expect("write source");

        let markdown =
            native_markdown_fallback(&path, ContextKind::Code, 13).expect("convert code");
        assert!(markdown.starts_with("```rust"));
        assert!(markdown.contains("fn main() {}"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn markdown_preview_collapses_blank_lines_and_truncates() {
        let preview = build_text_preview("a\n\n\nb\nlongline", 5);
        assert_eq!(preview, "a\n\nb\n...");
    }

    #[test]
    fn installed_converter_candidates_cover_wrapper_and_markitdown() {
        let root = PathBuf::from("/tmp/bluey-install-root");
        let candidates = bluey_doc_converter_candidates_for_roots(vec![root.clone()]);

        assert!(candidates.contains(&root.join("bin/bluey-doc-converter")));
        assert!(candidates.contains(&root.join("tools/doc-converter/bin/bluey-doc-converter")));
        assert!(candidates.contains(&root.join("tools/doc-converter/.venv/bin/markitdown")));

        #[cfg(target_os = "windows")]
        {
            assert!(candidates.contains(&root.join("bin/bluey-doc-converter.cmd")));
            assert!(
                candidates.contains(&root.join("tools/doc-converter/bin/bluey-doc-converter.cmd"))
            );
            assert!(
                candidates.contains(&root.join("tools/doc-converter/.venv/Scripts/markitdown.exe"))
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn packaged_converter_resolution_rejects_symlink_escape() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let base = env::temp_dir().join(format!(
            "bluey-converter-resolution-{}",
            uuid::Uuid::new_v4()
        ));
        let root = base.join("install");
        let outside = base.join("outside-converter");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(&outside, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();

        let candidate = root.join("bin/bluey-doc-converter");
        symlink(&outside, &candidate).unwrap();
        assert!(canonical_packaged_executable(&candidate, &root).is_none());

        fs::remove_file(&candidate).unwrap();
        fs::write(&candidate, b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            canonical_packaged_executable(&candidate, &root),
            Some(candidate.canonicalize().unwrap())
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn document_child_environment_excludes_credentials_and_injection_hooks() {
        assert!(!DOCUMENT_CHILD_ENV_ALLOWLIST.contains(&"OPENAI_API_KEY"));
        assert!(!DOCUMENT_CHILD_ENV_ALLOWLIST.contains(&"BLUEY_API_TOKEN"));
        assert!(!DOCUMENT_CHILD_ENV_ALLOWLIST.contains(&"LD_PRELOAD"));
        assert!(!DOCUMENT_CHILD_ENV_ALLOWLIST.contains(&"DYLD_INSERT_LIBRARIES"));

        let mut command = Command::new("unused-converter");
        configure_document_command(&mut command);
        for (name, _) in command.get_envs() {
            let name = name.to_string_lossy();
            assert!(
                DOCUMENT_CHILD_ENV_ALLOWLIST
                    .iter()
                    .any(|allowed| *allowed == name)
                    || name == "PATH"
                    || (cfg!(windows)
                        && matches!(name.as_ref(), "SystemRoot" | "WINDIR" | "ComSpec")),
                "unexpected child environment variable: {name}"
            );
        }
    }

    #[test]
    fn command_output_workspace_is_private_and_removed_on_drop() {
        let (output, file) = PrivateCommandOutput::new("test", "txt").unwrap();
        let directory = output.directory.clone();
        drop(file);
        cue_core::app_paths::validate_private_file(output.path()).unwrap();
        drop(output);
        assert!(!directory.exists());
    }

    #[cfg(unix)]
    #[test]
    fn converter_failure_does_not_surface_raw_stderr() {
        let error = run_bounded_stdout_command(
            Path::new("/bin/sh"),
            ["-c", "echo BLUEY_TEST_SECRET >&2; exit 7"],
            "test converter",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("exited unsuccessfully"));
        assert!(!error.contains("BLUEY_TEST_SECRET"));
    }

    #[cfg(unix)]
    #[test]
    fn converter_output_and_runtime_are_bounded() {
        let (output, file) = PrivateCommandOutput::new("limit-test", "txt").unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("while :; do printf '0123456789abcdef'; done");
        configure_document_command(&mut command);
        command.stdout(Stdio::from(file));
        let child = command.spawn().unwrap();
        let error = wait_for_bounded_child(
            child,
            output.path(),
            Duration::from_secs(2),
            32 * 1024,
            "test converter",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("output limit"));

        let (output, file) = PrivateCommandOutput::new("timeout-test", "txt").unwrap();
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("sleep 5");
        configure_document_command(&mut command);
        command.stdout(Stdio::from(file));
        let child = command.spawn().unwrap();
        let started = Instant::now();
        let error = wait_for_bounded_child(
            child,
            output.path(),
            Duration::from_millis(75),
            32 * 1024,
            "test converter",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("time limit"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn write_markdown_artifact_persists_under_bluey_data_dir() {
        let base = env::temp_dir().join(format!("bluey-md-artifact-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4();
        let path = write_markdown_artifact(&base, id, "# hello").expect("write markdown");
        let expected = format!("{id}.md");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(expected.as_str())
        );
        assert_eq!(fs::read_to_string(&path).expect("read markdown"), "# hello");
        cue_core::app_paths::validate_private_file(&path).expect("private markdown artifact");

        let _ = fs::remove_dir_all(base);
    }
}
