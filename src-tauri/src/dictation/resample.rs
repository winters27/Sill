//! Brings captured audio to the 16 kHz whisper.cpp requires.

use crate::dictation::error::DictationError;
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

/// The only input rate whisper.cpp accepts.
pub const TARGET_RATE: u32 = 16_000;

/// Frames per processing block. Only affects internal buffering, not the
/// result: `process_all_into_buffer` drives the whole clip through.
const CHUNK_FRAMES: usize = 1024;

const MONO: usize = 1;

/// Resamples mono `samples` from `from_rate` to [`TARGET_RATE`].
///
/// Audio already at the target rate is returned untouched, which is the
/// common case whenever the capture device can negotiate 16 kHz directly.
pub fn to_target_rate(samples: &[f32], from_rate: u32) -> Result<Vec<f32>, DictationError> {
    if from_rate == 0 {
        return Err(DictationError::Validation(
            "Cannot resample audio captured at 0 Hz".to_string(),
        ));
    }
    if from_rate == TARGET_RATE || samples.is_empty() {
        return Ok(samples.to_vec());
    }

    let mut resampler = Fft::<f32>::new(
        from_rate as usize,
        TARGET_RATE as usize,
        CHUNK_FRAMES,
        MONO,
        FixedSync::Both,
    )
    .map_err(|e| DictationError::Other(format!("Could not build a {from_rate} Hz resampler: {e}")))?;

    let needed = resampler.process_all_needed_output_len(samples.len());
    let mut out = vec![0.0f32; needed];

    let input = InterleavedSlice::new(samples, MONO, samples.len())
        .map_err(|e| DictationError::Other(format!("Invalid resampler input buffer: {e}")))?;
    let mut output = InterleavedSlice::new_mut(&mut out, MONO, needed)
        .map_err(|e| DictationError::Other(format!("Invalid resampler output buffer: {e}")))?;

    let (_read, written) = resampler
        .process_all_into_buffer(&input, &mut output, samples.len(), None)
        .map_err(|e| DictationError::Other(format!("Resampling to {TARGET_RATE} Hz failed: {e}")))?;

    out.truncate(written);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn tone(freq_hz: f32, rate: u32, seconds: f32) -> Vec<f32> {
        let count = (rate as f32 * seconds) as usize;
        (0..count)
            .map(|i| (TAU * freq_hz * i as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// Frequency of a roughly-sinusoidal signal, via zero crossings. Cheaper
    /// and less brittle here than an FFT, and enough to catch a resampler
    /// that shifts pitch (the classic wrong-ratio bug).
    fn dominant_freq_hz(samples: &[f32], rate: u32) -> f32 {
        let crossings = samples
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        crossings as f32 * rate as f32 / (2.0 * samples.len() as f32)
    }

    #[test]
    fn passes_through_untouched_when_already_at_the_target_rate() {
        let input = tone(440.0, TARGET_RATE, 0.05);

        let out = to_target_rate(&input, TARGET_RATE).unwrap();

        assert_eq!(out, input, "resampling 16k to 16k must not touch samples");
    }

    #[test]
    fn downsamples_48k_to_a_third_as_many_frames() {
        let input = tone(1_000.0, 48_000, 1.0);

        let out = to_target_rate(&input, 48_000).unwrap();

        let expected = 16_000_f32;
        let drift = (out.len() as f32 - expected).abs() / expected;
        assert!(
            drift < 0.02,
            "expected about {expected} frames, got {} ({:.1}% off)",
            out.len(),
            drift * 100.0
        );
    }

    #[test]
    fn downsamples_the_awkward_44_1k_ratio() {
        // 44100 to 16000 is 441:160, the non-integer ratio a naive
        // decimate-by-N cannot express at all.
        let input = tone(1_000.0, 44_100, 1.0);

        let out = to_target_rate(&input, 44_100).unwrap();

        let drift = (out.len() as f32 - 16_000.0).abs() / 16_000.0;
        assert!(drift < 0.02, "got {} frames", out.len());
    }

    #[test]
    fn preserves_the_pitch_of_a_speech_band_tone() {
        let input = tone(1_000.0, 48_000, 1.0);

        let out = to_target_rate(&input, 48_000).unwrap();

        let freq = dominant_freq_hz(&out, TARGET_RATE);
        assert!(
            (freq - 1_000.0).abs() < 20.0,
            "1 kHz must stay 1 kHz, got {freq:.0} Hz"
        );
    }

    #[test]
    fn preserves_the_level_of_a_speech_band_tone() {
        let input = tone(1_000.0, 48_000, 1.0);

        let out = to_target_rate(&input, 48_000).unwrap();

        let ratio = rms(&out) / rms(&input);
        assert!(
            (0.8..1.2).contains(&ratio),
            "level must survive resampling, RMS ratio was {ratio:.2}"
        );
    }

    #[test]
    fn attenuates_content_above_the_new_nyquist_instead_of_aliasing_it() {
        // THE test that earns the dependency. Downsampling 48k to 16k drops
        // Nyquist from 24 kHz to 8 kHz, so a 12 kHz tone has nowhere to go.
        // A filtered resampler removes it; a naive decimate-by-3 folds it
        // back to 4 kHz at nearly full amplitude, dropping a loud phantom
        // tone right in the middle of the speech band.
        let input = tone(12_000.0, 48_000, 1.0);

        let out = to_target_rate(&input, 48_000).unwrap();

        let ratio = rms(&out) / rms(&input);
        assert!(
            ratio < 0.2,
            "out-of-band tone must be attenuated, not aliased into the \
             speech band; RMS ratio was {ratio:.2}"
        );
    }

    #[test]
    fn rejects_a_zero_source_rate() {
        assert!(to_target_rate(&[0.0; 16], 0).is_err());
    }

    #[test]
    fn handles_an_empty_clip() {
        let out = to_target_rate(&[], 48_000).unwrap();

        assert!(out.is_empty());
    }
}
