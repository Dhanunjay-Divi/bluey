# Bluey Jobs native runner storage

This crate is the native capability boundary for a persistent Bluey Jobs runner data root.

The root is opened once, retained by handle, and exclusively locked for the process lifetime.
All child operations are directory-handle-relative, reject symlinks, hardlinks, special files,
and cross-device entries, and verify that the configured root path still names the opened root.

Unix is implemented with `openat`/`mkdirat`/`renameat`/`unlinkat`, `O_NOFOLLOW`, retained file
descriptors, and `flock`. Windows intentionally returns `unsupported_platform` until the
equivalent reparse-point, file-ID, ACL, and `LockFileEx` implementation is complete. That is a
distribution gate, not a fallback to pathname checks.

The N-API methods that write, replace, read, inventory, or recursively remove return promises and
run on the N-API worker pool. Each task owns a duplicate of the retained directory capability, so
large hashes, durable writes, and purges do not block the runner heartbeat or lease event loop.

Focused verification:

```sh
cargo fmt --manifest-path jobs/runner/native-storage/Cargo.toml --all --check
cargo clippy --manifest-path jobs/runner/native-storage/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path jobs/runner/native-storage/Cargo.toml --all-targets
cargo build --manifest-path jobs/runner/native-storage/Cargo.toml --release
```

Release builds produce
`jobs/runner/native-storage/target/release/libbluey_jobs_runner_native_storage.dylib` on macOS and
`jobs/runner/native-storage/target/release/libbluey_jobs_runner_native_storage.so` on Linux. Node
loads the staged artifact as `jobs/runner/dist/native/bluey_jobs_runner_native_storage.node`.
Stage it only after the TypeScript build (which recreates `dist`):

```sh
# macOS development staging
mkdir -p jobs/runner/dist/native
install -m 0555 \
  jobs/runner/native-storage/target/release/libbluey_jobs_runner_native_storage.dylib \
  jobs/runner/dist/native/bluey_jobs_runner_native_storage.node

# Linux/Docker staging in the final runner image
install -D -m 0555 \
  jobs/runner/native-storage/target/release/libbluey_jobs_runner_native_storage.so \
  /app/runner/dist/native/bluey_jobs_runner_native_storage.node
```
