use crate::audio::com::{Apartment, ComAllocation};
use crate::audio::mixer::Mixer;
use crate::audio::queue::AudioQueue;
use crate::audio::sample_format::{SampleEncoding, decode_samples, parse_format};
use crate::audio::types::{AudioBlock, AudioFormat};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

#[derive(Clone, Copy, Debug)]
pub struct LoopbackCaptureConfig {
    pub block_frames: usize,
    pub queue_capacity: usize,
    pub no_data_fill_interval: Duration,
    pub environment_volume: bool,
}

impl Default for LoopbackCaptureConfig {
    fn default() -> Self {
        Self {
            block_frames: 512,
            queue_capacity: 4,
            no_data_fill_interval: Duration::from_millis(10),
            environment_volume: false,
        }
    }
}

pub struct LoopbackCapture {
    queue: AudioQueue,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    notification_join: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct AudioStream {
    queue: AudioQueue,
}

impl AudioStream {
    pub fn pop_latest(&self) -> Option<AudioBlock> {
        self.queue.pop_latest()
    }

    pub fn queue(&self) -> &AudioQueue {
        &self.queue
    }
}

impl LoopbackCapture {
    pub fn start_default_loopback(config: LoopbackCaptureConfig) -> anyhow::Result<Self> {
        anyhow::ensure!(config.block_frames > 0, "block_frames must be positive");
        anyhow::ensure!(config.queue_capacity > 0, "queue_capacity must be positive");
        let mut capture = Self {
            queue: AudioQueue::new(config.queue_capacity),
            stop: Arc::new(AtomicBool::new(false)),
            join: None,
            notification_join: None,
        };
        let restart = Arc::new(AtomicBool::new(false));
        let queue = capture.queue.clone();
        let stop = capture.stop.clone();
        let restart_capture = restart.clone();
        capture.join = Some(
            std::thread::Builder::new()
                .name("audio-capture".into())
                .spawn(move || Self::capture_loop(config, queue, stop, restart_capture))?,
        );
        let stop = capture.stop.clone();
        // If the second spawn fails, Drop stops and joins the first thread.
        capture.notification_join = Some(
            std::thread::Builder::new()
                .name("audio-device-poll".into())
                .spawn(move || {
                    Self::poll_device_change(restart, stop, config.environment_volume)
                })?,
        );
        Ok(capture)
    }

    fn poll_device_change(restart: Arc<AtomicBool>, stop: Arc<AtomicBool>, mixed: bool) {
        let _apartment = match Apartment::new() {
            Ok(apartment) => apartment,
            Err(error) => {
                tracing::warn!(%error, "device polling COM initialization failed");
                return;
            }
        };
        let mut previous = None;
        while !stop.load(Ordering::Relaxed) {
            // Track both endpoints in mixed mode, including removal/reconnection.
            if let Ok(devices) = default_device_ids(mixed) {
                if previous.as_ref().is_some_and(|old| old != &devices) {
                    tracing::info!("default audio devices changed");
                    restart.store(true, Ordering::Relaxed);
                }
                previous = Some(devices);
            }
            std::thread::park_timeout(Duration::from_millis(500));
        }
        tracing::debug!("audio device polling stopped");
    }

    fn capture_loop(
        config: LoopbackCaptureConfig,
        queue: AudioQueue,
        stop: Arc<AtomicBool>,
        restart: Arc<AtomicBool>,
    ) {
        while !stop.load(Ordering::Relaxed) {
            restart.store(false, Ordering::Relaxed);
            let result = (|| {
                let _apartment = Apartment::new()?;
                Self::capture_session(config, &queue, &stop, &restart)
            })();
            if let Err(error) = result
                && !stop.load(Ordering::Relaxed)
            {
                tracing::warn!(%error, "capture failed; retrying with current default devices");
            }
            if !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_millis(100));
            }
        }
        tracing::debug!("audio capture stopped");
    }

    fn capture_session(
        config: LoopbackCaptureConfig,
        queue: &AudioQueue,
        stop: &AtomicBool,
        restart: &AtomicBool,
    ) -> anyhow::Result<()> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let render_device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
        let render_client: IAudioClient = unsafe { render_device.Activate(CLSCTX_ALL, None)? };
        let mix_format = ComAllocation(unsafe { render_client.GetMixFormat()? });
        anyhow::ensure!(!mix_format.0.is_null(), "GetMixFormat returned null");
        let (format, encoding, block_align) = unsafe { parse_format(mix_format.0)? };
        let render = CaptureEndpoint::new(
            render_client,
            mix_format.0,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            format,
            encoding,
            block_align,
        )?;
        let mic = if config.environment_volume {
            let device = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)? };
            let client = unsafe { device.Activate(CLSCTX_ALL, None)? };
            // Windows converts microphone rate, channels and encoding to the
            // render format. Never interpret native microphone bytes as speaker bytes.
            Some(CaptureEndpoint::new(
                client,
                mix_format.0,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                format,
                encoding,
                block_align,
            )?)
        } else {
            None
        };
        drop(mix_format);
        let block_samples = config
            .block_frames
            .checked_mul(format.channels as usize)
            .ok_or_else(|| anyhow::anyhow!("audio block is too large"))?;
        let fill_interval = config.no_data_fill_interval.max(Duration::from_secs_f64(
            config.block_frames as f64 / format.sample_rate as f64,
        ));
        let mut last_output = Instant::now();
        let started = last_output;
        let mut accum = std::collections::VecDeque::new();
        let mut mixer = Mixer::new(config.block_frames, format.channels, format.sample_rate);
        tracing::info!(device = %device_id(&render_device)?, mixed = mic.is_some(),
            sample_rate = format.sample_rate, channels = format.channels, "audio capture started");
        while !stop.load(Ordering::Relaxed) && !restart.load(Ordering::Relaxed) {
            let render_samples = render.read_available(stop, restart)?;
            if let Some(mic) = &mic {
                mixer.push(0, render_samples);
                mixer.push(1, mic.read_available(stop, restart)?);
                for _ in 0..config.queue_capacity {
                    let Some(samples) = mixer.pop(started.elapsed()) else {
                        break;
                    };
                    queue.push(AudioBlock {
                        format,
                        samples,
                        frames: config.block_frames,
                    });
                    if stop.load(Ordering::Relaxed) || restart.load(Ordering::Relaxed) {
                        break;
                    }
                }
            } else {
                accum.extend(render_samples);
                while accum.len() >= block_samples {
                    let samples = accum.drain(..block_samples).collect();
                    queue.push(AudioBlock {
                        format,
                        samples,
                        frames: config.block_frames,
                    });
                    last_output = Instant::now();
                }
                if last_output.elapsed() >= fill_interval {
                    // Flush any partial packet before padding the missing frames.
                    let mut samples: Vec<_> = accum.drain(..).collect();
                    samples.resize(block_samples, 0.0);
                    queue.push(AudioBlock {
                        format,
                        samples,
                        frames: config.block_frames,
                    });
                    last_output = Instant::now();
                }
            }
            std::thread::park_timeout(Duration::from_millis(1));
        }
        Ok(())
    }

    pub fn stream(&self) -> AudioStream {
        AudioStream {
            queue: self.queue.clone(),
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for join in [&self.join, &self.notification_join].into_iter().flatten() {
            join.thread().unpark();
        }
        for join in [self.join.take(), self.notification_join.take()]
            .into_iter()
            .flatten()
        {
            if join.join().is_err() {
                tracing::warn!("audio worker panicked during shutdown");
            }
        }
    }
}

fn device_id(device: &IMMDevice) -> anyhow::Result<String> {
    let id = unsafe { device.GetId()? };
    let _allocation = ComAllocation(id.0);
    Ok(unsafe { id.to_string()? })
}

fn default_device_ids(mixed: bool) -> anyhow::Result<(Option<String>, Option<String>)> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let render = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .ok()
        .and_then(|d| device_id(&d).ok());
    let mic = if mixed {
        unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
            .ok()
            .and_then(|d| device_id(&d).ok())
    } else {
        None
    };
    Ok((render, mic))
}

// All COM resources are released before the enclosing apartment guard.
struct CaptureEndpoint {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    format: AudioFormat,
    encoding: SampleEncoding,
    block_align: usize,
}

impl CaptureEndpoint {
    fn new(
        client: IAudioClient,
        format_ptr: *const WAVEFORMATEX,
        flags: u32,
        format: AudioFormat,
        encoding: SampleEncoding,
        block_align: usize,
    ) -> anyhow::Result<Self> {
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                flags,
                1_000_000,
                0,
                format_ptr,
                None,
            )?
        };
        let capture = unsafe { client.GetService()? };
        let endpoint = Self {
            client,
            capture,
            format,
            encoding,
            block_align,
        };
        unsafe { endpoint.client.Start()? };
        Ok(endpoint)
    }

    fn read_available(&self, stop: &AtomicBool, restart: &AtomicBool) -> anyhow::Result<Vec<f32>> {
        let mut samples = Vec::new();
        while !stop.load(Ordering::Relaxed)
            && !restart.load(Ordering::Relaxed)
            && unsafe { self.capture.GetNextPacketSize()? } > 0
        {
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            unsafe {
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?
            };
            if frames == 0 {
                break;
            }
            let decoded = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                Ok(vec![0.0; frames as usize * self.format.channels as usize])
            } else if data.is_null() {
                Err(anyhow::anyhow!("capture returned a null non-silent buffer"))
            } else {
                Ok(unsafe {
                    decode_samples(
                        data,
                        frames as usize * self.block_align,
                        self.format.channels,
                        self.encoding,
                    )
                })
            };
            // Release even if decoding rejects a packet; propagate device failures
            // so the session is recreated instead of silently looping on a dead client.
            unsafe { self.capture.ReleaseBuffer(frames)? };
            samples.extend(decoded?);
        }
        Ok(samples)
    }
}

impl Drop for CaptureEndpoint {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires live Windows render and microphone endpoints"]
    fn live_system_and_mixed_capture_restart() {
        let _apartment = Apartment::new().unwrap();
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.unwrap();
        for (name, flow) in [("render", eRender), ("microphone", eCapture)] {
            let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.unwrap();
            let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.unwrap();
            let native = ComAllocation(unsafe { client.GetMixFormat() }.unwrap());
            println!(
                "{name} native format: {:?}",
                unsafe { parse_format(native.0) }.unwrap()
            );
        }
        // Exercise conversion even when the current devices happen to share a
        // native format. This changes only our stream, not Windows device settings.
        let converted_format = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 1,
            nSamplesPerSec: 44_100,
            nAvgBytesPerSec: 44_100 * 4,
            nBlockAlign: 4,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }.unwrap();
        let client = unsafe { device.Activate(CLSCTX_ALL, None) }.unwrap();
        let (format, encoding, align) = unsafe { parse_format(&converted_format) }.unwrap();
        let converted = CaptureEndpoint::new(
            client,
            &converted_format,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            format,
            encoding,
            align,
        )
        .unwrap();
        let stop = AtomicBool::new(false);
        let start = Instant::now();
        loop {
            let samples = converted.read_available(&stop, &stop).unwrap();
            if !samples.is_empty() {
                assert!(samples.iter().all(|v| v.is_finite()));
                println!("microphone conversion to mono 44100 Hz succeeded");
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "conversion produced no packets"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(converted);
        for mixed in [false, true] {
            for cycle in 0..2 {
                let capture = LoopbackCapture::start_default_loopback(LoopbackCaptureConfig {
                    environment_volume: mixed,
                    ..Default::default()
                })
                .unwrap();
                let stream = capture.stream();
                let start = Instant::now();
                let block = loop {
                    if let Some(block) = stream.pop_latest() {
                        break block;
                    }
                    assert!(
                        start.elapsed() < Duration::from_secs(5),
                        "no audio packets: mixed={mixed}, cycle={cycle}"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                };
                assert!(block.format.sample_rate > 0 && block.format.channels > 0);
                assert_eq!(
                    block.samples.len(),
                    block.frames * block.format.channels as usize
                );
                assert!(block.samples.iter().all(|sample| sample.is_finite()));
                println!(
                    "mixed={mixed}, cycle={cycle}, format={:?}, first_packet_ms={}",
                    block.format,
                    start.elapsed().as_millis()
                );
                drop(capture);
            }
        }
    }

    #[test]
    fn invalid_capture_sizes_return_errors_without_spawning() {
        for config in [
            LoopbackCaptureConfig {
                block_frames: 0,
                ..Default::default()
            },
            LoopbackCaptureConfig {
                queue_capacity: 0,
                ..Default::default()
            },
        ] {
            assert!(LoopbackCapture::start_default_loopback(config).is_err());
        }
    }

    #[test]
    fn shutdown_wakes_and_joins_waiting_workers() {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel();
        let joins: Vec<_> = (0..2)
            .map(|_| {
                let stop = stop.clone();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    tx.send(()).unwrap();
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::park_timeout(Duration::from_secs(60));
                    }
                })
            })
            .collect();
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let mut joins = joins.into_iter();
        let capture = LoopbackCapture {
            queue: AudioQueue::new(1),
            stop,
            join: joins.next(),
            notification_join: joins.next(),
        };
        let start = Instant::now();
        drop(capture);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
