use crate::{Inventory, MoveOutcome, StorageError, StorageErrorCode, StorageResult};
use std::path::{Path, PathBuf};

/// Windows intentionally fails closed until directory-handle-relative reparse-point protection,
/// file-ID binding, owner-only ACL validation, and `LockFileEx` have been implemented and tested.
pub struct RunnerStorageRoot;

pub struct RunnerStorageDirectory;

#[derive(Clone)]
pub(crate) struct DeferredRunnerStorageDirectory;

impl RunnerStorageRoot {
    /// Opens no storage on Windows.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn open(_configured_root: &Path) -> StorageResult<Self> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    #[must_use]
    pub fn configured_path(&self) -> &Path {
        Path::new("")
    }

    /// Returns no Windows root device identity.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn device_id(&self) -> StorageResult<String> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Returns no Windows root link count.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn link_count(&self) -> StorageResult<u64> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Performs no Windows root validation.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn assert_unchanged(&self) -> StorageResult<()> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Creates no Windows directory.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn ensure_directory(
        &self,
        _components: &[String],
    ) -> StorageResult<RunnerStorageDirectory> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Opens no Windows directory.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn open_directory(&self, _components: &[String]) -> StorageResult<RunnerStorageDirectory> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Moves no Windows entry.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn move_entry_noreplace(
        &self,
        _source_components: &[String],
        _destination_components: &[String],
    ) -> StorageResult<MoveOutcome> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }
}

impl RunnerStorageDirectory {
    #[allow(clippy::unused_self)]
    pub(crate) fn deferred_capability(&self) -> DeferredRunnerStorageDirectory {
        DeferredRunnerStorageDirectory
    }

    #[must_use]
    pub fn relative_path(&self) -> String {
        String::new()
    }

    /// Returns no Windows directory device identity.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn device_id(&self) -> StorageResult<String> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Returns no Windows directory link count.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn link_count(&self) -> StorageResult<u64> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Resolves no Windows path.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn canonical_path(&self) -> StorageResult<PathBuf> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Creates no Windows child directory.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn ensure_child_directory(&self, _name: &str) -> StorageResult<Self> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Opens no Windows child directory.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn open_child_directory(&self, _name: &str) -> StorageResult<Self> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Writes no Windows file.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn write_file_exclusive(&self, _name: &str, _contents: &[u8]) -> StorageResult<bool> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Replaces no Windows file.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn replace_file(&self, _name: &str, _contents: &[u8]) -> StorageResult<()> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Reads no Windows file.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn read_file_bounded(&self, _name: &str, _maximum_bytes: u64) -> StorageResult<Vec<u8>> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Inventories no Windows directory.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn inventory(&self) -> StorageResult<Inventory> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }

    /// Removes no Windows entry.
    ///
    /// # Errors
    ///
    /// Always returns [`StorageErrorCode::UnsupportedPlatform`].
    pub fn remove_entry(&self, _name: &str) -> StorageResult<()> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }
}

impl DeferredRunnerStorageDirectory {
    pub(crate) fn open(&self) -> StorageResult<RunnerStorageDirectory> {
        Err(StorageError::new(StorageErrorCode::UnsupportedPlatform))
    }
}
