use std::fmt;
use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, timeout_at, Instant};

use crate::app_paths::AppPaths;
use crate::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use crate::ipc_auth::{
    error_chain_has_not_found, load_ipc_capability, AuthenticatedDaemonRequest, DaemonWireRequest,
    IpcAuthErrorCode, IpcAuthorization,
};

pub const IPC_MAX_REQUEST_BYTES: usize = 256 * 1024;
pub const IPC_MAX_RESPONSE_BYTES: usize = 256 * 1024;
pub const IPC_MAX_CONNECTIONS: usize = 64;
pub const IPC_REQUEST_READ_DEADLINE: Duration = Duration::from_secs(3);
pub const IPC_RESPONSE_WRITE_DEADLINE: Duration = Duration::from_secs(3);
pub const IPC_CLIENT_OPERATION_TIMEOUT: Duration = Duration::from_secs(120);

#[cfg(unix)]
pub const IPC_SOCKET_FILE_NAME: &str = "daemon-ipc.sock";
#[cfg(unix)]
const IPC_LOCK_FILE_NAME: &str = "daemon-ipc.lock";
#[cfg(windows)]
const WINDOWS_IPC_PIPE_PREFIX: &str = r"\\.\pipe\bluey-daemon-v1-";

/// Compatibility TCP never resolves names. This prevents hostname ambiguity
/// and makes it impossible to send a capability to a non-loopback address.
pub fn validated_loopback_ipc_addr(value: &str) -> Result<SocketAddr> {
    let address: SocketAddr = value
        .parse()
        .with_context(|| format!("daemon IPC address must be numeric: {value}"))?;
    if !address.ip().is_loopback() {
        bail!("daemon IPC refused non-loopback address {address}");
    }
    Ok(address)
}

pub async fn bind_compatibility_listener(value: &str) -> Result<(TcpListener, SocketAddr)> {
    let address = validated_loopback_ipc_addr(value)?;
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("bind loopback daemon IPC at {address}"))?;
    let bound_address = listener.local_addr()?;
    Ok((listener, bound_address))
}

#[derive(Debug, thiserror::Error)]
pub enum IpcFrameReadError {
    #[error("daemon IPC frame read timed out")]
    Timeout,
    #[error("daemon IPC peer closed before sending a frame")]
    Closed,
    #[error("daemon IPC frame exceeds {limit} bytes")]
    TooLarge { limit: usize },
    #[error("daemon IPC frame is missing its delimiter")]
    MissingDelimiter,
    #[error("daemon IPC frame read failed: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum IpcFrameEncodeError {
    #[error("daemon IPC frame exceeds {limit} bytes")]
    TooLarge { limit: usize },
    #[error("daemon IPC frame serialization failed: {0}")]
    Serialize(serde_json::Error),
}

pub async fn read_bounded_frame<R>(
    reader: &mut R,
    limit: usize,
    deadline: Duration,
) -> std::result::Result<Vec<u8>, IpcFrameReadError>
where
    R: AsyncRead + Unpin,
{
    let mut bounded = BufReader::new(reader).take((limit + 1) as u64);
    let mut frame = Vec::with_capacity(limit.min(8 * 1024));
    let read = timeout(deadline, bounded.read_until(b'\n', &mut frame))
        .await
        .map_err(|_| IpcFrameReadError::Timeout)??;
    if read == 0 {
        return Err(IpcFrameReadError::Closed);
    }
    if frame.len() > limit {
        return Err(IpcFrameReadError::TooLarge { limit });
    }
    if frame.last() != Some(&b'\n') {
        return Err(IpcFrameReadError::MissingDelimiter);
    }
    frame.pop();
    Ok(frame)
}

pub fn serialize_bounded_frame<T>(
    value: &T,
    limit: usize,
) -> std::result::Result<Vec<u8>, IpcFrameEncodeError>
where
    T: Serialize,
{
    let payload_limit = limit
        .checked_sub(1)
        .ok_or(IpcFrameEncodeError::TooLarge { limit })?;
    let mut writer = BoundedFrameWriter::new(payload_limit);
    if let Err(error) = serde_json::to_writer(&mut writer, value) {
        if writer.overflowed {
            return Err(IpcFrameEncodeError::TooLarge { limit });
        }
        return Err(IpcFrameEncodeError::Serialize(error));
    }
    if writer.overflowed {
        return Err(IpcFrameEncodeError::TooLarge { limit });
    }
    writer.bytes.push(b'\n');
    Ok(writer.bytes)
}

pub fn serialize_daemon_response(response: &DaemonResponse) -> Result<Vec<u8>> {
    match serialize_bounded_frame(response, IPC_MAX_RESPONSE_BYTES) {
        Ok(frame) => Ok(frame),
        Err(IpcFrameEncodeError::TooLarge { .. }) => serialize_bounded_frame(
            &DaemonResponse::Error {
                message: "daemon response exceeded IPC size limit".to_string(),
            },
            IPC_MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into),
        Err(error) => Err(error.into()),
    }
}

pub async fn write_frame<W>(writer: &mut W, frame: &[u8], deadline: Duration) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    timeout(deadline, async {
        writer.write_all(frame).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| anyhow!("daemon IPC frame write timed out"))??;
    Ok(())
}

struct BoundedFrameWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl BoundedFrameWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8 * 1024)),
            limit,
            overflowed: false,
        }
    }
}

impl io::Write for BoundedFrameWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next_len) = self.bytes.len().checked_add(bytes.len()) else {
            self.overflowed = true;
            return Err(io::Error::other("daemon IPC frame limit exceeded"));
        };
        if next_len > self.limit {
            self.overflowed = true;
            return Err(io::Error::other("daemon IPC frame limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ClientEndpoint {
    Local,
    Compatibility(SocketAddr),
}

impl fmt::Debug for ClientEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => formatter.write_str("Local"),
            Self::Compatibility(address) => formatter
                .debug_tuple("Compatibility")
                .field(address)
                .finish(),
        }
    }
}

/// Send one typed request over the selected local transport. Protected calls
/// load the owner-only capability only after any compatibility address has
/// passed numeric-loopback validation.
pub async fn request_daemon(
    paths: &AppPaths,
    compatibility_addr: Option<&str>,
    request: DaemonRequest,
) -> Result<DaemonResponse> {
    request_daemon_with_timeout(
        paths,
        compatibility_addr,
        request,
        IPC_CLIENT_OPERATION_TIMEOUT,
    )
    .await
}

pub async fn request_daemon_with_timeout(
    paths: &AppPaths,
    compatibility_addr: Option<&str>,
    request: DaemonRequest,
    operation_timeout: Duration,
) -> Result<DaemonResponse> {
    let mut endpoint = match compatibility_addr {
        Some(value) => ClientEndpoint::Compatibility(validated_loopback_ipc_addr(value)?),
        None => ClientEndpoint::Local,
    };
    let deadline = Instant::now() + operation_timeout;

    if request.ipc_authorization() == IpcAuthorization::Public {
        let response = exchange_wire_request(
            paths,
            endpoint,
            DaemonWireRequest::Public(request),
            deadline,
        )
        .await?;
        return reject_auth_error(response);
    }

    // The release immediately before capability authentication can only be
    // crossed with commands needed to inspect and stop that daemon. These
    // requests contain no user payload, and every compatibility TCP endpoint
    // has already been constrained to a numeric loopback address.
    let capability = match load_ipc_capability(paths) {
        Ok(capability) => Some(capability),
        Err(error) if legacy_lifecycle_request(&request) && error_chain_has_not_found(&error) => {
            endpoint = legacy_lifecycle_endpoint(endpoint)?;
            None
        }
        Err(error) => return Err(error).context("daemon authentication unavailable"),
    };
    let wire_request = match capability.as_ref() {
        Some(capability) => DaemonWireRequest::Authenticated(AuthenticatedDaemonRequest::new(
            capability,
            request.clone(),
        )),
        None => DaemonWireRequest::Public(request.clone()),
    };
    let response = exchange_wire_request(paths, endpoint, wire_request, deadline).await?;

    let refresh_capability = matches!(
        (&capability, &response),
        (
            Some(_),
            DaemonResponse::IpcAuthError {
                code: IpcAuthErrorCode::StaleBoot | IpcAuthErrorCode::InvalidCredentials,
            }
        ) | (
            None,
            DaemonResponse::IpcAuthError {
                code: IpcAuthErrorCode::AuthenticationRequired,
            }
        )
    );
    if refresh_capability {
        let capability =
            load_ipc_capability(paths).context("daemon authentication refresh unavailable")?;
        let response = exchange_wire_request(
            paths,
            endpoint,
            DaemonWireRequest::Authenticated(AuthenticatedDaemonRequest::new(&capability, request)),
            deadline,
        )
        .await?;
        return reject_auth_error(response);
    }
    reject_auth_error(response)
}

fn legacy_lifecycle_request(request: &DaemonRequest) -> bool {
    match request {
        DaemonRequest::WithTrace { request, .. } => legacy_lifecycle_request(request),
        DaemonRequest::Status | DaemonRequest::Shutdown => true,
        _ => false,
    }
}

fn legacy_lifecycle_endpoint(endpoint: ClientEndpoint) -> Result<ClientEndpoint> {
    match endpoint {
        // The pre-capability release used this TCP endpoint by default. The
        // new local socket/pipe cannot reach a daemon left running by that
        // release after package replacement.
        ClientEndpoint::Local => Ok(ClientEndpoint::Compatibility(validated_loopback_ipc_addr(
            DEFAULT_DAEMON_ADDR,
        )?)),
        compatibility => Ok(compatibility),
    }
}

fn reject_auth_error(response: DaemonResponse) -> Result<DaemonResponse> {
    if let DaemonResponse::IpcAuthError { code } = response {
        bail!("daemon IPC authentication failed: {code:?}");
    }
    Ok(response)
}

async fn exchange_wire_request(
    paths: &AppPaths,
    endpoint: ClientEndpoint,
    request: DaemonWireRequest,
    deadline: Instant,
) -> Result<DaemonResponse> {
    let operation = async {
        match endpoint {
            ClientEndpoint::Compatibility(address) => {
                let stream = TcpStream::connect(address)
                    .await
                    .with_context(|| format!("connect to loopback daemon IPC at {address}"))?;
                exchange_on_stream(stream, &request).await
            }
            ClientEndpoint::Local => exchange_on_local_stream(paths, &request).await,
        }
    };
    timeout_at(deadline, operation)
        .await
        .map_err(|_| anyhow!("daemon request timed out"))?
}

async fn exchange_on_stream<S>(mut stream: S, request: &DaemonWireRequest) -> Result<DaemonResponse>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let frame = serialize_bounded_frame(request, IPC_MAX_REQUEST_BYTES)?;
    write_frame(&mut stream, &frame, IPC_RESPONSE_WRITE_DEADLINE).await?;
    let response = read_bounded_frame(
        &mut stream,
        IPC_MAX_RESPONSE_BYTES,
        IPC_CLIENT_OPERATION_TIMEOUT,
    )
    .await?;
    serde_json::from_slice(&response).context("parse daemon IPC response")
}

#[cfg(unix)]
async fn exchange_on_local_stream(
    paths: &AppPaths,
    request: &DaemonWireRequest,
) -> Result<DaemonResponse> {
    exchange_on_stream(connect_owner_only_unix_socket(paths).await?, request).await
}

#[cfg(windows)]
async fn exchange_on_local_stream(
    _paths: &AppPaths,
    request: &DaemonWireRequest,
) -> Result<DaemonResponse> {
    exchange_on_stream(connect_windows_daemon_pipe().await?, request).await
}

#[cfg(not(any(unix, windows)))]
async fn exchange_on_local_stream(
    _paths: &AppPaths,
    _request: &DaemonWireRequest,
) -> Result<DaemonResponse> {
    bail!("local daemon IPC is unsupported on this platform")
}

#[cfg(unix)]
pub fn ipc_socket_path(paths: &AppPaths) -> PathBuf {
    paths.runtime_dir.join(IPC_SOCKET_FILE_NAME)
}

#[cfg(unix)]
pub struct OwnerOnlyUnixListener {
    listener: tokio::net::UnixListener,
    _guard: UnixSocketGuard,
}

#[cfg(unix)]
impl OwnerOnlyUnixListener {
    pub fn bind(paths: &AppPaths) -> Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

        crate::ipc_auth::validate_private_runtime_dir(&paths.runtime_dir)?;
        let lock = acquire_startup_lock(&paths.runtime_dir)?;
        let socket_path = ipc_socket_path(paths);

        match std::fs::symlink_metadata(&socket_path) {
            Ok(metadata) => {
                if !metadata.file_type().is_socket()
                    || metadata.file_type().is_symlink()
                    || metadata.uid() != unsafe { libc::geteuid() }
                    || metadata.mode() & 0o777 != 0o600
                {
                    bail!(
                        "refused unsafe stale daemon IPC path {}",
                        socket_path.display()
                    );
                }
                std::fs::remove_file(&socket_path)
                    .with_context(|| format!("remove stale {}", socket_path.display()))?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("inspect daemon IPC path {}", socket_path.display()))
            }
        }

        let listener = std::os::unix::net::UnixListener::bind(&socket_path)
            .with_context(|| format!("bind owner-only daemon IPC at {}", socket_path.display()))?;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let metadata = std::fs::symlink_metadata(&socket_path)?;
        validate_unix_socket_metadata(&socket_path, &metadata)?;
        let guard = UnixSocketGuard {
            _lock: lock,
            path: socket_path,
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        Ok(Self {
            listener: tokio::net::UnixListener::from_std(listener)?,
            _guard: guard,
        })
    }

    pub async fn accept(&self) -> Result<tokio::net::UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        validate_unix_peer_owner(&stream)?;
        Ok(stream)
    }
}

#[cfg(unix)]
struct UnixSocketGuard {
    _lock: std::fs::File,
    path: PathBuf,
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl Drop for UnixSocketGuard {
    fn drop(&mut self) {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};

        let Ok(metadata) = std::fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn acquire_startup_lock(runtime_dir: &Path) -> Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let path = runtime_dir.join(IPC_LOCK_FILE_NAME);
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .with_context(|| format!("open daemon IPC startup lock {}", path.display()))?,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("create daemon IPC startup lock {}", path.display()))
        }
    };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.mode() & 0o777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
    {
        bail!("daemon IPC startup lock failed owner-only validation");
    }
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            bail!("another Bluey daemon already owns the local IPC endpoint");
        }
        return Err(error).context("lock daemon IPC startup file");
    }
    Ok(file)
}

#[cfg(unix)]
fn validate_unix_socket_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    if !metadata.file_type().is_socket()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
    {
        bail!(
            "daemon IPC socket {} failed owner/mode/type validation",
            path.display()
        );
    }
    Ok(())
}

#[cfg(unix)]
pub async fn connect_owner_only_unix_socket(paths: &AppPaths) -> Result<tokio::net::UnixStream> {
    crate::ipc_auth::validate_private_runtime_dir(&paths.runtime_dir)?;
    let path = ipc_socket_path(paths);
    let metadata = std::fs::symlink_metadata(&path)
        .with_context(|| format!("inspect daemon IPC socket {}", path.display()))?;
    validate_unix_socket_metadata(&path, &metadata)?;
    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .with_context(|| format!("connect to daemon IPC socket {}", path.display()))?;
    validate_unix_peer_owner(&stream)?;
    Ok(stream)
}

#[cfg(unix)]
pub fn validate_unix_peer_owner<T>(stream: &T) -> Result<()>
where
    T: std::os::fd::AsRawFd,
{
    let peer_uid = unix_peer_uid(stream.as_raw_fd())?;
    let current_uid = unsafe { libc::geteuid() };
    if peer_uid != current_uid {
        bail!("daemon IPC peer owner mismatch");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn unix_peer_uid(fd: std::os::fd::RawFd) -> Result<libc::uid_t> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(io::Error::last_os_error()).context("query Unix IPC peer credentials");
    }
    if length as usize != std::mem::size_of::<libc::ucred>() {
        bail!("Unix IPC peer credentials had an unexpected size");
    }
    Ok(credentials.uid)
}

#[cfg(target_os = "macos")]
fn unix_peer_uid(fd: std::os::fd::RawFd) -> Result<libc::uid_t> {
    let mut uid = 0;
    let mut gid = 0;
    if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error()).context("query Unix IPC peer credentials");
    }
    Ok(uid)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn unix_peer_uid(_fd: std::os::fd::RawFd) -> Result<libc::uid_t> {
    bail!("Unix IPC peer credential validation is unsupported on this platform")
}

#[cfg(windows)]
pub fn windows_named_pipe_name() -> Result<String> {
    use sha2::{Digest, Sha256};

    let sid = crate::ipc_auth::current_windows_user_sid()?;
    let digest = Sha256::digest(sid.as_bytes());
    Ok(format!(
        "{WINDOWS_IPC_PIPE_PREFIX}{}",
        encode_hex(&digest[..16])
    ))
}

#[cfg(windows)]
pub fn create_windows_ipc_pipe(
    name: &str,
    first_instance: bool,
) -> Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let mut security = crate::ipc_auth::WindowsOwnerOnlySecurity::new()?;
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first_instance)
        .access_inbound(true)
        .access_outbound(true)
        .reject_remote_clients(true)
        // One instance listens and one permits listener replacement while a
        // saturated accepted client is being dropped. Handler tasks remain
        // capped at IPC_MAX_CONNECTIONS by the daemon semaphore.
        .max_instances(IPC_MAX_CONNECTIONS + 2)
        .in_buffer_size(IPC_MAX_REQUEST_BYTES as u32)
        .out_buffer_size(IPC_MAX_RESPONSE_BYTES as u32);
    unsafe {
        options.create_with_security_attributes_raw(name, security.as_raw_security_attributes())
    }
    .context("create owner-only Bluey Windows named pipe")
}

#[cfg(windows)]
pub struct OwnerOnlyWindowsPipeListener {
    name: String,
    server: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl OwnerOnlyWindowsPipeListener {
    pub fn bind() -> Result<Self> {
        let name = windows_named_pipe_name()?;
        let server = create_windows_ipc_pipe(&name, true)?;
        Ok(Self { name, server })
    }

    pub async fn accept(&mut self) -> Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        loop {
            self.server
                .connect()
                .await
                .context("accept Windows daemon IPC client")?;
            let replacement = create_windows_ipc_pipe(&self.name, false)?;
            let connected = std::mem::replace(&mut self.server, replacement);
            match validate_windows_named_pipe_client(&connected) {
                Ok(()) => return Ok(connected),
                Err(error) => {
                    tracing::warn!(%error, "rejected Windows daemon IPC peer");
                }
            }
        }
    }
}

#[cfg(windows)]
async fn connect_windows_daemon_pipe() -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;

    const RETRY_DELAY: Duration = Duration::from_millis(20);
    let pipe_name = windows_named_pipe_name()?;
    loop {
        match ClientOptions::new().open(&pipe_name) {
            Ok(client) => {
                validate_windows_named_pipe_server(&client)?;
                return Ok(client);
            }
            Err(error)
                if error.raw_os_error()
                    == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32) =>
            {
                tokio::time::sleep(RETRY_DELAY).await;
            }
            Err(error) => {
                return Err(error).context("connect to Bluey Windows named pipe");
            }
        }
    }
}

#[cfg(windows)]
pub fn validate_windows_named_pipe_client<T>(pipe: &T) -> Result<()>
where
    T: std::os::windows::io::AsRawHandle,
{
    validate_windows_named_pipe_peer(pipe.as_raw_handle() as _, true)
}

#[cfg(windows)]
pub fn validate_windows_named_pipe_server<T>(pipe: &T) -> Result<()>
where
    T: std::os::windows::io::AsRawHandle,
{
    validate_windows_named_pipe_peer(pipe.as_raw_handle() as _, false)
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
        return Err(io::Error::last_os_error()).context("query Windows named-pipe peer process");
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
        return Err(io::Error::last_os_error()).context("query Windows named-pipe peer session");
    }
    let mut current_session = 0_u32;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current_session) } == 0 {
        return Err(io::Error::last_os_error()).context("query current Windows session");
    }
    if peer_session != current_session {
        bail!("Windows named-pipe peer session mismatch");
    }

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, peer_pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error()).context("open Windows named-pipe peer process");
    }
    let peer_sid = crate::ipc_auth::windows_process_user_sid(process);
    unsafe {
        windows_sys::Win32::Foundation::CloseHandle(process);
    }
    if peer_sid? != crate::ipc_auth::current_windows_user_sid()? {
        bail!("Windows named-pipe peer owner mismatch");
    }
    Ok(())
}

#[cfg(windows)]
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_addresses_are_numeric_loopback_only() {
        assert!(validated_loopback_ipc_addr("127.0.0.1:57321").is_ok());
        assert!(validated_loopback_ipc_addr("[::1]:57321").is_ok());
        assert!(validated_loopback_ipc_addr("0.0.0.0:57321").is_err());
        assert!(validated_loopback_ipc_addr("192.0.2.10:57321").is_err());
        assert!(validated_loopback_ipc_addr("localhost:57321").is_err());
    }

    #[tokio::test]
    async fn compatibility_listener_binds_only_loopback() {
        assert!(bind_compatibility_listener("0.0.0.0:0").await.is_err());
        let (_listener, address) = bind_compatibility_listener("127.0.0.1:0").await.unwrap();
        assert!(address.ip().is_loopback());
        assert_ne!(address.port(), 0);
    }

    #[test]
    fn request_frames_accept_exact_limit_and_reject_one_byte_over() {
        let empty = DaemonWireRequest::Public(DaemonRequest::InstructionsSet {
            text: String::new(),
        });
        let base = serialize_bounded_frame(&empty, IPC_MAX_REQUEST_BYTES)
            .unwrap()
            .len();
        let exact = DaemonWireRequest::Public(DaemonRequest::InstructionsSet {
            text: "x".repeat(IPC_MAX_REQUEST_BYTES - base),
        });
        assert_eq!(
            serialize_bounded_frame(&exact, IPC_MAX_REQUEST_BYTES)
                .unwrap()
                .len(),
            IPC_MAX_REQUEST_BYTES
        );
        let over = DaemonWireRequest::Public(DaemonRequest::InstructionsSet {
            text: "x".repeat(IPC_MAX_REQUEST_BYTES - base + 1),
        });
        assert!(matches!(
            serialize_bounded_frame(&over, IPC_MAX_REQUEST_BYTES),
            Err(IpcFrameEncodeError::TooLarge { .. })
        ));
    }

    #[test]
    fn response_frames_accept_exact_limit_and_bound_oversized_fallback() {
        let empty = DaemonResponse::Text {
            text: String::new(),
        };
        let base = serialize_bounded_frame(&empty, IPC_MAX_RESPONSE_BYTES)
            .unwrap()
            .len();
        let exact = DaemonResponse::Text {
            text: "x".repeat(IPC_MAX_RESPONSE_BYTES - base),
        };
        assert_eq!(
            serialize_bounded_frame(&exact, IPC_MAX_RESPONSE_BYTES)
                .unwrap()
                .len(),
            IPC_MAX_RESPONSE_BYTES
        );
        let over = DaemonResponse::Text {
            text: "x".repeat(IPC_MAX_RESPONSE_BYTES - base + 1),
        };
        assert!(matches!(
            serialize_bounded_frame(&over, IPC_MAX_RESPONSE_BYTES),
            Err(IpcFrameEncodeError::TooLarge { .. })
        ));
        let fallback = serialize_daemon_response(&over).unwrap();
        assert!(fallback.len() <= IPC_MAX_RESPONSE_BYTES);
        let response: DaemonResponse =
            serde_json::from_slice(&fallback[..fallback.len() - 1]).unwrap();
        assert!(matches!(response, DaemonResponse::Error { .. }));
    }

    #[tokio::test]
    async fn bounded_reader_accepts_at_limit_and_rejects_one_over() {
        let (mut client, mut server) = tokio::io::duplex(IPC_MAX_REQUEST_BYTES + 2);
        let write = tokio::spawn(async move {
            client
                .write_all(&vec![b'x'; IPC_MAX_REQUEST_BYTES - 1])
                .await
                .unwrap();
            client.write_all(b"\n").await.unwrap();
        });
        let frame = read_bounded_frame(&mut server, IPC_MAX_REQUEST_BYTES, Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(frame.len(), IPC_MAX_REQUEST_BYTES - 1);
        write.await.unwrap();

        let (mut client, mut server) = tokio::io::duplex(IPC_MAX_REQUEST_BYTES + 2);
        let write = tokio::spawn(async move {
            client
                .write_all(&vec![b'x'; IPC_MAX_REQUEST_BYTES])
                .await
                .unwrap();
            client.write_all(b"\n").await.unwrap();
        });
        assert!(matches!(
            read_bounded_frame(&mut server, IPC_MAX_REQUEST_BYTES, Duration::from_secs(1)).await,
            Err(IpcFrameReadError::TooLarge { .. })
        ));
        write.await.unwrap();
    }

    #[tokio::test]
    async fn slow_partial_frame_hits_read_deadline() {
        let (mut client, mut server) = tokio::io::duplex(64);
        client.write_all(b"{").await.unwrap();
        assert!(matches!(
            read_bounded_frame(&mut server, 64, Duration::from_millis(20)).await,
            Err(IpcFrameReadError::Timeout)
        ));
    }

    #[test]
    fn legacy_upgrade_boundary_is_lifecycle_only() {
        assert!(legacy_lifecycle_request(&DaemonRequest::Status));
        assert!(legacy_lifecycle_request(
            &DaemonRequest::Shutdown.with_trace_id("upgrade")
        ));
        assert!(!legacy_lifecycle_request(&DaemonRequest::ContextList));
        assert!(!legacy_lifecycle_request(&DaemonRequest::OverlayShow));
        assert!(matches!(
            legacy_lifecycle_endpoint(ClientEndpoint::Local).unwrap(),
            ClientEndpoint::Compatibility(address) if address.to_string() == DEFAULT_DAEMON_ADDR
        ));
    }

    #[cfg(unix)]
    async fn assert_client_refreshes_capability_after(code: IpcAuthErrorCode) {
        use crate::ipc_auth::{publish_ipc_capability, IpcCapabilityRecord};

        let paths = test_paths("ipc-client-refresh");
        paths.ensure().expect("paths");
        let first = IpcCapabilityRecord::generate().expect("first capability");
        let second = IpcCapabilityRecord::generate().expect("second capability");
        publish_ipc_capability(&paths, &first).expect("publish first capability");

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server_paths = paths.clone();
        let first_boot = first.boot_id;
        let second_boot = second.boot_id;
        let server = tokio::spawn(async move {
            let (mut first_stream, _) = listener.accept().await.unwrap();
            let first_frame = read_bounded_frame(
                &mut first_stream,
                IPC_MAX_REQUEST_BYTES,
                Duration::from_secs(1),
            )
            .await
            .unwrap();
            let first_wire: DaemonWireRequest = serde_json::from_slice(&first_frame).unwrap();
            let DaemonWireRequest::Authenticated(first_request) = first_wire else {
                panic!("first request was not authenticated");
            };
            assert_eq!(first_request.boot_id, first_boot);

            publish_ipc_capability(&server_paths, &second).expect("publish second capability");
            let stale = serialize_bounded_frame(
                &DaemonResponse::IpcAuthError { code },
                IPC_MAX_RESPONSE_BYTES,
            )
            .unwrap();
            write_frame(&mut first_stream, &stale, Duration::from_secs(1))
                .await
                .unwrap();

            let (mut second_stream, _) = listener.accept().await.unwrap();
            let second_frame = read_bounded_frame(
                &mut second_stream,
                IPC_MAX_REQUEST_BYTES,
                Duration::from_secs(1),
            )
            .await
            .unwrap();
            let second_wire: DaemonWireRequest = serde_json::from_slice(&second_frame).unwrap();
            let DaemonWireRequest::Authenticated(second_request) = second_wire else {
                panic!("refreshed request was not authenticated");
            };
            assert_eq!(second_request.boot_id, second_boot);

            let ok = serialize_bounded_frame(&DaemonResponse::Ok, IPC_MAX_RESPONSE_BYTES).unwrap();
            write_frame(&mut second_stream, &ok, Duration::from_secs(1))
                .await
                .unwrap();
        });

        let response = request_daemon_with_timeout(
            &paths,
            Some(&address.to_string()),
            DaemonRequest::Status,
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(matches!(response, DaemonResponse::Ok));
        server.await.unwrap();

        let current = load_ipc_capability(&paths).unwrap();
        crate::ipc_auth::remove_ipc_capability_if_current(&paths, current.boot_id).unwrap();
        let _ = std::fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn client_refreshes_replaced_capability_once_for_rejected_credentials() {
        assert_client_refreshes_capability_after(IpcAuthErrorCode::StaleBoot).await;
        assert_client_refreshes_capability_after(IpcAuthErrorCode::InvalidCredentials).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn legacy_probe_retries_authenticated_when_new_daemon_publishes_capability() {
        use crate::ipc_auth::{publish_ipc_capability, IpcCapabilityRecord};

        let paths = test_paths("ipc-client-legacy-refresh");
        paths.ensure().expect("paths");
        let capability = IpcCapabilityRecord::generate().expect("capability");
        let capability_boot = capability.boot_id;
        let server_paths = paths.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut legacy_stream, _) = listener.accept().await.unwrap();
            let legacy_frame = read_bounded_frame(
                &mut legacy_stream,
                IPC_MAX_REQUEST_BYTES,
                Duration::from_secs(1),
            )
            .await
            .unwrap();
            let legacy_wire: DaemonWireRequest = serde_json::from_slice(&legacy_frame).unwrap();
            assert!(matches!(legacy_wire, DaemonWireRequest::Public(_)));

            publish_ipc_capability(&server_paths, &capability).expect("publish capability");
            let required = serialize_bounded_frame(
                &DaemonResponse::IpcAuthError {
                    code: IpcAuthErrorCode::AuthenticationRequired,
                },
                IPC_MAX_RESPONSE_BYTES,
            )
            .unwrap();
            write_frame(&mut legacy_stream, &required, Duration::from_secs(1))
                .await
                .unwrap();

            let (mut authenticated_stream, _) = listener.accept().await.unwrap();
            let authenticated_frame = read_bounded_frame(
                &mut authenticated_stream,
                IPC_MAX_REQUEST_BYTES,
                Duration::from_secs(1),
            )
            .await
            .unwrap();
            let authenticated_wire: DaemonWireRequest =
                serde_json::from_slice(&authenticated_frame).unwrap();
            let DaemonWireRequest::Authenticated(authenticated) = authenticated_wire else {
                panic!("retry was not authenticated");
            };
            assert_eq!(authenticated.boot_id, capability_boot);

            let ok = serialize_bounded_frame(&DaemonResponse::Ok, IPC_MAX_RESPONSE_BYTES).unwrap();
            write_frame(&mut authenticated_stream, &ok, Duration::from_secs(1))
                .await
                .unwrap();
        });

        let response = request_daemon_with_timeout(
            &paths,
            Some(&address.to_string()),
            DaemonRequest::Shutdown,
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(matches!(response, DaemonResponse::Ok));
        server.await.unwrap();

        crate::ipc_auth::remove_ipc_capability_if_current(&paths, capability_boot).unwrap();
        let _ = std::fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn capability_free_upgrade_bridge_sends_only_legacy_lifecycle_requests() {
        let paths = test_paths("ipc-client-legacy");
        paths.ensure().expect("paths");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let frame =
                read_bounded_frame(&mut stream, IPC_MAX_REQUEST_BYTES, Duration::from_secs(1))
                    .await
                    .unwrap();
            let request: DaemonRequest = serde_json::from_slice(&frame).unwrap();
            let (request, trace_id) = request.into_trace_parts();
            assert!(matches!(request, DaemonRequest::Status));
            assert_eq!(trace_id.as_deref(), Some("upgrade-status"));

            let ok = serialize_bounded_frame(&DaemonResponse::Ok, IPC_MAX_RESPONSE_BYTES).unwrap();
            write_frame(&mut stream, &ok, Duration::from_secs(1))
                .await
                .unwrap();
        });

        let response = request_daemon_with_timeout(
            &paths,
            Some(&address.to_string()),
            DaemonRequest::Status.with_trace_id("upgrade-status"),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(matches!(response, DaemonResponse::Ok));
        server.await.unwrap();

        std::fs::write(crate::ipc_auth::ipc_capability_path(&paths), b"not json").unwrap();
        let error = request_daemon_with_timeout(
            &paths,
            Some(&address.to_string()),
            DaemonRequest::Status,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("daemon authentication unavailable"));
        std::fs::remove_file(crate::ipc_auth::ipc_capability_path(&paths)).unwrap();

        let error = request_daemon_with_timeout(
            &paths,
            Some(&address.to_string()),
            DaemonRequest::OverlayShow,
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("daemon authentication unavailable"));
        let _ = std::fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(unix)]
    fn test_paths(label: &str) -> AppPaths {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let base = std::env::temp_dir().join(format!("b-{label}-{}", &id[..8]));
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
    #[tokio::test]
    async fn unix_socket_is_owner_only_and_concurrent_start_is_rejected() {
        use std::os::unix::fs::PermissionsExt;

        let paths = test_paths("ipc-listener");
        paths.ensure().unwrap();
        let first = OwnerOnlyUnixListener::bind(&paths).unwrap();
        assert_eq!(
            std::fs::metadata(ipc_socket_path(&paths))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let error = OwnerOnlyUnixListener::bind(&paths)
            .err()
            .expect("second daemon must fail");
        assert!(error.to_string().contains("already owns"));
        drop(first);

        let restarted = OwnerOnlyUnixListener::bind(&paths).unwrap();
        drop(restarted);
        assert!(!ipc_socket_path(&paths).exists());
        let _ = std::fs::remove_dir_all(paths.runtime_dir.parent().unwrap());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn windows_named_pipe_is_owner_and_session_bound() {
        use tokio::net::windows::named_pipe::ClientOptions;

        let name = format!(r"\\.\pipe\bluey-ipc-test-{}", uuid::Uuid::new_v4());
        let server = create_windows_ipc_pipe(&name, true).unwrap();
        let client = ClientOptions::new().open(&name).unwrap();
        server.connect().await.unwrap();
        validate_windows_named_pipe_client(&server).unwrap();
        validate_windows_named_pipe_server(&client).unwrap();
    }
}
