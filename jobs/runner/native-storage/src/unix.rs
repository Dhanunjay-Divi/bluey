use crate::{
    Inventory, InventoryEntry, InventoryEntryKind, MoveOutcome, StorageError, StorageErrorCode,
    StorageResult,
};
use sha2::{Digest, Sha256};
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

const PRIVATE_DIRECTORY_MODE: libc::mode_t = 0o700;
const PRIVATE_FILE_MODE: libc::mode_t = 0o600;
const ROOT_LOCK_FILE: &str = ".bluey-runner-storage.lock";
const MAX_INVENTORY_ENTRIES: usize = 100_000;
const MAX_INVENTORY_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_INVENTORY_DEPTH: usize = 128;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct RunnerStorageRoot {
    inner: Arc<RootState>,
}

struct RootState {
    configured_path: PathBuf,
    directory: File,
    _lock: File,
    operation_lock: Mutex<()>,
    boundary: StorageBoundary,
    inode: libc::ino_t,
}

#[derive(Clone, Copy)]
struct StorageBoundary {
    device: libc::dev_t,
    mount_id: u64,
}

pub struct RunnerStorageDirectory {
    root: Arc<RootState>,
    directory: File,
    components: Vec<String>,
    boundary: StorageBoundary,
    inode: libc::ino_t,
}

#[derive(Clone)]
pub(crate) struct DeferredRunnerStorageDirectory {
    root: Arc<RootState>,
    components: Vec<String>,
    boundary: StorageBoundary,
    inode: libc::ino_t,
}

impl RunnerStorageRoot {
    /// Opens or creates an owner-private root and holds its exclusive process lock.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is not an absolute canonical path, any component is unsafe,
    /// the root metadata is not owner-private, or another process already holds the root lock.
    pub fn open(configured_root: &Path) -> StorageResult<Self> {
        let (configured_path, directory) = open_or_create_absolute_root(configured_root)?;
        let metadata = file_metadata(&directory)?;
        validate_root_metadata(&metadata)?;
        let boundary = StorageBoundary {
            device: metadata.st_dev,
            mount_id: mount_identity(&directory)?,
        };
        validate_directory_handle(&directory, boundary)?;
        let lock = open_root_lock(&directory, boundary)?;
        cleanup_staging_files(&directory, boundary, 0)?;
        let state = RootState {
            configured_path,
            directory,
            _lock: lock,
            operation_lock: Mutex::new(()),
            boundary,
            inode: metadata.st_ino,
        };
        state.assert_binding()?;
        Ok(Self {
            inner: Arc::new(state),
        })
    }

    #[must_use]
    pub fn configured_path(&self) -> &Path {
        &self.inner.configured_path
    }

    /// Returns the stable device identifier for the retained root.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured root binding changed.
    pub fn device_id(&self) -> StorageResult<String> {
        self.inner.assert_binding()?;
        Ok(device_id(self.inner.boundary))
    }

    /// Returns the current link count for the retained root directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured root binding changed.
    pub fn link_count(&self) -> StorageResult<u64> {
        self.inner.assert_binding()?;
        Ok(metadata_link_count(&file_metadata(&self.inner.directory)?))
    }

    /// Confirms that the configured pathname still identifies the retained root handle.
    ///
    /// # Errors
    ///
    /// Returns [`StorageErrorCode::RootChanged`] when the root path was replaced and otherwise
    /// reports unsafe root metadata or an I/O failure.
    pub fn assert_unchanged(&self) -> StorageResult<()> {
        self.inner.assert_binding()
    }

    /// Opens an owner-private directory chain, creating missing components handle-relatively.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid components, unsafe metadata, a changed root, or an I/O
    /// failure.
    pub fn ensure_directory(&self, components: &[String]) -> StorageResult<RunnerStorageDirectory> {
        self.open_directory_internal(components, true)
    }

    /// Opens an existing owner-private directory chain relative to the retained root.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or missing components, unsafe metadata, a changed root, or an
    /// I/O failure.
    pub fn open_directory(&self, components: &[String]) -> StorageResult<RunnerStorageDirectory> {
        self.open_directory_internal(components, false)
    }

    /// Atomically moves one safe entry within this retained root without replacing a destination.
    ///
    /// Both component arrays include the entry name as their final component. The destination
    /// parent must already exist. Exact outcomes distinguish a completed move, a missing source,
    /// and a safe existing destination so a crash-recovery journal can verify the final contents.
    ///
    /// # Errors
    ///
    /// Returns an error for empty or invalid component arrays, an unsafe entry, a move into the
    /// source subtree, a changed binding, or an I/O failure.
    pub fn move_entry_noreplace(
        &self,
        source_components: &[String],
        destination_components: &[String],
    ) -> StorageResult<MoveOutcome> {
        let _operation = self
            .inner
            .operation_lock
            .lock()
            .map_err(|_| StorageError::new(StorageErrorCode::Io))?;
        self.inner.assert_binding()?;
        let (source_name, source_parent_components) = source_components
            .split_last()
            .ok_or_else(|| StorageError::new(StorageErrorCode::Configuration))?;
        let (destination_name, destination_parent_components) = destination_components
            .split_last()
            .ok_or_else(|| StorageError::new(StorageErrorCode::Configuration))?;
        for component in source_parent_components
            .iter()
            .chain(destination_parent_components)
        {
            validate_component(component)?;
        }
        validate_entry_name(source_name)?;
        validate_entry_name(destination_name)?;
        if source_components == destination_components
            || destination_parent_components.starts_with(source_components)
        {
            return Err(StorageError::new(StorageErrorCode::Configuration));
        }

        let source_parent = self
            .inner
            .open_components(source_parent_components, false)?;
        let destination_parent = self
            .inner
            .open_components(destination_parent_components, false)?;
        let outcome = move_entry_noreplace_at(
            &source_parent,
            &c_entry_name(source_name)?,
            &destination_parent,
            &c_entry_name(destination_name)?,
            self.inner.boundary,
        )?;
        self.inner.assert_binding()?;
        Ok(outcome)
    }

    fn open_directory_internal(
        &self,
        components: &[String],
        create: bool,
    ) -> StorageResult<RunnerStorageDirectory> {
        self.inner.assert_binding()?;
        for component in components {
            validate_component(component)?;
        }
        let directory = self.inner.open_components(components, create)?;
        let metadata = file_metadata(&directory)?;
        validate_directory_handle(&directory, self.inner.boundary)?;
        self.inner.assert_binding()?;
        Ok(RunnerStorageDirectory {
            root: Arc::clone(&self.inner),
            directory,
            components: components.to_vec(),
            boundary: self.inner.boundary,
            inode: metadata.st_ino,
        })
    }
}

impl RunnerStorageDirectory {
    pub(crate) fn deferred_capability(&self) -> DeferredRunnerStorageDirectory {
        DeferredRunnerStorageDirectory {
            root: Arc::clone(&self.root),
            components: self.components.clone(),
            boundary: self.boundary,
            inode: self.inode,
        }
    }

    fn reopen_deferred(capability: &DeferredRunnerStorageDirectory) -> StorageResult<Self> {
        capability.root.assert_binding()?;
        let directory = capability
            .root
            .open_components(&capability.components, false)?;
        let metadata = file_metadata(&directory)?;
        validate_directory_handle(&directory, capability.root.boundary)?;
        if metadata.st_dev != capability.boundary.device || metadata.st_ino != capability.inode {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        capability.root.assert_binding()?;
        Ok(Self {
            root: Arc::clone(&capability.root),
            directory,
            components: capability.components.clone(),
            boundary: capability.boundary,
            inode: capability.inode,
        })
    }

    #[must_use]
    pub fn relative_path(&self) -> String {
        self.components.join("/")
    }

    /// Returns the stable device identifier for this retained directory.
    ///
    /// # Errors
    ///
    /// Returns an error if this directory or the root binding changed.
    pub fn device_id(&self) -> StorageResult<String> {
        self.assert_binding()?;
        Ok(device_id(self.boundary))
    }

    /// Returns the current link count for this retained directory.
    ///
    /// # Errors
    ///
    /// Returns an error if this directory or the root binding changed.
    pub fn link_count(&self) -> StorageResult<u64> {
        self.assert_binding()?;
        Ok(metadata_link_count(&file_metadata(&self.directory)?))
    }

    /// Returns the verified configured pathname corresponding to this retained directory handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the root or any directory binding has changed.
    pub fn canonical_path(&self) -> StorageResult<PathBuf> {
        self.assert_binding()?;
        let mut path = self.root.configured_path.clone();
        for component in &self.components {
            path.push(component);
        }
        Ok(path)
    }

    /// Opens or creates one owner-private child directory relative to this retained handle.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name, unsafe metadata, a changed binding, or an I/O
    /// failure.
    pub fn ensure_child_directory(&self, name: &str) -> StorageResult<Self> {
        self.open_child_directory_internal(name, true)
    }

    /// Opens one existing owner-private child directory relative to this retained handle.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid or missing name, unsafe metadata, a changed binding, or an
    /// I/O failure.
    pub fn open_child_directory(&self, name: &str) -> StorageResult<Self> {
        self.open_child_directory_internal(name, false)
    }

    /// Creates and durably writes a new owner-private regular file without replacing an entry.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name, an existing entry, unsafe metadata, a changed
    /// binding, or an I/O failure.
    pub fn write_file_exclusive(&self, name: &str, contents: &[u8]) -> StorageResult<bool> {
        let _operation = self.lock_operation()?;
        self.assert_binding()?;
        validate_entry_name(name)?;
        let destination = c_entry_name(name)?;
        if let Some(metadata) = metadata_at(self.directory.as_raw_fd(), &destination)? {
            validate_regular_file_metadata(&metadata, self.root.boundary)?;
            self.assert_binding()?;
            return Ok(false);
        }

        let (staging, mut file) = create_staging_file(&self.directory)?;
        let staged_metadata = file_metadata(&file)?;
        let publish = (|| {
            validate_regular_file_metadata(&staged_metadata, self.root.boundary)?;
            file.write_all(contents)
                .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
            file.sync_all()
                .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
            let written = file_metadata(&file)?;
            validate_regular_file_metadata(&written, self.root.boundary)?;
            if staged_metadata.st_dev != written.st_dev || staged_metadata.st_ino != written.st_ino
            {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            if !rename_noreplace_at(&self.directory, &staging, &destination)? {
                let existing = metadata_at(self.directory.as_raw_fd(), &destination)?
                    .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
                validate_regular_file_metadata(&existing, self.root.boundary)?;
                return Ok(false);
            }
            let persisted =
                open_regular_file_at(self.directory.as_raw_fd(), &destination, self.root.boundary)?;
            let persisted_metadata = file_metadata(&persisted)?;
            if persisted_metadata.st_dev != staged_metadata.st_dev
                || persisted_metadata.st_ino != staged_metadata.st_ino
            {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            sync_directory(&self.directory)?;
            Ok(true)
        })();
        if metadata_at(self.directory.as_raw_fd(), &staging)?.is_some() {
            unlink_at(self.directory.as_raw_fd(), &staging, 0)?;
            sync_directory(&self.directory)?;
        }
        let created = publish?;
        self.assert_binding()?;
        Ok(created)
    }

    /// Durably replaces a regular file through a same-directory staged rename.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name, unsafe existing metadata, a changed binding, or an
    /// I/O failure.
    pub fn replace_file(&self, name: &str, contents: &[u8]) -> StorageResult<()> {
        let _operation = self.lock_operation()?;
        self.assert_binding()?;
        validate_entry_name(name)?;
        let destination = c_entry_name(name)?;
        if let Some(metadata) = metadata_at(self.directory.as_raw_fd(), &destination)? {
            validate_regular_file_metadata(&metadata, self.root.boundary)?;
        }

        let (staging, mut file) = create_staging_file(&self.directory)?;
        let staged_metadata = file_metadata(&file)?;
        let publish = (|| {
            validate_regular_file_metadata(&staged_metadata, self.root.boundary)?;
            file.write_all(contents)
                .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
            file.sync_all()
                .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
            let result = unsafe {
                libc::renameat(
                    self.directory.as_raw_fd(),
                    staging.as_ptr(),
                    self.directory.as_raw_fd(),
                    destination.as_ptr(),
                )
            };
            if result != 0 {
                return Err(last_io(StorageErrorCode::Io));
            }
            let persisted =
                open_regular_file_at(self.directory.as_raw_fd(), &destination, self.root.boundary)?;
            let persisted_metadata = file_metadata(&persisted)?;
            if persisted_metadata.st_dev != staged_metadata.st_dev
                || persisted_metadata.st_ino != staged_metadata.st_ino
            {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            sync_directory(&self.directory)
        })();
        if publish.is_err() {
            let _ = unlink_at(self.directory.as_raw_fd(), &staging, 0);
        }
        publish?;
        self.assert_binding()
    }

    /// Reads an owner-private regular file while enforcing an explicit byte limit.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero limit, an invalid name, an oversized or unsafe file, a changed
    /// binding, or an I/O failure.
    pub fn read_file_bounded(&self, name: &str, maximum_bytes: u64) -> StorageResult<Vec<u8>> {
        let _operation = self.lock_operation()?;
        self.assert_binding()?;
        validate_entry_name(name)?;
        if maximum_bytes == 0 {
            return Err(StorageError::new(StorageErrorCode::Configuration));
        }
        let encoded = c_entry_name(name)?;
        let mut file =
            open_regular_file_at(self.directory.as_raw_fd(), &encoded, self.root.boundary)?;
        let before = file_metadata(&file)?;
        if before.st_size < 0 || before.st_size.cast_unsigned() > maximum_bytes {
            return Err(StorageError::new(StorageErrorCode::InventoryLimit));
        }
        let capacity: usize = before
            .st_size
            .try_into()
            .map_err(|_| StorageError::new(StorageErrorCode::InventoryLimit))?;
        let mut contents = Vec::with_capacity(capacity);
        let read_limit = maximum_bytes
            .checked_add(1)
            .ok_or_else(|| StorageError::new(StorageErrorCode::InventoryLimit))?;
        Read::by_ref(&mut file)
            .take(read_limit)
            .read_to_end(&mut contents)
            .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
        if u64::try_from(contents.len()).unwrap_or(u64::MAX) > maximum_bytes {
            return Err(StorageError::new(StorageErrorCode::InventoryLimit));
        }
        let after = file_metadata(&file)?;
        validate_regular_file_metadata(&after, self.root.boundary)?;
        if before.st_dev != after.st_dev
            || before.st_ino != after.st_ino
            || before.st_size != after.st_size
            || usize::try_from(after.st_size).ok() != Some(contents.len())
        {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        self.assert_binding()?;
        Ok(contents)
    }

    /// Builds a bounded, deterministic inventory without following links or crossing devices.
    ///
    /// # Errors
    ///
    /// Returns an error when an entry is unsafe, an inventory bound is exceeded, a binding
    /// changes, or an I/O operation fails.
    pub fn inventory(&self) -> StorageResult<Inventory> {
        let _operation = self.lock_operation()?;
        self.assert_binding()?;
        let mut state = InventoryState::default();
        inventory_directory(&self.directory, self.root.boundary, "", 0, &mut state)?;
        state
            .entries
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let sha256 = inventory_digest(&state.entries);
        self.assert_binding()?;
        Ok(Inventory {
            count: state.entries.len(),
            bytes: state.bytes,
            entries: state.entries,
            sha256,
        })
    }

    /// Recursively removes one safe entry without following links or crossing devices.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid name, unsafe metadata, excessive depth, a changed binding,
    /// or an I/O failure.
    pub fn remove_entry(&self, name: &str) -> StorageResult<()> {
        let _operation = self.lock_operation()?;
        self.assert_binding()?;
        validate_entry_name(name)?;
        let encoded = c_entry_name(name)?;
        remove_entry_at(&self.directory, &encoded, self.root.boundary, 0)?;
        self.assert_binding()
    }

    fn open_child_directory_internal(&self, name: &str, create: bool) -> StorageResult<Self> {
        self.assert_binding()?;
        validate_entry_name(name)?;
        let encoded = c_entry_name(name)?;
        if create {
            ensure_directory_at(&self.directory, &encoded)?;
        }
        let directory = open_directory_at(self.directory.as_raw_fd(), &encoded)?;
        let metadata = file_metadata(&directory)?;
        validate_directory_handle(&directory, self.root.boundary)?;
        let mut components = self.components.clone();
        components.push(name.to_owned());
        let child = Self {
            root: Arc::clone(&self.root),
            directory,
            components,
            boundary: self.root.boundary,
            inode: metadata.st_ino,
        };
        child.assert_binding()?;
        Ok(child)
    }

    fn assert_binding(&self) -> StorageResult<()> {
        self.root.assert_binding()?;
        let current = self.root.open_components(&self.components, false)?;
        let metadata = file_metadata(&current)?;
        validate_directory_handle(&current, self.root.boundary)?;
        if metadata.st_dev != self.boundary.device || metadata.st_ino != self.inode {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        Ok(())
    }

    fn lock_operation(&self) -> StorageResult<MutexGuard<'_, ()>> {
        self.root
            .operation_lock
            .lock()
            .map_err(|_| StorageError::new(StorageErrorCode::Io))
    }
}

impl DeferredRunnerStorageDirectory {
    pub(crate) fn open(&self) -> StorageResult<RunnerStorageDirectory> {
        RunnerStorageDirectory::reopen_deferred(self)
    }
}

impl RootState {
    fn assert_binding(&self) -> StorageResult<()> {
        let current = open_absolute_existing(&self.configured_path)
            .map_err(|_| StorageError::new(StorageErrorCode::RootChanged))?;
        let metadata = file_metadata(&current)
            .map_err(|_| StorageError::new(StorageErrorCode::RootChanged))?;
        if metadata.st_dev != self.boundary.device
            || metadata.st_ino != self.inode
            || !is_directory(&metadata)
            || mount_identity(&current)
                .map_err(|_| StorageError::new(StorageErrorCode::RootChanged))?
                != self.boundary.mount_id
        {
            return Err(StorageError::new(StorageErrorCode::RootChanged));
        }
        validate_root_metadata(&metadata)?;
        validate_mount_identity(&current, self.boundary)?;
        validate_no_extended_acl(&current)
    }

    fn open_components(&self, components: &[String], create: bool) -> StorageResult<File> {
        let mut current = self
            .directory
            .try_clone()
            .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
        for component in components {
            let encoded = c_entry_name(component)?;
            if create {
                ensure_directory_at(&current, &encoded)?;
            }
            let next = open_directory_at(current.as_raw_fd(), &encoded)?;
            validate_directory_handle(&next, self.boundary)?;
            current = next;
        }
        Ok(current)
    }
}

#[derive(Default)]
struct InventoryState {
    entries: Vec<InventoryEntry>,
    bytes: u64,
}

fn inventory_directory(
    directory: &File,
    boundary: StorageBoundary,
    prefix: &str,
    depth: usize,
    state: &mut InventoryState,
) -> StorageResult<()> {
    if depth > MAX_INVENTORY_DEPTH {
        return Err(StorageError::new(StorageErrorCode::InventoryLimit));
    }
    for name in read_directory_names(directory)? {
        let display_name = name
            .to_str()
            .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        validate_inventory_component(display_name)
            .map_err(|_| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        let relative_path = if prefix.is_empty() {
            display_name.to_owned()
        } else {
            format!("{prefix}/{display_name}")
        };
        let encoded = c_os_component(&name)?;
        let metadata = metadata_at(directory.as_raw_fd(), &encoded)?
            .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        if is_directory(&metadata) {
            validate_directory_metadata(&metadata, boundary)?;
            let child = open_directory_at(directory.as_raw_fd(), &encoded)?;
            validate_directory_handle(&child, boundary)?;
            let opened = file_metadata(&child)?;
            if opened.st_dev != metadata.st_dev || opened.st_ino != metadata.st_ino {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            add_inventory_entry(
                state,
                InventoryEntry {
                    relative_path: relative_path.clone(),
                    kind: InventoryEntryKind::Directory,
                    device_id: device_id(boundary),
                    link_count: metadata_link_count(&metadata),
                    size_bytes: 0,
                    sha256: directory_digest(),
                },
            )?;
            inventory_directory(&child, boundary, &relative_path, depth + 1, state)?;
            continue;
        }
        validate_regular_file_metadata(&metadata, boundary)?;
        if metadata.st_size < 0 || metadata.st_size.cast_unsigned() > MAX_FILE_BYTES {
            return Err(StorageError::new(StorageErrorCode::InventoryLimit));
        }
        let size = metadata.st_size.cast_unsigned();
        state.bytes = state
            .bytes
            .checked_add(size)
            .filter(|bytes| *bytes <= MAX_INVENTORY_BYTES)
            .ok_or_else(|| StorageError::new(StorageErrorCode::InventoryLimit))?;
        let sha256 = hash_file_at(directory.as_raw_fd(), &encoded, boundary, &metadata)?;
        add_inventory_entry(
            state,
            InventoryEntry {
                relative_path,
                kind: InventoryEntryKind::File,
                device_id: device_id(boundary),
                link_count: metadata_link_count(&metadata),
                size_bytes: size,
                sha256,
            },
        )?;
    }
    Ok(())
}

fn add_inventory_entry(state: &mut InventoryState, entry: InventoryEntry) -> StorageResult<()> {
    if state.entries.len() >= MAX_INVENTORY_ENTRIES {
        return Err(StorageError::new(StorageErrorCode::InventoryLimit));
    }
    state.entries.push(entry);
    Ok(())
}

fn hash_file_at(
    parent: RawFd,
    name: &CStr,
    boundary: StorageBoundary,
    expected: &libc::stat,
) -> StorageResult<String> {
    let mut file = open_regular_file_at(parent, name, boundary)?;
    let opened = file_metadata(&file)?;
    if opened.st_dev != expected.st_dev
        || opened.st_ino != expected.st_ino
        || opened.st_size != expected.st_size
    {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| StorageError::from_io(StorageErrorCode::Io, error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let after = file_metadata(&file)?;
    validate_regular_file_metadata(&after, boundary)?;
    if opened.st_dev != after.st_dev
        || opened.st_ino != after.st_ino
        || opened.st_size != after.st_size
    {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn inventory_digest(entries: &[InventoryEntry]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-runner-native-inventory-v1\0");
    for entry in entries {
        digest.update(entry.relative_path.len().to_string().as_bytes());
        digest.update(b":");
        digest.update(entry.relative_path.as_bytes());
        digest.update(b"\n");
        digest.update(entry.kind.as_str().as_bytes());
        digest.update(b"\n");
        digest.update(entry.size_bytes.to_string().as_bytes());
        digest.update(b"\n");
        digest.update(entry.sha256.as_bytes());
        digest.update(b"\n");
    }
    format!("{:x}", digest.finalize())
}

fn directory_digest() -> String {
    format!(
        "{:x}",
        Sha256::digest(b"bluey-jobs-runner-native-inventory-directory-v1")
    )
}

fn device_id(boundary: StorageBoundary) -> String {
    format!("unix:{:x}:mount:{:x}", boundary.device, boundary.mount_id)
}

// `nlink_t` is narrower on Apple targets but already `u64` on Linux x86-64.
#[allow(clippy::useless_conversion)]
fn metadata_link_count(metadata: &libc::stat) -> u64 {
    u64::from(metadata.st_nlink)
}

fn remove_entry_at(
    parent: &File,
    name: &CStr,
    boundary: StorageBoundary,
    depth: usize,
) -> StorageResult<()> {
    if depth > MAX_INVENTORY_DEPTH {
        return Err(StorageError::new(StorageErrorCode::InventoryLimit));
    }
    let Some(metadata) = metadata_at(parent.as_raw_fd(), name)? else {
        return Ok(());
    };
    if is_regular_file(&metadata) {
        validate_regular_file_metadata(&metadata, boundary)?;
        let opened = open_regular_file_at(parent.as_raw_fd(), name, boundary)?;
        let opened_metadata = file_metadata(&opened)?;
        if opened_metadata.st_dev != metadata.st_dev || opened_metadata.st_ino != metadata.st_ino {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        let current = metadata_at(parent.as_raw_fd(), name)?
            .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        if current.st_dev != metadata.st_dev || current.st_ino != metadata.st_ino {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        unlink_at(parent.as_raw_fd(), name, 0)?;
        return sync_directory(parent);
    }
    if !is_directory(&metadata) {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    validate_directory_metadata(&metadata, boundary)?;
    let directory = open_directory_at(parent.as_raw_fd(), name)?;
    validate_directory_handle(&directory, boundary)?;
    let opened = file_metadata(&directory)?;
    if opened.st_dev != metadata.st_dev || opened.st_ino != metadata.st_ino {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    for child in read_directory_names(&directory)? {
        let encoded = c_os_component(&child)?;
        remove_entry_at(&directory, &encoded, boundary, depth + 1)?;
    }
    let current = metadata_at(parent.as_raw_fd(), name)?
        .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
    if current.st_dev != metadata.st_dev || current.st_ino != metadata.st_ino {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    unlink_at(parent.as_raw_fd(), name, libc::AT_REMOVEDIR)?;
    sync_directory(parent)
}

fn move_entry_noreplace_at(
    source_parent: &File,
    source_name: &CStr,
    destination_parent: &File,
    destination_name: &CStr,
    boundary: StorageBoundary,
) -> StorageResult<MoveOutcome> {
    validate_directory_handle(source_parent, boundary)?;
    validate_directory_handle(destination_parent, boundary)?;
    let Some(source_metadata) = metadata_at(source_parent.as_raw_fd(), source_name)? else {
        if let Some(destination_metadata) =
            metadata_at(destination_parent.as_raw_fd(), destination_name)?
        {
            validate_move_entry(
                destination_parent,
                destination_name,
                boundary,
                &destination_metadata,
            )?;
        }
        return Ok(MoveOutcome::SourceMissing);
    };
    let source_handle =
        validate_move_entry(source_parent, source_name, boundary, &source_metadata)?;
    if let Some(destination_metadata) =
        metadata_at(destination_parent.as_raw_fd(), destination_name)?
    {
        validate_move_entry(
            destination_parent,
            destination_name,
            boundary,
            &destination_metadata,
        )?;
        return Ok(MoveOutcome::DestinationExists);
    }

    if !rename_noreplace_between(
        source_parent,
        source_name,
        destination_parent,
        destination_name,
    )? {
        let current_source = metadata_at(source_parent.as_raw_fd(), source_name)?
            .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        if current_source.st_dev != source_metadata.st_dev
            || current_source.st_ino != source_metadata.st_ino
        {
            return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
        }
        let destination_metadata =
            metadata_at(destination_parent.as_raw_fd(), destination_name)?
                .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        validate_move_entry(
            destination_parent,
            destination_name,
            boundary,
            &destination_metadata,
        )?;
        return Ok(MoveOutcome::DestinationExists);
    }

    if metadata_at(source_parent.as_raw_fd(), source_name)?.is_some() {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    let destination_metadata = metadata_at(destination_parent.as_raw_fd(), destination_name)?
        .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
    let retained_metadata = file_metadata(&source_handle)?;
    if destination_metadata.st_dev != source_metadata.st_dev
        || destination_metadata.st_ino != source_metadata.st_ino
        || retained_metadata.st_dev != source_metadata.st_dev
        || retained_metadata.st_ino != source_metadata.st_ino
    {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    validate_move_entry(
        destination_parent,
        destination_name,
        boundary,
        &destination_metadata,
    )?;
    sync_directory(source_parent)?;
    let source_parent_metadata = file_metadata(source_parent)?;
    let destination_parent_metadata = file_metadata(destination_parent)?;
    if source_parent_metadata.st_dev != destination_parent_metadata.st_dev
        || source_parent_metadata.st_ino != destination_parent_metadata.st_ino
    {
        sync_directory(destination_parent)?;
    }
    Ok(MoveOutcome::Moved)
}

fn validate_move_entry(
    parent: &File,
    name: &CStr,
    boundary: StorageBoundary,
    expected: &libc::stat,
) -> StorageResult<File> {
    let opened = if is_regular_file(expected) {
        open_regular_file_at(parent.as_raw_fd(), name, boundary)?
    } else if is_directory(expected) {
        validate_directory_metadata(expected, boundary)?;
        let directory = open_directory_at(parent.as_raw_fd(), name)?;
        validate_directory_handle(&directory, boundary)?;
        let mut inventory = InventoryState::default();
        inventory_directory(&directory, boundary, "", 0, &mut inventory)?;
        directory
    } else {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    };
    let metadata = file_metadata(&opened)?;
    if metadata.st_dev != expected.st_dev || metadata.st_ino != expected.st_ino {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    Ok(opened)
}

fn open_or_create_absolute_root(configured_root: &Path) -> StorageResult<(PathBuf, File)> {
    let components = absolute_components(configured_root)?;
    let (name, parents) = components
        .split_last()
        .ok_or_else(|| StorageError::new(StorageErrorCode::Configuration))?;
    let mut parent = open_filesystem_root()?;
    for component in parents {
        parent = open_directory_at(parent.as_raw_fd(), &c_os_component(component)?)?;
    }
    let encoded = c_os_component(name)?;
    let directory = match open_directory_at(parent.as_raw_fd(), &encoded) {
        Ok(directory) => directory,
        Err(error)
            if error.source.as_ref().and_then(std::io::Error::raw_os_error)
                == Some(libc::ENOENT) =>
        {
            let result = unsafe {
                libc::mkdirat(parent.as_raw_fd(), encoded.as_ptr(), PRIVATE_DIRECTORY_MODE)
            };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::EEXIST) {
                    return Err(StorageError::from_io(StorageErrorCode::Io, error));
                }
            }
            sync_directory(&parent)?;
            open_directory_at(parent.as_raw_fd(), &encoded)?
        }
        Err(error) => return Err(error),
    };
    Ok((configured_root.to_path_buf(), directory))
}

fn open_absolute_existing(path: &Path) -> StorageResult<File> {
    let components = absolute_components(path)?;
    let mut current = open_filesystem_root()?;
    for component in components {
        current = open_directory_at(current.as_raw_fd(), &c_os_component(&component)?)?;
    }
    Ok(current)
}

fn absolute_components(path: &Path) -> StorageResult<Vec<OsString>> {
    if !path.is_absolute() {
        return Err(StorageError::new(StorageErrorCode::Configuration));
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(value) => components.push(value.to_os_string()),
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                return Err(StorageError::new(StorageErrorCode::Configuration));
            }
        }
    }
    if components.is_empty() {
        return Err(StorageError::new(StorageErrorCode::Configuration));
    }
    Ok(components)
}

fn open_filesystem_root() -> StorageResult<File> {
    let root = CString::new("/").expect("static root path contains no NUL");
    let descriptor = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn open_root_lock(root: &File, boundary: StorageBoundary) -> StorageResult<File> {
    let name = CString::new(ROOT_LOCK_FILE).expect("static lock name contains no NUL");
    let descriptor = unsafe {
        libc::openat(
            root.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            libc::c_uint::from(PRIVATE_FILE_MODE),
        )
    };
    if descriptor < 0 {
        return Err(last_io(StorageErrorCode::UnsafeEntry));
    }
    let lock = unsafe { File::from_raw_fd(descriptor) };
    let metadata = file_metadata(&lock)?;
    validate_regular_file_metadata(&metadata, boundary)?;
    validate_mount_identity(&lock, boundary)?;
    validate_no_extended_acl(&lock)?;
    let result = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error
            .raw_os_error()
            .is_some_and(|code| [libc::EWOULDBLOCK, libc::EAGAIN].contains(&code))
        {
            return Err(StorageError::from_io(StorageErrorCode::RootLocked, error));
        }
        return Err(StorageError::from_io(StorageErrorCode::Io, error));
    }
    sync_directory(root)?;
    Ok(lock)
}

fn create_staging_file(parent: &File) -> StorageResult<(CString, File)> {
    for _ in 0..128 {
        let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let staging_name = format!(".bluey-stage-{}-{sequence:016x}", std::process::id());
        let staging = CString::new(staging_name)
            .map_err(|_| StorageError::new(StorageErrorCode::PathEscape))?;
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                staging.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                libc::c_uint::from(PRIVATE_FILE_MODE),
            )
        };
        if descriptor >= 0 {
            return Ok((staging, unsafe { File::from_raw_fd(descriptor) }));
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EEXIST) {
            return Err(StorageError::from_io(StorageErrorCode::Io, error));
        }
    }
    Err(StorageError::from_io(
        StorageErrorCode::Io,
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "native storage staging namespace exhausted",
        ),
    ))
}

fn cleanup_staging_files(
    directory: &File,
    boundary: StorageBoundary,
    depth: usize,
) -> StorageResult<()> {
    if depth > MAX_INVENTORY_DEPTH {
        return Err(StorageError::new(StorageErrorCode::InventoryLimit));
    }
    let mut removed = false;
    for name in read_directory_names(directory)? {
        let encoded = c_os_component(&name)?;
        let metadata = metadata_at(directory.as_raw_fd(), &encoded)?
            .ok_or_else(|| StorageError::new(StorageErrorCode::UnsafeEntry))?;
        if is_directory(&metadata) {
            validate_directory_metadata(&metadata, boundary)?;
            let child = open_directory_at(directory.as_raw_fd(), &encoded)?;
            validate_directory_handle(&child, boundary)?;
            let opened = file_metadata(&child)?;
            if opened.st_dev != metadata.st_dev || opened.st_ino != metadata.st_ino {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            cleanup_staging_files(&child, boundary, depth + 1)?;
        } else if is_staging_name(&name) {
            validate_regular_file_metadata(&metadata, boundary)?;
            let staged = open_regular_file_at(directory.as_raw_fd(), &encoded, boundary)?;
            let opened = file_metadata(&staged)?;
            if opened.st_dev != metadata.st_dev || opened.st_ino != metadata.st_ino {
                return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
            }
            unlink_at(directory.as_raw_fd(), &encoded, 0)?;
            removed = true;
        }
    }
    if removed {
        sync_directory(directory)?;
    }
    Ok(())
}

fn is_staging_name(name: &OsStr) -> bool {
    let Some(value) = name.to_str() else {
        return false;
    };
    let Some(suffix) = value.strip_prefix(".bluey-stage-") else {
        return false;
    };
    let Some((process_id, sequence)) = suffix.split_once('-') else {
        return false;
    };
    !process_id.is_empty()
        && process_id.bytes().all(|value| value.is_ascii_digit())
        && sequence.len() == 16
        && sequence.bytes().all(|value| value.is_ascii_hexdigit())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_noreplace_at(parent: &File, source: &CStr, destination: &CStr) -> StorageResult<bool> {
    rename_noreplace_between(parent, source, parent, destination)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn rename_noreplace_at(parent: &File, source: &CStr, destination: &CStr) -> StorageResult<bool> {
    rename_noreplace_between(parent, source, parent, destination)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_noreplace_between(
    source_parent: &File,
    source: &CStr,
    destination_parent: &File,
    destination: &CStr,
) -> StorageResult<bool> {
    let result = unsafe {
        libc::renameat2(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            destination_parent.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    rename_noreplace_result(result)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn rename_noreplace_between(
    source_parent: &File,
    source: &CStr,
    destination_parent: &File,
    destination: &CStr,
) -> StorageResult<bool> {
    let result = unsafe {
        libc::renameatx_np(
            source_parent.as_raw_fd(),
            source.as_ptr(),
            destination_parent.as_raw_fd(),
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    rename_noreplace_result(result)
}

fn rename_noreplace_result(result: libc::c_int) -> StorageResult<bool> {
    if result == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EEXIST) {
        return Ok(false);
    }
    if error.raw_os_error() == Some(libc::EXDEV) {
        return Err(StorageError::from_io(StorageErrorCode::UnsafeEntry, error));
    }
    Err(StorageError::from_io(StorageErrorCode::Io, error))
}

fn ensure_directory_at(parent: &File, name: &CStr) -> StorageResult<()> {
    let result =
        unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), PRIVATE_DIRECTORY_MODE) };
    if result == 0 {
        return sync_directory(parent);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EEXIST) {
        return Ok(());
    }
    Err(StorageError::from_io(StorageErrorCode::Io, error))
}

fn open_directory_at(parent: RawFd, name: &CStr) -> StorageResult<File> {
    let descriptor = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        let code = if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
            StorageErrorCode::UnsafeEntry
        } else {
            StorageErrorCode::Io
        };
        return Err(StorageError::from_io(code, error));
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn open_regular_file_at(
    parent: RawFd,
    name: &CStr,
    boundary: StorageBoundary,
) -> StorageResult<File> {
    let descriptor = unsafe {
        libc::openat(
            parent,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        let code = if error.raw_os_error() == Some(libc::ELOOP) {
            StorageErrorCode::UnsafeEntry
        } else {
            StorageErrorCode::Io
        };
        return Err(StorageError::from_io(code, error));
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    validate_regular_file_metadata(&file_metadata(&file)?, boundary)?;
    validate_mount_identity(&file, boundary)?;
    validate_no_extended_acl(&file)?;
    Ok(file)
}

fn file_metadata(file: &File) -> StorageResult<libc::stat> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe { libc::fstat(file.as_raw_fd(), metadata.as_mut_ptr()) };
    if result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    Ok(unsafe { metadata.assume_init() })
}

fn metadata_at(parent: RawFd, name: &CStr) -> StorageResult<Option<libc::stat>> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            parent,
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        return Ok(Some(unsafe { metadata.assume_init() }));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ENOENT) {
        return Ok(None);
    }
    Err(StorageError::from_io(StorageErrorCode::Io, error))
}

fn validate_root_metadata(metadata: &libc::stat) -> StorageResult<()> {
    if !is_directory(metadata) {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    validate_owner_private(metadata, PRIVATE_DIRECTORY_MODE)
}

fn validate_directory_handle(file: &File, boundary: StorageBoundary) -> StorageResult<()> {
    validate_directory_metadata(&file_metadata(file)?, boundary)?;
    validate_mount_identity(file, boundary)?;
    validate_no_extended_acl(file)
}

fn validate_mount_identity(file: &File, boundary: StorageBoundary) -> StorageResult<()> {
    if mount_identity(file)? != boundary.mount_id {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn mount_identity(file: &File) -> StorageResult<u64> {
    let empty = CString::new("").expect("static empty path contains no NUL");
    let mut attributes = std::mem::MaybeUninit::<libc::statx>::zeroed();
    let result = unsafe {
        libc::statx(
            file.as_raw_fd(),
            empty.as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_STATX_SYNC_AS_STAT,
            libc::STATX_MNT_ID,
            attributes.as_mut_ptr(),
        )
    };
    if result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    let attributes = unsafe { attributes.assume_init() };
    if attributes.stx_mask & libc::STATX_MNT_ID == 0 {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    Ok(attributes.stx_mnt_id)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn mount_identity(file: &File) -> StorageResult<u64> {
    let mut attributes = std::mem::MaybeUninit::<libc::statfs>::zeroed();
    let result = unsafe { libc::fstatfs(file.as_raw_fd(), attributes.as_mut_ptr()) };
    if result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    let attributes = unsafe { attributes.assume_init() };
    let mounted_at = unsafe { CStr::from_ptr(attributes.f_mntonname.as_ptr()) }.to_bytes();
    let mounted_from = unsafe { CStr::from_ptr(attributes.f_mntfromname.as_ptr()) }.to_bytes();
    let digest = Sha256::new()
        .chain_update(b"bluey-jobs-runner-mount-identity-v1\0")
        .chain_update(mounted_from)
        .chain_update(b"\0")
        .chain_update(mounted_at)
        .finalize();
    Ok(u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix has exactly eight bytes"),
    ))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
// Keep the fallible contract shared with the Darwin ACL implementation.
#[allow(clippy::unnecessary_wraps)]
fn validate_no_extended_acl(_file: &File) -> StorageResult<()> {
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn validate_no_extended_acl(file: &File) -> StorageResult<()> {
    type Acl = *mut libc::c_void;
    unsafe extern "C" {
        fn acl_get_fd_np(file_descriptor: libc::c_int, acl_type: libc::c_int) -> Acl;
        fn acl_get_entry(
            acl: Acl,
            entry_id: libc::c_int,
            entry: *mut *mut libc::c_void,
        ) -> libc::c_int;
        fn acl_free(value: *mut libc::c_void) -> libc::c_int;
    }
    const ACL_TYPE_EXTENDED: libc::c_int = 0x0000_0100;
    const ACL_FIRST_ENTRY: libc::c_int = 0;

    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(());
        }
        return Err(StorageError::from_io(StorageErrorCode::Io, error));
    }
    let mut entry = std::ptr::null_mut();
    let result = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &raw mut entry) };
    let saved_error = std::io::Error::last_os_error();
    let free_result = unsafe { acl_free(acl) };
    if free_result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    if result == 0 {
        return Err(StorageError::new(StorageErrorCode::UnsafePermissions));
    }
    if saved_error.raw_os_error() == Some(libc::ENOENT) {
        return Ok(());
    }
    Err(StorageError::from_io(StorageErrorCode::Io, saved_error))
}

fn validate_directory_metadata(
    metadata: &libc::stat,
    boundary: StorageBoundary,
) -> StorageResult<()> {
    if !is_directory(metadata) || metadata.st_dev != boundary.device {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    validate_owner_private(metadata, PRIVATE_DIRECTORY_MODE)
}

fn validate_regular_file_metadata(
    metadata: &libc::stat,
    boundary: StorageBoundary,
) -> StorageResult<()> {
    if !is_regular_file(metadata) || metadata.st_dev != boundary.device || metadata.st_nlink != 1 {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    validate_owner_private(metadata, PRIVATE_FILE_MODE)
}

fn validate_owner_private(metadata: &libc::stat, expected_mode: libc::mode_t) -> StorageResult<()> {
    if metadata.st_uid != unsafe { libc::geteuid() } || metadata.st_mode & 0o777 != expected_mode {
        return Err(StorageError::new(StorageErrorCode::UnsafePermissions));
    }
    Ok(())
}

fn is_directory(metadata: &libc::stat) -> bool {
    metadata.st_mode & libc::S_IFMT == libc::S_IFDIR
}

fn is_regular_file(metadata: &libc::stat) -> bool {
    metadata.st_mode & libc::S_IFMT == libc::S_IFREG
}

fn read_directory_names(directory: &File) -> StorageResult<Vec<OsString>> {
    let current = CString::new(".").expect("static current-directory name contains no NUL");
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            current.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if descriptor < 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    let reopened = unsafe { File::from_raw_fd(descriptor) };
    let original_metadata = file_metadata(directory)?;
    let reopened_metadata = file_metadata(&reopened)?;
    if original_metadata.st_dev != reopened_metadata.st_dev
        || original_metadata.st_ino != reopened_metadata.st_ino
    {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    let stream = unsafe { libc::fdopendir(reopened.into_raw_fd()) };
    if stream.is_null() {
        let error = std::io::Error::last_os_error();
        return Err(StorageError::from_io(StorageErrorCode::Io, error));
    }
    let guard = DirectoryStream(stream);
    let mut names = Vec::new();
    loop {
        set_errno_zero();
        let entry = unsafe { libc::readdir(guard.0) };
        if entry.is_null() {
            if current_errno() != 0 {
                return Err(last_io(StorageErrorCode::Io));
            }
            break;
        }
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        names.push(OsString::from_vec(name.to_vec()));
    }
    names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    Ok(names)
}

struct DirectoryStream(*mut libc::DIR);

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

fn sync_directory(directory: &File) -> StorageResult<()> {
    let result = unsafe { libc::fsync(directory.as_raw_fd()) };
    if result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    Ok(())
}

fn unlink_at(parent: RawFd, name: &CStr, flags: libc::c_int) -> StorageResult<()> {
    let result = unsafe { libc::unlinkat(parent, name.as_ptr(), flags) };
    if result != 0 {
        return Err(last_io(StorageErrorCode::Io));
    }
    Ok(())
}

fn validate_component(component: &str) -> StorageResult<()> {
    if component.is_empty()
        || component.len() > 160
        || component == "."
        || component == ".."
        || component
            .bytes()
            .any(|value| !(value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-' | b'.')))
        || component.starts_with('.')
    {
        return Err(StorageError::new(StorageErrorCode::PathEscape));
    }
    Ok(())
}

fn validate_inventory_component(component: &str) -> StorageResult<()> {
    if component.is_empty()
        || component.len() > 255
        || component == "."
        || component == ".."
        || component.bytes().any(|value| {
            value == b'/' || value == b'\\' || value == 0 || value < 0x20 || value == 0x7f
        })
    {
        return Err(StorageError::new(StorageErrorCode::UnsafeEntry));
    }
    Ok(())
}

fn validate_entry_name(component: &str) -> StorageResult<()> {
    validate_inventory_component(component)
        .map_err(|_| StorageError::new(StorageErrorCode::PathEscape))?;
    if component == ROOT_LOCK_FILE || is_staging_name(OsStr::new(component)) {
        return Err(StorageError::new(StorageErrorCode::PathEscape));
    }
    Ok(())
}

fn c_entry_name(component: &str) -> StorageResult<CString> {
    validate_entry_name(component)?;
    CString::new(component).map_err(|_| StorageError::new(StorageErrorCode::PathEscape))
}

fn c_os_component(component: &OsStr) -> StorageResult<CString> {
    let bytes = component.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(StorageError::new(StorageErrorCode::PathEscape));
    }
    CString::new(bytes).map_err(|_| StorageError::new(StorageErrorCode::PathEscape))
}

fn last_io(code: StorageErrorCode) -> StorageError {
    StorageError::from_io(code, std::io::Error::last_os_error())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn set_errno_zero() {
    unsafe {
        *libc::__errno_location() = 0;
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn set_errno_zero() {
    unsafe {
        *libc::__error() = 0;
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn current_errno() -> libc::c_int {
    unsafe { *libc::__errno_location() }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn current_errno() -> libc::c_int {
    unsafe { *libc::__error() }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios"
)))]
compile_error!("Bluey runner native storage needs an audited errno implementation on this Unix");

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, DirBuilder, OpenOptions};
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

    #[test]
    // `mode_t` is narrower on Apple targets but already `u32` on Linux.
    #[allow(clippy::useless_conversion)]
    fn same_device_mount_mismatch_blocks_inventory_and_recursive_removal() {
        let path = std::env::temp_dir().join(format!(
            "bluey-native-mount-boundary-{}-{}",
            std::process::id(),
            STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        DirBuilder::new()
            .mode(PRIVATE_DIRECTORY_MODE.into())
            .create(&path)
            .expect("create private test directory");
        let file_path = path.join("private.bin");
        let mut private_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(PRIVATE_FILE_MODE.into())
            .open(&file_path)
            .expect("create private test file");
        private_file.write_all(b"sentinel").expect("write sentinel");
        drop(private_file);

        let directory = File::open(&path).expect("open test directory");
        let metadata = file_metadata(&directory).expect("read directory metadata");
        let actual_mount = mount_identity(&directory).expect("read mount identity");
        let boundary = StorageBoundary {
            device: metadata.st_dev,
            mount_id: actual_mount.wrapping_add(1),
        };
        assert_eq!(
            validate_directory_handle(&directory, boundary)
                .expect_err("reject mismatched mount identity")
                .code(),
            StorageErrorCode::UnsafeEntry,
        );

        let mut inventory = InventoryState::default();
        assert_eq!(
            inventory_directory(&directory, boundary, "", 0, &mut inventory)
                .expect_err("inventory must not cross a same-device mount boundary")
                .code(),
            StorageErrorCode::UnsafeEntry,
        );
        let name = c_entry_name("private.bin").expect("encode test file name");
        assert_eq!(
            remove_entry_at(&directory, &name, boundary, 0)
                .expect_err("removal must not cross a same-device mount boundary")
                .code(),
            StorageErrorCode::UnsafeEntry,
        );
        assert_eq!(
            fs::read(&file_path).expect("preserve sentinel"),
            b"sentinel"
        );

        drop(directory);
        fs::remove_dir_all(&path).expect("remove test directory");
    }
}
