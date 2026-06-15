use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    WAVE_FORMAT_PCM,
};
use windows::Win32::Media::Audio::{eConsole, eRender, MMDeviceEnumerator};
use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use crate::audio::queue::AudioQueue;
use crate::audio::types::{AudioBlock, AudioFormat};

#[derive(Clone, Copy, Debug)]
pub struct LoopbackCaptureConfig {
    pub block_frames: usize,
    pub queue_capacity: usize,
    pub no_data_fill_interval: Duration,
}

impl Default for LoopbackCaptureConfig {
    fn default() -> Self {
        Self {
            block_frames: 512,
            queue_capacity: 4,
            no_data_fill_interval: Duration::from_millis(10),
        }
    }
}

pub struct LoopbackCapture {
    queue: AudioQueue,
    stop: Arc<AtomicBool>,
    restart: Arc<AtomicBool>,
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
        let queue = AudioQueue::new(config.queue_capacity);
        let stop = Arc::new(AtomicBool::new(false));
        let restart = Arc::new(AtomicBool::new(false));
        
        let queue_clone = queue.clone();
        let stop_clone = stop.clone();
        let restart_clone = restart.clone();
        
        let join = std::thread::spawn(move || {
            Self::capture_loop(config, queue_clone, stop_clone, restart_clone);
        });
        
        let stop_clone2 = stop.clone();
        let restart_clone2 = restart.clone();
        let notification_join = std::thread::spawn(move || {
            Self::poll_device_change(restart_clone2, stop_clone2);
        });
        
        Ok(Self {
            queue,
            stop,
            restart,
            join: Some(join),
            notification_join: Some(notification_join),
        })
    }

    fn poll_device_change(
        restart: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
    ) {
        let com_initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let mut last_device_id = String::new();
        
        while !stop.load(Ordering::Relaxed) {
            let device_id = match Self::get_default_device_id() {
                Ok(id) => id,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
            };
            
            if !last_device_id.is_empty() && device_id != last_device_id {
                tracing::info!(old_device = %last_device_id, new_device = %device_id, "default audio device changed");
                restart.store(true, Ordering::Relaxed);
            }
            
            last_device_id = device_id;
            std::thread::sleep(Duration::from_millis(500));
        }
        
        if com_initialized {
            unsafe { CoUninitialize() };
        }
    }

    fn get_default_device_id() -> anyhow::Result<String> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
        let id = unsafe { device.GetId()? };
        let id_str = unsafe { id.to_string().map_err(|e| anyhow::anyhow!("failed to convert device id: {}", e))? };
        Ok(id_str)
    }

    fn capture_loop(
        config: LoopbackCaptureConfig,
        queue: AudioQueue,
        stop: Arc<AtomicBool>,
        restart: Arc<AtomicBool>,
    ) {
        while !stop.load(Ordering::Relaxed) {
            let com_initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
            
            match Self::capture_thread_inner(config, queue.clone(), stop.clone(), restart.clone()) {
                Ok(_) => {}
                Err(err) => {
                    if !stop.load(Ordering::Relaxed) {
                        tracing::warn!(error = %err, "capture thread failed, restarting...");
                    }
                }
            }
            
            if com_initialized {
                unsafe { CoUninitialize() };
            }
            
            if stop.load(Ordering::Relaxed) {
                break;
            }
            
            std::thread::sleep(Duration::from_millis(100));
            restart.store(false, Ordering::Relaxed);
        }
    }

    fn capture_thread_inner(
        config: LoopbackCaptureConfig,
        queue: AudioQueue,
        stop: Arc<AtomicBool>,
        restart: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
        
        let device_id = unsafe { device.GetId()? };
        let device_id_str = unsafe { device_id.to_string().unwrap_or_else(|_| "unknown".to_string()) };
        
        tracing::info!(device_id = %device_id_str, "started capturing from default audio device");
        
        let audio_client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None)? };
        let pwfx = unsafe { audio_client.GetMixFormat()? };
        anyhow::ensure!(!pwfx.is_null(), "GetMixFormat returned null");
        let (format, encoding, block_align_bytes) = unsafe { parse_format(pwfx)? };
        let hns_buffer_duration: i64 = 10_000_000 / 10;
        unsafe {
            audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                hns_buffer_duration,
                0,
                pwfx,
                None,
            )?;
        }
        unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(pwfx as _)) };
        let capture_client: IAudioCaptureClient = unsafe { audio_client.GetService()? };
        unsafe { audio_client.Start()? };
        let mut last_fill = Instant::now();
        let mut accum: Vec<f32> = Vec::with_capacity(config.block_frames * format.channels as usize);
        
        while !stop.load(Ordering::Relaxed) {
            if restart.load(Ordering::Relaxed) {
                tracing::info!("device change detected, restarting capture");
                break;
            }
            
            let hr = unsafe { capture_client.GetNextPacketSize() };
            if hr.is_err() {
                tracing::warn!("GetNextPacketSize failed, restarting capture");
                break;
            }
            
            let mut packet_frames = hr.unwrap();
            
            if packet_frames == 0 {
                let now = Instant::now();
                if now.duration_since(last_fill) >= config.no_data_fill_interval {
                    let samples_len = config.block_frames * format.channels as usize;
                    let samples = vec![0.0; samples_len];
                    queue.push(AudioBlock {
                        format,
                        samples,
                        frames: config.block_frames,
                    });
                    last_fill = now;
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
                continue;
            }
            
            while packet_frames > 0 && !stop.load(Ordering::Relaxed) {
                if restart.load(Ordering::Relaxed) {
                    tracing::info!("device change detected during capture, stopping");
                    break;
                }
                
                let mut data_ptr: *mut u8 = std::ptr::null_mut();
                let mut num_frames = 0u32;
                let mut flags = 0u32;
                
                let hr = unsafe {
                    capture_client.GetBuffer(
                        &mut data_ptr,
                        &mut num_frames,
                        &mut flags,
                        None,
                        None,
                    )
                };
                
                if hr.is_err() {
                    tracing::warn!("GetBuffer failed, restarting capture");
                    break;
                }
                
                let samples = if (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0 {
                    vec![0.0; num_frames as usize * format.channels as usize]
                } else {
                    let bytes_len = num_frames as usize * block_align_bytes;
                    unsafe { decode_samples(data_ptr, bytes_len, format.channels, encoding) }
                };
                
                let hr = unsafe { capture_client.ReleaseBuffer(num_frames) };
                if hr.is_err() {
                    tracing::warn!("ReleaseBuffer failed, restarting capture");
                    break;
                }
                
                accum.extend_from_slice(&samples);
                
                while accum.len() >= config.block_frames * format.channels as usize {
                    let take = config.block_frames * format.channels as usize;
                    let block_samples = accum.drain(0..take).collect::<Vec<_>>();
                    queue.push(AudioBlock {
                        format,
                        samples: block_samples,
                        frames: config.block_frames,
                    });
                    last_fill = Instant::now();
                }
                
                let hr = unsafe { capture_client.GetNextPacketSize() };
                if hr.is_err() {
                    tracing::warn!("GetNextPacketSize failed during loop, restarting capture");
                    break;
                }
                packet_frames = hr.unwrap();
            }
            
            if restart.load(Ordering::Relaxed) {
                break;
            }
        }
        
        unsafe {
            let _ = audio_client.Stop();
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
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        if let Some(join) = self.notification_join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum SampleEncoding {
    Float32,
    Pcm16,
    Pcm24Packed,
    Pcm24In32 { valid_bits: u16 },
    Pcm32,
}

unsafe fn parse_format(
    pwfx: *const WAVEFORMATEX,
) -> anyhow::Result<(AudioFormat, SampleEncoding, usize)> {
    let wfex = unsafe { &*pwfx };
    const WAVE_FORMAT_IEEE_FLOAT: u32 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u32 = 0xFFFE;
    let format = AudioFormat {
        sample_rate: wfex.nSamplesPerSec,
        channels: wfex.nChannels,
    };
    let (encoding, block_align_bytes) = match wfex.wFormatTag as u32 {
        WAVE_FORMAT_IEEE_FLOAT => {
            anyhow::ensure!(wfex.wBitsPerSample == 32, "unexpected float bits per sample");
            (SampleEncoding::Float32, wfex.nBlockAlign as usize)
        }
        WAVE_FORMAT_PCM => match wfex.wBitsPerSample {
            16 => (SampleEncoding::Pcm16, wfex.nBlockAlign as usize),
            24 => (SampleEncoding::Pcm24Packed, wfex.nBlockAlign as usize),
            32 => (SampleEncoding::Pcm32, wfex.nBlockAlign as usize),
            b => anyhow::bail!("unsupported PCM bits per sample: {b}"),
        },
        WAVE_FORMAT_EXTENSIBLE => {
            let wfext = unsafe { &*(pwfx as *const WAVEFORMATEXTENSIBLE) };
            let valid_bits = unsafe { wfext.Samples.wValidBitsPerSample };
            let sub = wfext.SubFormat;
            if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                anyhow::ensure!(wfex.wBitsPerSample == 32, "unexpected extensible float bits");
                (SampleEncoding::Float32, wfex.nBlockAlign as usize)
            } else if sub == KSDATAFORMAT_SUBTYPE_PCM {
                match wfex.wBitsPerSample {
                    16 => (SampleEncoding::Pcm16, wfex.nBlockAlign as usize),
                    24 => {
                        let bytes_per_sample = wfex.nBlockAlign as usize / wfex.nChannels as usize;
                        if bytes_per_sample == 3 {
                            (SampleEncoding::Pcm24Packed, wfex.nBlockAlign as usize)
                        } else {
                            (SampleEncoding::Pcm24In32 { valid_bits }, wfex.nBlockAlign as usize)
                        }
                    }
                    32 => (SampleEncoding::Pcm32, wfex.nBlockAlign as usize),
                    b => anyhow::bail!("unsupported extensible PCM bits per sample: {b}"),
                }
            } else {
                anyhow::bail!("unsupported extensible subformat");
            }
        }
        tag => anyhow::bail!("unsupported wFormatTag: {tag}"),
    };
    Ok((format, encoding, block_align_bytes))
}

unsafe fn decode_samples(
    data_ptr: *const u8,
    bytes_len: usize,
    channels: u16,
    encoding: SampleEncoding,
) -> Vec<f32> {
    let bytes_per_sample = match encoding {
        SampleEncoding::Float32 => 4,
        SampleEncoding::Pcm16 => 2,
        SampleEncoding::Pcm24Packed => 3,
        SampleEncoding::Pcm24In32 { .. } => 4,
        SampleEncoding::Pcm32 => 4,
    };
    let sample_count = bytes_len / bytes_per_sample;
    let _frames = sample_count / channels as usize;
    match encoding {
        SampleEncoding::Float32 => {
            let src = unsafe { std::slice::from_raw_parts(data_ptr as *const f32, sample_count) };
            src.to_vec()
        }
        SampleEncoding::Pcm16 => {
            let src = unsafe { std::slice::from_raw_parts(data_ptr as *const i16, sample_count) };
            src.iter().map(|&v| (v as f32) / 32768.0).collect()
        }
        SampleEncoding::Pcm32 => {
            let src = unsafe { std::slice::from_raw_parts(data_ptr as *const i32, sample_count) };
            src.iter().map(|&v| (v as f32) / 2147483648.0).collect()
        }
        SampleEncoding::Pcm24Packed => unsafe { decode_pcm24_packed(data_ptr, bytes_len) },
        SampleEncoding::Pcm24In32 { valid_bits } => unsafe { decode_pcm24_in32(data_ptr, sample_count, valid_bits) },
    }
}

unsafe fn decode_pcm24_packed(data_ptr: *const u8, bytes_len: usize) -> Vec<f32> {
    let src = unsafe { std::slice::from_raw_parts(data_ptr, bytes_len) };
    let mut out = Vec::with_capacity(bytes_len / 3);
    for chunk in src.chunks_exact(3) {
        let mut v = (chunk[0] as i32) | ((chunk[1] as i32) << 8) | ((chunk[2] as i32) << 16);
        if (v & 0x800000) != 0 {
            v |= !0xFFFFFF;
        }
        out.push((v as f32) / 8388608.0);
    }
    out
}

unsafe fn decode_pcm24_in32(data_ptr: *const u8, sample_count: usize, valid_bits: u16) -> Vec<f32> {
    let src = unsafe { std::slice::from_raw_parts(data_ptr as *const i32, sample_count) };
    let shift = 32u16.saturating_sub(valid_bits.min(32));
    if valid_bits >= 32 {
        return src.iter().map(|&v| (v as f32) / 2147483648.0).collect();
    }
    let shift = shift as i32;
    let scale = (1u32 << (valid_bits.max(1) - 1)) as f32;
    src.iter()
        .map(|&v| {
            let v = v >> shift;
            (v as f32) / scale
        })
        .collect()
}
