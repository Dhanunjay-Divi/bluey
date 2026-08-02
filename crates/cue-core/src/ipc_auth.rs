use std::fmt;
use std::fs;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::Read;
#[cfg(unix)]
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::ffi::c_void;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use subtle::ConstantTimeEq;

use crate::app_paths::AppPaths;
use crate::ipc::DaemonRequest;

pub const IPC_CAPABILITY_SCHEMA_VERSION: u8 = 1;
pub const IPC_BEARER_BYTES: usize = 32;
pub const IPC_MAX_REQUEST_BYTES: usize = 256 * 1024;
pub const IPC_MAX_RESPONSE_BYTES: usize = 256 * 1024;
pub const IPC_MAX_CONNECTIONS: usize = 64;
pub const IPC_REPLAY_CACHE_CAPACITY: usize = 4_096;
pub const IPC_CAPABILITY_FILE_NAME: &str = "daemon-ipc-capability.json";
#[cfg(windows)]
pub const WINDOWS_IPC_PIPE_PREFIX: &str = r"\\.\pipe\bluey-daemon-v1-";
const IPC_CAPABILITY_MAX_BYTES: u64 = 4 * 1024;

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
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&encode_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for IpcBearer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
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
    /// Exhaustive classification: adding a request variant fails compilation
    /// until its local authorization boundary is explicitly chosen.
    pub fn ipc_authorization(&self) -> IpcAuthorization {
        match self {
            Self::WithTrace { request, .. } => request.ipc_authorization(),
            Self::Ping => IpcAuthorization::Public,
            Self::Status
            | Self::ContextList
            | Self::ScreenshotContextDestination
            | Self::InstructionsGet
            | Self::AssistantProfileGet
            | Self::WorkspaceList
            | Self::WorkspaceGet { .. }
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
            | Self::MeetingStart { .. }
            | Self::MeetingEnd
            | Self::TranscriptAdd { .. }
            | Self::Ask { .. }
            | Self::Answer { .. }
            | Self::ContextAdd { .. }
            | Self::ScreenshotContextAttach { .. }
            | Self::ActivePageCapture
            | Self::ScreenCaptureStart { .. }
            | Self::ScreenCaptureStop
            | Self::InstructionsSet { .. }
            | Self::InstructionsClear
            | Self::AssistantProfileSet { .. }
            | Self::WorkspaceCreate { .. }
            | Self::WorkspaceUpdate { .. }
            | Self::WorkspaceActivate { .. }
            | Self::WorkspaceDelete { .. }
            | Self::JobsHandoffImport { .. }
            | Self::AudioReadinessProbe
            | Self::AudioStart { .. }
            | Self::AudioStop
            | Self::CloudLogin
            | Self::CloudLogout
            | Self::SessionsMoveLocalToCurrentAccount { .. }
            | Self::CloudSyncNow => IpcAuthorization::Mutation,
        }
    }
}

pub fn ipc_capability_path(paths: &AppPaths) -> PathBuf {
    paths.runtime_dir.join(IPC_CAPABILITY_FILE_NAME)
}

/// IPC capabilities must never leave the local host. Requiring a numeric
/// address also avoids DNS rebinding and hostname-resolution ambiguity.
pub fn validated_loopback_ipc_addr(value: &str) -> Result<SocketAddr> {
    let address: SocketAddr = value
        .parse()
        .with_context(|| format!("daemon IPC address must be a numeric socket address: {value}"))?;
    if !address.ip().is_loopback() {
        return Err(anyhow!("daemon IPC refused non-loopback address {address}"));
    }
    Ok(address)
}

pub fn publish_ipc_capability(paths: &AppPaths, capability: &IpcCapabilityRecord) -> Result<()> {
    capability.validate()?;
    validate_private_runtime_dir(&paths.runtime_dir)?;

    #[cfg(windows)]
    {
        let path = ipc_capability_path(paths);
        let temporary = paths.runtime_dir.join(format!(
            ".daemon-ipc-capability-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        let bytes = serde_json::to_vec(capability)?;
        let result = (|| {
            let mut file = create_private_file(&temporary)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            file.sync_all()?;
            drop(file);
            replace_windows_file(&temporary, &path)?;
            validate_private_capability_file(&path)?;
            Ok::<(), anyhow::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    #[cfg(unix)]
    {
        let path = ipc_capability_path(paths);
        let temporary = paths.runtime_dir.join(format!(
            ".daemon-ipc-capability-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        let bytes = serde_json::to_vec(capability)?;
        let result = (|| {
            let mut file = create_private_file(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path).with_context(|| format!("publish {}", path.display()))?;
            sync_directory(&paths.runtime_dir)?;
            validate_private_capability_file(&path)?;
            Ok::<(), anyhow::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    #[cfg(not(any(unix, windows)))]
    Err(anyhow!(
        "daemon IPC capability refused: private owner verification is unsupported on this platform"
    ))
}

pub fn load_ipc_capability(paths: &AppPaths) -> Result<IpcCapabilityRecord> {
    validate_private_runtime_dir(&paths.runtime_dir)?;
    let path = ipc_capability_path(paths);
    let file = open_private_capability_file(&path)?;
    let metadata = file.metadata()?;
    if metadata.len() > IPC_CAPABILITY_MAX_BYTES {
        return Err(anyhow!("daemon IPC capability file is oversized"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
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

/// Remove only the capability published by the expected daemon boot. The
/// boot-id and inode checks prevent an older daemon from unlinking a newer
/// daemon's capability record during overlapping shutdown/startup.
pub fn remove_ipc_capability_if_current(
    paths: &AppPaths,
    expected_boot_id: uuid::Uuid,
) -> Result<bool> {
    validate_private_runtime_dir(&paths.runtime_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let path = ipc_capability_path(paths);
        let file = match open_private_capability_file(&path) {
            Ok(file) => file,
            Err(error) if error_chain_has_not_found(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        let opened = file.metadata()?;
        if opened.len() > IPC_CAPABILITY_MAX_BYTES {
            return Err(anyhow!("daemon IPC capability file is oversized"));
        }
        let mut bytes = Vec::with_capacity(opened.len() as usize);
        file.take(IPC_CAPABILITY_MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > IPC_CAPABILITY_MAX_BYTES {
            return Err(anyhow!("daemon IPC capability file is oversized"));
        }
        let capability: IpcCapabilityRecord =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        capability.validate()?;
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
        let path = ipc_capability_path(paths);
        let file = match open_private_capability_file(&path) {
            Ok(file) => file,
            Err(error) if error_chain_has_not_found(&error) => return Ok(false),
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        if metadata.len() > IPC_CAPABILITY_MAX_BYTES {
            return Err(anyhow!("daemon IPC capability file is oversized"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(IPC_CAPABILITY_MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > IPC_CAPABILITY_MAX_BYTES {
            return Err(anyhow!("daemon IPC capability file is oversized"));
        }
        let capability: IpcCapabilityRecord =
            serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
        capability.validate()?;
        if capability.boot_id != expected_boot_id {
            return Ok(false);
        }

        // Reopen and revalidate immediately before deletion. A different-user
        // process cannot replace an owner-only file, and the boot check keeps
        // an older daemon from deleting a newer daemon's record.
        let current = open_private_capability_file(&path)?;
        let mut current_bytes = Vec::new();
        current
            .take(IPC_CAPABILITY_MAX_BYTES + 1)
            .read_to_end(&mut current_bytes)?;
        let current: IpcCapabilityRecord = serde_json::from_slice(&current_bytes)
            .with_context(|| format!("recheck {} before cleanup", path.display()))?;
        if current.boot_id != expected_boot_id {
            return Ok(false);
        }
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        Ok(true)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = expected_boot_id;
        Err(anyhow!(
            "daemon IPC capability cleanup refused: private owner verification is unsupported on this platform"
        ))
    }
}

#[cfg(any(unix, windows))]
fn error_chain_has_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(unix)]
fn validate_private_runtime_dir(path: &Path) -> Result<()> {
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
fn validate_private_runtime_dir(path: &Path) -> Result<()> {
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
fn validate_private_runtime_dir(_path: &Path) -> Result<()> {
    Err(anyhow!(
        "daemon IPC capability refused: private owner verification is unsupported on this platform"
    ))
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
    use std::os::unix::fs::OpenOptionsExt;

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    validate_open_capability_metadata(path, &file.metadata()?)?;
    Ok(file)
}

#[cfg(windows)]
fn open_private_capability_file(path: &Path) -> Result<fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE};
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
            GENERIC_READ,
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

#[cfg(not(any(unix, windows)))]
fn open_private_capability_file(_path: &Path) -> Result<fs::File> {
    Err(anyhow!("private daemon IPC file reads are unsupported"))
}

#[cfg(any(unix, windows))]
fn validate_private_capability_file(path: &Path) -> Result<()> {
    let file = open_private_capability_file(path)?;
    drop(file);
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
pub struct WindowsOwnerOnlySecurity {
    descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
    attributes: windows_sys::Win32::Security::SECURITY_ATTRIBUTES,
}

#[cfg(windows)]
impl WindowsOwnerOnlySecurity {
    pub fn new() -> Result<Self> {
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

    pub fn as_security_attributes(
        &mut self,
    ) -> *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        &mut self.attributes
    }

    pub fn as_raw_security_attributes(&mut self) -> *mut c_void {
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
pub fn windows_named_pipe_name() -> Result<String> {
    use sha2::{Digest, Sha256};

    let sid = current_windows_user_sid()?;
    let digest = Sha256::digest(sid.as_bytes());
    Ok(format!(
        "{WINDOWS_IPC_PIPE_PREFIX}{}",
        encode_hex(&digest[..16])
    ))
}

#[cfg(windows)]
pub fn validate_windows_named_pipe_client(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<()> {
    validate_windows_named_pipe_peer(handle, true)
}

#[cfg(windows)]
pub fn validate_windows_named_pipe_server(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<()> {
    validate_windows_named_pipe_peer(handle, false)
}

#[cfg(windows)]
fn validate_windows_named_pipe_peer(
    handle: windows_sys::Win32::Foundation::HANDLE,
    peer_is_client: bool,
) -> Result<()> {
    use windows_sys::Win32::System::Pipes::{
        GetNamedPipeClientProcessId, GetNamedPipeClientSessionId, GetNamedPipeServerProcessId,
        GetNamedPipeServerSessionId,
    };
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let mut peer_pid = 0_u32;
    let pid_ok = unsafe {
        if peer_is_client {
            GetNamedPipeClientProcessId(handle, &mut peer_pid)
        } else {
            GetNamedPipeServerProcessId(handle, &mut peer_pid)
        }
    };
    if pid_ok == 0 || peer_pid == 0 {
        return Err(std::io::Error::last_os_error())
            .context("query Windows named-pipe peer process");
    }

    let mut peer_session = 0_u32;
    let session_ok = unsafe {
        if peer_is_client {
            GetNamedPipeClientSessionId(handle, &mut peer_session)
        } else {
            GetNamedPipeServerSessionId(handle, &mut peer_session)
        }
    };
    if session_ok == 0 {
        return Err(std::io::Error::last_os_error())
            .context("query Windows named-pipe peer session");
    }
    let mut current_session = 0_u32;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current_session) } == 0 {
        return Err(std::io::Error::last_os_error()).context("query current Windows session");
    }
    if peer_session != current_session {
        return Err(anyhow!("Windows named-pipe peer session mismatch"));
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, peer_pid) };
    if process.is_null() {
        return Err(std::io::Error::last_os_error())
            .context("open Windows named-pipe peer process");
    }
    let peer_sid = process_windows_user_sid(process);
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(process);
    }
    if peer_sid? != current_windows_user_sid()? {
        return Err(anyhow!("Windows named-pipe peer owner mismatch"));
    }
    Ok(())
}

#[cfg(windows)]
fn current_windows_user_sid() -> Result<String> {
    let process = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    process_windows_user_sid(process)
}

#[cfg(windows)]
fn process_windows_user_sid(process: windows_sys::Win32::Foundation::HANDLE) -> Result<String> {
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
fn windows_sid_to_string(sid: windows_sys::Win32::Security::PSID) -> Result<String> {
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
fn validate_windows_owner_only_file(file: &fs::File) -> Result<()> {
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
            return Err(anyhow!(
                "daemon IPC capability DACL contains a non-owner ACE"
            ));
        }
        let ace_sid = (&ace.SidStart as *const u32).cast_mut().cast();
        if windows_sid_to_string(ace_sid)? != expected_sid {
            return Err(anyhow!(
                "daemon IPC capability DACL grants another principal"
            ));
        }
        Ok(())
    })();
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(descriptor);
    }
    result
}

#[cfg(unix)]
fn validate_open_capability_metadata(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    if !metadata.is_file()
        || metadata.mode() & 0o777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
    {
        return Err(anyhow!(
            "daemon IPC capability {} failed uid/mode/nlink validation",
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
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (decode_nibble(pair[0])? << 4) | decode_nibble(pair[1])?;
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
    fn bearer_round_trip_is_fixed_length_and_debug_redacted() {
        let bearer = IpcBearer::generate().expect("bearer");
        let encoded = serde_json::to_string(&bearer).expect("serialize");
        assert_eq!(encoded.len(), 66);
        let decoded: IpcBearer = serde_json::from_str(&encoded).expect("deserialize");
        assert!(bearer.constant_time_eq(&decoded));
        assert!(!format!("{bearer:?}").contains(encoded.trim_matches('"')));
    }

    #[test]
    fn authorization_classification_protects_reads_mutations_and_shutdown() {
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
            DaemonRequest::Shutdown.ipc_authorization(),
            IpcAuthorization::Shutdown
        );
    }

    #[test]
    fn ipc_addresses_are_numeric_and_loopback_only() {
        assert!(validated_loopback_ipc_addr("127.0.0.1:57321").is_ok());
        assert!(validated_loopback_ipc_addr("[::1]:57321").is_ok());
        assert!(validated_loopback_ipc_addr("0.0.0.0:57321").is_err());
        assert!(validated_loopback_ipc_addr("192.0.2.10:57321").is_err());
        assert!(validated_loopback_ipc_addr("localhost:57321").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn capability_file_is_private_rotated_and_rejects_links() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let base = std::env::temp_dir().join(format!("bluey-ipc-auth-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("runtime"),
            state_file: base.join("runtime/state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };
        paths.ensure().expect("paths");
        let first = IpcCapabilityRecord::generate().expect("first");
        publish_ipc_capability(&paths, &first).expect("publish first");
        let loaded = load_ipc_capability(&paths).expect("load first");
        assert_eq!(loaded.boot_id, first.boot_id);
        assert!(loaded.bearer.constant_time_eq(&first.bearer));
        assert_eq!(
            fs::metadata(ipc_capability_path(&paths))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let second = IpcCapabilityRecord::generate().expect("second");
        publish_ipc_capability(&paths, &second).expect("rotate");
        assert_eq!(load_ipc_capability(&paths).unwrap().boot_id, second.boot_id);
        assert!(!remove_ipc_capability_if_current(&paths, first.boot_id).unwrap());
        assert!(ipc_capability_path(&paths).is_file());
        assert!(remove_ipc_capability_if_current(&paths, second.boot_id).unwrap());
        assert!(!ipc_capability_path(&paths).exists());

        publish_ipc_capability(&paths, &second).expect("republish second");

        let capability_path = ipc_capability_path(&paths);
        let external = base.join("external.json");
        fs::write(&external, serde_json::to_vec(&second).unwrap()).unwrap();
        fs::remove_file(&capability_path).unwrap();
        symlink(&external, &capability_path).unwrap();
        assert!(load_ipc_capability(&paths).is_err());
        let _ = fs::remove_dir_all(base);
    }
}
