use std::fs;
use std::path::Path;

fn main() {
    // Ensure ui/dist exists for tauri generate_context! macro.
    // In production, `npm run build` populates this. For Rust-only builds,
    // we create a minimal placeholder so compilation succeeds.
    let dist = Path::new("ui/dist");
    if !dist.join("index.html").exists() {
        fs::create_dir_all(dist).expect("failed to create ui/dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><html><body></body></html>",
        )
        .expect("failed to write placeholder index.html");
    }
    tauri_build::build()
}
