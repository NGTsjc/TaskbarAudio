use serde::Deserialize;

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

    #[cfg(test)]
    pub fn init_test_config() {
        USER_CONFIG.get_or_init(UserConfig::default);
    }

    pub fn load_user_config() -> anyhow::Result<()> {
        let config = load_user_config_from_file()?;
        USER_CONFIG
            .set(config)
            .map_err(|_| anyhow::anyhow!("user config already initialized"))?;
        Ok(())
    }

    pub fn get_user_config() -> &'static UserConfig {
        USER_CONFIG.get().expect("user config not initialized")
    }

    fn load_user_config_from_file() -> anyhow::Result<UserConfig> {
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("exe has no parent directory"))?;
        let config_path = exe_dir.join("config.json");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            match parse_user_config(&content) {
                Ok(user_config) => {
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

fn parse_user_config(content: &str) -> serde_json::Result<UserConfig> {
    // Accept a UTF-8 BOM from Windows editors and preserve both supported layouts.
    let mut value: serde_json::Value =
        serde_json::from_str(content.trim_start_matches('\u{feff}'))?;
    if let Some(user) = value.get_mut("user") {
        serde_json::from_value(user.take())
    } else {
        serde_json::from_value(value)
    }
}

fn deserialize_toggle_hotkey<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
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
        for value in [
            serde_json::json!(12),
            serde_json::json!(true),
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::Value::Null,
        ] {
            let config: UserConfig = serde_json::from_value(
                serde_json::json!({"position": "left", "toggle_hotkey": value}),
            )
            .unwrap();
            assert_eq!(config.position, "left");
            assert!(config.toggle_hotkey.is_none());
        }
        let config: UserConfig = serde_json::from_str(r#"{"toggle_hotkey":"Ctrl+Alt+V"}"#).unwrap();
        assert_eq!(config.toggle_hotkey.as_deref(), Some("Ctrl+Alt+V"));
        assert!(
            serde_json::from_str::<UserConfig>("{}")
                .unwrap()
                .toggle_hotkey
                .is_none()
        );
        let nested =
            parse_user_config(r#"{"user":{"position":"right","toggle_hotkey":"Ctrl+Alt+V"}}"#)
                .unwrap();
        assert_eq!(nested.position, "right");
        assert_eq!(nested.toggle_hotkey, config.toggle_hotkey);
    }

    #[test]
    fn actual_parser_supports_flat_nested_bom_and_old_config() {
        for content in [
            r#"{"position":"left","offset_x":12}"#,
            r#"{"user":{"position":"left","offset_x":12}}"#,
            "\u{feff}{\"position\":\"left\",\"offset_x\":12}",
        ] {
            let config = parse_user_config(content).unwrap();
            assert_eq!(config.position, "left");
            assert_eq!(config.offset_x, 12);
            assert!(config.toggle_hotkey.is_none());
            assert!(!config.environment_volume);
        }
        let config =
            parse_user_config(r#"{"user":{"auto_start":true,"toggle_hotkey":[]}}"#).unwrap();
        assert!(config.auto_start);
        assert!(config.toggle_hotkey.is_none());
        for content in ["{", "null", r#"{"user":false}"#, r#"{"offset_x":"wrong"}"#] {
            assert!(parse_user_config(content).is_err());
        }
    }
}
