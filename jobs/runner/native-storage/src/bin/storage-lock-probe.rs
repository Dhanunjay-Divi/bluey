use bluey_jobs_runner_native_storage::RunnerStorageRoot;
use std::path::Path;

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        std::process::exit(64);
    };
    match RunnerStorageRoot::open(Path::new(&path)) {
        Ok(_root) => std::process::exit(0),
        Err(error) => {
            eprintln!("{}", error.code().as_str());
            std::process::exit(73);
        }
    }
}
