use serde::Deserialize;
use std::sync::OnceLock;

pub struct UiConfigConst;

impl UiConfigConst {
    pub const BAR_WIDTH_PX: i32 = 5;
    pub const PANEL_WIDTH_PX: i32 = SpectrumDefaults::BARS as i32 * Self::BAR_WIDTH_PX
        + (SpectrumDefaults::BARS as i32 - 1) * Self::BAR_GAP_PX as i32
        + 2 * Self::CONTENT_PAD_X_PX as i32;
    pub const COLORKEY_FUCHSIA_RGB: u32 = 0x00FF00FF;
    pub const WATCHDOG_INTERVAL_MS: u32 = 3000;
    pub const ATTACH_SETTLE_INTERVAL_MS: u32 = 50;
    pub const RETRY_ATTACH_INTERVAL_MS: u32 = 3000;
    pub const RENDER_ACTIVE_INTERVAL_MS: u32 = 16;
    pub const RENDER_IDLE_INTERVAL_MS: u32 = 120;
    pub const RENDER_HIDDEN_INTERVAL_MS: u32 = 500;
    pub const RENDER_IDLE_AFTER_MS: u64 = 1500;
    pub const RENDER_DEEP_IDLE_AFTER_MS: u64 = 6000;
    pub const RENDER_DEEP_IDLE_INTERVAL_MS: u32 = 250;
    pub const RENDER_ENERGY_ACTIVE_THRESHOLD: f32 = 0.01;
    pub const BAR_COLOR_R: f32 = 0.58;
    pub const BAR_COLOR_G: f32 = 0.88;
    pub const BAR_COLOR_B: f32 = 1.00;
    pub const BAR_GAP_PX: f32 = 2.0;
    pub const CONTENT_PAD_X_PX: f32 = 6.0;
    pub const CONTENT_PAD_TOP_PX: f32 = 0.0;
    pub const CONTENT_PAD_BOTTOM_PX: f32 = 0.0;
    pub const MIN_BAR_HEIGHT_PX: f32 = 3.0;
    pub const PEAK_HEIGHT_PX: f32 = 2.0;
    pub const PEAK_GAP_PX: f32 = 1.0;
    pub const PEAK_COLOR_R: f32 = 0.90;
    pub const PEAK_COLOR_G: f32 = 0.98;
    pub const PEAK_COLOR_B: f32 = 1.00;
    pub const GRADIENT_BOTTOM_R: f32 = 0.62;
    pub const GRADIENT_BOTTOM_G: f32 = 0.90;
    pub const GRADIENT_BOTTOM_B: f32 = 1.00;
    pub const GRADIENT_TOP_R: f32 = 0.90;
    pub const GRADIENT_TOP_G: f32 = 0.98;
    pub const GRADIENT_TOP_B: f32 = 1.00;
    pub const GRADIENT_SHIMMER_SPEED: f32 = 0.35;
    pub const GRADIENT_SHIMMER_STRENGTH: f32 = 0.18;
    pub const EXIT_ON_CONSOLE_CLOSE: bool = false;
}

pub type UiConfig = UiConfigConst;

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct SpectrumConfig {
    pub fft_size: usize,
    pub bars: usize,
    pub min_hz: f32,
    pub max_hz: f32,
    pub update_hz: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub gamma: f32,
    pub noise_floor: f32,
    pub bar_gain: f32,
    pub bar_pow: f32,
    pub vis_attack_ms: f32,
    pub vis_release_ms: f32,
    pub peak_fall_per_sec: f32,
    pub color_glow_base: f32,
    pub color_glow_range: f32,
    pub silent_decay: f32,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: 512,
            bars: 64,
            min_hz: 40.0,
            max_hz: 16_000.0,
            update_hz: 90.0,
            attack_ms: 15.0,
            release_ms: 150.0,
            gamma: 0.55,
            noise_floor: 0.0,
            bar_gain: 3.10,
            bar_pow: 0.50,
            vis_attack_ms: 20.0,
            vis_release_ms: 100.0,
            peak_fall_per_sec: 1.35,
            color_glow_base: 0.78,
            color_glow_range: 0.22,
            silent_decay: 0.985,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct UserConfig {
    pub position: String,
    pub display_mode: String,
    pub offset_x: i32,
    pub environment_volume: bool,
    pub auto_start: bool,
    #[serde(deserialize_with = "deserialize_toggle_hotkey")]
    pub toggle_hotkey: Option<String>,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            position: "center".to_string(),
            display_mode: "bar".to_string(),
            offset_x: 0,
            environment_volume: false,
            auto_start: false,
            toggle_hotkey: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct AudioConfig {
    pub block_frames: usize,
    pub queue_capacity: usize,
    pub no_data_fill_interval_ms: u64,
    pub smooth_tau_ms: u64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            block_frames: 1024,
            queue_capacity: 8,
            no_data_fill_interval_ms: 20,
            smooth_tau_ms: 180,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub file_enabled: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "debug".to_string(),
            file_enabled: false,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub spectrum: SpectrumConfig,
    pub user: UserConfig,
    pub audio: AudioConfig,
    pub logging: LoggingConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            spectrum: SpectrumConfig::default(),
            user: UserConfig::default(),
            audio: AudioConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

static APP_CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub fn load_config() -> anyhow::Result<()> {
    let config_path = config_file_path()?;
    let config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        match serde_json::from_str::<AppConfig>(&content) {
            Ok(cfg) => {
                tracing::info!(path = %config_path.display(), "config loaded");
                cfg
            }
            Err(e) => {
                tracing::warn!(error = %e, path = %config_path.display(), "config parse failed, using defaults");
                AppConfig::default()
            }
        }
    } else {
        tracing::debug!(path = %config_path.display(), "config not found, using defaults");
        AppConfig::default()
    };
    APP_CONFIG.set(config).map_err(|_| anyhow::anyhow!("config already initialized"))?;
    Ok(())
}

pub fn get_config() -> &'static AppConfig {
    APP_CONFIG.get().expect("config not initialized")
}

pub fn get_spectrum_config() -> &'static SpectrumConfig {
    &get_config().spectrum
}

pub fn get_user_config() -> &'static UserConfig {
    &get_config().user
}

pub fn get_audio_config() -> &'static AudioConfig {
    &get_config().audio
}

pub fn get_logging_config() -> &'static LoggingConfig {
    &get_config().logging
}

fn config_file_path() -> anyhow::Result<std::path::PathBuf> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("exe has no parent directory"))?;
    Ok(exe_dir.join("config.json"))
}

pub struct SpectrumDefaults;

impl SpectrumDefaults {
    pub const FFT_SIZE: usize = 1024;
    pub const BARS: usize = 50;
    pub const MIN_HZ: f32 = 40.0;
    pub const MAX_HZ: f32 = 16_000.0;
    pub const UPDATE_HZ: f32 = 60.0;
    pub const ATTACK_MS: f32 = 30.0;
    pub const RELEASE_MS: f32 = 220.0;
    pub const GAMMA: f32 = 0.55;
    pub const NOISE_FLOOR: f32 = 0.0;
    pub const BAR_GAIN: f32 = 3.10;
    pub const BAR_POW: f32 = 0.50;
    pub const VIS_ATTACK_MS: f32 = 45.0;
    pub const VIS_RELEASE_MS: f32 = 140.0;
    pub const PEAK_FALL_PER_SEC: f32 = 1.35;
    pub const COLOR_GLOW_BASE: f32 = 0.78;
    pub const COLOR_GLOW_RANGE: f32 = 0.22;
    pub const SILENT_DECAY: f32 = 0.985;
}

pub mod user {
    use super::*;
    use std::sync::OnceLock;

    static USER_CONFIG: OnceLock<UserConfig> = OnceLock::new();

    pub fn load_user_config() -> anyhow::Result<()> {
        let config = load_user_config_from_file()?;
        USER_CONFIG.set(config).map_err(|_| anyhow::anyhow!("user config already initialized"))?;
        Ok(())
    }

    pub fn get_user_config() -> &'static UserConfig {
        USER_CONFIG.get().expect("user config not initialized")
    }

    fn load_user_config_from_file() -> anyhow::Result<UserConfig> {
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path.parent().ok_or_else(|| anyhow::anyhow!("exe has no parent directory"))?;
        let config_path = exe_dir.join("config.json");
        
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(value) => {
                    let user_config: UserConfig = if let Some(user) = value.get("user") {
                        serde_json::from_value(user.clone())?
                    } else {
                        serde_json::from_str(&content)?
                    };
                    tracing::info!(path = %config_path.display(), "user config loaded");
                    Ok(user_config)
                }
                Err(e) => {
                    tracing::warn!(error = %e, path = %config_path.display(), "user config parse failed, using defaults");
                    Ok(UserConfig::default())
                }
            }
        } else {
            tracing::debug!(path = %config_path.display(), "user config not found, using defaults");
            Ok(UserConfig::default())
        }
    }
}

fn deserialize_toggle_hotkey<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value)),
        _ => {
            tracing::warn!("toggle_hotkey must be a string or null; ignoring");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod hotkey_config_tests {
    use super::*;

    #[test]
    fn invalid_hotkey_types_do_not_discard_other_settings() {
        for value in [serde_json::json!(12), serde_json::json!(true), serde_json::json!([]), serde_json::json!({}), serde_json::Value::Null] {
            let config: UserConfig = serde_json::from_value(serde_json::json!({"position": "left", "toggle_hotkey": value})).unwrap();
            assert_eq!(config.position, "left");
            assert!(config.toggle_hotkey.is_none());
        }
        let config: UserConfig = serde_json::from_str(r#"{"toggle_hotkey":"Ctrl+Alt+V"}"#).unwrap();
        assert_eq!(config.toggle_hotkey.as_deref(), Some("Ctrl+Alt+V"));
        assert!(serde_json::from_str::<UserConfig>("{}").unwrap().toggle_hotkey.is_none());
        let nested: AppConfig = serde_json::from_str(
            r#"{"user":{"position":"right","toggle_hotkey":"Ctrl+Alt+V"}}"#,
        ).unwrap();
        assert_eq!(nested.user.position, "right");
        assert_eq!(nested.user.toggle_hotkey, config.toggle_hotkey);
    }
}
