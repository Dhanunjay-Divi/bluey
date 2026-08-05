#![cfg(unix)]

use bluey_jobs_runner_native_storage::{
    InventoryEntryKind, MoveOutcome, RunnerStorageRoot, StorageErrorCode, StorageResult,
};
use std::ffi::CString;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn writes_replaces_inventories_and_removes_with_retained_handles() {
    let fixture = TestDirectory::new("roundtrip");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let subjects = root
        .ensure_directory(&["account-data-v2".to_owned(), "subjects".to_owned()])
        .expect("create subject directory");
    let subject = subjects
        .ensure_child_directory(&"a".repeat(64))
        .expect("create subject");
    let profiles = subject
        .ensure_child_directory("profiles")
        .expect("create profiles");

    assert!(profiles
        .write_file_exclusive("record.json", b"one")
        .expect("write exact record"));
    assert!(!profiles
        .write_file_exclusive("record.json", b"conflict")
        .expect("report existing exact path without replacing it"));
    assert_eq!(
        profiles
            .read_file_bounded("record.json", 32)
            .expect("read record"),
        b"one"
    );
    profiles
        .replace_file("record.json", b"three")
        .expect("replace exact record");
    assert_eq!(
        profiles
            .read_file_bounded("record.json", 32)
            .expect("read replacement"),
        b"three"
    );

    let inventory = subject.inventory().expect("inventory subject");
    assert_eq!(inventory.count, 2);
    assert_eq!(inventory.bytes, 5);
    assert_eq!(inventory.entries[0].relative_path, "profiles");
    assert_eq!(inventory.entries[0].kind, InventoryEntryKind::Directory);
    assert_eq!(inventory.entries[1].relative_path, "profiles/record.json");
    assert_eq!(inventory.entries[1].kind, InventoryEntryKind::File);
    assert_eq!(inventory.sha256.len(), 64);

    subject
        .remove_entry("profiles")
        .expect("remove subject subtree");
    assert!(subject
        .inventory()
        .expect("empty inventory")
        .entries
        .is_empty());
}

#[test]
fn atomically_moves_safe_entries_without_replacing_a_destination() {
    let fixture = TestDirectory::new("move-no-replace");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let legacy = root
        .ensure_directory(&["account-data-v1".to_owned()])
        .expect("create legacy root");
    let source = legacy
        .ensure_child_directory("profile-a")
        .expect("create legacy profile");
    source
        .write_file_exclusive("state.json", b"legacy")
        .expect("write legacy state");
    let destination_parent = root
        .ensure_directory(&[
            "account-data-v2".to_owned(),
            "subjects".to_owned(),
            "subject-a".to_owned(),
            "profiles".to_owned(),
        ])
        .expect("create destination parent");

    assert_eq!(
        root.move_entry_noreplace(
            &["account-data-v1".to_owned(), "profile-a".to_owned()],
            &[
                "account-data-v2".to_owned(),
                "subjects".to_owned(),
                "subject-a".to_owned(),
                "profiles".to_owned(),
                "profile-a".to_owned(),
            ],
        )
        .expect("move legacy profile"),
        MoveOutcome::Moved
    );
    assert_eq!(
        result_error_code(legacy.open_child_directory("profile-a")),
        StorageErrorCode::Io
    );
    let moved = destination_parent
        .open_child_directory("profile-a")
        .expect("open moved profile");
    assert_eq!(
        moved
            .read_file_bounded("state.json", 32)
            .expect("read moved state"),
        b"legacy"
    );

    let conflicting_source = legacy
        .ensure_child_directory("profile-b")
        .expect("create conflicting source");
    conflicting_source
        .write_file_exclusive("source.json", b"source")
        .expect("write conflicting source");
    let conflicting_destination = destination_parent
        .ensure_child_directory("profile-b")
        .expect("create conflicting destination");
    conflicting_destination
        .write_file_exclusive("destination.json", b"destination")
        .expect("write conflicting destination");
    let source_components = &["account-data-v1".to_owned(), "profile-b".to_owned()];
    let destination_components = &[
        "account-data-v2".to_owned(),
        "subjects".to_owned(),
        "subject-a".to_owned(),
        "profiles".to_owned(),
        "profile-b".to_owned(),
    ];
    assert_eq!(
        root.move_entry_noreplace(source_components, destination_components)
            .expect("report destination conflict"),
        MoveOutcome::DestinationExists
    );
    assert_eq!(
        conflicting_source
            .read_file_bounded("source.json", 32)
            .expect("preserve source on conflict"),
        b"source"
    );
    assert_eq!(
        conflicting_destination
            .read_file_bounded("destination.json", 32)
            .expect("preserve destination on conflict"),
        b"destination"
    );

    legacy
        .remove_entry("profile-b")
        .expect("remove conflict source");
    assert_eq!(
        root.move_entry_noreplace(source_components, destination_components)
            .expect("report absent source"),
        MoveOutcome::SourceMissing
    );
}

#[test]
fn move_rejects_unsafe_entries_and_a_destination_inside_the_source() {
    let fixture = TestDirectory::new("move-unsafe");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let source = root
        .ensure_directory(&["source".to_owned(), "nested".to_owned()])
        .expect("create source subtree");
    let destination = root
        .ensure_directory(&["destination".to_owned()])
        .expect("create destination");
    let outside = fixture.parent.join("outside.txt");
    write_private(&outside, b"sentinel");
    symlink(&outside, source.canonical_path().unwrap().join("unsafe")).expect("create symlink");

    assert_eq!(
        root.move_entry_noreplace(
            &[
                "source".to_owned(),
                "nested".to_owned(),
                "unsafe".to_owned(),
            ],
            &["destination".to_owned(), "unsafe".to_owned()],
        )
        .unwrap_err()
        .code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(fs::read(&outside).expect("outside sentinel"), b"sentinel");
    assert!(destination
        .inventory()
        .expect("safe destination")
        .entries
        .is_empty());
    assert_eq!(
        root.move_entry_noreplace(
            &["source".to_owned()],
            &[
                "source".to_owned(),
                "nested".to_owned(),
                "renamed".to_owned(),
            ],
        )
        .unwrap_err()
        .code(),
        StorageErrorCode::Configuration
    );

    let chromium_source = root
        .ensure_directory(&["legacy-active".to_owned()])
        .expect("create chromium source");
    chromium_source
        .write_file_exclusive("Local State", b"state")
        .expect("write chromium entry");
    assert_eq!(
        root.move_entry_noreplace(
            &["legacy-active".to_owned(), "Local State".to_owned()],
            &["destination".to_owned(), "Local State".to_owned()],
        )
        .expect("move safe chromium name"),
        MoveOutcome::Moved
    );
    assert_eq!(
        destination
            .read_file_bounded("Local State", 32)
            .expect("read moved chromium entry"),
        b"state"
    );
}

#[test]
fn rejects_symlinks_without_reading_or_removing_their_targets() {
    let fixture = TestDirectory::new("symlink");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let directory = root
        .ensure_directory(&["subject".to_owned()])
        .expect("create directory");
    let outside = fixture.parent.join("outside.txt");
    write_private(&outside, b"sentinel");
    symlink(&outside, directory.canonical_path().unwrap().join("unsafe")).expect("create symlink");

    assert_eq!(
        directory.inventory().unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(
        directory.remove_entry("unsafe").unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(fs::read(&outside).expect("outside sentinel"), b"sentinel");
}

#[test]
fn rejects_a_parent_swap_instead_of_writing_through_the_replacement() {
    let fixture = TestDirectory::new("parent-swap");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let directory = root
        .ensure_directory(&["subject".to_owned()])
        .expect("create subject");
    directory
        .write_file_exclusive("owned.json", b"owned")
        .expect("write owned entry");
    let original = fixture.root.join("subject");
    let displaced = fixture.root.join("subject-old");
    let outside = fixture.parent.join("outside");
    private_directory(&outside);

    fs::rename(&original, &displaced).expect("displace subject directory");
    symlink(&outside, &original).expect("replace subject with symlink");

    assert_eq!(
        directory
            .write_file_exclusive("escaped.json", b"private")
            .unwrap_err()
            .code(),
        StorageErrorCode::UnsafeEntry
    );
    assert!(!outside.join("escaped.json").exists());
    assert!(!displaced.join("escaped.json").exists());
    assert_eq!(
        directory.remove_entry("owned.json").unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(
        fs::read(displaced.join("owned.json")).expect("preserve displaced entry"),
        b"owned"
    );
    assert!(!outside.join("owned.json").exists());
}

#[test]
fn rejects_hardlinks_and_preserves_the_outside_link() {
    let fixture = TestDirectory::new("hardlink");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let directory = root
        .ensure_directory(&["subject".to_owned()])
        .expect("create subject");
    let outside = fixture.parent.join("outside.txt");
    write_private(&outside, b"sentinel");
    fs::hard_link(
        &outside,
        directory.canonical_path().unwrap().join("linked.txt"),
    )
    .expect("create hardlink");

    assert_eq!(
        directory.inventory().unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(
        directory.remove_entry("linked.txt").unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(fs::read(&outside).expect("outside sentinel"), b"sentinel");
    assert_eq!(fs::metadata(&outside).expect("outside metadata").nlink(), 2);
}

#[test]
fn rejects_special_files_without_unlinking_them() {
    let fixture = TestDirectory::new("special");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let directory = root
        .ensure_directory(&["subject".to_owned()])
        .expect("create subject");
    let fifo = directory.canonical_path().unwrap().join("pipe");
    let encoded = CString::new(fifo.as_os_str().as_bytes()).expect("fifo path");
    let result = unsafe { libc::mkfifo(encoded.as_ptr(), 0o600) };
    assert_eq!(
        result,
        0,
        "create fifo: {}",
        std::io::Error::last_os_error()
    );

    assert_eq!(
        directory.inventory().unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(
        directory.remove_entry("pipe").unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
    assert_eq!(
        fs::symlink_metadata(&fifo).expect("fifo metadata").mode() & u32::from(libc::S_IFMT),
        u32::from(libc::S_IFIFO)
    );
}

#[test]
fn separates_controlled_layout_components_from_safe_entry_names() {
    let fixture = TestDirectory::new("invalid-name");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let directory = root
        .ensure_directory(&["subject".to_owned()])
        .expect("create subject");
    write_private(
        &directory.canonical_path().unwrap().join("unexpected name"),
        b"sentinel",
    );

    let inventory = directory.inventory().expect("inventory name with a space");
    assert_eq!(inventory.entries[0].relative_path, "unexpected name");
    assert_eq!(
        directory
            .inventory()
            .expect("repeat inventory from a fresh directory stream"),
        inventory
    );
    assert!(directory
        .write_file_exclusive("another name", b"allowed")
        .expect("write safe entry name with a space"));
    assert_eq!(
        directory
            .read_file_bounded("another name", 32)
            .expect("read safe entry name"),
        b"allowed"
    );
    assert_eq!(
        result_error_code(root.ensure_directory(&["another name".to_owned()])),
        StorageErrorCode::PathEscape
    );
    assert_eq!(
        directory
            .write_file_exclusive(".bluey-runner-storage.lock", b"blocked")
            .unwrap_err()
            .code(),
        StorageErrorCode::PathEscape
    );

    write_private(
        &directory.canonical_path().unwrap().join("line\nbreak"),
        b"unsafe-evidence",
    );
    assert_eq!(
        directory.inventory().unwrap_err().code(),
        StorageErrorCode::UnsafeEntry
    );
}

#[test]
fn detects_configured_root_replacement_before_any_child_operation() {
    let fixture = TestDirectory::new("root-swap");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let displaced = fixture.parent.join("runner-old");
    fs::rename(&fixture.root, &displaced).expect("displace root");
    private_directory(&fixture.root);

    assert_eq!(
        root.assert_unchanged().unwrap_err().code(),
        StorageErrorCode::RootChanged
    );
    assert_eq!(
        result_error_code(root.ensure_directory(&["subject".to_owned()])),
        StorageErrorCode::RootChanged
    );
    assert!(!fixture.root.join("subject").exists());
    assert!(!displaced.join("subject").exists());
}

#[test]
fn rejects_a_symlink_root_and_symlinked_ancestor() {
    let fixture = TestDirectory::new("root-link");
    let real = fixture.parent.join("real");
    private_directory(&real);
    let alias = fixture.parent.join("alias");
    symlink(&real, &alias).expect("create root alias");
    assert_eq!(
        result_error_code(RunnerStorageRoot::open(&alias)),
        StorageErrorCode::UnsafeEntry
    );

    let ancestor = fixture.parent.join("ancestor-link");
    symlink(&fixture.parent, &ancestor).expect("create ancestor alias");
    assert_eq!(
        result_error_code(RunnerStorageRoot::open(&ancestor.join("nested"))),
        StorageErrorCode::UnsafeEntry
    );
}

#[test]
fn inventories_native_lock_and_realistic_chromium_names() {
    let fixture = TestDirectory::new("chromium-names");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    write_private(&fixture.root.join("Local State"), b"state");
    let directory = root
        .open_directory(&[])
        .expect("open retained root directory");

    let inventory = directory.inventory().expect("inventory root");
    assert!(inventory
        .entries
        .iter()
        .any(|entry| entry.relative_path == ".bluey-runner-storage.lock"));
    assert!(inventory
        .entries
        .iter()
        .any(|entry| entry.relative_path == "Local State"));
}

#[test]
fn opening_root_recovers_only_exact_private_staging_remnants() {
    let fixture = TestDirectory::new("staging-recovery");
    {
        let root = RunnerStorageRoot::open(&fixture.root).expect("create root");
        root.ensure_directory(&["account-data-v2".to_owned()])
            .expect("create nested directory");
    }
    let root_stage = fixture.root.join(".bluey-stage-123-0000000000000001");
    let nested_stage = fixture
        .root
        .join("account-data-v2")
        .join(".bluey-stage-456-0000000000000002");
    write_private(&root_stage, b"partial");
    write_private(&nested_stage, b"partial");

    let _root = RunnerStorageRoot::open(&fixture.root).expect("recover staging remnants");
    assert!(!root_stage.exists());
    assert!(!nested_stage.exists());
}

#[test]
fn a_second_process_cannot_open_the_locked_root() {
    let fixture = TestDirectory::new("process-lock");
    let root = RunnerStorageRoot::open(&fixture.root).expect("open root");
    let locked_probe = probe(&fixture.root);
    assert_eq!(locked_probe.status.code(), Some(73));
    assert_eq!(
        String::from_utf8_lossy(&locked_probe.stderr).trim(),
        "root_locked"
    );

    drop(root);
    let unlocked_probe = probe(&fixture.root);
    assert!(
        unlocked_probe.status.success(),
        "{}",
        String::from_utf8_lossy(&unlocked_probe.stderr)
    );
}

fn result_error_code<T>(result: StorageResult<T>) -> StorageErrorCode {
    match result {
        Ok(_) => panic!("operation unexpectedly succeeded"),
        Err(error) => error.code(),
    }
}

fn probe(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_bluey-runner-storage-lock-probe"))
        .arg(root)
        .output()
        .expect("run lock probe")
}

fn write_private(path: &Path, contents: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("create private file");
    file.write_all(contents).expect("write private file");
}

fn private_directory(path: &Path) {
    DirBuilder::new()
        .mode(0o700)
        .create(path)
        .expect("create private directory");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("secure private directory");
}

struct TestDirectory {
    parent: PathBuf,
    root: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("native-storage-tests");
        fs::create_dir_all(&base).expect("create test base");
        let parent = base.join(format!("{label}-{}-{sequence}", std::process::id()));
        private_directory(&parent);
        let root = parent.join("runner");
        Self { parent, root }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.parent);
    }
}
