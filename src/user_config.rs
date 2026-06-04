use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Deserialize)]
pub struct UserConfig {
    #[serde(default = "default_position")]
    pub position: String,
    #[serde(default = "default_display_mode")]
    pub display_mode: String,
}

fn default_position() -> String {
    "center".to_string()
}

fn default_display_mode() -> String {
    "bar".to_string()
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            position: default_position(),
            display_mode: default_display_mode(),
        }
    }
}

static USER_CONFIG: OnceLock<UserConfig> = OnceLock::new();

pub fn load_user_config() -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    let config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        match serde_json::from_str::<UserConfig>(&content) {
            Ok(cfg) => {
                tracing::info!(path = %config_path.display(), position = cfg.position, display_mode = cfg.display_mode, "user config loaded");
                cfg
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %config_path.display(), "user config parse failed, using defaults");
                UserConfig::default()
            }
        }
    } else {
        tracing::debug!(path = %config_path.display(), "user config not found, using defaults");
        UserConfig::default()
    };
    USER_CONFIG.set(config).map_err(|_| anyhow::anyhow!("user config already initialized"))?;
    Ok(())
}

pub fn get_user_config() -> &'static UserConfig {
    USER_CONFIG.get().expect("user config not initialized")
}

fn config_file_path() -> anyhow::Result<std::path::PathBuf> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("exe has no parent directory"))?;
    Ok(exe_dir.join("config.json"))
}
