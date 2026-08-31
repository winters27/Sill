//! Band energies for the listening waveform.
//!
//! A port of Neve's `DictationBands` (`app/src/main/java/app/winters/keys/
//! input/DictationBands.kt`), so this surface and the keyboard's react to
//! speech the same way. Same author, so the lift is clean.
//!
//! The point, as written there: a single loudness number makes every dot
//! move together, which reads as a pulsing meter rather than a voice. So the
//! window is transformed and split into bands, and each dot tracks one part
//! of the spectrum.
//!
//! A 512-sample window at 16 kHz is 32 ms and 256 usable bins at 31.25 Hz
//! each. Bands are spaced logarithmically across the range speech actually
//! occupies, because linear spacing puts most of the dots in high
//! frequencies that barely move while talking.

use std::f32::consts::PI;

/// Dots in the waveform. Fine-grained on purpose: a coarse row reads as a
/// meter, not a voice.
pub const COUNT: usize = 29;

/// Target analysis window. Neve's 512 points at 16 kHz is 32 ms; keeping the
/// duration rather than the point count is what makes the bins resolve the
/// same way at any device rate.
const WINDOW_SECONDS: f32 = 0.032;
const MIN_N: usize = 512;
const MAX_N: usize = 4096;

const LOW_HZ: f32 = 90.0;
const HIGH_HZ: f32 = 4_000.0;

/// Lifts quiet speech without letting loud speech peg every dot flat.
///
/// Higher than Neve's 18: a desktop condenser at conversational distance
/// sits well below a phone mic held at the face. This is the one number to
/// turn if the row reads as too flat or pegs at full height.
const GAIN: f32 = 70.0;

/// Reusable transform state. Not shared between threads: it owns scratch
/// buffers and is only ever driven from whichever thread is metering.
pub struct Bands {
    n: usize,
    norm: f32,
    re: Vec<f32>,
    im: Vec<f32>,
    hann: Vec<f32>,
    cos_t: Vec<f32>,
    sin_t: Vec<f32>,
    edges: [usize; COUNT + 1],
}

/// Smallest power of two giving at least `WINDOW_SECONDS` at `sample_rate`.
///
/// A fixed 512 points is 32 ms at 16 kHz but only 12 ms at 44.1 kHz, and the
/// bins go from 31 Hz wide to 86 Hz. At that resolution the 90 Hz to 4 kHz
/// range covers barely 45 bins, the low bands collide, the one-bin-minimum
/// guard forces them apart, and the bottom of the row degenerates into
/// linear spacing. Visibly, all the movement bunches at the left.
fn window_points(sample_rate: f32) -> usize {
    let wanted = sample_rate * WINDOW_SECONDS;
    let mut n = MIN_N;
    while (n as f32) < wanted && n < MAX_N {
        n <<= 1;
    }
    n
}

impl Bands {
    /// Builds the band edges for `sample_rate`.
    ///
    /// Neve can hardcode 16 kHz because its capture always is. Here the
    /// device picks the rate, and using the wrong one shifts every edge by
    /// that ratio: at 44.1 kHz against 16 kHz constants, the whole speech
    /// range collapses into the lowest few dots and the row barely moves.
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate.max(8_000) as f32;
        let n = window_points(sample_rate);
        // Hann window: without it a tone leaks across neighbouring bands and
        // every dot twitches at once.
        let mut hann = vec![0.0f32; n];
        for (i, slot) in hann.iter_mut().enumerate() {
            *slot = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos();
        }

        // Precomputed twiddles. Building them by recurrence inside the
        // transform accumulates enough float error over 512 points to smear
        // which band a tone lands in.
        let mut cos_t = vec![0.0f32; n / 2];
        let mut sin_t = vec![0.0f32; n / 2];
        for i in 0..n / 2 {
            let angle = -2.0 * PI * i as f32 / n as f32;
            cos_t[i] = angle.cos();
            sin_t[i] = angle.sin();
        }

        // Band edges in bins, log-spaced over the speech range.
        let mut edges = [0usize; COUNT + 1];
        let lo = LOW_HZ * n as f32 / sample_rate;
        let hi = HIGH_HZ * n as f32 / sample_rate;
        let ratio = (hi / lo).ln();
        for (i, edge) in edges.iter_mut().enumerate() {
            let bin = lo * (ratio * i as f32 / COUNT as f32).exp();
            *edge = (bin as usize).clamp(1, n / 2 - 1);
        }
        // Guarantee every band owns at least one bin, however the rounding
        // lands.
        for i in 1..=COUNT {
            if edges[i] <= edges[i - 1] {
                edges[i] = edges[i - 1] + 1;
            }
        }

        Self {
            n,
            norm: 2.0 / n as f32,
            re: vec![0.0; n],
            im: vec![0.0; n],
            hann,
            cos_t,
            sin_t,
            edges,
        }
    }

    /// How many mono samples this instance wants per frame.
    ///
    /// Hand it fewer and the tail is zero-padded, which quietly costs
    /// resolution rather than failing.
    pub fn window_len(&self) -> usize {
        self.n
    }

    /// Fills `out` with 0..1 band energies for `samples`.
    ///
    /// Windows shorter than the transform are zero-padded, which is fine:
    /// the caller hands over whatever the last capture callback produced.
    pub fn compute(&mut self, samples: &[f32], out: &mut [f32; COUNT]) {
        let count = samples.len().min(self.n);
        for (i, sample) in samples.iter().take(count).enumerate() {
            self.re[i] = sample * self.hann[i];
            self.im[i] = 0.0;
        }
        for i in count..self.n {
            self.re[i] = 0.0;
            self.im[i] = 0.0;
        }

        self.transform();

        for (b, slot) in out.iter_mut().enumerate() {
            // Peak rather than mean across the band: bands get wider as they
            // go up, and averaging would quietly penalise the high dots for
            // owning more bins.
            let mut peak = 0.0f32;
            for k in self.edges[b]..self.edges[b + 1] {
                let mag = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() * self.norm;
                if mag > peak {
                    peak = mag;
                }
            }
            // sqrt curve: speech energy spans orders of magnitude, and a
            // linear map leaves quiet consonants invisible while the vowels
            // sit pinned at the top.
            *slot = (peak * GAIN).sqrt().clamp(0.0, 1.0);
        }
    }

    /// In-place radix-2 FFT.
    fn transform(&mut self) {
        let n = self.n;
        let mut j = 0usize;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j |= bit;
            if i < j {
                self.re.swap(i, j);
                self.im.swap(i, j);
            }
        }

        let mut len = 2usize;
        while len <= n {
            let half = len >> 1;
            let step = n / len;
            let mut base = 0usize;
            while base < n {
                for k in 0..half {
                    let tw = k * step;
                    let wr = self.cos_t[tw];
                    let wi = self.sin_t[tw];
                    let a = base + k;
                    let b = a + half;
                    let vr = self.re[b] * wr - self.im[b] * wi;
                    let vi = self.re[b] * wi + self.im[b] * wr;
                    self.re[b] = self.re[a] - vr;
                    self.im[b] = self.im[a] - vi;
                    self.re[a] += vr;
                    self.im[a] += vi;
                }
                base += len;
            }
            len <<= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RATE: f32 = 16_000.0;

    fn tone(freq_hz: f32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| (2.0 * PI * freq_hz * i as f32 / TEST_RATE).sin())
            .collect()
    }

    /// Which band a frequency should land in, from the same log spacing.
    fn expected_band(freq_hz: f32) -> usize {
        let ratio = (HIGH_HZ / LOW_HZ).ln();
        (((freq_hz / LOW_HZ).ln() / ratio) * COUNT as f32) as usize
    }

    #[test]
    fn silence_lights_nothing() {
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut out = [0.0f32; COUNT];

        bands.compute(&vec![0.0; 512], &mut out);

        assert!(out.iter().all(|&v| v == 0.0), "{out:?}");
    }

    #[test]
    fn a_tone_lights_its_own_band_hardest() {
        // The whole reason for the transform: one dot moves, not all of them.
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut out = [0.0f32; COUNT];

        bands.compute(&tone(1000.0, 512), &mut out);

        let loudest = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let expected = expected_band(1000.0);
        assert!(
            loudest.abs_diff(expected) <= 1,
            "1 kHz lit band {loudest}, expected about {expected}: {out:?}"
        );
    }

    #[test]
    fn a_low_tone_and_a_high_tone_light_different_bands() {
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut low = [0.0f32; COUNT];
        let mut high = [0.0f32; COUNT];

        bands.compute(&tone(150.0, 512), &mut low);
        bands.compute(&tone(3000.0, 512), &mut high);

        let peak = |b: &[f32; COUNT]| {
            b.iter()
                .enumerate()
                .max_by(|a, c| a.1.partial_cmp(c.1).unwrap())
                .map(|(i, _)| i)
                .unwrap()
        };
        assert!(
            peak(&low) < peak(&high),
            "low peaked at {} and high at {}",
            peak(&low),
            peak(&high)
        );
    }

    #[test]
    fn a_tone_does_not_light_the_whole_row() {
        // A single loudness number would. This is the difference between
        // reading as a voice and reading as a meter.
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut out = [0.0f32; COUNT];

        bands.compute(&tone(1000.0, 512), &mut out);

        let lit = out.iter().filter(|&&v| v > 0.5).count();
        assert!(lit < COUNT / 3, "{lit} of {COUNT} bands lit: {out:?}");
    }

    #[test]
    fn output_stays_within_range_for_a_clipped_input() {
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut out = [0.0f32; COUNT];

        bands.compute(&vec![1.0; 512], &mut out);

        assert!(out.iter().all(|&v| (0.0..=1.0).contains(&v)), "{out:?}");
    }

    #[test]
    fn the_same_tone_lands_in_different_bands_at_different_rates() {
        // The bug this parameter exists for: computing 44.1 kHz audio against
        // 16 kHz edges pushes everything down the row until only the first
        // few dots ever move.
        let mut at_16k = Bands::new(16_000);
        let mut at_44k = Bands::new(44_100);
        let mut low = [0.0f32; COUNT];
        let mut high = [0.0f32; COUNT];

        at_16k.compute(&tone(1000.0, 512), &mut low);
        at_44k.compute(&tone(1000.0, 512), &mut high);

        let peak = |b: &[f32; COUNT]| {
            b.iter()
                .enumerate()
                .max_by(|a, c| a.1.partial_cmp(c.1).unwrap())
                .map(|(i, _)| i)
                .unwrap()
        };
        assert_ne!(
            peak(&low),
            peak(&high),
            "the rate must actually move the band edges"
        );
    }

    #[test]
    fn a_higher_rate_gets_a_longer_transform_so_the_bins_stay_fine() {
        // The bug behind "all the movement is on the left": at 512 points a
        // 44.1 kHz bin is 86 Hz, leaving ~45 bins for 29 bands.
        assert_eq!(window_points(16_000.0), 512);
        assert!(window_points(44_100.0) >= 1024);
        assert!(window_points(48_000.0) >= 1024);
    }

    #[test]
    fn every_band_owns_a_distinct_bin_range_at_a_high_rate() {
        // Colliding edges are what collapse the low end into linear spacing.
        let bands = Bands::new(44_100);
        for i in 1..=COUNT {
            assert!(
                bands.edges[i] > bands.edges[i - 1],
                "band {i} starts at or before {}",
                i - 1
            );
        }
    }

    #[test]
    fn a_short_window_is_zero_padded_rather_than_rejected() {
        let mut bands = Bands::new(TEST_RATE as u32);
        let mut out = [0.0f32; COUNT];

        bands.compute(&tone(1000.0, 128), &mut out);

        assert!(out.iter().any(|&v| v > 0.0), "a short window still reads");
    }
}
