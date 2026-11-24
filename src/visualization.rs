use crate::audio::SpectrumConfig;
use crate::config::SpectrumDefaults;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualizationConfig {
    pub min_hz: f32,
    pub max_hz: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub vis_attack_ms: f32,
    pub vis_release_ms: f32,
    pub bar_gain: f32,
    pub peak_fall_per_sec: f32,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            min_hz: SpectrumDefaults::MIN_HZ,
            max_hz: SpectrumDefaults::MAX_HZ,
            attack_ms: SpectrumDefaults::ATTACK_MS,
            release_ms: SpectrumDefaults::RELEASE_MS,
            vis_attack_ms: SpectrumDefaults::VIS_ATTACK_MS,
            vis_release_ms: SpectrumDefaults::VIS_RELEASE_MS,
            bar_gain: SpectrumDefaults::BAR_GAIN,
            peak_fall_per_sec: SpectrumDefaults::PEAK_FALL_PER_SEC,
        }
    }
}

impl<'de> Deserialize<'de> for VisualizationConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut config = Self::default();
        let Some(object) = value.as_object() else {
            tracing::warn!("visualization must be an object; using defaults");
            return Ok(config);
        };
        for (key, target) in [
            ("min_hz", &mut config.min_hz),
            ("max_hz", &mut config.max_hz),
            ("attack_ms", &mut config.attack_ms),
            ("release_ms", &mut config.release_ms),
            ("vis_attack_ms", &mut config.vis_attack_ms),
            ("vis_release_ms", &mut config.vis_release_ms),
            ("bar_gain", &mut config.bar_gain),
            ("peak_fall_per_sec", &mut config.peak_fall_per_sec),
        ] {
            let Some(value) = object.get(key) else {
                continue;
            };
            match value.as_f64().map(|v| v as f32) {
                Some(v) if v.is_finite() && v > 0.0 => *target = v,
                _ => tracing::warn!(
                    key,
                    "visualization value must be finite and positive; using default"
                ),
            }
        }
        if config.min_hz >= config.max_hz {
            tracing::warn!(
                "visualization min_hz must be below max_hz; using default frequency range"
            );
            config.min_hz = SpectrumDefaults::MIN_HZ;
            config.max_hz = SpectrumDefaults::MAX_HZ;
        }
        Ok(config)
    }
}

impl VisualizationConfig {
    pub fn spectrum_config(self) -> SpectrumConfig {
        SpectrumConfig {
            min_hz: self.min_hz,
            max_hz: self.max_hz,
            attack_ms: self.attack_ms,
            release_ms: self.release_ms,
            ..SpectrumConfig::default()
        }
    }

    pub fn height(self, value: f32) -> f32 {
        (value * self.bar_gain)
            .clamp(0.0, 1.0)
            .powf(SpectrumDefaults::BAR_POW)
    }

    pub fn animate(self, targets: &[f32], display: &mut Vec<f32>, peaks: &mut Vec<f32>, dt: f32) {
        display.resize(targets.len(), 0.0);
        peaks.resize(targets.len(), 0.0);
        let dt = dt.clamp(0.0, 0.25);
        let a_up = 1.0 - (-dt / (self.vis_attack_ms / 1000.0).max(1e-3)).exp();
        let a_down = 1.0 - (-dt / (self.vis_release_ms / 1000.0).max(1e-3)).exp();
        for ((target, current), peak) in targets.iter().zip(display).zip(peaks) {
            let target = target.clamp(0.0, 1.0);
            let a = if target > *current { a_up } else { a_down };
            *current = (*current + (target - *current) * a).clamp(0.0, 1.0);
            *peak = (*peak - self.peak_fall_per_sec * dt)
                .max(*current)
                .clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
#[path = "visualization_tests.rs"]
mod tests;
