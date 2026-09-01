//! A neural voice Sill fetches and runs itself.
//!
//! The same bargain dictation already makes for whisper.cpp: nothing ships in
//! the installer, and the first time somebody wants it, a binary and a model
//! are downloaded with a progress bar and kept. So a good offline voice costs
//! one button rather than an account, a key, or a server to run.
//!
//! ## Which Piper, and why the old one
//!
//! Piper split. `OHF-Voice/piper1-gpl` is the current one and it is GPL-3.0
//! shipped as a Python wheel, which would put a Python requirement into a
//! launcher that deliberately has none. `rhasspy/piper` is **MIT**, ships a
//! self-contained Windows executable, and is archived at its 2023 release.
//!
//! An archived speech synthesiser is not the same thing as a broken one: it is
//! a fixed binary running fixed ONNX models, both of which still do exactly
//! what they did. The trade is knowingly made, and the voices are fetched from
//! the same project's model repository at a **pinned revision** rather than a
//! branch, so what is downloaded tomorrow is what was downloaded today.
//!
//! ## Why a subprocess rather than a library
//!
//! Piper is an executable that reads text and writes a WAV. Doing it in
//! process would mean an ONNX runtime, a phonemiser and their model data
//! linked into Sill, against a spawn and a file for a feature that reads a
//! paragraph aloud.

use std::path::PathBuf;

use tauri::AppHandle;

/// The voice offered first.
///
/// A medium-quality American English voice: the quality tiers are x_low, low,
/// medium and high, and medium is the one that sounds good without being slow
/// on a machine with no GPU.
pub const DEFAULT_VOICE: &str = "en_US-amy-medium";

/// Voices offered for download.
///
/// A short list on purpose. The model repository has hundreds across dozens of
/// languages, and a picker showing all of them is a worse answer to "I want a
/// better voice" than four that are known to be good.
pub const VOICES: &[Voice] = &[
    Voice { id: "en_US-amy-medium", label: "Amy", locale: "English (US)", path: "en/en_US/amy/medium" },
    Voice { id: "en_US-ryan-medium", label: "Ryan", locale: "English (US)", path: "en/en_US/ryan/medium" },
    Voice { id: "en_GB-alba-medium", label: "Alba", locale: "English (UK)", path: "en/en_GB/alba/medium" },
    Voice { id: "en_US-lessac-medium", label: "Lessac", locale: "English (US)", path: "en/en_US/lessac/medium" },
];

/// One downloadable voice.
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    pub id: &'static str,
    pub label: &'static str,
    pub locale: &'static str,
    /// Where it sits in the model repository.
    path: &'static str,
}

/// The model repository revision every voice is fetched from.
///
/// Pinned rather than `main`, for the reason `dictation::assets` pins its own:
/// a branch is a moving target, and a model that changes underneath a person
/// who already downloaded it is a voice that changes for no reason they asked
/// for. There is a test that this is not a branch name.
const REVISION: &str = "39ab474be869e9181350af6a65e4953eef67aaa0";

/// The Piper release the executable comes from.
///
/// The last one `rhasspy/piper` made, and the only Windows build that needs no
/// Python.
const PIPER_TAG: &str = "2023.11.14-2";

pub fn voice(id: &str) -> Option<&'static Voice> {
    VOICES.iter().find(|voice| voice.id == id)
}

/// Where the engine and its voices live: `<app data>/piper/`.
pub fn home(app: &AppHandle) -> PathBuf {
    crate::state::data_dir(app).join("piper")
}

/// The executable, once it has been unpacked.
pub fn exe(app: &AppHandle) -> PathBuf {
    home(app).join("piper").join("piper.exe")
}

/// Where one voice's two files live.
fn voice_files(app: &AppHandle, id: &str) -> (PathBuf, PathBuf) {
    let dir = home(app).join("voices");
    (dir.join(format!("{id}.onnx")), dir.join(format!("{id}.onnx.json")))
}

/// Whether the engine is unpacked and this voice is present.
///
/// Both halves, because either alone cannot speak, and a half-finished
/// download that reports itself installed is a button that does nothing.
pub fn is_installed(app: &AppHandle, id: &str) -> bool {
    let (model, config) = voice_files(app, id);
    exe(app).is_file() && model.is_file() && config.is_file()
}

/// Where the Windows build of Piper is fetched from.
pub fn engine_url() -> String {
    format!("https://github.com/rhasspy/piper/releases/download/{PIPER_TAG}/piper_windows_amd64.zip")
}

/// Where one voice's model and its config are fetched from.
pub fn voice_urls(voice: &Voice) -> (String, String) {
    let base = format!(
        "https://huggingface.co/rhasspy/piper-voices/resolve/{REVISION}/{}/{}",
        voice.path, voice.id
    );
    (format!("{base}.onnx"), format!("{base}.onnx.json"))
}

/// Downloads the engine and one voice, reporting progress as it goes.
///
/// Both halves in one call because either alone cannot speak, and a button
/// that leaves somebody with an engine and no voice has not finished. The
/// engine is fetched once and reused by every later voice.
///
/// Progress is reported as a fraction of the whole job rather than per file,
/// since three bars in a row for one button is three chances to look stalled.
pub async fn install(
    app: &AppHandle,
    voice_id: &str,
    mut progress: impl FnMut(f64, &str),
) -> Result<(), String> {
    let voice = voice(voice_id).ok_or_else(|| format!("no such voice: {voice_id}"))?;
    let dir = home(app);
    std::fs::create_dir_all(dir.join("voices"))
        .map_err(|err| format!("could not make room for the voice: {err}"))?;

    let client = crate::dictation::fetch::client();

    if !exe(app).is_file() {
        let archive = dir.join(".piper.zip.partial");
        progress(0.0, "Fetching the speech engine");

        crate::dictation::fetch::download_to(&client, &engine_url(), &archive, |done, total| {
            // The engine is the larger half of the job, so it is given the
            // larger half of the bar.
            let share = if total > 0 { done as f64 / total as f64 } else { 0.0 };
            progress(share * 0.7, "Fetching the speech engine");
        })
        .await
        .map_err(|err| format!("could not download the speech engine: {err}"))?;

        progress(0.7, "Unpacking the speech engine");
        let unpacked = unpack(&archive, &dir);
        let _ = std::fs::remove_file(&archive);
        unpacked?;

        if !exe(app).is_file() {
            return Err("the speech engine archive did not contain piper.exe".to_string());
        }
    }

    let (model_url, config_url) = voice_urls(voice);
    let (model, config) = voice_files(app, voice_id);

    progress(0.7, "Downloading the voice");
    crate::dictation::fetch::download_to(&client, &model_url, &model, |done, total| {
        let share = if total > 0 { done as f64 / total as f64 } else { 0.0 };
        progress(0.7 + share * 0.28, "Downloading the voice");
    })
    .await
    .map_err(|err| format!("could not download the voice: {err}"))?;

    // Small, and the voice cannot be spoken without it: Piper reads the
    // sample rate and the phoneme table out of this file.
    crate::dictation::fetch::download_to(&client, &config_url, &config, |_, _| {})
        .await
        .map_err(|err| format!("could not download the voice's settings: {err}"))?;

    progress(1.0, "Ready");
    Ok(())
}

/// Extracts the archive, keeping the directory it puts everything under.
///
/// Unlike the whisper archive, this one is *kept* nested: piper.exe loads its
/// ONNX runtime and espeak data from beside itself, so flattening the root
/// away would separate the executable from what it needs.
fn unpack(archive: &std::path::Path, into: &std::path::Path) -> Result<(), String> {
    let file = std::fs::File::open(archive)
        .map_err(|err| format!("could not open the downloaded engine: {err}"))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|err| format!("the speech engine archive is unreadable: {err}"))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("could not read an archive entry: {err}"))?;

        // `enclosed_name` is what refuses an entry whose path climbs out of
        // the directory it is being written into.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };

        let out = into.join(relative);

        if entry.is_dir() {
            let _ = std::fs::create_dir_all(&out);
            continue;
        }

        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not make {}: {err}", parent.display()))?;
        }

        let mut file = std::fs::File::create(&out)
            .map_err(|err| format!("could not write {}: {err}", out.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|err| format!("could not unpack {}: {err}", out.display()))?;
    }

    Ok(())
}

/// Removes a downloaded voice. The engine is left alone.
pub fn remove(app: &AppHandle, voice_id: &str) -> Result<bool, String> {
    let (model, config) = voice_files(app, voice_id);
    let had = model.is_file();

    for path in [model, config] {
        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|err| format!("could not remove {}: {err}", path.display()))?;
        }
    }

    Ok(had)
}

/// Turns text into a WAV clip by running Piper over it.
pub async fn speak(app: &AppHandle, voice_id: &str, text: &str) -> Result<Vec<u8>, String> {
    if !is_installed(app, voice_id) {
        return Err(format!(
            "The {voice_id} voice is not downloaded yet. Get it in Settings under Speech."
        ));
    }

    let exe = exe(app);
    let (model, _) = voice_files(app, voice_id);
    let out = home(app).join("out.wav");
    let text = text.to_string();
    let out_for_task = out.clone();

    // Off the async runtime: this is a subprocess that takes as long as the
    // text is long.
    tauri::async_runtime::spawn_blocking(move || run(&exe, &model, &out_for_task, &text))
        .await
        .map_err(|err| format!("the voice did not finish: {err}"))??;

    let wav = std::fs::read(&out).map_err(|err| format!("could not read the clip: {err}"))?;
    // The clip is in memory now and the file is a leftover.
    let _ = std::fs::remove_file(&out);

    Ok(wav)
}

#[cfg(windows)]
fn run(exe: &std::path::Path, model: &std::path::Path, out: &std::path::Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut child = Command::new(exe)
        .arg("--model")
        .arg(model)
        .arg("--output_file")
        .arg(out)
        // Piper resolves espeak-ng's data relative to where it runs, so it is
        // run from beside its own executable rather than from wherever Sill
        // happens to have been started.
        .current_dir(exe.parent().unwrap_or(exe))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|err| format!("could not run the voice: {err}"))?;

    // The text goes in on stdin rather than as an argument: a command line has
    // a length limit and quoting rules, and a paragraph off somebody's
    // clipboard respects neither.
    child
        .stdin
        .take()
        .ok_or("the voice would not take the text")?
        .write_all(text.as_bytes())
        .map_err(|err| format!("could not hand the text to the voice: {err}"))?;

    let finished = child
        .wait_with_output()
        .map_err(|err| format!("the voice stopped: {err}"))?;

    if finished.status.success() {
        return Ok(());
    }

    let said = String::from_utf8_lossy(&finished.stderr);
    let said = said.trim();

    Err(if said.is_empty() {
        "the voice failed and said nothing".to_string()
    } else {
        said.chars().take(300).collect()
    })
}

#[cfg(not(windows))]
fn run(_exe: &std::path::Path, _model: &std::path::Path, _out: &std::path::Path, _text: &str) -> Result<(), String> {
    Err("the downloaded voice is Windows only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same rule `dictation::assets` holds itself to.
    #[test]
    fn voices_are_fetched_from_a_pinned_revision_rather_than_a_branch() {
        for voice in VOICES {
            let (model, config) = voice_urls(voice);

            for url in [&model, &config] {
                assert!(url.contains(REVISION), "{url}");
                assert!(!url.contains("/main/"), "not pinned: {url}");
            }
        }

        assert!(
            REVISION.len() == 40 && REVISION.chars().all(|c| c.is_ascii_hexdigit()),
            "a revision is a commit, not a branch name: {REVISION}"
        );
    }

    /// A voice's id has to be the file name, because that is how it is found
    /// on disk after it is downloaded.
    #[test]
    fn a_voices_url_ends_in_its_own_id() {
        for voice in VOICES {
            let (model, config) = voice_urls(voice);
            assert!(model.ends_with(&format!("{}.onnx", voice.id)), "{model}");
            assert!(config.ends_with(&format!("{}.onnx.json", voice.id)), "{config}");
        }
    }

    #[test]
    fn the_default_voice_is_one_that_is_offered() {
        assert!(voice(DEFAULT_VOICE).is_some(), "{DEFAULT_VOICE} is not in the list");
    }

    #[test]
    fn no_two_voices_share_an_id() {
        let mut ids: Vec<&str> = VOICES.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "two voices share an id");
    }

    #[test]
    fn the_engine_comes_from_the_mit_release_rather_than_the_gpl_one() {
        let url = engine_url();
        assert!(url.contains("rhasspy/piper"), "{url}");
        assert!(!url.contains("piper1-gpl"), "the GPL build needs Python: {url}");
    }
}
