//! Native, capability-style storage primitives for persistent Bluey Jobs runner roots.
//!
//! The JavaScript runner must not turn these primitives back into pathname checks. A root
//! remains open and exclusively locked for the lifetime of this object. Child operations are
//! relative to retained directory handles, reject links and special entries, and require every
//! object to remain on the root filesystem.

use napi::bindgen_prelude::{AsyncTask, Buffer};
use napi::{Env, Task};
use napi_derive::napi;
use std::fmt;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::DeferredRunnerStorageDirectory;
#[cfg(unix)]
pub use unix::{RunnerStorageDirectory, RunnerStorageRoot};

#[cfg(not(unix))]
mod windows;
#[cfg(not(unix))]
use windows::DeferredRunnerStorageDirectory;
#[cfg(not(unix))]
pub use windows::{RunnerStorageDirectory, RunnerStorageRoot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageErrorCode {
    Configuration,
    InventoryLimit,
    Io,
    PathEscape,
    RootChanged,
    RootLocked,
    UnsafeEntry,
    UnsafePermissions,
    UnsupportedPlatform,
}

impl StorageErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::InventoryLimit => "inventory_limit",
            Self::Io => "io_failure",
            Self::PathEscape => "path_escape",
            Self::RootChanged => "root_changed",
            Self::RootLocked => "root_locked",
            Self::UnsafeEntry => "unsafe_entry",
            Self::UnsafePermissions => "unsafe_permissions",
            Self::UnsupportedPlatform => "unsupported_platform",
        }
    }
}

#[derive(Debug)]
pub struct StorageError {
    code: StorageErrorCode,
    source: Option<std::io::Error>,
}

impl StorageError {
    #[must_use]
    pub const fn new(code: StorageErrorCode) -> Self {
        Self { code, source: None }
    }

    #[must_use]
    pub fn from_io(code: StorageErrorCode, source: std::io::Error) -> Self {
        Self {
            code,
            source: Some(source),
        }
    }

    #[must_use]
    pub const fn code(&self) -> StorageErrorCode {
        self.code
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveOutcome {
    DestinationExists,
    Moved,
    SourceMissing,
}

impl MoveOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DestinationExists => "destination_exists",
            Self::Moved => "moved",
            Self::SourceMissing => "source_missing",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InventoryEntryKind {
    Directory,
    File,
}

impl InventoryEntryKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryEntry {
    pub relative_path: String,
    pub kind: InventoryEntryKind,
    pub device_id: String,
    pub link_count: u64,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inventory {
    pub entries: Vec<InventoryEntry>,
    pub count: usize,
    pub bytes: u64,
    pub sha256: String,
}

#[napi(object)]
pub struct NativeInventoryEntry {
    pub relative_path: String,
    pub kind: String,
    pub device_id: String,
    pub link_count: String,
    pub size_bytes: String,
    pub sha256: String,
}

#[napi(object)]
pub struct NativeInventory {
    pub entries: Vec<NativeInventoryEntry>,
    pub count: u32,
    pub bytes: String,
    pub sha256: String,
}

impl TryFrom<Inventory> for NativeInventory {
    type Error = StorageError;

    fn try_from(inventory: Inventory) -> StorageResult<Self> {
        Ok(Self {
            entries: inventory
                .entries
                .into_iter()
                .map(|entry| NativeInventoryEntry {
                    relative_path: entry.relative_path,
                    kind: entry.kind.as_str().to_owned(),
                    device_id: entry.device_id,
                    link_count: entry.link_count.to_string(),
                    size_bytes: entry.size_bytes.to_string(),
                    sha256: entry.sha256,
                })
                .collect(),
            count: inventory
                .count
                .try_into()
                .map_err(|_| StorageError::new(StorageErrorCode::InventoryLimit))?,
            bytes: inventory.bytes.to_string(),
            sha256: inventory.sha256,
        })
    }
}

#[napi(js_name = "RunnerStorageRoot")]
pub struct NativeRunnerStorageRoot {
    inner: RunnerStorageRoot,
}

#[napi]
// N-API owns these public signatures and moves JavaScript values into each generated wrapper.
#[allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value
)]
impl NativeRunnerStorageRoot {
    #[napi(constructor)]
    pub fn new(configured_root: String) -> napi::Result<Self> {
        let inner = RunnerStorageRoot::open(Path::new(&configured_root)).map_err(to_napi_error)?;
        Ok(Self { inner })
    }

    #[napi(getter, js_name = "configuredPath")]
    pub fn configured_path(&self) -> String {
        self.inner.configured_path().to_string_lossy().into_owned()
    }

    #[napi(getter, js_name = "deviceId")]
    pub fn device_id(&self) -> napi::Result<String> {
        self.inner.device_id().map_err(to_napi_error)
    }

    #[napi(getter, js_name = "linkCount")]
    pub fn link_count(&self) -> napi::Result<String> {
        self.inner
            .link_count()
            .map(|value| value.to_string())
            .map_err(to_napi_error)
    }

    #[napi(js_name = "assertUnchanged")]
    pub fn assert_unchanged(&self) -> napi::Result<()> {
        self.inner.assert_unchanged().map_err(to_napi_error)
    }

    #[napi(js_name = "ensureDirectory")]
    pub fn ensure_directory(&self, components: Vec<String>) -> AsyncTask<NativeDirectoryOpenTask> {
        AsyncTask::new(NativeDirectoryOpenTask {
            source: DirectoryOpenSource::Root {
                root: self.inner.clone(),
                components,
                create: true,
            },
        })
    }

    #[napi(js_name = "openDirectory")]
    pub fn open_directory(&self, components: Vec<String>) -> AsyncTask<NativeDirectoryOpenTask> {
        AsyncTask::new(NativeDirectoryOpenTask {
            source: DirectoryOpenSource::Root {
                root: self.inner.clone(),
                components,
                create: false,
            },
        })
    }

    #[napi(js_name = "moveEntryNoReplace")]
    pub fn move_entry_noreplace(
        &self,
        source_components: Vec<String>,
        destination_components: Vec<String>,
    ) -> AsyncTask<NativeMoveEntryTask> {
        AsyncTask::new(NativeMoveEntryTask {
            root: self.inner.clone(),
            source_components,
            destination_components,
        })
    }
}

#[napi(js_name = "RunnerStorageDirectory")]
pub struct NativeRunnerStorageDirectory {
    inner: RunnerStorageDirectory,
}

#[napi]
// N-API owns these public signatures and moves JavaScript values into each generated wrapper.
#[allow(
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value
)]
impl NativeRunnerStorageDirectory {
    #[napi(getter, js_name = "relativePath")]
    pub fn relative_path(&self) -> String {
        self.inner.relative_path()
    }

    #[napi(getter, js_name = "canonicalPath")]
    pub fn canonical_path(&self) -> napi::Result<String> {
        self.inner
            .canonical_path()
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(to_napi_error)
    }

    #[napi(getter, js_name = "deviceId")]
    pub fn device_id(&self) -> napi::Result<String> {
        self.inner.device_id().map_err(to_napi_error)
    }

    #[napi(getter, js_name = "linkCount")]
    pub fn link_count(&self) -> napi::Result<String> {
        self.inner
            .link_count()
            .map(|value| value.to_string())
            .map_err(to_napi_error)
    }

    #[napi(js_name = "ensureChildDirectory")]
    pub fn ensure_child_directory(&self, name: String) -> AsyncTask<NativeDirectoryOpenTask> {
        AsyncTask::new(NativeDirectoryOpenTask {
            source: DirectoryOpenSource::Child {
                directory: self.inner.deferred_capability(),
                name,
                create: true,
            },
        })
    }

    #[napi(js_name = "openChildDirectory")]
    pub fn open_child_directory(&self, name: String) -> AsyncTask<NativeDirectoryOpenTask> {
        AsyncTask::new(NativeDirectoryOpenTask {
            source: DirectoryOpenSource::Child {
                directory: self.inner.deferred_capability(),
                name,
                create: false,
            },
        })
    }

    #[napi(js_name = "writeFileExclusive")]
    pub fn write_file_exclusive(
        &self,
        name: String,
        contents: Buffer,
    ) -> napi::Result<AsyncTask<NativeFileCreateTask>> {
        Ok(AsyncTask::new(NativeFileCreateTask {
            directory: self.inner.deferred_capability(),
            name,
            contents: contents.to_vec(),
        }))
    }

    #[napi(js_name = "replaceFile")]
    pub fn replace_file(
        &self,
        name: String,
        contents: Buffer,
    ) -> napi::Result<AsyncTask<NativeFileWriteTask>> {
        Ok(AsyncTask::new(NativeFileWriteTask {
            directory: self.inner.deferred_capability(),
            name,
            contents: contents.to_vec(),
        }))
    }

    #[napi(js_name = "readFileBounded")]
    pub fn read_file_bounded(
        &self,
        name: String,
        maximum_bytes: u32,
    ) -> napi::Result<AsyncTask<NativeFileReadTask>> {
        Ok(AsyncTask::new(NativeFileReadTask {
            directory: self.inner.deferred_capability(),
            name,
            maximum_bytes: u64::from(maximum_bytes),
        }))
    }

    #[napi]
    pub fn inventory(&self) -> napi::Result<AsyncTask<NativeInventoryTask>> {
        Ok(AsyncTask::new(NativeInventoryTask {
            directory: self.inner.deferred_capability(),
        }))
    }

    #[napi(js_name = "removeEntry")]
    pub fn remove_entry(&self, name: String) -> napi::Result<AsyncTask<NativeRemoveEntryTask>> {
        Ok(AsyncTask::new(NativeRemoveEntryTask {
            directory: self.inner.deferred_capability(),
            name,
        }))
    }
}

enum DirectoryOpenSource {
    Root {
        root: RunnerStorageRoot,
        components: Vec<String>,
        create: bool,
    },
    Child {
        directory: DeferredRunnerStorageDirectory,
        name: String,
        create: bool,
    },
}

#[doc(hidden)]
pub struct NativeDirectoryOpenTask {
    source: DirectoryOpenSource,
}

impl Task for NativeDirectoryOpenTask {
    type Output = RunnerStorageDirectory;
    type JsValue = NativeRunnerStorageDirectory;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        match &self.source {
            DirectoryOpenSource::Root {
                root,
                components,
                create,
            } => {
                if *create {
                    root.ensure_directory(components)
                } else {
                    root.open_directory(components)
                }
            }
            DirectoryOpenSource::Child {
                directory,
                name,
                create,
            } => {
                let directory = directory.open().map_err(to_napi_error)?;
                if *create {
                    directory.ensure_child_directory(name)
                } else {
                    directory.open_child_directory(name)
                }
            }
        }
        .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(NativeRunnerStorageDirectory { inner: output })
    }
}

#[doc(hidden)]
pub struct NativeMoveEntryTask {
    root: RunnerStorageRoot,
    source_components: Vec<String>,
    destination_components: Vec<String>,
}

impl Task for NativeMoveEntryTask {
    type Output = MoveOutcome;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.root
            .move_entry_noreplace(&self.source_components, &self.destination_components)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output.as_str().to_owned())
    }
}

#[doc(hidden)]
pub struct NativeFileCreateTask {
    directory: DeferredRunnerStorageDirectory,
    name: String,
    contents: Vec<u8>,
}

impl Task for NativeFileCreateTask {
    type Output = bool;
    type JsValue = bool;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let directory = self.directory.open().map_err(to_napi_error)?;
        directory
            .write_file_exclusive(&self.name, &self.contents)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[doc(hidden)]
pub struct NativeFileWriteTask {
    directory: DeferredRunnerStorageDirectory,
    name: String,
    contents: Vec<u8>,
}

impl Task for NativeFileWriteTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.directory
            .open()
            .map_err(to_napi_error)?
            .replace_file(&self.name, &self.contents)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[doc(hidden)]
pub struct NativeFileReadTask {
    directory: DeferredRunnerStorageDirectory,
    name: String,
    maximum_bytes: u64,
}

impl Task for NativeFileReadTask {
    type Output = Vec<u8>;
    type JsValue = Buffer;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.directory
            .open()
            .map_err(to_napi_error)?
            .read_file_bounded(&self.name, self.maximum_bytes)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(Buffer::from(output))
    }
}

#[doc(hidden)]
pub struct NativeInventoryTask {
    directory: DeferredRunnerStorageDirectory,
}

impl Task for NativeInventoryTask {
    type Output = NativeInventory;
    type JsValue = NativeInventory;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.directory
            .open()
            .map_err(to_napi_error)?
            .inventory()
            .and_then(NativeInventory::try_from)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[doc(hidden)]
pub struct NativeRemoveEntryTask {
    directory: DeferredRunnerStorageDirectory,
    name: String,
}

impl Task for NativeRemoveEntryTask {
    type Output = ();
    type JsValue = ();

    fn compute(&mut self) -> napi::Result<Self::Output> {
        self.directory
            .open()
            .map_err(to_napi_error)?
            .remove_entry(&self.name)
            .map_err(to_napi_error)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn to_napi_error(error: StorageError) -> napi::Error {
    napi::Error::new(
        napi::Status::GenericFailure,
        format!("bluey_runner_storage:{}", error.code().as_str()),
    )
}
