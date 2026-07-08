use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use cue_core::ContextKind;
use tracing::debug;

const MAX_DIRECT_TEXT_BYTES: u64 = 1_000_000;
const MAX_MARKITDOWN_INPUT_BYTES: u64 = 25_000_000;
const MAX_MARKITDOWN_OUTPUT_BYTES: u64 = 2_000_000;
const MARKITDOWN_TIMEOUT: Duration = Duration::from_secs(20);
const PREVIEW_CHARS: usize = 16_000;

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
                    path = %path.display(),
                    "MarkItDown returned empty markdown; falling back to native parser"
                );
            }
            Err(error) => {
                debug!(
                    path = %path.display(),
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

    for name in ["BLUEY_DOC_CONVERTER_BIN", "BLUEY_MARKITDOWN_BIN"] {
        if let Some(value) = env::var_os(name).filter(|value| !value.is_empty()) {
            push(PathBuf::from(value));
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            for candidate in bluey_local_doc_converter_candidates(dir) {
                if candidate.is_file() {
                    push(candidate);
                }
            }
        }
    }

    for candidate in bluey_home_doc_converter_candidates() {
        if candidate.is_file() {
            push(candidate);
        }
    }

    // Let Command::new resolve PATH. If neither command exists, conversion
    // falls back without making users install anything manually.
    push(PathBuf::from("bluey-doc-converter"));
    push(PathBuf::from("markitdown"));

    commands
}

fn bluey_local_doc_converter_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend([
        exe_dir.join("bluey-doc-converter"),
        exe_dir.join("markitdown"),
        exe_dir.join("bin/bluey-doc-converter"),
        exe_dir.join("bin/markitdown"),
    ]);

    #[cfg(target_os = "windows")]
    candidates.extend([
        exe_dir.join("bluey-doc-converter.cmd"),
        exe_dir.join("markitdown.exe"),
        exe_dir.join("bin/bluey-doc-converter.cmd"),
        exe_dir.join("bin/markitdown.exe"),
    ]);

    if let Some(install_root) = exe_dir.parent() {
        candidates.extend([
            install_root.join("tools/doc-converter/bin/bluey-doc-converter"),
            install_root.join("tools/doc-converter/.venv/bin/markitdown"),
        ]);

        #[cfg(target_os = "windows")]
        candidates.extend([
            install_root.join("tools/doc-converter/bin/bluey-doc-converter.cmd"),
            install_root.join("tools/doc-converter/.venv/Scripts/markitdown.exe"),
        ]);
    }

    candidates
}

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
            root.join("bin/bluey-doc-converter"),
            root.join("tools/doc-converter/bin/bluey-doc-converter"),
            root.join("tools/doc-converter/.venv/bin/markitdown"),
        ]);

        #[cfg(target_os = "windows")]
        candidates.extend([
            root.join("bin/bluey-doc-converter.cmd"),
            root.join("tools/doc-converter/bin/bluey-doc-converter.cmd"),
            root.join("tools/doc-converter/.venv/Scripts/markitdown.exe"),
        ]);
    }
    candidates
}

fn run_markitdown(converter: &ConverterCommand, path: &Path) -> Result<String> {
    let output_path = env::temp_dir().join(format!(
        "bluey-markitdown-{}-{}.md",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    let mut child = Command::new(&converter.program)
        .arg(path)
        .arg("-o")
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch {}", converter.program.display()))?;

    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed to poll document converter")?
        {
            if !status.success() {
                let _ = fs::remove_file(&output_path);
                return Err(anyhow!(
                    "{} exited with status {}",
                    converter.program.display(),
                    status
                ));
            }
            break;
        }

        if started.elapsed() > MARKITDOWN_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&output_path);
            return Err(anyhow!(
                "{} timed out after {}s",
                converter.program.display(),
                MARKITDOWN_TIMEOUT.as_secs()
            ));
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    let markdown = read_utf8_prefix(&output_path, MAX_MARKITDOWN_OUTPUT_BYTES)
        .with_context(|| format!("failed to read {}", output_path.display()))?;
    let _ = fs::remove_file(&output_path);
    Ok(markdown)
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
    let output = match Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(anyhow!(
                "PDF text extraction needs `pdftotext` locally or the bundled MarkItDown converter"
            ));
        }
        Err(error) => return Err(error).context("failed to run PDF text extractor"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(anyhow!(
            "PDF text extraction failed: {}",
            if detail.is_empty() {
                "unknown pdftotext error"
            } else {
                detail
            }
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn extract_word_text(path: &Path) -> Result<String> {
    let output = match Command::new("textutil")
        .arg("-convert")
        .arg("txt")
        .arg("-stdout")
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(anyhow!(
                "DOC/DOCX text extraction needs macOS `textutil` or the bundled MarkItDown converter"
            ));
        }
        Err(error) => return Err(error).context("failed to run document text extractor"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(anyhow!(
            "document text extraction failed: {}",
            if detail.is_empty() {
                "unknown textutil error"
            } else {
                detail
            }
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .output()
        .context("failed to launch Windows document parser")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(anyhow!(
            "document text extraction failed: {}",
            if detail.is_empty() {
                "unknown PowerShell parser error"
            } else {
                detail
            }
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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

fn read_utf8_prefix(path: &Path, max_bytes: u64) -> Result<String> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > max_bytes;
    if truncated {
        bytes.truncate(max_bytes as usize);
    }
    let mut text = String::from_utf8_lossy(&bytes).to_string();
    if truncated {
        text.push_str("\n\n...");
    }
    Ok(text)
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
    let mut file =
        File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
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

        let _ = fs::remove_dir_all(base);
    }
}
