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
    ActionRegistry::new(vec![
        Box::new(Launch),
        Box::new(RunExtensionCommand),
        Box::new(OpenSystemSetting),
        Box::new(OpenSillSetting),
        Box::new(RunBuiltin),
        Box::new(PasteSnippet),
        Box::new(OpenQuicklink),
        Box::new(CopyAnswer),
        Box::new(CopyPath),
        Box::new(RevealInFolder),
        Box::new(CopyName),
    ])
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
        let mode = if record.mode == "no-view" {
            crate::exthost::CommandMode::NoView
        } else {
            crate::exthost::CommandMode::View
        };

        let hosts = ctx.app.state::<crate::state::HostState>();
        let host = crate::host::host_of(&hosts).await?;

        let opts = crate::exthost::LoadOptions::with_preferences(
            record.entrypoint.clone(),
            &record.extension,
            &record.command,
            mode,
            record.preferences.clone(),
        );

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
