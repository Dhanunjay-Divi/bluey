/// Intel macOS ONLY: point ONNX Runtime at the dylib we ship beside the binary.
///
/// `ort` has no prebuilt ONNX Runtime for macOS x86_64, so on that target
/// `parakeet-rs` is built with `load-dynamic` (see cue-daemon/Cargo.toml) and
/// resolves `libonnxruntime.dylib` at RUNTIME. Without this, the lookup falls
/// back to the system search path, finds nothing on a stock Mac (no Homebrew),
/// and on-device STT fails at first use.
///
/// The package ships the dylib at `<install>/lib/libonnxruntime.dylib` with the
/// binaries in `<install>/bin/`, so we resolve it relative to the running
/// executable — the install location can be anywhere (AirDrop, /Applications,
/// a user's home) and this still works. An existing `ORT_DYLIB_PATH` always
/// wins, so a developer can point at their own build.
///
/// No-op on every other target: Apple Silicon, Windows, and Linux link the
/// prebuilt native binaries via `ort-defaults`.
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn resolve_onnxruntime_dylib() {
    use std::path::PathBuf;

    if std::env::var_os("ORT_DYLIB_PATH").is_some() {
        return; // explicit override wins
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // <install>/bin/bluey-daemon -> <install>/lib/libonnxruntime.dylib, plus a
    // sibling fallback for a flat layout (everything in one directory).
    let candidates: [Option<PathBuf>; 2] = [
        exe.parent()
            .and_then(|d| d.parent())
            .map(|root| root.join("lib").join("libonnxruntime.dylib")),
        exe.parent().map(|d| d.join("libonnxruntime.dylib")),
    ];
    for candidate in candidates.into_iter().flatten() {
        if candidate.is_file() {
            // SAFETY: single-threaded startup, before any runtime is spawned.
            std::env::set_var("ORT_DYLIB_PATH", &candidate);
            return;
        }
    }
    // Not found: leave ORT_DYLIB_PATH unset so ort falls back to the system
    // search path and reports its own error if STT is actually used.
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn resolve_onnxruntime_dylib() {}

fn main() -> anyhow::Result<()> {
    resolve_onnxruntime_dylib();
    cue_daemon::app::run()
}
