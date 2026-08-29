//! Frames mono f32 samples as a 16-bit PCM WAV clip.
//!
//! Hand-written rather than pulled from a crate: the canonical PCM header is
//! 44 fixed bytes and this is the only WAV shape dictation ever emits.

/// Size of the canonical PCM WAV header this module writes.
pub const WAV_HEADER_BYTES: usize = 44;

const PCM_FORMAT_TAG: u16 = 1;
const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const BLOCK_ALIGN: u16 = CHANNELS * BITS_PER_SAMPLE / 8;

/// Encodes `samples` as a mono 16-bit PCM WAV at `sample_rate`.
///
/// `sample_rate` is a parameter rather than a hardcoded 16 kHz on purpose: a
/// header that misreports the rate does not fail, it transcribes at the
/// wrong speed, so the caller states what it actually captured.
pub fn encode_mono_16bit(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = samples.len() * BLOCK_ALIGN as usize;
    let mut out = Vec::with_capacity(WAV_HEADER_BYTES + data_len);

    out.extend_from_slice(b"RIFF");
    // Everything after this field, i.e. the header's remaining 36 bytes
    // plus the samples.
    out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&PCM_FORMAT_TAG.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * BLOCK_ALIGN as u32).to_le_bytes());
    out.extend_from_slice(&BLOCK_ALIGN.to_le_bytes());
    out.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &sample in samples {
        out.extend_from_slice(&to_i16(sample).to_le_bytes());
    }

    out
}

/// Clamping conversion to 16-bit PCM.
///
/// Clamping before scaling is what makes a clipped capture saturate instead
/// of wrapping. NaN survives `clamp` as NaN and the saturating float-to-int
/// cast turns it into 0, so a non-finite sample lands as silence.
fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn read_u16(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn read_i16(bytes: &[u8], at: usize) -> i16 {
        i16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    #[test]
    fn encode_writes_the_canonical_chunk_magics() {
        let wav = encode_mono_16bit(&[0.0; 4], 16_000);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }

    #[test]
    fn encode_declares_16_bit_mono_pcm_at_the_requested_rate() {
        let wav = encode_mono_16bit(&[0.0; 8], 16_000);

        assert_eq!(read_u32(&wav, 16), 16, "PCM fmt chunks are 16 bytes");
        assert_eq!(read_u16(&wav, 20), 1, "format 1 is uncompressed PCM");
        assert_eq!(read_u16(&wav, 22), 1, "mono");
        assert_eq!(read_u32(&wav, 24), 16_000, "sample rate");
        assert_eq!(read_u32(&wav, 28), 32_000, "byte rate = rate * blockAlign");
        assert_eq!(read_u16(&wav, 32), 2, "block align = channels * bytes");
        assert_eq!(read_u16(&wav, 34), 16, "bits per sample");
    }

    #[test]
    fn encode_reports_a_rate_other_than_16k_honestly() {
        // The encoder must never claim 16 kHz for audio that is not: a WAV
        // whose header lies about its rate transcribes as gibberish at the
        // wrong speed rather than failing loudly.
        let wav = encode_mono_16bit(&[0.0; 8], 44_100);

        assert_eq!(read_u32(&wav, 24), 44_100);
        assert_eq!(read_u32(&wav, 28), 88_200);
    }

    #[test]
    fn encode_sizes_are_header_plus_two_bytes_per_sample() {
        let wav = encode_mono_16bit(&[0.0; 100], 16_000);

        assert_eq!(wav.len(), WAV_HEADER_BYTES + 200);
        assert_eq!(read_u32(&wav, 40), 200, "data chunk size");
        assert_eq!(
            read_u32(&wav, 4),
            36 + 200,
            "RIFF size excludes the first 8"
        );
    }

    #[test]
    fn encode_handles_an_empty_clip() {
        let wav = encode_mono_16bit(&[], 16_000);

        assert_eq!(wav.len(), WAV_HEADER_BYTES);
        assert_eq!(read_u32(&wav, 40), 0);
        assert_eq!(read_u32(&wav, 4), 36);
    }

    #[test]
    fn encode_scales_full_scale_samples_symmetrically() {
        let wav = encode_mono_16bit(&[1.0, -1.0, 0.0], 16_000);

        assert_eq!(read_i16(&wav, 44), 32_767);
        assert_eq!(read_i16(&wav, 46), -32_767);
        assert_eq!(read_i16(&wav, 48), 0);
    }

    #[test]
    fn encode_clamps_out_of_range_samples_instead_of_wrapping() {
        // A clipped capture must saturate. Wrapping would turn a loud
        // syllable into a full-scale sign flip, which sounds like a click
        // and derails transcription.
        let wav = encode_mono_16bit(&[2.5, -2.5], 16_000);

        assert_eq!(read_i16(&wav, 44), 32_767);
        assert_eq!(read_i16(&wav, 46), -32_767);
    }

    #[test]
    fn encode_maps_non_finite_samples_to_silence() {
        let wav = encode_mono_16bit(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY], 16_000);

        assert_eq!(read_i16(&wav, 44), 0, "NaN is silence, not a panic");
        assert_eq!(read_i16(&wav, 46), 32_767);
        assert_eq!(read_i16(&wav, 48), -32_767);
    }
}
