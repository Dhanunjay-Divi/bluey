use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::fs;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::Read;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(windows)]
use std::ffi::c_void;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use subtle::ConstantTimeEq;

use crate::app_paths::AppPaths;
use crate::ipc::DaemonRequest;

pub const IPC_CAPABILITY_SCHEMA_VERSION: u8 = 1;
pub const IPC_BEARER_BYTES: usize = 32;
pub const IPC_REPLAY_CACHE_CAPACITY: usize = 4_096;
pub const IPC_CAPABILITY_FILE_NAME: &str = "daemon-ipc-capability.json";
const IPC_CAPABILITY_MAX_BYTES: u64 = 4 * 1024;
#[cfg(unix)]
const IPC_CAPABILITY_REPLACEMENT_RETRIES: usize = 3;

#[derive(Clone, PartialEq, Eq)]
pub struct IpcBearer([u8; IPC_BEARER_BYTES]);

impl IpcBearer {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0_u8; IPC_BEARER_BYTES];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| anyhow!("generate daemon IPC bearer: {error}"))?;
        Ok(Self(bytes))
    }

    pub fn constant_time_eq(&self, candidate: &Self) -> bool {
        bool::from(self.0.ct_eq(&candidate.0))
    }
}

impl fmt::Debug for IpcBearer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IpcBearer([REDACTED])")
    }
}

impl Serialize for IpcBearer {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for IpcBearer {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        decode_hex_32(&encoded)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IpcCapabilityRecord {
    pub schema_version: u8,
    pub boot_id: uuid::Uuid,
    pub bearer: IpcBearer,
}

impl fmt::Debug for IpcCapabilityRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IpcCapabilityRecord")
            .field("schema_version", &self.schema_version)
            .field("boot_id", &self.boot_id)
            .field("bearer", &"[REDACTED]")
            .finish()
    }
}

impl IpcCapabilityRecord {
    pub fn generate() -> Result<Self> {
        Ok(Self {
            schema_version: IPC_CAPABILITY_SCHEMA_VERSION,
            boot_id: uuid::Uuid::new_v4(),
            bearer: IpcBearer::generate()?,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != IPC_CAPABILITY_SCHEMA_VERSION {
            return Err(anyhow!("unsupported daemon IPC capability schema"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthenticatedDaemonRequest {
    #[serde(rename = "type")]
    pub envelope_type: IpcEnvelopeType,
    pub boot_id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub bearer: IpcBearer,
    pub request: DaemonRequest,
}

impl AuthenticatedDaemonRequest {
    pub fn new(capability: &IpcCapabilityRecord, request: DaemonRequest) -> Self {
        Self {
            envelope_type: IpcEnvelopeType::Authenticated,
            boot_id: capability.boot_id,
            request_id: uuid::Uuid::new_v4(),
            bearer: capability.bearer.clone(),
            request,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcEnvelopeType {
    Authenticated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DaemonWireRequest {
    Authenticated(AuthenticatedDaemonRequest),
    Public(DaemonRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcAuthErrorCode {
    AuthenticationRequired,
    InvalidCredentials,
    StaleBoot,
    Replay,
    RequestTooLarge,
    ReadTimeout,
    MalformedRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcAuthorization {
    Public,
    ProtectedRead,
    Mutation,
    Shutdown,
}

impl DaemonRequest {
    /// This match is intentionally exhaustive so every new daemon command
    /// must choose an IPC authorization boundary before it can compile.
    pub fn ipc_authorization(&self) -> IpcAuthorization {
        match self {
            Self::WithTrace { request, .. } => request.ipc_authorization(),
            Self::Ping => IpcAuthorization::Public,
            Self::Status
            | Self::ContextList
            | Self::InstructionsGet
            | Self::MemorySearch { .. }
            | Self::AudioStatus
            | Self::AiStatus
            | Self::CloudStatus
            | Self::Recap
            | Self::ActionItems => IpcAuthorization::ProtectedRead,
            Self::Shutdown => IpcAuthorization::Shutdown,
            Self::OverlayShow
            | Self::OverlayHide
            | Self::OverlayToggle
            | Self::OverlayClear
            | Self::OverlayBoot { .. }
            | Self::OverlaySetOpacity { .. }
            | Self::OverlaySetPosition { .. }
            | Self::PushCard { .. }
            | Self::PushCardBound { .. }
            | Self::MeetingStart { .. }
            | Self::MeetingStartBound { .. }
            | Self::MeetingEnd
            | Self::MeetingEndBound { .. }
            | Self::SessionCreate { .. }
            | Self::SessionCreateBound { .. }
            | Self::SessionActivate { .. }
            | Self::SessionActivateBound { .. }
            | Self::SessionContinue
            | Self::SessionContinueBound { .. }
            | Self::SessionDeactivate
            | Self::SessionDeactivateBound { .. }
            | Self::SessionRename { .. }
            | Self::SessionRenameBound { .. }
            | Self::SessionArchive { .. }
            | Self::SessionArchiveBound { .. }
            | Self::SessionDelete { .. }
            | Self::SessionDeleteBound { .. }
            | Self::TranscriptAdd { .. }
            | Self::TranscriptAddBound { .. }
            | Self::Ask { .. }
            | Self::AskBound { .. }
            | Self::Answer { .. }
            | Self::AnswerBound { .. }
            | Self::ContextAdd { .. }
            | Self::ContextAddBound { .. }
            | Self::ContextRoleSet { .. }
            | Self::ContextRoleSetBound { .. }
            | Self::ActivePageCapture
            | Self::ActivePageCaptureBound { .. }
            | Self::ScreenCaptureStart { .. }
            | Self::ScreenCaptureStartBound { .. }
            | Self::ScreenCaptureStop
            | Self::ScreenCaptureStopBound { .. }
            | Self::MeetingDetectionSettingsReload
            | Self::InstructionsSet { .. }
            | Self::InstructionsSetBound { .. }
            | Self::InstructionsClear
            | Self::InstructionsClearBound { .. }
            | Self::AudioStart { .. }
            | Self::AudioStartBound { .. }
            | Self::AudioStop
            | Self::AudioStopBound { .. }
            | Self::CloudLogin
            | Self::CloudLogout
            | Self::CloudLogoutBound { .. }
            | Self::CloudPrepareAccountDeletion { .. }
            | Self::CloudAbortAccountDeletion { .. }
            | Self::CloudPurgeDeletedAccount { .. }
            | Self::CloudAcknowledgeDeletedAccountPurge { .. }
            | Self::SessionsMoveLocalToCurrentAccount { .. }
            | Self::SessionsMoveLocalToCurrentAccountBound { .. }
            | Self::CloudSyncNow => IpcAuthorization::Mutation,
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct ReplayKey {
    boot_id: uuid::Uuid,
    request_id: uuid::Uuid,
}

struct ReplayCache {
    ids: HashSet<ReplayKey>,
    order: VecDeque<ReplayKey>,
}

pub struct IpcAuthenticator {
    capability: IpcCapabilityRecord,
    replay: Mutex<ReplayCache>,
}

impl IpcAuthenticator {
    pub fn new(capability: IpcCapabilityRecord) -> Self {
        Self {
            capability,
            replay: Mutex::new(ReplayCache {
                ids: HashSet::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn authorize(
        &self,
        wire: DaemonWireRequest,
    ) -> std::result::Result<DaemonRequest, IpcAuthErrorCode> {
        match wire {
            DaemonWireRequest::Public(request) => {
                if request.ipc_authorization() == IpcAuthorization::Public {
                    Ok(request)
                } else {
                    Err(IpcAuthErrorCode::AuthenticationRequired)
                }
            }
            DaemonWireRequest::Authenticated(envelope) => self.authorize_envelope(envelope),
        }
    }

    pub fn boot_id(&self) -> uuid::Uuid {
        self.capability.boot_id
    }

    fn authorize_envelope(
        &self,
        envelope: AuthenticatedDaemonRequest,
    ) -> std::result::Result<DaemonRequest, IpcAuthErrorCode> {
        if envelope.boot_id != self.capability.boot_id {
            return Err(IpcAuthErrorCode::StaleBoot);
        }
        if !self.capability.bearer.constant_time_eq(&envelope.bearer) {
            return Err(IpcAuthErrorCode::InvalidCredentials);
        }

        let replay_key = ReplayKey {
            boot_id: envelope.boot_id,
            request_id: envelope.request_id,
        };
        let mut replay = self
            .replay
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !replay.ids.insert(replay_key) {
            return Err(IpcAuthErrorCode::Replay);
        }
        replay.order.push_back(replay_key);
        while replay.order.len() > IPC_REPLAY_CACHE_CAPACITY {
            if let Some(expired) = replay.order.pop_front() {
                replay.ids.remove(&expired);
            }
        }
        Ok(envelope.request)
    }
}

pub fn ipc_capability_path(paths: &AppPaths) -> PathBuf {
    paths.runtime_dir.join(IPC_CAPABILITY_FILE_NAME)
}

pub fn publish_ipc_capability(paths: &AppPaths, capability: &IpcCapabilityRecord) -> Result<()> {
    capability.validate()?;
    validate_private_runtime_dir(&paths.runtime_dir)?;

    let path = ipc_capability_path(paths);
    let temporary = paths.runtime_dir.join(format!(
        ".daemon-ipc-capability-{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    let bytes = serde_json::to_vec(capability)?;

    #[cfg(unix)]
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &path).with_context(|| format!("publish {}", path.display()))?;
        sync_directory(&paths.runtime_dir)?;
        validate_private_capability_file(&path)
    })();

    #[cfg(windows)]
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        std::io::Write::write_all(&mut file, &bytes)?;
        file.sync_all()?;
        drop(file);
        replace_windows_file(&temporary, &path)?;
        validate_private_capability_file(&path)
    })();

    #[cfg(not(any(unix, windows)))]
    let result = Err(anyhow!(
        "daemon IPC capability refused: private owner verification is unsupported"
    ));

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn load_ipc_capability(paths: &AppPaths) -> Result<IpcCapabilityRecord> {
    validate_private_runtime_dir(&paths.runtime_dir)?;
    read_private_capability_file(&ipc_capability_path(paths))
}

/// Remove only the capability published by this daemon boot. Revalidation
/// prevents an older process from deleting a newer daemon's record.
pub fn remove_ipc_capability_if_current(
    paths: &AppPaths,
    expected_boot_id: uuid::Uuid,
) -> Result<bool> {
    validate_private_runtime_dir(&paths.runtime_dir)?;
    let path = ipc_capability_path(paths);

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let file = match open_private_capability_file(&path) {
            Ok(file) => file,
            Err(error) if error_chain_has_not_found(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        let opened = file.metadata()?;
        let capability = read_capability_from(file, opened.len(), &path)?;
        if capability.boot_id != expected_boot_id {
            return Ok(false);
        }

        let current = fs::symlink_metadata(&path)
            .with_context(|| format!("recheck {} before cleanup", path.display()))?;
        validate_open_capability_metadata(&path, &current)?;
        if current.dev() != opened.dev() || current.ino() != opened.ino() {
            return Ok(false);
        }
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        sync_directory(&paths.runtime_dir)?;
        Ok(true)
    }

    #[cfg(windows)]
    {
        let file = match open_private_capability_file_for_delete(&path) {
            Ok(file) => file,
            Err(error) if error_chain_has_not_found(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        let capability = read_capability_from(&file, file.metadata()?.len(), &path)?;
        if capability.boot_id != expected_boot_id {
            return Ok(false);
        }
        mark_windows_file_for_delete(&file)
            .with_context(|| format!("remove exact capability {}", path.display()))?;
        Ok(true)
    }

    #[cfg(not(any(unix, windows)))]
    Err(anyhow!(
        "daemon IPC capability cleanup refused: unsupported platform"
    ))
}

fn read_private_capability_file(path: &Path) -> Result<IpcCapabilityRecord> {
    let file = open_private_capability_file(path)?;
    let length = file.metadata()?.len();
    read_capability_from(file, length, path)
}

fn read_capability_from<R>(file: R, length: u64, path: &Path) -> Result<IpcCapabilityRecord>
where
    R: Read,
{
    if length > IPC_CAPABILITY_MAX_BYTES {
        return Err(anyhow!("daemon IPC capability file is oversized"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(IPC_CAPABILITY_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > IPC_CAPABILITY_MAX_BYTES {
        return Err(anyhow!("daemon IPC capability file is oversized"));
    }
    let capability: IpcCapabilityRecord =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    capability.validate()?;
    Ok(capability)
}

pub(crate) fn error_chain_has_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(unix)]
pub(crate) fn validate_private_runtime_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect IPC runtime directory {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "daemon IPC runtime path is not a private directory"
        ));
    }
    if metadata.mode() & 0o777 != 0o700 {
        return Err(anyhow!("daemon IPC runtime directory must have mode 0700"));
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(anyhow!("daemon IPC runtime directory owner mismatch"));
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn validate_private_runtime_dir(path: &Path) -> Result<()> {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect IPC runtime directory {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(anyhow!("daemon IPC runtime path is a reparse point"));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn validate_private_runtime_dir(_path: &Path) -> Result<()> {
    Err(anyhow!("private daemon IPC paths are unsupported"))
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("create {}", path.display()))
}

#[cfg(windows)]
fn create_private_file(path: &Path) -> Result<fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let encoded = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut security = WindowsOwnerOnlySecurity::new()?;
    let handle = unsafe {
        CreateFileW(
            encoded.as_ptr(),
            GENERIC_WRITE,
            0,
            security.as_security_attributes(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("create {}", path.display()));
    }
    let file = unsafe { fs::File::from_raw_handle(handle) };
    validate_windows_owner_only_file(&file)?;
    Ok(file)
}

#[cfg(unix)]
fn open_private_capability_file(path: &Path) -> Result<fs::File> {
    open_private_capability_file_with(path, |_| {})
}

#[cfg(unix)]
fn open_private_capability_file_with<F>(path: &Path, mut after_open: F) -> Result<fs::File>
where
    F: FnMut(usize),
{
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    for attempt in 0..=IPC_CAPABILITY_REPLACEMENT_RETRIES {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("open {}", path.display()))?;
        after_open(attempt);

        let metadata = file.metadata()?;
        validate_open_capability_owner_and_mode(path, &metadata)?;
        if metadata.nlink() == 0 && attempt < IPC_CAPABILITY_REPLACEMENT_RETRIES {
            continue;
        }
        validate_open_capability_metadata(path, &metadata)?;
        return Ok(file);
    }

    unreachable!("bounded capability open loop always returns")
}

#[cfg(windows)]
fn open_private_capability_file(path: &Path) -> Result<fs::File> {
    use windows_sys::Win32::Foundation::GENERIC_READ;

    open_windows_private_capability_file(path, GENERIC_READ)
}

#[cfg(windows)]
fn open_private_capability_file_for_delete(path: &Path) -> Result<fs::File> {
    use windows_sys::Win32::Foundation::GENERIC_READ;
    use windows_sys::Win32::Storage::FileSystem::DELETE;

    open_windows_private_capability_file(path, GENERIC_READ | DELETE)
}

#[cfg(windows)]
fn open_windows_private_capability_file(path: &Path, desired_access: u32) -> Result<fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
        FILE_SHARE_READ, OPEN_EXISTING,
    };

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    let encoded = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            encoded.as_ptr(),
            desired_access,
            FILE_SHARE_READ | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("open {}", path.display()));
    }
    let file = unsafe { fs::File::from_raw_handle(handle) };
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(anyhow!("daemon IPC capability is a reparse point"));
    }
    validate_windows_owner_only_file(&file)?;
    Ok(file)
}

#[cfg(windows)]
fn mark_windows_file_for_delete(file: &fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };

    let disposition = FILE_DISPOSITION_INFO { DeleteFile: 1 };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as _,
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("delete capability by handle");
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn open_private_capability_file(_path: &Path) -> Result<fs::File> {
    Err(anyhow!("private daemon IPC file reads are unsupported"))
}

#[cfg(any(unix, windows))]
fn validate_private_capability_file(path: &Path) -> Result<()> {
    drop(open_private_capability_file(path)?);
    Ok(())
}

#[cfg(unix)]
fn validate_open_capability_owner_and_mode(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.is_file()
        || metadata.mode() & 0o777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(anyhow!(
            "daemon IPC capability {} failed owner/mode/type validation",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_open_capability_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    validate_open_capability_owner_and_mode(path, metadata)?;
    if metadata.nlink() != 1 {
        return Err(anyhow!(
            "daemon IPC capability {} failed nlink validation",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn replace_windows_file(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("publish daemon IPC capability");
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) struct WindowsOwnerOnlySecurity {
    descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
    attributes: windows_sys::Win32::Security::SECURITY_ATTRIBUTES,
}

#[cfg(windows)]
impl WindowsOwnerOnlySecurity {
    pub(crate) fn new() -> Result<Self> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

        let sid = current_windows_user_sid()?;
        let sddl = format!("O:{sid}D:P(A;;GA;;;{sid})");
        let encoded = std::ffi::OsStr::new(&sddl)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                encoded.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("build owner-only Windows security descriptor");
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        Ok(Self {
            descriptor,
            attributes,
        })
    }

    pub(crate) fn as_security_attributes(
        &mut self,
    ) -> *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        &mut self.attributes
    }

    pub(crate) fn as_security_descriptor(
        &self,
    ) -> windows_sys::Win32::Security::PSECURITY_DESCRIPTOR {
        self.descriptor
    }

    pub(crate) fn as_raw_security_attributes(&mut self) -> *mut c_void {
        self.as_security_attributes().cast()
    }
}

#[cfg(windows)]
impl Drop for WindowsOwnerOnlySecurity {
    fn drop(&mut self) {
        if !self.descriptor.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::LocalFree(self.descriptor);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) fn current_windows_user_sid() -> Result<String> {
    let process = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    windows_process_user_sid(process)
}

#[cfg(windows)]
pub(crate) fn windows_process_user_sid(
    process: windows_sys::Win32::Foundation::HANDLE,
) -> Result<String> {
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::OpenProcessToken;

    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error()).context("open Windows process token");
    }
    let result = (|| {
        let mut length = 0_u32;
        unsafe {
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut length);
        }
        if length < std::mem::size_of::<TOKEN_USER>() as u32 {
            return Err(std::io::Error::last_os_error()).context("size Windows token user");
        }
        let mut buffer = vec![0_u8; length as usize];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("read Windows token user");
        }
        let token_user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
        windows_sid_to_string(token_user.User.Sid)
    })();
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(token);
    }
    result
}

#[cfg(windows)]
pub(crate) fn windows_sid_to_string(sid: windows_sys::Win32::Security::PSID) -> Result<String> {
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;

    let mut encoded = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut encoded) } == 0 {
        return Err(std::io::Error::last_os_error()).context("format Windows user SID");
    }
    let mut length = 0;
    while unsafe { *encoded.add(length) } != 0 {
        length += 1;
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(encoded, length) })
        .context("decode Windows user SID");
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(encoded.cast());
    }
    result
}

#[cfg(windows)]
pub(crate) fn validate_windows_owner_only_file(file: &fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, GetAce, GetAclInformation, ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;

    let expected_sid = current_windows_user_sid()?;
    let mut owner = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as _,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status as i32))
            .context("inspect Windows capability ACL");
    }
    let result = (|| {
        if owner.is_null() || windows_sid_to_string(owner)? != expected_sid {
            return Err(anyhow!("daemon IPC capability owner mismatch"));
        }
        if dacl.is_null() {
            return Err(anyhow!("daemon IPC capability has no DACL"));
        }
        let mut info: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("inspect Windows capability ACEs");
        }
        if info.AceCount != 1 {
            return Err(anyhow!("daemon IPC capability DACL is not owner-only"));
        }
        let mut raw_ace = std::ptr::null_mut();
        if unsafe { GetAce(dacl, 0, &mut raw_ace) } == 0 || raw_ace.is_null() {
            return Err(std::io::Error::last_os_error()).context("read Windows capability ACE");
        }
        let ace = unsafe { &*(raw_ace as *const ACCESS_ALLOWED_ACE) };
        if ace.Header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8 {
            return Err(anyhow!("daemon IPC capability DACL has a non-owner ACE"));
        }
        let ace_sid = (&ace.SidStart as *const u32).cast_mut().cast();
        if windows_sid_to_string(ace_sid)? != expected_sid {
            return Err(anyhow!("daemon IPC capability grants another principal"));
        }
        Ok(())
    })();
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(descriptor);
    }
    result
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex_32(value: &str) -> std::result::Result<[u8; IPC_BEARER_BYTES], String> {
    if value.len() != IPC_BEARER_BYTES * 2 {
        return Err("daemon IPC bearer must contain exactly 32 bytes".to_string());
    }
    let mut output = [0_u8; IPC_BEARER_BYTES];
    let bytes = value.as_bytes();
    for (index, slot) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *slot = (decode_nibble(bytes[offset])? << 4) | decode_nibble(bytes[offset + 1])?;
    }
    Ok(output)
}

fn decode_nibble(value: u8) -> std::result::Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("daemon IPC bearer is not hexadecimal".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_round_trip_is_fixed_length_constant_time_and_redacted() {
        let bearer = IpcBearer::generate().expect("bearer");
        let encoded = serde_json::to_string(&bearer).expect("serialize");
        assert_eq!(encoded.len(), 66);
        let decoded: IpcBearer = serde_json::from_str(&encoded).expect("deserialize");
        assert!(bearer.constant_time_eq(&decoded));
        assert!(!format!("{bearer:?}").contains(encoded.trim_matches('"')));
    }

    #[test]
    fn authorization_is_exhaustive_for_public_read_mutation_and_shutdown() {
        assert_eq!(
            DaemonRequest::Ping.ipc_authorization(),
            IpcAuthorization::Public
        );
        assert_eq!(
            DaemonRequest::Status.ipc_authorization(),
            IpcAuthorization::ProtectedRead
        );
        assert_eq!(
            DaemonRequest::OverlayShow.ipc_authorization(),
            IpcAuthorization::Mutation
        );
        assert_eq!(
            DaemonRequest::SessionDelete {
                id: uuid::Uuid::new_v4(),
            }
            .ipc_authorization(),
            IpcAuthorization::Mutation
        );
        assert_eq!(
            DaemonRequest::SessionContinue.ipc_authorization(),
            IpcAuthorization::Mutation
        );
        assert_eq!(
            DaemonRequest::ContextRoleSet {
                id: uuid::Uuid::new_v4(),
                answer_context_role: crate::AnswerContextRole::CandidateResume,
            }
            .ipc_authorization(),
            IpcAuthorization::Mutation
        );
        assert_eq!(
            DaemonRequest::CloudAcknowledgeDeletedAccountPurge {
                owner_account_id: "account-a".to_string(),
                operation_id: uuid::Uuid::new_v4().to_string(),
                recovery_token: uuid::Uuid::new_v4().to_string(),
            }
            .ipc_authorization(),
            IpcAuthorization::Mutation
        );
        assert_eq!(
            DaemonRequest::Shutdown.ipc_authorization(),
            IpcAuthorization::Shutdown
        );
        assert_eq!(
            DaemonRequest::Shutdown
                .with_trace_id("shutdown-test")
                .ipc_authorization(),
            IpcAuthorization::Shutdown
        );
    }

    #[test]
    fn every_account_scoped_bound_request_requires_mutation_authorization() {
        let id = uuid::Uuid::new_v4();
        let fence = crate::ipc::DaemonMutationFence {
            owner_account_id: Some("account-a".to_string()),
            credential_generation: Some(7),
            meeting_id: Some(id),
            audio_session_id: Some("audio-a".to_string()),
            capture_generation: Some(3),
        };
        let requests = vec![
            DaemonRequest::PushCardBound {
                card: crate::CueCard::new(crate::CardKind::System, "title", "body"),
                fence: fence.clone(),
            },
            DaemonRequest::MeetingStartBound {
                title: None,
                fence: fence.clone(),
            },
            DaemonRequest::MeetingEndBound {
                fence: fence.clone(),
            },
            DaemonRequest::SessionCreateBound {
                title: None,
                fence: fence.clone(),
            },
            DaemonRequest::SessionActivateBound {
                id,
                fence: fence.clone(),
            },
            DaemonRequest::SessionContinueBound {
                fence: fence.clone(),
            },
            DaemonRequest::SessionDeactivateBound {
                fence: fence.clone(),
            },
            DaemonRequest::SessionRenameBound {
                id,
                title: "renamed".to_string(),
                fence: fence.clone(),
            },
            DaemonRequest::SessionArchiveBound {
                id,
                fence: fence.clone(),
            },
            DaemonRequest::SessionDeleteBound {
                id,
                fence: fence.clone(),
            },
            DaemonRequest::TranscriptAddBound {
                speaker: crate::Speaker::User,
                text: "hello".to_string(),
                is_final: true,
                fence: fence.clone(),
            },
            DaemonRequest::AskBound {
                question: "question".to_string(),
                fence: fence.clone(),
            },
            DaemonRequest::AnswerBound {
                request: crate::AnswerRequest::new(
                    "question",
                    crate::ProviderRoute::managed_commercial(),
                ),
                fence: fence.clone(),
            },
            DaemonRequest::ContextAddBound {
                path: "/tmp/context".to_string(),
                title: None,
                note: None,
                answer_context_role: crate::AnswerContextRole::Other,
                fence: fence.clone(),
            },
            DaemonRequest::ContextRoleSetBound {
                id,
                answer_context_role: crate::AnswerContextRole::CandidateResume,
                fence: fence.clone(),
            },
            DaemonRequest::ActivePageCaptureBound {
                fence: fence.clone(),
            },
            DaemonRequest::ScreenCaptureStartBound {
                interval_secs: Some(5),
                fence: fence.clone(),
            },
            DaemonRequest::ScreenCaptureStopBound {
                fence: fence.clone(),
            },
            DaemonRequest::InstructionsSetBound {
                text: "instruction".to_string(),
                fence: fence.clone(),
            },
            DaemonRequest::InstructionsClearBound {
                fence: fence.clone(),
            },
            DaemonRequest::AudioStartBound {
                enable_system: true,
                enable_microphone: true,
                mic_device_id: None,
                fence: fence.clone(),
            },
            DaemonRequest::AudioStopBound {
                fence: fence.clone(),
            },
            DaemonRequest::CloudLogoutBound {
                fence: fence.clone(),
            },
            DaemonRequest::SessionsMoveLocalToCurrentAccountBound {
                confirmed: true,
                fence,
            },
        ];

        for request in requests {
            assert_eq!(request.ipc_authorization(), IpcAuthorization::Mutation);
        }
    }

    #[test]
    fn wrong_bearer_stale_boot_duplicate_and_public_shutdown_are_rejected() {
        let capability = IpcCapabilityRecord::generate().expect("capability");
        let auth = IpcAuthenticator::new(capability.clone());

        assert!(matches!(
            auth.authorize(DaemonWireRequest::Public(DaemonRequest::Ping)),
            Ok(DaemonRequest::Ping)
        ));
        assert_eq!(
            auth.authorize(DaemonWireRequest::Public(DaemonRequest::Status))
                .unwrap_err(),
            IpcAuthErrorCode::AuthenticationRequired
        );
        assert_eq!(
            auth.authorize(DaemonWireRequest::Public(DaemonRequest::OverlayShow))
                .unwrap_err(),
            IpcAuthErrorCode::AuthenticationRequired
        );
        assert_eq!(
            auth.authorize(DaemonWireRequest::Public(DaemonRequest::Shutdown))
                .unwrap_err(),
            IpcAuthErrorCode::AuthenticationRequired
        );

        let valid = AuthenticatedDaemonRequest::new(&capability, DaemonRequest::Status);
        assert!(matches!(
            auth.authorize(DaemonWireRequest::Authenticated(valid.clone())),
            Ok(DaemonRequest::Status)
        ));
        assert_eq!(
            auth.authorize(DaemonWireRequest::Authenticated(valid))
                .unwrap_err(),
            IpcAuthErrorCode::Replay
        );
        assert!(matches!(
            auth.authorize(DaemonWireRequest::Authenticated(
                AuthenticatedDaemonRequest::new(&capability, DaemonRequest::OverlayShow)
            )),
            Ok(DaemonRequest::OverlayShow)
        ));
        assert!(matches!(
            auth.authorize(DaemonWireRequest::Authenticated(
                AuthenticatedDaemonRequest::new(&capability, DaemonRequest::Shutdown)
            )),
            Ok(DaemonRequest::Shutdown)
        ));

        let stale = IpcCapabilityRecord::generate().expect("stale");
        assert_eq!(
            auth.authorize(DaemonWireRequest::Authenticated(
                AuthenticatedDaemonRequest::new(&stale, DaemonRequest::Status)
            ))
            .unwrap_err(),
            IpcAuthErrorCode::StaleBoot
        );

        let wrong = IpcCapabilityRecord::generate().expect("wrong bearer");
        let mut forged = AuthenticatedDaemonRequest::new(&capability, DaemonRequest::Status);
        forged.bearer = wrong.bearer;
        assert_eq!(
            auth.authorize(DaemonWireRequest::Authenticated(forged))
                .unwrap_err(),
            IpcAuthErrorCode::InvalidCredentials
        );
    }

    #[cfg(unix)]
    fn test_paths(label: &str) -> AppPaths {
        let base =
            std::env::temp_dir().join(format!("bluey-{label}-{}", uuid::Uuid::new_v4().simple()));
        AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("runtime"),
            state_file: base.join("runtime/state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn capability_file_is_owner_only_atomic_and_boot_scoped() {
        use std::os::unix::fs::PermissionsExt;

        let paths = test_paths("ipc-capability");
        paths.ensure().expect("paths");
        let first = IpcCapabilityRecord::generate().expect("first");
        publish_ipc_capability(&paths, &first).expect("publish first");
        let first_inode = fs::metadata(ipc_capability_path(&paths)).unwrap();

        let second = IpcCapabilityRecord::generate().expect("second");
        publish_ipc_capability(&paths, &second).expect("atomic replace");
        let second_inode = fs::metadata(ipc_capability_path(&paths)).unwrap();
        let loaded = load_ipc_capability(&paths).expect("load second");
        assert_eq!(loaded.boot_id, second.boot_id);
        assert!(loaded.bearer.constant_time_eq(&second.bearer));
        assert_eq!(second_inode.permissions().mode() & 0o777, 0o600);

        use std::os::unix::fs::MetadataExt;
        assert_ne!(first_inode.ino(), second_inode.ino());
        assert!(!remove_ipc_capability_if_current(&paths, first.boot_id).unwrap());
        assert!(remove_ipc_capability_if_current(&paths, second.boot_id).unwrap());
        assert!(!ipc_capability_path(&paths).exists());
        let _ = fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn capability_reader_reopens_zero_link_inode_without_relaxing_validation() {
        use std::os::unix::fs::MetadataExt;

        let paths = test_paths("ipc-capability-open-replace");
        paths.ensure().expect("paths");
        let first = IpcCapabilityRecord::generate().expect("first");
        let second = IpcCapabilityRecord::generate().expect("second");
        publish_ipc_capability(&paths, &first).expect("publish first");

        let path = ipc_capability_path(&paths);
        let opened = open_private_capability_file(&path).expect("open first");
        publish_ipc_capability(&paths, &second).expect("replace first");

        let metadata = opened.metadata().expect("opened metadata");
        assert_eq!(metadata.nlink(), 0);
        assert!(validate_open_capability_metadata(&path, &metadata).is_err());
        drop(opened);

        publish_ipc_capability(&paths, &first).expect("restore first");
        let reopened = open_private_capability_file_with(&path, |attempt| {
            if attempt == 0 {
                publish_ipc_capability(&paths, &second).expect("replace after open");
            }
        })
        .expect("reopen replacement");
        let reopened_metadata = reopened.metadata().expect("reopened metadata");
        assert_eq!(reopened_metadata.nlink(), 1);
        let reopened_capability = read_capability_from(reopened, reopened_metadata.len(), &path)
            .expect("read replacement");
        assert_eq!(reopened_capability.boot_id, second.boot_id);

        assert!(remove_ipc_capability_if_current(&paths, second.boot_id).unwrap());
        let _ = fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn capability_reader_still_rejects_non_private_modes_and_hard_links() {
        use std::os::unix::fs::PermissionsExt;

        let paths = test_paths("ipc-capability-hard-link");
        paths.ensure().expect("paths");
        let capability = IpcCapabilityRecord::generate().expect("capability");
        publish_ipc_capability(&paths, &capability).expect("publish");

        let path = ipc_capability_path(&paths);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("relax mode");
        assert!(load_ipc_capability(&paths).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore mode");

        let alias = paths.runtime_dir.join("capability-hard-link.json");
        fs::hard_link(&path, &alias).expect("hard link");
        assert!(load_ipc_capability(&paths).is_err());

        fs::remove_file(alias).expect("remove hard link");
        assert!(remove_ipc_capability_if_current(&paths, capability.boot_id).unwrap());
        let _ = fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn capability_reader_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let paths = test_paths("ipc-capability-link");
        paths.ensure().expect("paths");
        let capability = IpcCapabilityRecord::generate().expect("capability");
        let outside = paths.data_dir.join("outside.json");
        fs::write(&outside, serde_json::to_vec(&capability).unwrap()).unwrap();
        symlink(&outside, ipc_capability_path(&paths)).unwrap();
        assert!(load_ipc_capability(&paths).is_err());
        let _ = fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn capability_replacement_never_exposes_a_partial_record() {
        let paths = test_paths("ipc-capability-race");
        paths.ensure().expect("paths");
        let first = IpcCapabilityRecord::generate().expect("first");
        let second = IpcCapabilityRecord::generate().expect("second");
        publish_ipc_capability(&paths, &first).expect("publish first");

        let reader_paths = paths.clone();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let reader_barrier = barrier.clone();
        let first_boot = first.boot_id;
        let second_boot = second.boot_id;
        let reader = std::thread::spawn(move || {
            reader_barrier.wait();
            for _ in 0..500 {
                let loaded = load_ipc_capability(&reader_paths).expect("atomic read");
                assert!(loaded.boot_id == first_boot || loaded.boot_id == second_boot);
            }
        });

        barrier.wait();
        for index in 0..100 {
            let capability = if index % 2 == 0 { &second } else { &first };
            publish_ipc_capability(&paths, capability).expect("atomic replacement");
        }
        reader.join().expect("reader");

        let current = load_ipc_capability(&paths).unwrap();
        assert!(remove_ipc_capability_if_current(&paths, current.boot_id).unwrap());
        let _ = fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }
}
