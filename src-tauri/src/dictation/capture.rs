//! Microphone capture: device discovery, format negotiation, and the pure
//! signal steps that turn whatever the device hands back into the mono clip
//! the rest of the pipeline expects.
//!
//! `cpal::Stream` is `!Send`, so it can never live in `AppState` alongside
//! the rest of the dictation state. A capture therefore owns a thread: that
//! thread builds the stream, holds it for the whole recording, and drops it
//! on stop. Everything crossing the thread boundary is the shared sample
//! buffer and a one-shot stop signal.

use crate::dictation::models::{AudioInputDevice, CaptureFormat, SupportedRange};
use crate::dictation::resample::TARGET_RATE;
use crate::dictation::error::DictationError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Peak amplitude at or below which a clip counts as silence.
///
/// A blocked microphone on Windows does not fail to open: it opens and
/// delivers frames that are all exactly zero. This threshold sits well above
/// digital silence and dither but far below any real speech.
const SILENCE_PEAK: f32 = 1e-3;

/// Picks the input format to open, given what the device advertises.
///
/// Exact 16 kHz wins outright because it skips resampling entirely. Failing
/// that, the lowest rate above the target beats anything below it, and
/// fewer channels break ties since every extra channel is one more to
/// average away.
pub fn choose_capture_format(
    supported: &[SupportedRange],
    default: CaptureFormat,
) -> CaptureFormat {
    supported
        .iter()
        .map(|range| CaptureFormat {
            sample_rate: range.best_rate_for(TARGET_RATE),
            channels: range.channels,
        })
        .min_by_key(|format| (rate_rank(format.sample_rate), format.channels))
        .unwrap_or(default)
}

/// Sort key placing the target rate first, then rates above it ascending,
/// then rates below it by how far short they fall.
fn rate_rank(rate: u32) -> (u8, u32) {
    match rate.cmp(&TARGET_RATE) {
        std::cmp::Ordering::Equal => (0, 0),
        std::cmp::Ordering::Greater => (1, rate),
        std::cmp::Ordering::Less => (2, TARGET_RATE - rate),
    }
}

/// Averages an interleaved buffer down to one channel.
///
/// A trailing partial frame is dropped rather than averaged: a stream cut
/// mid-frame would otherwise divide a real sample by the full channel count
/// and quietly attenuate it.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    match channels {
        0 => Vec::new(),
        1 => interleaved.to_vec(),
        channels => {
            let channels = channels as usize;
            interleaved
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect()
        }
    }
}

/// Whether a clip carries no audible signal. See [`SILENCE_PEAK`].
pub fn is_silent(samples: &[f32]) -> bool {
    peak_amplitude(samples) <= SILENCE_PEAK
}

fn peak_amplitude(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
}

/// How fast the meter rises to a louder sound.
const LEVEL_ATTACK: f32 = 0.6;
/// How fast it falls when the room goes quiet. Deliberately slower than the
/// attack: speech is transient, and a meter that fell as fast as it rose would
/// flicker on every syllable gap instead of reading as a voice.
const LEVEL_RELEASE: f32 = 0.12;

/// Advances the input-level meter by one buffer.
///
/// Exponential attack/release rather than the raw peak, because the raw peak
/// per audio buffer is far too jumpy to look at. Non-finite input is treated
/// as silence: a single NaN would otherwise propagate into every later frame
/// and freeze the meter for the rest of the session.
pub fn smooth_level(previous: f32, peak: f32) -> f32 {
    let peak = if peak.is_finite() {
        peak.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let previous = if previous.is_finite() { previous } else { 0.0 };
    let rate = if peak > previous {
        LEVEL_ATTACK
    } else {
        LEVEL_RELEASE
    };
    (previous + (peak - previous) * rate).clamp(0.0, 1.0)
}

/// Every microphone the default host can see.
pub fn list_input_devices() -> Result<Vec<AudioInputDevice>, DictationError> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok());

    let devices = host
        .input_devices()
        .map_err(|e| DictationError::Other(format!("Could not enumerate microphones: {e}")))?;

    Ok(devices
        .filter_map(|device| {
            // A device that cannot report an id cannot be selected later, so
            // listing it would only offer the user a dead entry.
            let id = device.id().ok()?;
            let name = device
                .description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| id.to_string());
            Some(AudioInputDevice {
                is_default: default_id.as_ref() == Some(&id),
                id: id.to_string(),
                name,
            })
        })
        .collect())
}

/// Audio captured from a single recording, already mixed to mono but still
/// at whatever rate the device negotiated.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// A recording in progress. Dropping it without calling [`stop`] ends the
/// recording and discards the audio.
///
/// [`stop`]: CaptureSession::stop
pub struct CaptureSession {
    stop_tx: mpsc::Sender<()>,
    worker: std::thread::JoinHandle<()>,
    buffer: Arc<Mutex<Vec<f32>>>,
    /// Smoothed input level, written by the audio callback and read by the
    /// panel. An atomic rather than a lock because the callback must never
    /// wait on anything.
    level: Arc<AtomicU32>,
    format: CaptureFormat,
}

impl CaptureSession {
    /// Opens `device_id` (or the default microphone) and starts recording.
    pub fn start(device_id: Option<&str>) -> Result<Self, DictationError> {
        let device = resolve_device(device_id)?;

        let supported: Vec<SupportedRange> = device
            .supported_input_configs()
            .map_err(|e| DictationError::Other(format!("Could not read microphone formats: {e}")))?
            .map(|range| SupportedRange {
                channels: range.channels(),
                min_rate: range.min_sample_rate(),
                max_rate: range.max_sample_rate(),
            })
            .collect();

        let default_config = device
            .default_input_config()
            .map_err(|e| DictationError::Other(format!("Microphone has no default format: {e}")))?;
        let default_format = CaptureFormat {
            sample_rate: default_config.sample_rate(),
            channels: default_config.channels(),
        };

        let format = choose_capture_format(&supported, default_format);
        let sample_format = default_config.sample_format();
        crate::say!("[dictation] capturing at {} Hz / {} ch ({:?}); resampling {}",
            format.sample_rate,
            format.channels,
            sample_format,
            if format.sample_rate == TARGET_RATE {
                "not needed"
            } else {
                "required"
            }
        );

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let level = Arc::new(AtomicU32::new(0));
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();

        let thread_buffer = Arc::clone(&buffer);
        let thread_level = Arc::clone(&level);
        let config = cpal::StreamConfig {
            channels: format.channels,
            sample_rate: format.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        // The stream is built, played, and dropped entirely inside this
        // thread because it is `!Send`.
        let worker = std::thread::spawn(move || {
            let stream =
                build_input_stream(&device, &config, sample_format, thread_buffer, thread_level);
            let stream = match stream {
                Ok(stream) => match stream.play() {
                    Ok(()) => {
                        let _ = ready_tx.send(Ok(()));
                        stream
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(DictationError::Other(format!(
                            "Could not start the microphone: {e}"
                        ))));
                        return;
                    }
                },
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };

            // Block until `stop` (or the session being dropped) closes the
            // channel, then drop the stream to end the recording.
            let _ = stop_rx.recv();
            drop(stream);
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop_tx,
                worker,
                buffer,
                level,
                format,
            }),
            Ok(Err(e)) => Err(e),
            // The worker died without reporting, which only happens if it
            // panicked while opening the device.
            Err(_) => Err(DictationError::Other(
                "Microphone capture thread stopped before it started".to_string(),
            )),
        }
    }

    /// Ends the recording and returns the captured mono audio.
    pub fn stop(self) -> CapturedClip {
        let _ = self.stop_tx.send(());
        let _ = self.worker.join();

        let interleaved = match self.buffer.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            // A poisoned lock means the audio callback panicked. The
            // recording is lost, but reporting an empty clip degrades to
            // "nothing was said" rather than taking the app down.
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };

        CapturedClip {
            samples: downmix_to_mono(&interleaved, self.format.channels),
            sample_rate: self.format.sample_rate,
        }
    }

    pub fn format(&self) -> CaptureFormat {
        self.format
    }

    /// Current smoothed input level, 0.0 to 1.0, for the panel's meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// A handle to the level alone, so the meter pump can read it without
    /// holding the lock the session itself lives behind.
    pub fn level_handle(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.level)
    }

    /// A closure yielding the most recent window of mono audio, for the
    /// waveform's transform.
    ///
    /// Returned as a closure over the shared buffer rather than a method,
    /// because the meter runs on its own thread and must not borrow the
    /// session (which the service keeps behind a lock for the whole
    /// recording). Reading the tail of what has already been captured avoids
    /// a second copy on the audio callback's path.
    pub fn window_handle(&self, frames: usize) -> impl Fn() -> Vec<f32> + Send + 'static {
        let buffer = Arc::clone(&self.buffer);
        let channels = self.format.channels;
        // Sized by the caller: the transform's window grows with the device
        // rate, and handing over less would silently zero-pad it.
        let wanted = frames * channels.max(1) as usize;
        move || {
            let Ok(samples) = buffer.lock() else {
                return Vec::new();
            };
            let tail = &samples[samples.len().saturating_sub(wanted)..];
            downmix_to_mono(tail, channels)
        }
    }
}

fn resolve_device(device_id: Option<&str>) -> Result<cpal::Device, DictationError> {
    let host = cpal::default_host();
    match device_id {
        Some(id) => {
            let parsed = id
                .parse::<cpal::DeviceId>()
                .map_err(|e| DictationError::Validation(format!("Invalid microphone id '{id}': {e}")))?;
            host.device_by_id(&parsed)
                .ok_or_else(|| DictationError::NotFound(format!("Microphone '{id}' is not connected")))
        }
        None => host
            .default_input_device()
            .ok_or_else(|| DictationError::NotFound("No microphone is available".to_string())),
    }
}

fn build_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream, DictationError> {
    match sample_format {
        SampleFormat::F32 => typed_input_stream::<f32>(device, config, buffer, level),
        SampleFormat::I16 => typed_input_stream::<i16>(device, config, buffer, level),
        SampleFormat::I32 => typed_input_stream::<i32>(device, config, buffer, level),
        SampleFormat::I8 => typed_input_stream::<i8>(device, config, buffer, level),
        SampleFormat::U8 => typed_input_stream::<u8>(device, config, buffer, level),
        SampleFormat::U16 => typed_input_stream::<u16>(device, config, buffer, level),
        other => Err(DictationError::Other(format!(
            "Microphone reports the unsupported sample format {other:?}"
        ))),
    }
}

fn typed_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream, DictationError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                // One pass: convert, track the buffer's peak, and store, so
                // the meter costs nothing beyond what capture already does.
                let mut peak = 0.0f32;
                if let Ok(mut samples) = buffer.lock() {
                    samples.reserve(data.len());
                    for &sample in data {
                        let value = f32::from_sample(sample);
                        peak = peak.max(value.abs());
                        samples.push(value);
                    }
                }
                let previous = f32::from_bits(level.load(Ordering::Relaxed));
                level.store(smooth_level(previous, peak).to_bits(), Ordering::Relaxed);
            },
            |e| crate::say!("[dictation] microphone stream error: {e}"),
            None,
        )
        .map_err(|e| DictationError::Other(format!("Could not open the microphone: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictation::models::{CaptureFormat, SupportedRange};

    fn range(channels: u16, min_rate: u32, max_rate: u32) -> SupportedRange {
        SupportedRange {
            channels,
            min_rate,
            max_rate,
        }
    }

    fn default_format() -> CaptureFormat {
        CaptureFormat {
            sample_rate: 48_000,
            channels: 2,
        }
    }

    // ── format selection ────────────────────────────────────────────────────

    #[test]
    fn prefers_mono_at_the_target_rate_when_the_device_offers_it() {
        // The ideal case: no resampling and no downmixing.
        let supported = [range(2, 44_100, 48_000), range(1, 8_000, 48_000)];

        let chosen = choose_capture_format(&supported, default_format());

        assert_eq!(
            chosen,
            CaptureFormat {
                sample_rate: 16_000,
                channels: 1
            }
        );
    }

    #[test]
    fn prefers_the_fewest_channels_at_the_target_rate() {
        let supported = [range(4, 16_000, 16_000), range(2, 16_000, 16_000)];

        let chosen = choose_capture_format(&supported, default_format());

        assert_eq!(chosen.sample_rate, 16_000);
        assert_eq!(chosen.channels, 2, "downmixing 2 is cheaper than 4");
    }

    #[test]
    fn picks_the_lowest_rate_above_the_target_when_16k_is_unavailable() {
        // Windows shared-mode WASAPI usually pins the device to its mix
        // format, so this is the common real path, not an edge case.
        let supported = [range(1, 48_000, 48_000), range(1, 44_100, 44_100)];

        let chosen = choose_capture_format(&supported, default_format());

        assert_eq!(chosen.sample_rate, 44_100, "less to throw away than 48k");
    }

    #[test]
    fn falls_back_below_the_target_only_when_nothing_higher_exists() {
        // An 8 kHz-only telephony device: upsampling invents nothing, but
        // whisper still needs 16 kHz, so this is the honest best effort.
        let supported = [range(1, 8_000, 8_000)];

        let chosen = choose_capture_format(&supported, default_format());

        assert_eq!(chosen.sample_rate, 8_000);
    }

    #[test]
    fn prefers_a_rate_above_the_target_over_one_below_it() {
        let supported = [range(1, 8_000, 8_000), range(1, 22_050, 22_050)];

        let chosen = choose_capture_format(&supported, default_format());

        assert_eq!(chosen.sample_rate, 22_050);
    }

    #[test]
    fn falls_back_to_the_device_default_when_nothing_is_advertised() {
        let chosen = choose_capture_format(&[], default_format());

        assert_eq!(chosen, default_format());
    }

    // ── downmixing ──────────────────────────────────────────────────────────

    #[test]
    fn downmix_passes_mono_through_untouched() {
        let frames = [0.1, -0.2, 0.3];

        assert_eq!(downmix_to_mono(&frames, 1), frames);
    }

    #[test]
    fn downmix_averages_stereo_pairs() {
        let interleaved = [1.0, 0.0, -1.0, 1.0];

        assert_eq!(downmix_to_mono(&interleaved, 2), vec![0.5, 0.0]);
    }

    #[test]
    fn downmix_averages_every_channel_of_a_wider_stream() {
        let interleaved = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 2.0];

        assert_eq!(downmix_to_mono(&interleaved, 4), vec![1.0, 0.5]);
    }

    #[test]
    fn downmix_drops_a_trailing_partial_frame() {
        // A stream can be cut mid-frame when the device stops; half a frame
        // is not a sample, and averaging it against missing channels would
        // quietly halve that sample's amplitude.
        let interleaved = [1.0, 1.0, 0.5];

        assert_eq!(downmix_to_mono(&interleaved, 2), vec![1.0]);
    }

    #[test]
    fn downmix_handles_an_empty_buffer() {
        assert!(downmix_to_mono(&[], 2).is_empty());
    }

    #[test]
    fn downmix_treats_a_zero_channel_count_as_empty_rather_than_dividing_by_zero() {
        assert!(downmix_to_mono(&[1.0, 2.0], 0).is_empty());
    }

    // ── silence detection ───────────────────────────────────────────────────

    #[test]
    fn a_digitally_silent_clip_is_silent() {
        // The symptom of a mic blocked by Windows privacy settings: the
        // stream opens and delivers frames, every one of them zero.
        assert!(is_silent(&[0.0; 16_000]));
    }

    #[test]
    fn an_empty_clip_is_silent() {
        assert!(is_silent(&[]));
    }

    #[test]
    fn dither_and_noise_floor_still_count_as_silent() {
        let noise: Vec<f32> = (0..1_000)
            .map(|i| if i % 2 == 0 { 1e-5 } else { -1e-5 })
            .collect();

        assert!(is_silent(&noise));
    }

    #[test]
    fn speech_level_audio_is_not_silent() {
        let speech: Vec<f32> = (0..1_000).map(|i| 0.2 * (i as f32).sin()).collect();

        assert!(!is_silent(&speech));
    }

    // ââ level meter ââ

    #[test]
    fn the_meter_rises_toward_a_louder_sound_without_snapping_to_it() {
        let next = smooth_level(0.0, 1.0);

        assert!(next > 0.0, "it has to move");
        assert!(
            next < 1.0,
            "snapping instantly reads as a strobe, not a level"
        );
    }

    #[test]
    fn the_meter_rises_faster_than_it_falls() {
        // Speech is transient. A meter that fell as fast as it rose would
        // flicker on every syllable gap instead of reading as a voice.
        let rise = smooth_level(0.0, 0.8) - 0.0;
        let fall = 0.8 - smooth_level(0.8, 0.0);

        assert!(rise > fall, "rise {rise} should outpace fall {fall}");
    }

    #[test]
    fn the_meter_decays_to_silence_when_the_room_goes_quiet() {
        let mut level = 1.0;
        for _ in 0..200 {
            level = smooth_level(level, 0.0);
        }

        assert!(level < 0.01, "still at {level} after 200 silent buffers");
    }

    #[test]
    fn the_meter_settles_at_a_sustained_level() {
        let mut level = 0.0;
        for _ in 0..200 {
            level = smooth_level(level, 0.5);
        }

        assert!((level - 0.5).abs() < 0.01, "settled at {level}, wanted 0.5");
    }

    #[test]
    fn the_meter_never_leaves_the_zero_to_one_range() {
        // Clipped input must not drive the bars off the end of the panel.
        assert!(smooth_level(0.9, 5.0) <= 1.0);
        assert!(smooth_level(0.0, -3.0) >= 0.0);
    }

    #[test]
    fn a_non_finite_peak_is_treated_as_silence_rather_than_poisoning_the_meter() {
        // One NaN would otherwise make every later frame NaN, freezing the
        // panel for the rest of the session.
        let next = smooth_level(0.5, f32::NAN);

        assert!(next.is_finite(), "got {next}");
    }

    #[test]
    fn a_single_loud_sample_is_enough_to_count_as_audio() {
        let mut clip = vec![0.0; 1_000];
        clip[500] = 0.9;

        assert!(!is_silent(&clip));
    }

    // ── hardware probes ─────────────────────────────────────────────────────
    //
    // Ignored because they need a real sound card, so CI and the normal
    // `cargo test` run skip them. Run one deliberately with:
    //   cargo test --lib dictation::capture::tests::probe -- --ignored --nocapture

    /// Reports what this machine's microphones advertise and which format
    /// the selection logic settles on. Reads metadata only: no stream is
    /// opened and no audio is recorded.
    #[test]
    #[ignore = "requires a sound card"]
    fn probe_reports_device_formats() {
        use cpal::traits::{DeviceTrait, HostTrait};

        let devices = list_input_devices().expect("enumerate microphones");
        println!("\n{} input device(s):", devices.len());
        for device in &devices {
            println!(
                "  {} {}\n    id: {}",
                if device.is_default { "*" } else { "-" },
                device.name,
                device.id
            );
        }

        let host = cpal::default_host();
        let Some(default_device) = host.default_input_device() else {
            println!("\nno default input device");
            return;
        };

        let supported: Vec<SupportedRange> = default_device
            .supported_input_configs()
            .expect("read formats")
            .map(|r| SupportedRange {
                channels: r.channels(),
                min_rate: r.min_sample_rate(),
                max_rate: r.max_sample_rate(),
            })
            .collect();

        println!("\ndefault device advertises {} range(s):", supported.len());
        for range in &supported {
            println!(
                "  {} ch, {} Hz to {} Hz",
                range.channels, range.min_rate, range.max_rate
            );
        }

        let default_config = default_device.default_input_config().expect("default");
        let default_format = CaptureFormat {
            sample_rate: default_config.sample_rate(),
            channels: default_config.channels(),
        };
        let chosen = choose_capture_format(&supported, default_format);

        println!(
            "\ndevice default : {} Hz / {} ch ({:?})",
            default_format.sample_rate,
            default_format.channels,
            default_config.sample_format()
        );
        println!(
            "chosen         : {} Hz / {} ch",
            chosen.sample_rate, chosen.channels
        );
        println!(
            "resampling     : {}\n",
            if chosen.sample_rate == crate::dictation::resample::TARGET_RATE {
                "NOT needed"
            } else {
                "REQUIRED on every dictation"
            }
        );
    }

    /// Records two seconds from the default microphone and reports what came
    /// back. This one really does open the mic, so it stays opt-in.
    #[test]
    #[ignore = "opens the microphone and records audio"]
    fn probe_records_two_seconds() {
        let session = CaptureSession::start(None).expect("start capture");
        let format = session.format();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let clip = session.stop();

        let seconds = clip.samples.len() as f32 / clip.sample_rate as f32;
        println!(
            "\ncaptured {} mono samples at {} Hz ({:.2}s, opened as {} ch)",
            clip.samples.len(),
            clip.sample_rate,
            seconds,
            format.channels
        );
        println!("peak amplitude : {:.4}", peak_amplitude(&clip.samples));
        println!(
            "silent         : {}\n",
            if is_silent(&clip.samples) {
                "YES - check the OS microphone privacy setting"
            } else {
                "no"
            }
        );

        assert!(
            (1.5..2.6).contains(&seconds),
            "expected roughly 2s of audio, got {seconds:.2}s"
        );
    }
}
