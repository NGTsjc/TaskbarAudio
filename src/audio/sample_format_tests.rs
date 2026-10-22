use super::*;

fn pcm(bits: u16) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: 48_000,
        nAvgBytesPerSec: 48_000 * 2 * bits as u32 / 8,
        nBlockAlign: 2 * bits / 8,
        wBitsPerSample: bits,
        cbSize: 0,
    }
}

#[test]
fn rejects_invalid_format_before_reading_extended_fields() {
    for format in [
        WAVEFORMATEX {
            nChannels: 0,
            ..pcm(16)
        },
        WAVEFORMATEX {
            nSamplesPerSec: 0,
            ..pcm(16)
        },
        WAVEFORMATEX {
            nBlockAlign: 3,
            ..pcm(16)
        },
        WAVEFORMATEX {
            wFormatTag: 0xfffe,
            ..pcm(32)
        },
    ] {
        assert!(unsafe { parse_format(&format) }.is_err());
    }
    assert!(unsafe { parse_format(std::ptr::null()) }.is_err());
}

#[test]
fn decodes_pcm_extremes_and_unaligned_buffers() {
    for (bits, bytes) in [
        (16, vec![0, 0x80, 0, 0, 0, 0x40]),
        (24, vec![0, 0, 0x80, 0, 0, 0, 0, 0, 0x40]),
        (32, vec![0, 0, 0, 0x80, 0, 0, 0, 0, 0, 0, 0, 0x40]),
    ] {
        let (_, encoding, _) = unsafe { parse_format(&pcm(bits)) }.unwrap();
        let mut unaligned = vec![0xff];
        unaligned.extend(&bytes);
        let result = unsafe { decode_samples(unaligned.as_ptr().add(1), bytes.len(), 1, encoding) };
        assert_eq!(result, vec![-1.0, 0.0, 0.5]);
    }
}

#[test]
fn float_decoder_removes_non_finite_values() {
    let samples = [-0.5_f32, 0.25, f32::NAN, f32::INFINITY];
    let bytes: Vec<_> = samples.into_iter().flat_map(f32::to_le_bytes).collect();
    assert_eq!(
        unsafe { decode_samples(bytes.as_ptr(), bytes.len(), 1, SampleEncoding::Float32) },
        vec![-0.5, 0.25, 0.0, 0.0]
    );
}

#[test]
fn extensible_pcm_24_bits_in_32_bit_container_is_left_aligned() {
    let mut format = WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: 0xfffe,
            cbSize: 22,
            ..pcm(32)
        },
        SubFormat: KSDATAFORMAT_SUBTYPE_PCM,
        ..Default::default()
    };
    format.Samples.wValidBitsPerSample = 24;
    let (_, encoding, align) = unsafe { parse_format(&format.Format) }.unwrap();
    assert_eq!(encoding, SampleEncoding::Pcm24In32 { valid_bits: 24 });
    assert_eq!(align, 8);
    let bytes: Vec<_> = [i32::MIN, 0, 1 << 30]
        .into_iter()
        .flat_map(i32::to_le_bytes)
        .collect();
    assert_eq!(
        unsafe { decode_samples(bytes.as_ptr(), bytes.len(), 1, encoding) },
        vec![-1.0, 0.0, 0.5]
    );
    format.Samples.wValidBitsPerSample = 33;
    assert!(unsafe { parse_format(&format.Format) }.is_err());
}
