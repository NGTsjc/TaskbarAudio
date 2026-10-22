use crate::audio::types::{AudioBlock, AudioFormat};
use crate::config::SpectrumDefaults;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowFunction {
    #[default]
    Hann,
}

#[derive(Clone, Copy, Debug)]
pub struct SpectrumConfig {
    pub fft_size: usize,
    pub bars: usize,
    pub min_hz: f32,
    pub max_hz: f32,
    pub update_hz: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub window: WindowFunction,
    pub gamma: f32,
    pub noise_floor: f32,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: SpectrumDefaults::FFT_SIZE,
            bars: SpectrumDefaults::BARS,
            min_hz: SpectrumDefaults::MIN_HZ,
            max_hz: SpectrumDefaults::MAX_HZ,
            update_hz: SpectrumDefaults::UPDATE_HZ,
            attack_ms: SpectrumDefaults::ATTACK_MS,
            release_ms: SpectrumDefaults::RELEASE_MS,
            window: WindowFunction::Hann,
            gamma: SpectrumDefaults::GAMMA,
            noise_floor: SpectrumDefaults::NOISE_FLOOR,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumFrame {
    pub bars: Vec<f32>,
}

pub struct SpectrumAnalyzer {
    config: SpectrumConfig,
    window: Vec<f32>,
    mono_ring: VecDeque<f32>,
    fft_buf: Vec<Complex32>,
    fft_plan: FftPlan,
    last_format: Option<AudioFormat>,
    prev_bars: Vec<f32>,
    last_output_frame: Option<usize>,
    frame_count: usize,
}

impl SpectrumAnalyzer {
    pub fn new(config: SpectrumConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.fft_size.is_power_of_two(),
            "fft_size must be power of two"
        );
        anyhow::ensure!(config.fft_size >= 16, "fft_size too small");
        anyhow::ensure!(config.bars >= 1, "bars must be >= 1");
        anyhow::ensure!(
            config.update_hz.is_finite() && config.update_hz > 0.0,
            "update_hz must be finite and > 0"
        );
        anyhow::ensure!(
            config.min_hz.is_finite()
                && config.max_hz.is_finite()
                && config.min_hz >= 0.0
                && config.max_hz > config.min_hz,
            "invalid frequency range"
        );
        anyhow::ensure!(
            [
                config.attack_ms,
                config.release_ms,
                config.gamma,
                config.noise_floor
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0),
            "invalid spectrum response parameter"
        );
        let window = build_window(config.fft_size, config.window);
        let fft_buf = vec![Complex32::default(); config.fft_size];
        Ok(Self {
            config,
            window,
            mono_ring: VecDeque::with_capacity(config.fft_size * 2),
            fft_buf,
            fft_plan: FftPlan::new(config.fft_size),
            last_format: None,
            prev_bars: vec![0.0; config.bars],
            last_output_frame: None,
            frame_count: 0,
        })
    }

    pub fn process_block(&mut self, block: &AudioBlock) -> Option<SpectrumFrame> {
        if block.format.channels == 0
            || block.format.sample_rate == 0
            || block.frames == 0
            || block.frames.checked_mul(block.format.channels as usize) != Some(block.samples.len())
        {
            return None;
        }
        if self.last_format != Some(block.format) {
            self.mono_ring.clear();
            self.prev_bars.fill(0.0);
            self.last_output_frame = None;
            self.frame_count = 0;
            self.last_format = Some(block.format);
        }
        push_block_mono(&mut self.mono_ring, block);
        // Bound memory even when the output cadence has not been reached.
        let excess = self
            .mono_ring
            .len()
            .saturating_sub(self.config.fft_size * 2);
        self.mono_ring.drain(..excess);
        self.frame_count += block.frames;
        if self.mono_ring.len() < self.config.fft_size {
            return None;
        }
        let frame_interval = (block.format.sample_rate as f32 / self.config.update_hz) as usize;
        if let Some(last_frame) = self.last_output_frame {
            let frames_since_last = self.frame_count - last_frame;
            if frames_since_last < frame_interval {
                return None;
            }
        }
        let sr = block.format.sample_rate as f32;
        let max_hz = self.config.max_hz.min(sr * 0.5);
        let min_hz = self.config.min_hz.max(0.0).min(max_hz.max(0.0));
        let sample_offset = self.mono_ring.len().saturating_sub(self.config.fft_size);
        for (i, c) in self.fft_buf.iter_mut().enumerate() {
            let s = self
                .mono_ring
                .get(sample_offset + i)
                .copied()
                .unwrap_or(0.0);
            let w = self.window[i];
            *c = Complex32::new(s * w, 0.0);
        }
        self.fft_plan.transform(&mut self.fft_buf);
        let target = aggregate_bars(
            &self.fft_buf,
            sr,
            min_hz,
            max_hz,
            self.config.bars,
            self.config.gamma,
            self.config.noise_floor,
        );
        let dt_s = 1.0f32 / self.config.update_hz.max(1e-3);
        let attack_tau = (self.config.attack_ms.max(0.1) / 1000.0).max(1e-4);
        let release_tau = (self.config.release_ms.max(0.1) / 1000.0).max(1e-4);
        for (i, t) in target.iter().copied().enumerate() {
            let p = self.prev_bars[i];
            let tau = if t >= p { attack_tau } else { release_tau };
            let a = 1.0 - (-dt_s / tau).exp();
            self.prev_bars[i] = (p + (t - p) * a).clamp(0.0, 1.0);
        }
        self.last_output_frame = Some(self.frame_count);
        Some(SpectrumFrame {
            bars: self.prev_bars.clone(),
        })
    }
}

fn push_block_mono(ring: &mut VecDeque<f32>, block: &AudioBlock) {
    let ch = block.format.channels as usize;
    if ch == 0 {
        return;
    }
    if ch == 1 {
        ring.extend(block.samples.iter().copied());
        return;
    }
    for frame in 0..block.frames {
        let base = frame * ch;
        if base + ch > block.samples.len() {
            break;
        }
        let mut sum = 0.0f32;
        for c in 0..ch {
            sum += block.samples[base + c];
        }
        ring.push_back(sum / ch as f32);
    }
}

fn build_window(n: usize, _w: WindowFunction) -> Vec<f32> {
    if n == 0 {
        return Vec::new();
    }
    let n1 = (n - 1).max(1) as f32;
    let tau = std::f32::consts::TAU;
    (0..n)
        .map(|i| {
            let x = i as f32 / n1;
            0.5 * (1.0 - (tau * x).cos())
        })
        .collect()
}

fn aggregate_bars(
    fft: &[Complex32],
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
    bars: usize,
    gamma: f32,
    noise_floor: f32,
) -> Vec<f32> {
    let n = fft.len();
    if n == 0 || bars == 0 || sample_rate <= 0.0 {
        return vec![0.0; bars];
    }
    let half = n / 2;
    let min_hz = min_hz.max(0.0);
    let max_hz = max_hz.max(min_hz);
    let ratio = if min_hz > 0.0 && max_hz > min_hz {
        max_hz / min_hz
    } else {
        1.0
    };
    let mut out = vec![0.0f32; bars];
    for (b, output) in out.iter_mut().enumerate() {
        let t0 = b as f32 / bars as f32;
        let t1 = (b + 1) as f32 / bars as f32;
        let f0 = if ratio > 1.0 {
            min_hz * ratio.powf(t0)
        } else {
            min_hz + (max_hz - min_hz) * t0
        };
        let f1 = if ratio > 1.0 {
            min_hz * ratio.powf(t1)
        } else {
            min_hz + (max_hz - min_hz) * t1
        };
        let mut i0 = ((f0 * n as f32) / sample_rate).ceil() as isize;
        let mut i1 = ((f1 * n as f32) / sample_rate).floor() as isize;
        i0 = i0.clamp(1, (half as isize).saturating_sub(1));
        i1 = i1.clamp(1, (half as isize).saturating_sub(1));
        if i1 < i0 {
            i1 = i0;
        }
        let mut peak = 0.0f32;
        for i in i0..=i1 {
            let c = fft[i as usize];
            let mag = (c.re * c.re + c.im * c.im).sqrt() / n as f32;
            if mag > peak {
                peak = mag;
            }
        }
        let v = (peak - noise_floor).max(0.0);
        *output = v.powf(gamma.max(0.01)).min(1.0);
    }
    out
}

#[derive(Clone, Copy, Debug, Default)]
struct Complex32 {
    re: f32,
    im: f32,
}

impl Complex32 {
    fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }
}

struct FftPlan {
    reversed: Vec<usize>,
    twiddles: Vec<Complex32>,
}

impl FftPlan {
    fn new(n: usize) -> Self {
        let bits = n.trailing_zeros();
        Self {
            reversed: (0..n)
                .map(|i| i.reverse_bits() >> (usize::BITS - bits))
                .collect(),
            twiddles: (0..n / 2)
                .map(|k| {
                    let angle = -std::f32::consts::TAU * k as f32 / n as f32;
                    Complex32::new(angle.cos(), angle.sin())
                })
                .collect(),
        }
    }

    fn transform(&self, buf: &mut [Complex32]) {
        let n = buf.len();
        debug_assert_eq!(n, self.reversed.len());
        for (i, &j) in self.reversed.iter().enumerate() {
            if j > i {
                buf.swap(i, j);
            }
        }
        let mut len = 2usize;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let w = self.twiddles[step * k];
                    let u = buf[start + k];
                    let t = mul(w, buf[start + k + half]);
                    buf[start + k] = add(u, t);
                    buf[start + k + half] = sub(u, t);
                }
            }
            len *= 2;
        }
    }
}

fn add(a: Complex32, b: Complex32) -> Complex32 {
    Complex32::new(a.re + b.re, a.im + b.im)
}

fn sub(a: Complex32, b: Complex32) -> Complex32 {
    Complex32::new(a.re - b.re, a.im - b.im)
}

fn mul(a: Complex32, b: Complex32) -> Complex32 {
    Complex32::new(a.re * b.re - a.im * b.im, a.re * b.im + a.im * b.re)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_fft_matches_direct_transform() {
        let n = 32;
        let original: Vec<_> = (0..n)
            .map(|i| Complex32::new((i as f32 * 0.7).sin(), 0.0))
            .collect();
        let mut actual = original.clone();
        let plan = FftPlan::new(n);
        plan.transform(&mut actual);
        for (k, actual) in actual.iter().enumerate() {
            let expected = original
                .iter()
                .enumerate()
                .fold(Complex32::default(), |sum, (i, x)| {
                    let angle = -std::f32::consts::TAU * (i * k) as f32 / n as f32;
                    add(sum, mul(*x, Complex32::new(angle.cos(), angle.sin())))
                });
            assert!((actual.re - expected.re).abs() < 0.0001);
            assert!((actual.im - expected.im).abs() < 0.0001);
        }
    }

    #[test]
    fn device_format_change_drops_previous_samples_and_smoothing() {
        let mut analyzer = SpectrumAnalyzer::new(SpectrumConfig::default()).unwrap();
        assert!(
            analyzer
                .process_block(&sine_block(1000.0, 48_000, 1024))
                .is_some()
        );
        let silence = AudioBlock {
            format: AudioFormat {
                sample_rate: 44_100,
                channels: 1,
            },
            samples: vec![0.0; 512],
            frames: 512,
        };
        assert!(analyzer.process_block(&silence).is_none());
        assert!(
            analyzer
                .process_block(&silence)
                .unwrap()
                .bars
                .iter()
                .all(|&v| v == 0.0)
        );
    }

    #[test]
    fn invalid_input_is_ignored_and_ring_stays_bounded_between_outputs() {
        let mut analyzer = SpectrumAnalyzer::new(SpectrumConfig {
            update_hz: 0.01,
            ..Default::default()
        })
        .unwrap();
        let mut block = sine_block(1000.0, 48_000, 1024);
        for _ in 0..100 {
            analyzer.process_block(&block);
        }
        assert!(analyzer.mono_ring.len() <= 2048);
        block.format.channels = 0;
        assert!(analyzer.process_block(&block).is_none());
        block.format.channels = 2;
        assert!(analyzer.process_block(&block).is_none());
        assert!(
            SpectrumAnalyzer::new(SpectrumConfig {
                update_hz: f32::INFINITY,
                ..Default::default()
            })
            .is_err()
        );
    }
    fn sine_block(freq: f32, sr: u32, frames: usize) -> AudioBlock {
        let mut samples = Vec::with_capacity(frames);
        for i in 0..frames {
            let t = i as f32 / sr as f32;
            samples.push((std::f32::consts::TAU * freq * t).sin());
        }
        AudioBlock {
            format: crate::audio::types::AudioFormat {
                sample_rate: sr,
                channels: 1,
            },
            samples,
            frames,
        }
    }

    #[test]
    fn fft_detects_tone_energy_in_expected_band() {
        let mut a = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: 1024,
            bars: 16,
            min_hz: 50.0,
            max_hz: 10_000.0,
            update_hz: 60.0,
            attack_ms: 1.0,
            release_ms: 1.0,
            window: WindowFunction::Hann,
            gamma: 0.55,
            noise_floor: 0.0,
        })
        .unwrap();
        let sr = 48_000;
        let block = sine_block(1000.0, sr, 1024);
        let frame = a.process_block(&block).unwrap();
        let (max_i, max_v) = frame
            .bars
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        assert!(max_v > 0.1);
        assert!(max_i > 0);
    }
}
