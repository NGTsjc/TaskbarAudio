use crate::audio::types::AudioFormat;
use windows::Win32::Media::Audio::{WAVE_FORMAT_PCM, WAVEFORMATEX, WAVEFORMATEXTENSIBLE};
use windows::Win32::Media::KernelStreaming::KSDATAFORMAT_SUBTYPE_PCM;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SampleEncoding {
    Float32,
    Pcm16,
    Pcm24Packed,
    Pcm24In32 { valid_bits: u16 },
    Pcm32,
}

pub(super) unsafe fn parse_format(
    pwfx: *const WAVEFORMATEX,
) -> anyhow::Result<(AudioFormat, SampleEncoding, usize)> {
    anyhow::ensure!(!pwfx.is_null(), "null audio format");
    let wfex = unsafe { &*pwfx };
    anyhow::ensure!(
        wfex.nChannels > 0 && wfex.nSamplesPerSec > 0,
        "invalid channels or sample rate"
    );
    anyhow::ensure!(
        matches!(wfex.wBitsPerSample, 16 | 24 | 32),
        "unsupported sample container"
    );
    anyhow::ensure!(
        wfex.nBlockAlign as usize == wfex.nChannels as usize * (wfex.wBitsPerSample as usize / 8),
        "invalid audio block alignment"
    );
    const WAVE_FORMAT_IEEE_FLOAT: u32 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u32 = 0xFFFE;
    let format = AudioFormat {
        sample_rate: wfex.nSamplesPerSec,
        channels: wfex.nChannels,
    };
    let (encoding, block_align_bytes) = match wfex.wFormatTag as u32 {
        WAVE_FORMAT_IEEE_FLOAT => {
            anyhow::ensure!(
                wfex.wBitsPerSample == 32,
                "unexpected float bits per sample"
            );
            (SampleEncoding::Float32, wfex.nBlockAlign as usize)
        }
        WAVE_FORMAT_PCM => match wfex.wBitsPerSample {
            16 => (SampleEncoding::Pcm16, wfex.nBlockAlign as usize),
            24 => (SampleEncoding::Pcm24Packed, wfex.nBlockAlign as usize),
            32 => (SampleEncoding::Pcm32, wfex.nBlockAlign as usize),
            b => anyhow::bail!("unsupported PCM bits per sample: {b}"),
        },
        WAVE_FORMAT_EXTENSIBLE => {
            anyhow::ensure!(
                wfex.cbSize as usize
                    >= std::mem::size_of::<WAVEFORMATEXTENSIBLE>()
                        - std::mem::size_of::<WAVEFORMATEX>(),
                "truncated extensible format"
            );
            let wfext = unsafe { &*(pwfx as *const WAVEFORMATEXTENSIBLE) };
            let valid_bits = unsafe { wfext.Samples.wValidBitsPerSample };
            anyhow::ensure!(
                valid_bits > 0 && valid_bits <= wfex.wBitsPerSample,
                "invalid valid bits"
            );
            let sub = wfext.SubFormat;
            if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
                anyhow::ensure!(
                    wfex.wBitsPerSample == 32,
                    "unexpected extensible float bits"
                );
                (SampleEncoding::Float32, wfex.nBlockAlign as usize)
            } else if sub == KSDATAFORMAT_SUBTYPE_PCM {
                match wfex.wBitsPerSample {
                    16 => (SampleEncoding::Pcm16, wfex.nBlockAlign as usize),
                    24 => (SampleEncoding::Pcm24Packed, wfex.nBlockAlign as usize),
                    32 if valid_bits < 32 => (
                        SampleEncoding::Pcm24In32 { valid_bits },
                        wfex.nBlockAlign as usize,
                    ),
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

pub(super) unsafe fn decode_samples(
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
    debug_assert!(channels > 0);
    if bytes_len == 0 {
        return Vec::new();
    }
    // Decode through bytes: WASAPI owns the buffer and typed slice alignment is
    // unnecessary here. Also prevent non-finite float samples poisoning the FFT.
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, bytes_len) };
    match encoding {
        SampleEncoding::Float32 => bytes
            .chunks_exact(4)
            .map(|b| {
                let v = f32::from_le_bytes(b.try_into().unwrap());
                if v.is_finite() { v } else { 0.0 }
            })
            .collect(),
        SampleEncoding::Pcm16 => bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes(b.try_into().unwrap()) as f32 / 32768.0)
            .collect(),
        SampleEncoding::Pcm32 => bytes
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f32 / 2147483648.0)
            .collect(),
        SampleEncoding::Pcm24Packed => unsafe { decode_pcm24_packed(data_ptr, bytes_len) },
        SampleEncoding::Pcm24In32 { valid_bits } => unsafe {
            decode_pcm24_in32(data_ptr, sample_count, valid_bits)
        },
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
    let bytes = unsafe { std::slice::from_raw_parts(data_ptr, sample_count * 4) };
    let src = bytes
        .chunks_exact(4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let shift = 32u16.saturating_sub(valid_bits.min(32));
    if valid_bits >= 32 {
        return src.map(|v| (v as f32) / 2147483648.0).collect();
    }
    let shift = shift as i32;
    let scale = (1u32 << (valid_bits.max(1) - 1)) as f32;
    src.map(|v| {
        let v = v >> shift;
        (v as f32) / scale
    })
    .collect()
}

#[cfg(test)]
#[path = "sample_format_tests.rs"]
mod tests;
