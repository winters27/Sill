//! What each extension has been allowed to reach, asked once and remembered.
//!
//! ## Asked when it happens, not at install
//!
//! The audit called for permissions declared in the manifest and agreed to on
//! install. A Raycast manifest has no field for them, and Sill runs Raycast
//! extensions, so a consent screen at install could only say "this could reach
//! anything", which is the kind of prompt that teaches somebody to press yes
//! without reading.
//!
//! So it is asked the first time it actually happens, on the same card an AI
//! action is approved on. The card can then name the real thing: this
//! extension is asking to read your clipboard, now, because it just tried to.
//! An answer is remembered, so it is asked once per permission per extension
//! and never again.
//!
//! A manifest that does declare permissions can still fill this in ahead of
//! time, and then nothing is asked at all. That is the better experience and
//! it is open to anything written for Sill; it just cannot be the only way in.
//!
//! ## Refusing is not failing
//!
//! A refused call comes back as an error naming the permission, so the
//! extension can say what to turn on. It is not a crash and not a silently
//! empty result, both of which read as a bug in the extension rather than as a
//! decision somebody made.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::Manager;

use crate::action::Capability;
use crate::ai::approval::{self, Answer, Asking, Pending};

use super::permission::{needs_granting, plainly, Permits};

const FILE: &str = "extension-grants.json";

/// The one report about grants not being written, named once so a save that
/// works withdraws the one that did not.
const TROUBLE: &str = "extension-grants";

/// How long a card waits before an unanswered one counts as no.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(90);

/// The real one: asks on a card, writes the answer down.
pub struct Granted {
    app: tauri::AppHandle,
    by_extension: Mutex<BTreeMap<String, Vec<Capability>>>,
    /// Whether the file has been read, so a save cannot precede a load.
    ///
    /// The same guard `chat.rs` carries, and for the reason it grew one: an
    /// empty map written over a real file is a silent loss, and here it would
    /// re-ask for every permission somebody had already granted, which trains
    /// exactly the reflex this whole file exists to avoid.
    read_the_file: AtomicBool,
}

impl Granted {
    pub fn new(app: tauri::AppHandle) -> Self {
        let this = Self {
            app,
            by_extension: Mutex::new(BTreeMap::new()),
            read_the_file: AtomicBool::new(false),
        };

        this.load();
        this
    }

    fn path(&self) -> Option<std::path::PathBuf> {
        Some(self.app.path().app_data_dir().ok()?.join(FILE))
    }

    fn load(&self) {
        let read = self
            .path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<BTreeMap<String, Vec<Capability>>>(&text).ok());

        if let (Some(found), Ok(mut held)) = (read, self.by_extension.lock()) {
            *held = found;
        }

        // Set even when there was no file. A machine that has granted nothing
        // has been read correctly, and refusing to save afterwards would mean
        // the first grant on a new machine is never written down.
        self.read_the_file.store(true, Ordering::Release);
    }

    fn save(&self) {
        if !self.read_the_file.load(Ordering::Acquire) {
            return;
        }

        let Ok(held) = self.by_extension.lock() else {
            return;
        };
        let Some(path) = self.path() else { return };

        let Ok(text) = serde_json::to_string_pretty(&*held) else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        /*
         * Reported, because the whole point of this file is being asked once.
         *
         * A grant that is not written down is granted for this run and gone by
         * the next one, so the same card comes back on every launch for a
         * permission the user has already agreed to. That is precisely the
         * pattern this module's own doc says teaches somebody to press yes
         * without reading, and there is nothing in the prompt to suggest a
         * failed write is why it keeps appearing.
         */
        match std::fs::write(path, text) {
            Ok(()) => crate::status::resolved(&self.app, TROUBLE),
            Err(err) => crate::status::report(
                &self.app,
                TROUBLE,
                format!(
                    "Sill could not save which extensions you have allowed, so it will \
                     ask again next time it starts: {err}"
                ),
                Some("extensions"),
            ),
        }
    }

    fn already(&self, extension: &str, capability: &Capability) -> bool {
        self.by_extension
            .lock()
            .map(|held| {
                held.get(extension)
                    .is_some_and(|list| list.contains(capability))
            })
            .unwrap_or(false)
    }

    fn remember(&self, extension: &str, capability: Capability) {
        if let Ok(mut held) = self.by_extension.lock() {
            let list = held.entry(extension.to_string()).or_default();

            if !list.contains(&capability) {
                list.push(capability);
            }
        }

        self.save();
    }

    /// What one extension holds, for the worker that has to enforce it.
    pub fn held(&self, extension: &str) -> Vec<Capability> {
        self.by_extension
            .lock()
            .map(|held| held.get(extension).cloned().unwrap_or_default())
            .unwrap_or_default()
    }

    /// Everything granted, by extension, for the screen that lists it.
    pub fn everything(&self) -> BTreeMap<String, Vec<Capability>> {
        self.by_extension
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default()
    }

    /// Records an answer given ahead of time.
    ///
    /// The case this file's own doc leaves open: "A manifest that does declare
    /// permissions can still fill this in ahead of time, and then nothing is
    /// asked at all." The store is that case. It shows what an extension's
    /// code reaches **before** anything is installed, derived from the source
    /// about to be built, and somebody agrees to it or does not install.
    ///
    /// Without this the two halves never met. Grants defaulted to nothing and
    /// the worker refuses `fs`, `net` and `child_process` at `require`, which
    /// happens at module load with no RPC to hang a card on, so an extension
    /// died before it rendered: **86 of the 104 commands in the twelve
    /// most-installed extensions**, measured.
    ///
    /// Only ever adds. Anything not on the list is still asked for on the card
    /// the first time it happens, which is what keeps the loud ones loud:
    /// pasting is not granted by accepting the clipboard.
    pub fn grant(&self, extension: &str, capabilities: &[Capability]) {
        for capability in capabilities {
            self.remember(extension, *capability);
        }
    }

    /// Forgets everything one extension was allowed.
    ///
    /// What uninstalling has to do. Leaving grants behind means installing the
    /// same extension again silently inherits permissions somebody agreed to
    /// for a version they removed, which is the quietest way for a permission
    /// system to stop meaning anything.
    pub fn forget(&self, extension: &str) {
        if let Ok(mut held) = self.by_extension.lock() {
            held.remove(extension);
        }

        self.save();
    }

    /// Takes one back. The extension is asked again next time it tries.
    pub fn revoke(&self, extension: &str, capability: &Capability) {
        if let Ok(mut held) = self.by_extension.lock() {
            if let Some(list) = held.get_mut(extension) {
                list.retain(|held| held != capability);
            }
        }

        self.save();
    }
}

#[async_trait::async_trait]
impl Permits for Granted {
    async fn allow(&self, extension: &str, needs: &[Capability]) -> Result<(), String> {
        for capability in needs {
            if !needs_granting(capability) || self.already(extension, capability) {
                continue;
            }

            let pending = self.app.state::<Pending>();
            let id = pending.next_id();

            approval::raise(
                &self.app,
                Asking {
                    id: id.clone(),
                    title: format!("{extension} wants permission"),
                    subject: plainly(capability).to_string(),
                    touches: crate::ai::acting::what_it_touches(std::slice::from_ref(capability))
                        .to_string(),
                },
            );

            match pending.wait_for(&id, PATIENCE).await {
                Answer::Allowed => self.remember(extension, *capability),
                // A refusal is not written down. Somebody who said no once has
                // said no once, and an extension that stops asking is one that
                // cannot be turned on later without hunting for where the no
                // was stored.
                Answer::Refused | Answer::Unanswered => {
                    return Err(format!(
                        "{extension} is not allowed to {}. Grant it in Settings, under Extensions.",
                        plainly(capability),
                    ))
                }
            }
        }

        Ok(())
    }
}

/// What an extension has been allowed, looked up from wherever the app is.
///
/// Keyed by `LoadOptions::extension_id`, which is the **same** string
/// `ExtHost` hands to `ApiLayer::dispatch` as `extension`. If those two ever
/// stopped agreeing, an extension would be granted a permission under one
/// name and checked against another, and the symptom would be a permission
/// that is granted, listed, and still refused.
///
/// Nothing managed means nothing granted, which is the safe reading: a caller
/// with no grant store gets an extension that can draw and nothing else.
pub fn for_extension(app: &tauri::AppHandle, extension: &str) -> Vec<Capability> {
    app.try_state::<std::sync::Arc<Granted>>()
        .map(|grants| grants.held(extension))
        .unwrap_or_default()
}
