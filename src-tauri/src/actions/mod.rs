//! Everything Sill can do, one type per verb.
//!
//! These were the branches of a two-hundred-line chain over eleven mode
//! strings inside `launch_command`. Nothing about them has changed here; they
//! have only stopped being a chain. What that buys is that a second way of
//! invoking one (a shortcut, a panel entry, a workflow step) does not mean a
//! second copy of the behaviour, which is how the same operation ends up
//! subtly different depending on how you reached it.
//!
//! Grouped in one module rather than one file each because they are small and
//! read better together: the whole vocabulary of the launcher fits on a couple
//! of screens, and that is worth being able to see.

use async_trait::async_trait;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::action::{Action, ActionCtx, ActionRegistry, Capability, Outcome, Undo};
use crate::object::{Object, ObjectKind};

/// The whole vocabulary.
///
/// Order is the order they appear in the action panel, after the primary one
/// for the kind is lifted to the front. So this list is a design decision, not
/// bookkeeping: it is what the second and third entries will be.
pub fn builtins() -> ActionRegistry {
    let core: Vec<Box<dyn Action>> = vec![
        Box::new(Launch),
        Box::new(RunExtensionCommand),
        Box::new(OpenSystemSetting),
        Box::new(OpenSillSetting),
        Box::new(RunBuiltin),
        Box::new(PasteSnippet),
        Box::new(PasteEmoji),
        Box::new(OpenQuicklink),
        Box::new(ExtractText),
        Box::new(MarkUp),
        Box::new(SearchWeb),
        Box::new(OpenUrl),
        // Above every other "Copy something", because the panel filters by
        // substring and is drawn in this order. Below them, typing "copy" on
        // an emoji selected "Copy Name" and copied the words "grinning face".
        Box::new(CopyClipboardEntry),
        Box::new(CopyUrl),
        Box::new(CopyAnswer),
        Box::new(CopyPath),
        Box::new(RevealInFolder),
        Box::new(CopyName),
        Box::new(TerminalHere),
        Box::new(ToggleSystem),
        Box::new(VerifyFile),
        Box::new(LookUpFile),
        Box::new(HashFile),
        Box::new(CompressFile),
        Box::new(RenameFile),
        Box::new(MoveFile),
        // Last, and deliberately. The panel is drawn in this order, and the
        // one action here that removes something should not sit above the ones
        // that copy a path.
        Box::new(RecycleFile),
        Box::new(ToggleSessionMute),
        Box::new(SessionLouder),
        Box::new(SessionQuieter),
        Box::new(SessionHalf),
        Box::new(SessionFull),
        Box::new(RestoreWorkspace),
        Box::new(ForgetWorkspace),
        Box::new(ReadAloud),
        Box::new(StopReading),
    ];

    ActionRegistry::new(
        core.into_iter()
            .chain(transforms())
            .chain(window_actions())
            .collect(),
    )
}

/// Replaces what is on the clipboard, remembering what was there.
///
/// The remembering is what makes a copy undoable, and it is the cheapest
/// possible undo: a string that was already in memory. Reading the old value
/// can fail perfectly normally (an image, or an empty clipboard), and that is
/// not a reason to refuse the copy, only a reason to offer no undo for it.
fn copy_with_undo(ctx: &ActionCtx, text: &str, message: &str) -> Result<Outcome, String> {
    let previous = ctx.app.clipboard().read_text().ok();

    ctx.app
        .clipboard()
        .write_text(text.to_string())
        .map_err(|err| format!("Could not copy: {err}"))?;

    Ok(match previous {
        Some(text) => Outcome::undoable(message, Undo::RestoreClipboard { text }),
        None => Outcome::done(message),
    })
}

// ------------------------------------------------------------------ launch

/// Opens an application, a file or a folder through the shell.
struct Launch;

#[async_trait]
impl Action for Launch {
    fn id(&self) -> &'static str {
        "sill.launch"
    }

    fn title(&self) -> &'static str {
        "Open"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::Application | ObjectKind::File | ObjectKind::Folder
        )
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        if let Some(app_id) = object.target.strip_prefix(crate::apps::APPS_FOLDER) {
            // A packaged app has no path to open. Explorer resolves an
            // AppUserModelID through the Apps folder, which is how the Start
            // Menu launches them too.
            std::process::Command::new("explorer.exe")
                .arg(format!("{}{}", crate::apps::APPS_FOLDER, app_id))
                .spawn()
                .map_err(|err| format!("could not launch {}: {err}", object.title))?;
        } else {
            tauri_plugin_opener::open_path(&object.target, None::<&str>)
                .map_err(|err| format!("could not launch {}: {err}", object.title))?;
        }

        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Loads a Raycast-compatible extension command.
struct RunExtensionCommand;

#[async_trait]
impl Action for RunExtensionCommand {
    fn id(&self) -> &'static str {
        "sill.runExtensionCommand"
    }

    fn title(&self) -> &'static str {
        "Run Command"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ExtensionCommand
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The record is looked up again rather than carried on the object,
        // because loading needs four fields the object has no reason to hold.
        // This is the one action whose target is not enough on its own, and
        // it is worth the lookup rather than widening the type for it.
        let registry = ctx.app.state::<crate::state::RegistryState>();
        let record = registry
            .inner
            .lock()
            .await
            .commands
            .iter()
            .find(|c| c.id == object.id)
            .cloned()
            .ok_or_else(|| format!("no such command: {}", object.id))?;

        // The manifest decides. A no-view command runs and exits without ever
        // rendering, so loading it as a view leaves the window waiting for a
        // tree that never arrives.
        //
        // A mode the type does not know is loaded as a view, which is the same
        // thing this did when the test was written by hand here. What changed
        // is that the store asks the same function and reports an unknown mode
        // as unrunnable rather than installing it.
        let mode = crate::exthost::CommandMode::from_manifest(&record.mode)
            .unwrap_or(crate::exthost::CommandMode::View);

        let hosts = ctx.app.state::<crate::state::HostState>();
        let host = crate::host::host_of(&hosts).await?;

        let mut opts = crate::exthost::LoadOptions::with_preferences(
            record.entrypoint.clone(),
            &record.extension,
            &record.command,
            mode,
            record.preferences.clone(),
        );

        opts.capabilities = crate::exthost::grants::for_extension(&ctx.app, &record.extension);

        let session = host.load(&opts).await.map_err(|e| e.to_string())?;
        Ok(Outcome::running(format!("Ran {}", object.title), session))
    }
}

// ---------------------------------------------------------------- settings

/// Opens a Windows settings page or Control Panel applet.
struct OpenSystemSetting;

#[async_trait]
impl Action for OpenSystemSetting {
    fn id(&self) -> &'static str {
        "sill.openSystemSetting"
    }

    fn title(&self) -> &'static str {
        "Open Setting"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::SystemSetting
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::settings_catalog::launch(&object.target)?;
        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Opens Sill's own settings at the panel a setting lives on.
struct OpenSillSetting;

#[async_trait]
impl Action for OpenSillSetting {
    fn id(&self) -> &'static str {
        "sill.openOwnSetting"
    }

    fn title(&self) -> &'static str {
        "Open in Settings"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Setting
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The target IS the panel, so nothing has to be looked up.
        crate::commands::settings::open_settings(ctx.app.clone(), Some(object.target.clone()))
            .await?;
        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Runs one of the things the launcher does to itself.
struct RunBuiltin;

#[async_trait]
impl Action for RunBuiltin {
    fn id(&self) -> &'static str {
        "sill.runBuiltin"
    }

    fn title(&self) -> &'static str {
        "Run"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Builtin
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui, Capability::ClipboardWrite]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let app = &ctx.app;
        let panel = |name: &str| Some(name.to_string());

        match object.target.as_str() {
            "settings" => crate::commands::settings::open_settings(app.clone(), None).await?,
            "ask" => crate::commands::ai::open_ask(app.clone()).await?,
            "reload" => crate::reload_index(app),
            // Dismissed first: the launcher is frontmost right now, and a
            // dictation started here has to land in whatever was in front of
            // it, not in Sill.
            "dictate" => {
                crate::dismiss_main(app);
                let service = app.state::<crate::dictation::service::DictationService>();
                service.start(app).map_err(String::from)?;
            }
            "snippets" => {
                crate::commands::settings::open_settings(app.clone(), panel("snippets")).await?
            }
            // The launcher is dismissed first: this opens a native folder
            // picker, and a dialog behind a launcher that closes on blur is a
            // dialog nobody can answer.
            #[cfg(windows)]
            // Not an action on the selected row: what it reverses is
            // whatever happened last, which is usually not what is selected
            // now. An action accepting every kind turns up in every panel, and
            // two invariants said so the moment it was written that way.
            "undo-last" => {
                let taken = app
                    .state::<crate::activity::Activity>()
                    .take_last()
                    .ok_or("There is nothing to take back.")?;

                let said = crate::action::undo(ctx, &taken.1)?;
                return Ok(Outcome::done(said));
            }
            "install-extension" => {
                use tauri_plugin_dialog::DialogExt;

                crate::dismiss_main(app);

                let chosen = app
                    .dialog()
                    .file()
                    .set_title("Choose an extension folder")
                    .blocking_pick_folder();

                // Nothing chosen is not a failure. Somebody opened the picker
                // and changed their mind, which is an ordinary thing to do.
                let Some(folder) = chosen else {
                    return Ok(Outcome::done("Nothing installed"));
                };

                let source = folder
                    .into_path()
                    .map_err(|err| format!("that folder cannot be read: {err}"))?;

                // Recorded even for a folder, so "where did this come from"
                // has an answer for every installed extension rather than
                // only for the ones the store fetched.
                let origin =
                    crate::store::Origin::folder(&source, crate::state::now_seconds());

                let installed = crate::extension_install::install(app, &source, &origin)?;

                return Ok(Outcome::done(format!(
                    "Installed {} ({})",
                    installed.title,
                    installed.commands.join(", ")
                )));
            }
            "quicklinks" => {
                crate::commands::settings::open_settings(app.clone(), panel("quicklinks")).await?
            }
            "dictation-history" => {
                crate::commands::settings::open_settings(app.clone(), panel("history")).await?
            }
            "vocabulary" => {
                crate::commands::settings::open_settings(app.clone(), panel("dictation")).await?
            }
            "last-transcription" => {
                let Some(entry) = crate::dictation::history::last(app) else {
                    return Err("Nothing has been dictated yet".to_string());
                };
                let outcome = copy_with_undo(ctx, &entry.text, "Copied the last transcript")?;
                crate::dismiss_main(app);
                return Ok(outcome);
            }
            other => return Err(format!("unknown Sill command: {other}")),
        }

        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Changes something about Windows.
///
/// Its own action rather than an arm of the one that runs Sill's own commands,
/// because the two ask for different things. Opening settings touches Sill;
/// changing the volume touches the machine, and a permission screen should be
/// able to say so.
struct ToggleSystem;

#[async_trait]
impl Action for ToggleSystem {
    fn id(&self) -> &'static str {
        "sill.system.run"
    }

    fn title(&self) -> &'static str {
        "Run"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::SystemControl
    }

    fn capabilities(&self) -> &'static [Capability] {
        // Not `Ui`, which is Sill's own surface. This reaches outside it.
        &[Capability::SystemControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // Each says what it did rather than what it was asked to do, because
        // "Volume 60%" is the useful sentence and "Turned the volume up" is
        // not, and it stays true if something else changed it in between.
        let said = run_system(&object.target)?;

        // The reading is a second stale otherwise, and a second is exactly how
        // long somebody looks at a switch they just pressed.
        crate::system::forget_live();

        /*
         * A switch stays on screen. A one-shot gets out of the way.
         *
         * Flipping something and watching the row change is the point of
         * drawing it as a control: mute, dark mode, a radio and an output are
         * all things somebody may want to press twice, or press and then look
         * at. Locking the screen is not, and neither is nudging the volume
         * away from the launcher.
         */
        if crate::system::toggle_state(&object.target, &crate::system::live()).is_none() {
            crate::dismiss_main(&ctx.app);
        }

        Ok(Outcome::done(said))
    }
}

/*
 * A program's own volume.
 *
 * Five actions rather than one, because the useful things to do to a program's
 * sound are not one thing. Enter mutes and unmutes, which is the reason people
 * open a mixer; the rest sit in the panel where a second thought belongs.
 *
 * All five take the session's identifier as the target. Not the program name,
 * which several programs share, and not its process id, which is a different
 * number every time it starts.
 */

/// How far one press moves a program's volume.
///
/// A tenth, matching the system volume nudge, so the two feel like the same
/// control rather than two controls that disagree about what a step is.
const SESSION_STEP: f32 = 0.1;

/// Runs a change against one program's volume and says where it ended up.
///
/// Shared by the four that move the slider, so they cannot drift on what a
/// step is, on rounding, or on what to say afterwards.
async fn nudge(
    // Unused, and taken anyway so all five of these read the same. A capture
    // or an undo would want it and the signature should not have to change.
    _ctx: &ActionCtx,
    object: &Object,
    to: impl Fn(f32) -> f32 + Send + 'static,
) -> Result<Outcome, String> {
    let id = object.target.clone();

    let level = tokio::task::spawn_blocking(move || {
        let now = crate::app_volume::sessions()
            .into_iter()
            .find(|session| session.id == id)
            .ok_or_else(|| "that program is not playing anything any more".to_string())?;

        let level = to(now.volume).clamp(0.0, 1.0);
        crate::app_volume::set_volume(&id, level)?;

        // Unmuting as well, because a slider that moves under a mute is a
        // control that appears to do nothing.
        if level > 0.0 && now.muted {
            crate::app_volume::set_muted(&id, false)?;
        }

        Ok::<f32, String>(level)
    })
    .await
    .map_err(|err| format!("could not reach the sound system: {err}"))??;

    crate::app_volume::forget_sessions();

    Ok(Outcome::done(format!(
        "{} at {}%",
        object.title,
        (level * 100.0).round() as i32,
    )))
}

struct ToggleSessionMute;

#[async_trait]
impl Action for ToggleSessionMute {
    fn id(&self) -> &'static str {
        "sill.audio.session.mute"
    }

    fn title(&self) -> &'static str {
        "Mute or Unmute"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::AudioSession
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = object.target.clone();

        // Read and invert rather than being told which way to go. The row was
        // drawn a moment ago and something else may have changed it since.
        let muted = tokio::task::spawn_blocking(move || {
            let now = crate::app_volume::sessions()
                .into_iter()
                .find(|session| session.id == id)
                .ok_or_else(|| "that program is not playing anything any more".to_string())?;

            crate::app_volume::set_muted(&id, !now.muted)?;
            Ok::<bool, String>(!now.muted)
        })
        .await
        .map_err(|err| format!("could not reach the sound system: {err}"))??;

        crate::app_volume::forget_sessions();

        Ok(Outcome::done(format!(
            "{} is {}",
            object.title,
            if muted { "muted" } else { "audible" },
        )))
    }
}

struct SessionLouder;

#[async_trait]
impl Action for SessionLouder {
    fn id(&self) -> &'static str {
        "sill.audio.session.louder"
    }

    fn title(&self) -> &'static str {
        "Louder"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::AudioSession
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        nudge(ctx, object, |level| level + SESSION_STEP).await
    }
}

struct SessionQuieter;

#[async_trait]
impl Action for SessionQuieter {
    fn id(&self) -> &'static str {
        "sill.audio.session.quieter"
    }

    fn title(&self) -> &'static str {
        "Quieter"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::AudioSession
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        nudge(ctx, object, |level| level - SESSION_STEP).await
    }
}

struct SessionHalf;

#[async_trait]
impl Action for SessionHalf {
    fn id(&self) -> &'static str {
        "sill.audio.session.half"
    }

    fn title(&self) -> &'static str {
        "Half Volume"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::AudioSession
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        nudge(ctx, object, |_| 0.5).await
    }
}

struct SessionFull;

#[async_trait]
impl Action for SessionFull {
    fn id(&self) -> &'static str {
        "sill.audio.session.full"
    }

    fn title(&self) -> &'static str {
        "Full Volume"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::AudioSession
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        nudge(ctx, object, |_| 1.0).await
    }
}

/// Flips one system switch, and says what the machine is now doing.
///
/// A toggle reads the current state and inverts it rather than being told,
/// because the row was drawn a moment ago and the answer could have changed
/// since. Being told would let a stale row turn the sound off twice.
/// What a radio row's id starts with, before which radio it is.
pub(crate) const RADIO: &str = "system.radio:";

/// What an audio output row's id starts with, before the device's own.
pub(crate) const AUDIO_OUTPUT: &str = "system.audio.output:";

fn run_system(id: &str) -> Result<String, String> {
    use crate::system;

    match id {
        "system.volume.up" => Ok(format!("Volume {}%", system::nudge_volume(true)?)),
        "system.volume.down" => Ok(format!("Volume {}%", system::nudge_volume(false)?)),
        "system.volume.half" => Ok(format!("Volume {}%", system::set_volume(50)?)),
        "system.volume.max" => Ok(format!("Volume {}%", system::set_volume(100)?)),

        "system.mute" => {
            let now = system::muted().unwrap_or(false);
            let set = system::set_muted(!now)?;

            Ok(if set { "Sound off".into() } else { "Sound on".into() })
        }

        "system.theme" => {
            let now = system::dark().unwrap_or(false);
            let set = system::set_dark(!now)?;

            Ok(if set { "Dark mode".into() } else { "Light mode".into() })
        }

        /*
         * An output row carries the device's own id after the prefix.
         *
         * Matched by prefix rather than listed, because which outputs exist is
         * a fact about the machine at the moment somebody plugged something
         * in, not something that can be written down here.
         */
        other if other.starts_with(AUDIO_OUTPUT) => {
            let device = &other[AUDIO_OUTPUT.len()..];

            let name = crate::audio::outputs()
                .into_iter()
                .find(|output| output.id == device)
                .map(|output| crate::audio::short_name(&output.name))
                .unwrap_or_else(|| "that output".to_string());

            crate::audio::set_output(device)?;
            Ok(format!("Sound goes to {name}"))
        }

        /*
         * A radio row carries which one after the prefix, and toggles it.
         *
         * Read then set, rather than remembering: something else may have
         * switched it since the index was built, and a row that says "on" on a
         * radio that is off would turn it off again.
         */
        other if other.starts_with(RADIO) => {
            let kind = &other[RADIO.len()..];

            let now = crate::radios::radios()
                .into_iter()
                .find(|radio| radio.kind == kind)
                .map(|radio| radio.on)
                .ok_or_else(|| "there is no such radio in this machine".to_string())?;

            let set = crate::radios::set_radio(kind, !now)?;
            let name = if kind == "wifi" { "Wi-Fi" } else { "Bluetooth" };

            Ok(format!("{name} {}", if set { "on" } else { "off" }))
        }

        "system.lock" => {
            system::lock()?;
            Ok("Locked".into())
        }

        other => Err(format!("unknown system command: {other}")),
    }
}

// ------------------------------------------------------------------- text

/// Expands a snippet and pastes it where the launcher was.
struct PasteSnippet;

#[async_trait]
impl Action for PasteSnippet {
    fn id(&self) -> &'static str {
        "sill.pasteSnippet"
    }

    fn title(&self) -> &'static str {
        "Paste Snippet"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Snippet
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite, Capability::InputInjection]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let expansion =
            crate::snippets::commands::expand_snippet(ctx.app.clone(), object.target.clone())?;

        ctx.app
            .clipboard()
            .write_text(expansion.text)
            .map_err(|err| format!("Could not copy the snippet: {err}"))?;

        // No undo. The text has already gone into somebody else's window, and
        // sending Ctrl+Z on their behalf would be a guess about an
        // application Sill knows nothing about.
        crate::dictation::paste::deliver(&ctx.app);
        Ok(Outcome::done(format!("Pasted {}", object.title)))
    }
}

/// Puts an emoji where the user was typing.
///
/// Pasting rather than copying, because that is what an emoji picker is for:
/// you were writing something and you wanted a face in it. Copying and leaving
/// the user to paste is one more step in the middle of a sentence.
///
/// Copy is still offered beside it, from the same action that copies any text.
struct PasteEmoji;

#[async_trait]
impl Action for PasteEmoji {
    fn id(&self) -> &'static str {
        "sill.emoji.paste"
    }

    fn title(&self) -> &'static str {
        "Paste"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Emoji
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite, Capability::InputInjection]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        ctx.app
            .clipboard()
            .write_text(object.target.clone())
            .map_err(|err| format!("Could not copy the emoji: {err}"))?;

        // No undo. It has gone into somebody else's window, and sending Ctrl+Z
        // on their behalf would be a guess about an application Sill knows
        // nothing about.
        crate::dictation::paste::deliver(&ctx.app);
        Ok(Outcome::done(format!("Pasted {}", object.target)))
    }
}

/// Opens a saved link.
struct OpenQuicklink;

#[async_trait]
impl Action for OpenQuicklink {
    fn id(&self) -> &'static str {
        "sill.openQuicklink"
    }

    fn title(&self) -> &'static str {
        "Open Link"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Quicklink
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // A link with a hole in it never reaches here: the window keeps it,
        // collects the text and calls `open_quicklink` itself, because the
        // asking is the feature.
        crate::quicklinks::commands::open_quicklink(
            ctx.app.clone(),
            object.target.clone(),
            String::new(),
        )?;

        crate::dismiss_main(&ctx.app);
        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Copies a calculator result.
struct CopyAnswer;

#[async_trait]
impl Action for CopyAnswer {
    fn id(&self) -> &'static str {
        "sill.copyAnswer"
    }

    fn title(&self) -> &'static str {
        "Copy Answer"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Answer
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The target is the result itself. Nothing is spawned and nothing is
        // indexed; the answer exists only while it is on screen.
        let outcome = copy_with_undo(ctx, &object.target, "Copied the answer")?;
        crate::dismiss_main(&ctx.app);
        Ok(outcome)
    }
}


/// Opens a terminal where a file lives.
///
/// The folder, always, even when a file was chosen: nobody means "open a
/// terminal inside README.md". A file's parent is what they meant.
struct TerminalHere;

#[async_trait]
impl Action for TerminalHere {
    fn id(&self) -> &'static str {
        "sill.file.terminal"
    }

    fn title(&self) -> &'static str {
        "Open Terminal Here"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let here = folder_of(&object.target)?;

        // Whichever terminal the machine has, best first. `wt` is the one
        // people who have it want; `powershell` is on every Windows; `cmd` is
        // the one that cannot be missing.
        let opened = ["wt.exe", "powershell.exe", "cmd.exe"]
            .into_iter()
            .find_map(|program| {
                let mut command = std::process::Command::new(program);

                // Each wants the starting folder said differently, and the
                // difference is not cosmetic: passing the wrong one silently
                // opens in the home folder.
                match program {
                    "wt.exe" => command.arg("-d").arg(&here),
                    other => {
                        command.current_dir(&here);
                        let _ = other;
                        &mut command
                    }
                };

                command.spawn().ok().map(|_| program)
            })
            .ok_or_else(|| "No terminal on this machine would start.".to_string())?;

        let _ = opened;
        Ok(Outcome::done(format!(
            "Opened a terminal in {}",
            name_of(&here)
        )))
    }
}

/// Sends a file to the recycle bin.
///
/// The one destructive thing here, and it is the recoverable kind on purpose.
/// Deleting outright is what a file manager is for; a launcher offering it
/// behind a fuzzy search and one keypress is how somebody loses work they
/// cannot get back.
struct RecycleFile;

#[async_trait]
impl Action for RecycleFile {
    fn id(&self) -> &'static str {
        "sill.file.recycle"
    }

    fn title(&self) -> &'static str {
        "Move to Recycle Bin"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);

        if !path.exists() {
            return Err(format!("{} is not there any more.", object.title));
        }

        recycle(&path)?;

        // No undo token. The recycle bin **is** the undo, it is where people
        // already know to look, and a token claiming to restore something the
        // system already holds would be a second answer to one question.
        Ok(Outcome::done(format!(
            "Moved {} to the recycle bin",
            name_of(&object.target)
        )))
    }
}

/// The folder a path is in, or the path itself when it is one.
pub fn folder_of(target: &str) -> Result<String, String> {
    let path = std::path::Path::new(target);

    if path.is_dir() {
        return Ok(target.to_string());
    }

    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{target} is not somewhere a terminal can open"))
}

/// The last part of a path, for saying what happened to it.
pub fn name_of(target: &str) -> String {
    std::path::Path::new(target)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string())
}

/// Hands a path to the shell's recycle bin.
///
/// `SHFileOperationW` rather than a delete, because the recycle bin is the
/// whole point: it is undo that outlives the process, that survives a crash,
/// and that people already know how to use.
///
/// The path is double null terminated. The API takes a list of paths and reads
/// until it finds an empty one, so a single terminator means it keeps reading
/// past the end of the string.
#[cfg(windows)]
pub fn recycle(path: &std::path::Path) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };

    use std::os::windows::ffi::OsStrExt;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0, 0]).collect();

    let mut operation = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE as u32,
        pFrom: PCWSTR(wide.as_ptr()),
        // The flags are declared wider than the field that holds them, which
        // is an oddity of the API rather than of the bindings.
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT).0 as u16,
        ..Default::default()
    };

    // SAFETY: `pFrom` points at a double null terminated buffer that outlives
    // the call, and every other field is either set above or zeroed.
    let code = unsafe { SHFileOperationW(&mut operation) };

    if code != 0 {
        return Err(format!("Windows refused to recycle that (error {code})"));
    }

    // Set when somebody cancelled a dialog. Not an error, but not a deletion
    // either, and reporting success would be a lie.
    if operation.fAnyOperationsAborted.as_bool() {
        return Err("That was not moved to the recycle bin.".to_string());
    }

    Ok(())
}

#[cfg(not(windows))]
fn recycle(_path: &std::path::Path) -> Result<(), String> {
    Err("Only Windows has a recycle bin.".to_string())
}

// --------------------------------------------------------------- secondary

/// Copies where something lives.
struct CopyPath;

#[async_trait]
impl Action for CopyPath {
    fn id(&self) -> &'static str {
        "sill.copyPath"
    }

    fn title(&self) -> &'static str {
        "Copy Path"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::Application | ObjectKind::File | ObjectKind::Folder
        )
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        if object.target.starts_with(crate::apps::APPS_FOLDER) {
            // A packaged app is an AppUserModelID rather than a path, and
            // pasting one into a terminal does nothing useful.
            return Err(format!("{} has no path on disk", object.title));
        }

        copy_with_undo(ctx, &object.target, "Copied the path")
    }
}

/// Opens the folder something sits in, with it selected.
struct RevealInFolder;

#[async_trait]
impl Action for RevealInFolder {
    fn id(&self) -> &'static str {
        "sill.revealInFolder"
    }

    fn title(&self) -> &'static str {
        "Show in Folder"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::Application | ObjectKind::File | ObjectKind::Folder
        )
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        if object.target.starts_with(crate::apps::APPS_FOLDER) {
            return Err(format!("{} has no folder on disk", object.title));
        }

        // `/select,` needs the path as one argument and Explorer is famously
        // particular about the comma being attached to it.
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", object.target))
            .spawn()
            .map_err(|err| format!("could not open the folder: {err}"))?;

        crate::dismiss_main(&ctx.app);
        Ok(Outcome::done("Opened the folder"))
    }
}

/// Copies what a thing is called.
struct CopyName;

#[async_trait]
impl Action for CopyName {
    fn id(&self) -> &'static str {
        "sill.copyName"
    }

    fn title(&self) -> &'static str {
        "Copy Name"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        // Everything has a name, including the things that have nothing else:
        // a builtin, an extension command, a setting.
        !matches!(kind, ObjectKind::Answer)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        copy_with_undo(ctx, &object.title, "Copied the name")
    }
}

// -------------------------------------------------------------- transforms

/// One text transform, described rather than hand-written.
///
/// Every one of these is the same action with a different function in the
/// middle: take the text, change it, put it back on the clipboard, offer to
/// undo. Writing eleven near-identical impls would be eleven places to fix
/// the day the undo behaviour changes.
struct Transform {
    id: &'static str,
    title: &'static str,
    apply: fn(&str) -> Result<String, String>,
}

#[async_trait]
impl Action for Transform {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        // Anything that *is* text. A clipboard row and a selection are the
        // same thing to a transform, which is the point of dispatching on a
        // kind rather than on where the text came from.
        matches!(kind, ObjectKind::ClipboardEntry | ObjectKind::Text)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let changed = (self.apply)(&object.target)?;

        if changed == object.target {
            // Saying "Uppercased" over text that was already uppercase reads
            // as the action having done nothing, which it did.
            return Ok(Outcome::done("Already like that"));
        }

        // The text goes back with the outcome so a shortcut can put it where
        // the original came from. Copying is what happens when there is
        // nowhere better to put it.
        Ok(copy_with_undo(ctx, &changed, self.title)?.producing(changed))
    }
}

/// Puts a saved arrangement back. P2.12.
struct RestoreWorkspace;

#[async_trait]
impl Action for RestoreWorkspace {
    fn id(&self) -> &'static str {
        "sill.workspace.restore"
    }

    fn title(&self) -> &'static str {
        "Restore"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Workspace
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let moved =
            crate::commands::system::restore_workspace(ctx.app.clone(), object.target.clone())?;

        Ok(Outcome::done(match moved {
            0 => "Nothing from that workspace is open".to_string(),
            1 => "Put one window back".to_string(),
            many => format!("Put {many} windows back"),
        }))
    }
}

/// Forgets a saved arrangement. P2.12.
struct ForgetWorkspace;

#[async_trait]
impl Action for ForgetWorkspace {
    fn id(&self) -> &'static str {
        "sill.workspace.forget"
    }

    fn title(&self) -> &'static str {
        "Forget"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Workspace
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::commands::system::forget_workspace(ctx.app.clone(), object.target.clone())?;
        Ok(Outcome::done(format!("Forgot {}", object.title)))
    }
}

/// Reads text out loud. P2.4.
///
/// Accepts exactly what a transform accepts, because a clipboard row and a
/// selection are the same thing to a voice as they are to a rewrite. Nothing
/// about where the text came from matters once it is text.
struct ReadAloud;

#[async_trait]
impl Action for ReadAloud {
    fn id(&self) -> &'static str {
        "sill.text.readAloud"
    }

    fn title(&self) -> &'static str {
        "Read Aloud"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::ClipboardEntry | ObjectKind::Text)
    }

    /// Nothing is read, written or launched. It makes a sound, which is the
    /// one thing this list has no name for and does not need one: a sound is
    /// not a capability anything has to be protected from.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::tts::aloud(&ctx.app, &object.target).await?;

        Ok(Outcome::done("Reading aloud"))
    }
}

/// Stops mid-sentence.
///
/// Its own action rather than a second press of the first one, because the
/// thing being read is usually no longer on screen by the time somebody wants
/// silence: they have moved on, which is why they want it to stop. Bindable to
/// a key for the same reason.
struct StopReading;

#[async_trait]
impl Action for StopReading {
    fn id(&self) -> &'static str {
        "sill.text.stopReading"
    }

    fn title(&self) -> &'static str {
        "Stop Reading"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::ClipboardEntry | ObjectKind::Text)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    async fn run(&self, ctx: &ActionCtx, _object: &Object) -> Result<Outcome, String> {
        crate::tts::stop(&ctx.app)?;
        Ok(Outcome::done("Stopped"))
    }
}

/// Every transform, in the order the action panel shows them.
///
/// Case first because it is what gets reached for most, then the encodings,
/// then JSON. Deliberately a short list: a launcher that offers forty text
/// operations has buried the four anybody uses.
fn transforms() -> Vec<Box<dyn Action>> {
    let entries: [(
        &'static str,
        &'static str,
        fn(&str) -> Result<String, String>,
    ); 9] = [
        ("sill.text.upper", "Upper Case", |s| {
            Ok(crate::text::upper(s))
        }),
        ("sill.text.lower", "Lower Case", |s| {
            Ok(crate::text::lower(s))
        }),
        ("sill.text.title", "Title Case", |s| {
            Ok(crate::text::title_case(s))
        }),
        ("sill.text.tidy", "Tidy Lines", |s| {
            Ok(crate::text::tidy_lines(s))
        }),
        ("sill.text.base64Encode", "Base64 Encode", |s| {
            Ok(crate::text::base64_encode(s))
        }),
        (
            "sill.text.base64Decode",
            "Base64 Decode",
            crate::text::base64_decode,
        ),
        ("sill.text.urlEncode", "URL Encode", |s| {
            Ok(crate::text::url_encode(s))
        }),
        ("sill.text.urlDecode", "URL Decode", crate::text::url_decode),
        (
            "sill.text.jsonPretty",
            "Format JSON",
            crate::text::json_pretty,
        ),
    ];

    entries
        .into_iter()
        .map(|(id, title, apply)| Box::new(Transform { id, title, apply }) as Box<dyn Action>)
        .collect()
}

/// Puts a clipboard row back on the clipboard, unchanged.
///
/// The primary action for a clipboard entry, and the one thing every other
/// transform is a variation on.
struct CopyClipboardEntry;

#[async_trait]
impl Action for CopyClipboardEntry {
    fn id(&self) -> &'static str {
        "sill.clipboard.copy"
    }

    fn title(&self) -> &'static str {
        "Copy"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::ClipboardEntry | ObjectKind::Text | ObjectKind::Emoji
        )
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        // Not for an emoji: pasting is what a picker is for, and two primaries
        // for one kind is a registry that cannot say what Enter does.
        self.accepts(kind) && kind != ObjectKind::Emoji
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        copy_with_undo(ctx, &object.target, "Copied")
    }
}

// ------------------------------------------------------------------ windows

/// The handle behind a window object, or a reason it is no longer one.
fn window_handle(object: &Object) -> Result<isize, String> {
    object
        .target
        .parse::<isize>()
        .map_err(|_| format!("{} is not a window", object.title))
}

/// What a window looked like before an action moved it.
///
/// Read before the move rather than reconstructed after, because after the
/// move the old rectangle is gone and there is nothing to reconstruct it from.
fn window_undo(id: isize) -> Option<Undo> {
    let window = crate::windowing::find(id)?;
    Some(Undo::RestoreWindow {
        id,
        rect: window.rect,
        maximized: window.maximized,
        title: window.title.clone(),
    })
}

/// Brings a window to the front.
struct FocusWindow;

#[async_trait]
impl Action for FocusWindow {
    fn id(&self) -> &'static str {
        "sill.window.focus"
    }

    fn title(&self) -> &'static str {
        "Switch To"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::windowing::focus(window_handle(object)?)?;
        Ok(Outcome::done(format!("Switched to {}", object.title)))
    }
}

/// Asks a window to close, the way its own close button does.
struct CloseWindow;

#[async_trait]
impl Action for CloseWindow {
    fn id(&self) -> &'static str {
        "sill.window.close"
    }

    fn title(&self) -> &'static str {
        "Close Window"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    /// No undo, and there must not be one.
    ///
    /// A closed window cannot be reopened, and offering an undo that reopens
    /// the *application* would be a lie about what was restored. The
    /// application gets to prompt about unsaved work; Sill does not second
    /// guess that.
    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::windowing::close(window_handle(object)?)?;
        Ok(Outcome::done(format!("Asked {} to close", object.title)))
    }
}

/// Minimize, maximize and restore, which are one action with three settings.
struct WindowState {
    id: &'static str,
    title: &'static str,
    apply: fn(isize) -> Result<(), String>,
    said: &'static str,
}

#[async_trait]
impl Action for WindowState {
    fn id(&self) -> &'static str {
        self.id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = window_handle(object)?;
        let undo = window_undo(id);

        (self.apply)(id)?;

        let message = format!("{} {}", object.title, self.said);
        Ok(match undo {
            Some(undo) => Outcome::undoable(message, undo),
            None => Outcome::done(message),
        })
    }
}

/// Sends a window to a named position on the display it is already on.
struct SnapWindow {
    slot: crate::windowing::Slot,
}

#[async_trait]
impl Action for SnapWindow {
    fn id(&self) -> &'static str {
        self.slot.action_id()
    }

    fn title(&self) -> &'static str {
        self.slot.title()
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = window_handle(object)?;

        // Read before the move. Afterwards the old rectangle is gone.
        let undo = window_undo(id);
        crate::windowing::snap(id, self.slot)?;

        let message = format!("{} sent to the {}", object.title, self.slot.title());
        Ok(match undo {
            Some(undo) => Outcome::undoable(message, undo),
            None => Outcome::done(message),
        })
    }
}

/// Moves a window to the next display, keeping its position proportionally.
struct NextDisplay;

#[async_trait]
impl Action for NextDisplay {
    fn id(&self) -> &'static str {
        "sill.window.nextDisplay"
    }

    fn title(&self) -> &'static str {
        "Move to Next Display"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = window_handle(object)?;
        let window =
            crate::windowing::find(id).ok_or_else(|| format!("{} has closed", object.title))?;

        let undo = window_undo(id);
        crate::windowing::send_to_monitor(id, window.monitor + 1)?;

        let message = format!("{} moved to the next display", object.title);
        Ok(match undo {
            Some(undo) => Outcome::undoable(message, undo),
            None => Outcome::done(message),
        })
    }
}

/// Everything that can be done to a window.
///
/// Switch first because it is what a window in a result list is for. The
/// layout slots come after the states, in the order [`Slot::ALL`] lists them,
/// so the panel reads halves, quarters, thirds rather than alphabetically.
fn window_actions() -> Vec<Box<dyn Action>> {
    let mut actions: Vec<Box<dyn Action>> = vec![
        Box::new(FocusWindow),
        Box::new(WindowState {
            id: "sill.window.minimize",
            title: "Minimize",
            apply: crate::windowing::minimize,
            said: "minimized",
        }),
        Box::new(WindowState {
            id: "sill.window.maximize",
            title: "Maximize",
            apply: crate::windowing::maximize,
            said: "maximized",
        }),
        Box::new(WindowState {
            id: "sill.window.restore",
            title: "Restore",
            apply: crate::windowing::restore,
            said: "restored",
        }),
    ];

    actions.extend(
        crate::windowing::Slot::ALL
            .into_iter()
            .map(|slot| Box::new(SnapWindow { slot }) as Box<dyn Action>),
    );

    actions.push(Box::new(NextDisplay));
    // Last on purpose. Closing is the one thing here that cannot be undone,
    // and it should not sit next to the arrow keys.
    actions.push(Box::new(CloseWindow));
    actions
}

/// Opens a picture from the clipboard for marking up.
///
/// The same shape as reading the words out of one: a picture is already in the
/// history, so this works on any of them rather than only on a fresh capture.
struct MarkUp;

#[async_trait]
impl Action for MarkUp {
    fn id(&self) -> &'static str {
        "sill.markUp"
    }

    fn title(&self) -> &'static str {
        "Mark Up"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ClipboardEntry
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::Ui]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let entry: i64 = object
            .id
            .parse()
            .map_err(|_| "that clipboard row cannot be looked up".to_string())?;

        crate::commands::system::open_markup(ctx.app.clone(), entry).await?;

        // No undo: nothing has changed yet. The window is open and whatever
        // comes out of it is a new picture on the clipboard, which the
        // clipboard's own history already keeps.
        Ok(Outcome::done("Opened for markup".to_string()))
    }
}

/// Reads the words out of a picture on the clipboard.
///
/// Screenshot something, then take the text out of it. The picture is already
/// in the clipboard history, so there is no capture surface to build and
/// nothing new to point at a screen.
///
/// Only ever when it is asked for. Reading every image that passed through the
/// clipboard would be a transcription service running over whatever happened
/// to be copied, which is not a thing to switch on by default.
struct ExtractText;

#[async_trait]
impl Action for ExtractText {
    fn id(&self) -> &'static str {
        "sill.extractText"
    }

    fn title(&self) -> &'static str {
        "Extract Text"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ClipboardEntry
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The row carries its own row number, which is what reaches the
        // picture: the target is the entry's text, and an image has none.
        let entry: i64 = object
            .id
            .parse()
            .map_err(|_| "that clipboard row cannot be looked up".to_string())?;

        let clipboard = ctx
            .app
            .try_state::<crate::clipboard::monitor::Clipboard>()
            .ok_or_else(|| "clipboard history is not running".to_string())?;

        let png = clipboard
            .store()
            .blob(entry)
            .map_err(|err| format!("could not read that entry: {err}"))?
            .ok_or_else(|| "there is no picture on that row to read".to_string())?;

        // Off the async worker: decoding and recognition are both a solid
        // chunk of blocking work, and this is the one call that does any.
        let text = tokio::task::spawn_blocking(move || {
            let (pixels, width, height) = crate::ocr::bgra_from_png(&png)?;
            crate::ocr::read_bgra(&pixels, width, height)
        })
        .await
        .map_err(|err| format!("reading that picture failed: {err}"))??;

        let text = text.trim().to_string();
        if text.is_empty() {
            // Not an error. Plenty of pictures have no words in them, and
            // saying so is more useful than an empty clipboard.
            return Ok(Outcome::done("No text in that picture".to_string()));
        }

        let words = text.split_whitespace().count();
        copy_with_undo(ctx, &text, &format!("Copied {words} word(s)"))
    }
}

/// Looks words up on the web.
///
/// The address is built here rather than by whatever offered the row, because
/// which engine to use is a setting and the escaping is the part that is easy
/// to get wrong. The window carries the words; Rust decides what they mean.
struct SearchWeb;

#[async_trait]
impl Action for SearchWeb {
    fn id(&self) -> &'static str {
        "sill.searchWeb"
    }

    fn title(&self) -> &'static str {
        "Search the Web"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Search
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let settings = {
            let prefs = ctx.app.state::<crate::state::PrefsState>();
            let held = prefs.inner.lock().await;
            held.web_search.clone()
        };

        let url = crate::websearch::url_for(&settings.engine, &settings.custom_url, &object.target);

        tauri_plugin_opener::open_url(url, None::<&str>)
            .map_err(|err| format!("Could not open the search: {err}"))?;

        crate::dismiss_main(&ctx.app);
        Ok(Outcome::done(format!("Searched for {}", object.target)))
    }
}

/// Opens a web address.
///
/// The address already exists somewhere else, which is what separates this from
/// opening a saved link: there is no name, no owner and nothing to fill in, so
/// there is nothing to do but hand it to whichever browser is the default.
struct OpenUrl;

#[async_trait]
impl Action for OpenUrl {
    fn id(&self) -> &'static str {
        "sill.openUrl"
    }

    fn title(&self) -> &'static str {
        "Open in Browser"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Url
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        tauri_plugin_opener::open_url(object.target.clone(), None::<&str>)
            .map_err(|err| format!("Could not open that address: {err}"))?;

        crate::dismiss_main(&ctx.app);

        // No undo. Opening a page is not a change to anything Sill could put
        // back, and closing whatever the browser did with it is not Sill's to
        // do either.
        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Copies a web address.
struct CopyUrl;

#[async_trait]
impl Action for CopyUrl {
    fn id(&self) -> &'static str {
        "sill.copyUrl"
    }

    fn title(&self) -> &'static str {
        "Copy Address"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Url
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        ctx.app
            .clipboard()
            .write_text(object.target.clone())
            .map_err(|err| format!("Could not copy the address: {err}"))?;

        Ok(Outcome::done("Copied the address".to_string()))
    }
}

/// Copies a file's SHA-256.
///
/// The question this answers is "is this the same file as that one", which
/// comes up when checking a download against what its publisher published.
///
/// Read in blocks rather than into memory: an installer is hundreds of
/// megabytes and there is no reason for any of it to be resident at once.
struct HashFile;

#[async_trait]
impl Action for HashFile {
    fn id(&self) -> &'static str {
        "sill.file.hash"
    }

    fn title(&self) -> &'static str {
        "Copy SHA-256"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::File
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);
        let name = crate::files_ops::name_of(&path);

        // Blocking and unbounded in time: a large file is a real read, and it
        // has no business on an async worker.
        let digest = tokio::task::spawn_blocking(move || crate::files_ops::sha256(&path))
            .await
            .map_err(|err| format!("could not read that file: {err}"))??;

        copy_with_undo(ctx, &digest, &format!("Copied the SHA-256 of {name}"))
    }
}

/// Puts a file or folder into a zip beside it.
///
/// Beside it rather than somewhere chosen, because choosing means a dialog and
/// the overwhelmingly common answer is "here". The name is the file's own, and
/// a number is added rather than overwriting anything.
struct CompressFile;

#[async_trait]
impl Action for CompressFile {
    fn id(&self) -> &'static str {
        "sill.file.compress"
    }

    fn title(&self) -> &'static str {
        "Compress"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileRead, Capability::FileWrite]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);
        let name = crate::files_ops::name_of(&path);

        let made = tokio::task::spawn_blocking(move || crate::files_ops::compress(&path))
            .await
            .map_err(|err| format!("could not compress that: {err}"))??;

        let into = crate::files_ops::name_of(&made);

        // Undoable, because this made a file that was not there before and
        // nothing else was touched. Deleting it puts things back exactly.
        Ok(Outcome::undoable(
            format!("Compressed {name} into {into}"),
            Undo::DeleteFile {
                path: made.to_string_lossy().into_owned(),
                name: into,
            },
        ))
    }
}

/// Renames a file, using the launcher's field to ask for the new name.
///
/// The asking is the feature, so this action never runs from the panel with a
/// name it guessed: the window takes over the field first, exactly as it does
/// for a quicklink with a hole in it.
struct RenameFile;

#[async_trait]
impl Action for RenameFile {
    fn id(&self) -> &'static str {
        "sill.file.rename"
    }

    fn title(&self) -> &'static str {
        "Rename"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    /// The window collects the new name and calls `rename_path`, so reaching
    /// here means something dispatched it without asking.
    async fn run(&self, _ctx: &ActionCtx, _object: &Object) -> Result<Outcome, String> {
        Err("renaming needs a new name, which the launcher asks for".to_string())
    }
}

/// Moves a file or folder somewhere else.
///
/// Refuses to run for the reason renaming does: it needs a destination, and an
/// action is handed one object and acts. The launcher asks where first and
/// then calls `move_path`. Reaching here means something dispatched it without
/// asking, and doing nothing loudly is better than guessing at a folder.
struct MoveFile;

#[async_trait]
impl Action for MoveFile {
    fn id(&self) -> &'static str {
        "sill.file.move"
    }

    fn title(&self) -> &'static str {
        // Not "Move To". The panel filters by substring, so that is a prefix
        // of "Move to Recycle Bin" and typing "move to" put the one that
        // removes the file above the one that moves it. The extra word is the
        // difference between the two.
        "Move to Folder"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        // Read as well: between two drives this copies before it removes.
        &[Capability::FileRead, Capability::FileWrite]
    }

    async fn run(&self, _ctx: &ActionCtx, _object: &Object) -> Result<Outcome, String> {
        Err("moving needs somewhere to move to, which the launcher asks for".to_string())
    }
}

/// Checks a file against the checksum on the clipboard.
///
/// This is what somebody actually wants when a download page prints a
/// checksum: a yes or a no. Copying the file's own hash leaves them comparing
/// sixty-four hex characters by eye, which is the step people skip and the
/// reason checksums go unchecked.
///
/// The expected value is taken from the clipboard because that is where it
/// already is, a moment after being copied off the page that published it.
struct VerifyFile;

#[async_trait]
impl Action for VerifyFile {
    fn id(&self) -> &'static str {
        "sill.file.verify"
    }

    fn title(&self) -> &'static str {
        "Check Against Copied Checksum"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::File
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileRead, Capability::ClipboardRead]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        let expected = ctx.app.clipboard().read_text().unwrap_or_default();

        let Some(kind) = crate::files_ops::looks_like_checksum(&expected) else {
            return Err(
                "copy the checksum you want to check against first, then run this".to_string(),
            );
        };

        // Saying which kind beats saying "does not match". Comparing a SHA-256
        // against a SHA-1 is the wrong question rather than a failure, and
        // reporting it as a mismatch sends somebody off to re-download a file
        // that was fine.
        if kind != crate::files_ops::Checksum::Sha256 {
            return Err(format!(
                "that is a {} and Sill checks SHA-256",
                kind.name()
            ));
        }

        let path = std::path::PathBuf::from(&object.target);
        let name = crate::files_ops::name_of(&path);

        let actual = tokio::task::spawn_blocking(move || crate::files_ops::sha256(&path))
            .await
            .map_err(|err| format!("could not read that file: {err}"))??;

        if crate::files_ops::same_checksum(&actual, &expected) {
            Ok(Outcome::done(format!("{name} matches the copied checksum")))
        } else {
            // An error rather than a message, so it is not read as a success
            // at a glance. This is the answer somebody most needs to notice.
            Err(format!("{name} does NOT match the copied checksum"))
        }
    }
}

/// Looks a file up by its checksum, without sending the file anywhere.
///
/// The address carries the hash, so the service is asked whether it has seen
/// this file before rather than being given a copy of it. That distinction is
/// the point: it works on something confidential, and uploading would not.
///
/// A file nobody has ever submitted simply has no page, which is itself an
/// answer worth having.
struct LookUpFile;

#[async_trait]
impl Action for LookUpFile {
    fn id(&self) -> &'static str {
        "sill.file.lookUp"
    }

    fn title(&self) -> &'static str {
        "Look Up on VirusTotal"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::File
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileRead, Capability::Network, Capability::ProcessLaunch]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);
        let name = crate::files_ops::name_of(&path);

        let digest = tokio::task::spawn_blocking(move || crate::files_ops::sha256(&path))
            .await
            .map_err(|err| format!("could not read that file: {err}"))??;

        tauri_plugin_opener::open_url(
            format!("https://www.virustotal.com/gui/file/{digest}"),
            None::<&str>,
        )
        .map_err(|err| format!("could not open that: {err}"))?;

        crate::dismiss_main(&ctx.app);

        Ok(Outcome::done(format!("Looking up {name} by its checksum")))
    }
}
