use std::path::PathBuf;
use std::{env, fs};

use anyhow::{Context, Result};

const APP_DIR_NAME: &str = "bluey";
const LEGACY_APP_DIR_NAME: &str = "cue";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub state_file: PathBuf,
    pub account_file: PathBuf,
    pub settings_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let data_base = dirs::data_local_dir()
            .or_else(dirs::data_dir)
            .context("could not locate a per-user data directory")?;
        let config_base = dirs::config_dir().context("could not locate a config directory")?;
        let runtime_base = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .unwrap_or_else(std::env::temp_dir);

        let data_dir = path_override_any("BLUEY_DATA_DIR", "CUE_DATA_DIR")
            .unwrap_or_else(|| product_or_legacy_dir(&data_base));
        let config_dir = path_override_any("BLUEY_CONFIG_DIR", "CUE_CONFIG_DIR")
            .unwrap_or_else(|| product_or_legacy_dir(&config_base));
        let runtime_dir = path_override_any("BLUEY_RUNTIME_DIR", "CUE_RUNTIME_DIR")
            .unwrap_or_else(|| product_or_legacy_dir(&runtime_base));
        let state_file = runtime_dir.join("daemon-state.json");
        let account_file = config_dir.join("account.json");
        let settings_file = config_dir.join("settings.json");

        Ok(Self {
            data_dir,
            config_dir,
            runtime_dir,
            state_file,
            account_file,
            settings_file,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        create_private_dir(&self.data_dir)?;
        create_private_dir(&self.config_dir)?;
        create_private_dir(&self.runtime_dir)?;
        Ok(())
    }
}

pub fn create_private_dir(path: &std::path::Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    // Permission tightening is best-effort: we cannot chmod a directory
    // we do not own (e.g. /tmp, /var, a mounted volume). Callers that
    // require a 0o700 invariant should verify it with metadata instead of
    // relying on this helper.
    let _ = set_private_dir_permissions(path);
    Ok(())
}

pub fn set_private_dir_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to set private permissions on {}", path.display()))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::{
            SetFileSecurityW, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION,
        };

        let encoded = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let security = crate::ipc_auth::WindowsOwnerOnlySecurity::new()?;
        if unsafe {
            SetFileSecurityW(
                encoded.as_ptr(),
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                security.as_security_descriptor(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!("failed to set private permissions on {}", path.display())
            });
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
    }

    Ok(())
}

/// Create a brand-new owner-only file without following a final-component
/// symlink or inheriting a broad Windows DACL.
///
/// Callers should use this for files that can contain credentials, private
/// conversion output, or other user data. The operation deliberately fails
/// when `path` already exists so an attacker cannot pre-place the output.
#[cfg(unix)]
pub fn create_private_file_new(path: &std::path::Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))
}

#[cfg(windows)]
pub fn create_private_file_new(path: &std::path::Path) -> Result<fs::File> {
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
    let mut security = crate::ipc_auth::WindowsOwnerOnlySecurity::new()?;
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
            .with_context(|| format!("failed to create {}", path.display()));
    }
    let file = unsafe { fs::File::from_raw_handle(handle) };
    crate::ipc_auth::validate_windows_owner_only_file(&file)?;
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
pub fn create_private_file_new(path: &std::path::Path) -> Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))
}

/// Validate the owner-only invariant of a private file after publication.
#[cfg(unix)]
pub fn validate_private_file(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if !metadata.is_file()
        || metadata.mode() & 0o777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
    {
        anyhow::bail!("{} is not an owner-only private file", path.display());
    }
    Ok(())
}

#[cfg(windows)]
pub fn validate_private_file(path: &std::path::Path) -> Result<()> {
    let file = fs::OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    crate::ipc_auth::validate_windows_owner_only_file(&file)
}

#[cfg(not(any(unix, windows)))]
pub fn validate_private_file(path: &std::path::Path) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to inspect {}", path.display()))?;
    anyhow::ensure!(metadata.is_file(), "{} is not a file", path.display());
    Ok(())
}

fn path_override(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn path_override_any(primary: &str, legacy: &str) -> Option<PathBuf> {
    path_override(primary).or_else(|| path_override(legacy))
}

fn product_or_legacy_dir(base: &std::path::Path) -> PathBuf {
    let product = base.join(APP_DIR_NAME);
    let legacy = base.join(LEGACY_APP_DIR_NAME);
    if !product.exists() && legacy.exists() {
        legacy
    } else {
        product
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn ensure_uses_private_directory_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let base =
            std::env::temp_dir().join(format!("bluey-app-paths-perms-{}", uuid::Uuid::new_v4()));
        let paths = AppPaths {
            data_dir: base.join("data"),
            config_dir: base.join("config"),
            runtime_dir: base.join("run"),
            state_file: base.join("run/daemon-state.json"),
            account_file: base.join("config/account.json"),
            settings_file: base.join("config/settings.json"),
        };

        paths.ensure().expect("ensure paths");

        for dir in [&paths.data_dir, &paths.config_dir, &paths.runtime_dir] {
            let mode = fs::metadata(dir).expect("metadata").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{}", dir.display());
        }

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn private_file_creation_is_owner_only_and_refuses_replacement() {
        let base =
            std::env::temp_dir().join(format!("bluey-private-file-perms-{}", uuid::Uuid::new_v4()));
        create_private_dir(&base).expect("private parent");
        let path = base.join("account.json");
        let file = create_private_file_new(&path).expect("private file");
        drop(file);

        validate_private_file(&path).expect("owner-only file");
        assert!(create_private_file_new(&path).is_err());

        let _ = fs::remove_dir_all(base);
    }
}
