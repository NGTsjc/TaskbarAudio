use serde::Deserialize;
use std::sync::OnceLock;

pub struct UiConfig;
impl UiConfig {
    pub const PANEL_WIDTH_PX: i32 = 240;
    pub const COLORKEY_FUCHSIA_RGB: u32 = 0x00FF00FF;
    pub const WATCHDOG_INTERVAL_MS: u32 = 1000;
    pub const RETRY_ATTACH_INTERVAL_MS: u32 = 3000;
    pub const RENDER_ACTIVE_INTERVAL_MS: u32 = 16;
    pub const RENDER_IDLE_INTERVAL_MS: u32 = 120;
    pub const RENDER_HIDDEN_INTERVAL_MS: u32 = 500;
    pub const RENDER_IDLE_AFTER_MS: u64 = 1500;
    pub const RENDER_DEEP_IDLE_AFTER_MS: u64 = 6000;
    pub const RENDER_DEEP_IDLE_INTERVAL_MS: u32 = 250;
    pub const RENDER_ENERGY_ACTIVE_THRESHOLD: f32 = 0.04;
    pub const BAR_COLOR_R: f32 = 0.58;
    pub const BAR_COLOR_G: f32 = 0.88;
    pub const BAR_COLOR_B: f32 = 1.00;
    pub const BAR_GAP_PX: f32 = 2.0;
    pub const CONTENT_PAD_X_PX: f32 = 6.0;
    pub const CONTENT_PAD_TOP_PX: f32 = 0.0;
    pub const CONTENT_PAD_BOTTOM_PX: f32 = 0.0;
    pub const CONTENT_BOTTOM_FUDGE_PX: f32 = 8.0;
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

pub struct SpectrumDefaults;
impl SpectrumDefaults {
    pub const FFT_SIZE: usize = 1024;
    pub const BARS: usize = 64;
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

    #[derive(Debug, Deserialize)]
    pub struct UserConfig {
        #[serde(default = "default_position")]
        pub position: String,
        #[serde(default = "default_display_mode")]
        pub display_mode: String,
        #[serde(default)]
        pub offset_x: i32,
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
                offset_x: 0,
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
                    tracing::info!(path = %config_path.display(), position = cfg.position, display_mode = cfg.display_mode, offset_x = cfg.offset_x, "user config loaded");
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
}