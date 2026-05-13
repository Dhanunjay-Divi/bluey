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
        fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create {}", self.data_dir.display()))?;
        fs::create_dir_all(&self.config_dir)
            .with_context(|| format!("failed to create {}", self.config_dir.display()))?;
        fs::create_dir_all(&self.runtime_dir)
            .with_context(|| format!("failed to create {}", self.runtime_dir.display()))?;
        Ok(())
    }
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
