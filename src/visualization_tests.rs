use super::*;
use crate::audio::SpectrumAnalyzer;
use crate::audio::types::{AudioBlock, AudioFormat};

fn legacy() -> VisualizationConfig {
    VisualizationConfig {
        attack_ms: 30.0,
        release_ms: 220.0,
        vis_attack_ms: 45.0,
        vis_release_ms: 140.0,
        bar_gain: 3.1,
        peak_fall_per_sec: 1.35,
        ..Default::default()
    }
}

#[test]
fn partial_overrides_and_invalid_fields_preserve_defaults() {
    let config: VisualizationConfig =
        serde_json::from_str(r#"{"min_hz":60,"bar_gain":4,"unknown":true}"#).unwrap();
    assert_eq!(config.min_hz, 60.0);
    assert_eq!(config.bar_gain, 4.0);
    assert_eq!(config.vis_attack_ms, 20.0);
    for value in ["null", "[]", "false", "42", "\"text\""] {
        assert_eq!(
            serde_json::from_str::<VisualizationConfig>(value).unwrap(),
            VisualizationConfig::default()
        );
    }
    for invalid in [
        "null", "false", "[]", "{}", "\"slow\"", "0", "-1", "1e100", "1e-100",
    ] {
        for key in [
            "min_hz",
            "max_hz",
            "attack_ms",
            "release_ms",
            "vis_attack_ms",
            "vis_release_ms",
            "bar_gain",
            "peak_fall_per_sec",
        ] {
            let json = format!(r#"{{"{key}":{invalid}}}"#);
            assert_eq!(
                serde_json::from_str::<VisualizationConfig>(&json).unwrap(),
                VisualizationConfig::default(),
                "{json}"
            );
        }
    }
    let config: VisualizationConfig =
        serde_json::from_str(r#"{"bar_gain":4,"attack_ms":null}"#).unwrap();
    assert_eq!(config.bar_gain, 4.0);
    assert_eq!(config.attack_ms, 15.0);
}

#[test]
fn invalid_frequency_order_resets_only_the_frequency_pair() {
    for range in [
        r#""min_hz":20000"#,
        r#""min_hz":80,"max_hz":80"#,
        r#""max_hz":20"#,
    ] {
        let config: VisualizationConfig =
            serde_json::from_str(&format!(r#"{{{range},"bar_gain":5}}"#)).unwrap();
        assert_eq!((config.min_hz, config.max_hz), (40.0, 16000.0));
        assert_eq!(config.bar_gain, 5.0);
    }
}

fn block(amplitude: f32, sr: u32) -> AudioBlock {
    let frames = 1024;
    AudioBlock {
        format: AudioFormat {
            sample_rate: sr,
            channels: 1,
        },
        frames,
        samples: (0..frames)
            .map(|i| amplitude * (std::f32::consts::TAU * 1000.0 * i as f32 / sr as f32).sin())
            .collect(),
    }
}

#[test]
fn pulse_rises_faster_and_silence_falls_faster_through_both_layers() {
    let fast = VisualizationConfig::default();
    let old = legacy();
    let mut fast_analyzer = SpectrumAnalyzer::new(fast.spectrum_config()).unwrap();
    let mut old_analyzer = SpectrumAnalyzer::new(old.spectrum_config()).unwrap();
    let tone = block(0.1, 48000);
    let fast_frame = fast_analyzer.process_block(&tone).unwrap();
    let old_frame = old_analyzer.process_block(&tone).unwrap();
    let mut fast_display = Vec::new();
    let mut fast_peaks = Vec::new();
    let mut old_display = Vec::new();
    let mut old_peaks = Vec::new();
    fast.animate(
        &fast_frame.bars,
        &mut fast_display,
        &mut fast_peaks,
        1.0 / 60.0,
    );
    old.animate(
        &old_frame.bars,
        &mut old_display,
        &mut old_peaks,
        1.0 / 60.0,
    );
    let fast_height = fast_display
        .iter()
        .map(|&v| fast.height(v))
        .fold(0.0, f32::max);
    let old_height = old_display
        .iter()
        .map(|&v| old.height(v))
        .fold(0.0, f32::max);
    assert!(fast_height > old_height && fast_height < 1.0);
    assert!(fast.height(0.05) > old.height(0.05));
    // Equalize both filters at steady state before measuring release.
    for _ in 0..300 {
        fast_analyzer.process_block(&tone);
        old_analyzer.process_block(&tone);
    }
    fast_display.fill(0.5);
    old_display.fill(0.5);
    fast_peaks.fill(0.7);
    old_peaks.fill(0.7);
    let silence = block(0.0, 48000);
    for _ in 0..30 {
        let f = fast_analyzer.process_block(&silence).unwrap();
        let o = old_analyzer.process_block(&silence).unwrap();
        fast.animate(&f.bars, &mut fast_display, &mut fast_peaks, 1.0 / 60.0);
        old.animate(&o.bars, &mut old_display, &mut old_peaks, 1.0 / 60.0);
        assert!(
            f.bars
                .iter()
                .chain(&fast_display)
                .chain(&fast_peaks)
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        );
    }
    assert!(fast_display.iter().sum::<f32>() < old_display.iter().sum::<f32>());
    assert!(fast_peaks.iter().sum::<f32>() < old_peaks.iter().sum::<f32>());
}

#[test]
fn configured_frequency_limits_respect_device_rate_and_gain_is_bounded() {
    let config = VisualizationConfig {
        min_hz: 100.0,
        max_hz: 96000.0,
        bar_gain: f32::MAX,
        ..Default::default()
    };
    let mut analyzer = SpectrumAnalyzer::new(config.spectrum_config()).unwrap();
    for sr in [8000, 44100, 48000] {
        let frame = analyzer.process_block(&block(0.1, sr)).unwrap();
        assert!(
            frame
                .bars
                .iter()
                .all(|&v| config.height(v).is_finite() && (0.0..=1.0).contains(&config.height(v)))
        );
    }
    let mut analyzer = SpectrumAnalyzer::new(
        VisualizationConfig {
            min_hz: 20.0,
            max_hz: 400.0,
            ..Default::default()
        }
        .spectrum_config(),
    )
    .unwrap();
    let outside = analyzer.process_block(&block(0.1, 48000)).unwrap();
    let mut analyzer =
        SpectrumAnalyzer::new(VisualizationConfig::default().spectrum_config()).unwrap();
    let inside = analyzer.process_block(&block(0.1, 48000)).unwrap();
    assert!(
        outside.bars.iter().copied().fold(0.0, f32::max)
            < inside.bars.iter().copied().fold(0.0, f32::max)
    );
}
