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

pub mod extension;
pub mod mcp;

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
        Box::new(RunScript),
        Box::new(ExtractText),
        Box::new(ReadQr),
        Box::new(ConvertImage {
            to: crate::images::Format::Png,
        }),
        Box::new(ConvertImage {
            to: crate::images::Format::Jpeg,
        }),
        Box::new(MarkUp),
        Box::new(SearchWeb),
        Box::new(OpenUrl),
        // Play or Pause claims Enter and so is lifted to the front whatever
        // this order says. Enter is the key somebody presses without looking,
        // and on a row that appeared because they typed "pause", stopping the
        // noise is what they meant by it; skipping a track is not undoable by
        // pressing the same key again.
        //
        // What this position does decide is the pair staying together. Both
        // are above "Copy Name", the only other action a media row has,
        // because a panel of three reading play, copy, next has put something
        // unrelated between the two halves of one control.
        Box::new(PlayPause),
        Box::new(NextTrack),
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
        // Beside the other action that means "take this path somewhere",
        // rather than beside the ones that copy it. Both are things you do
        // with a folder once you have found it.
        Box::new(JumpInDialog),
        Box::new(OpenTerminalProfile),
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
        // Quit above Force Quit, and it is this order plus `is_primary` that
        // puts them that way round on the row rather than a comment saying so.
        // The one that lets a program save its work is what Enter runs and
        // what the panel offers first; the one that destroys unsaved work is
        // below it, reached deliberately.
        Box::new(QuitProcess),
        Box::new(ForceQuitProcess),
        // Below every action that opens or copies, for the reason "Move to
        // Recycle Bin" is: the panel is drawn in this order and the entry that
        // removes a program should not sit above the ones that do not.
        Box::new(UninstallApp),
        Box::new(RestoreWorkspace),
        Box::new(MakeWorkspacePortable),
        Box::new(ForgetWorkspace),
        Box::new(ForgetConversation),
        Box::new(CopyConversation),
        Box::new(CopyStoreSource),
        // Below the one that copies a link, for the reason Uninstall sits
        // below everything on an application: the panel is drawn in this
        // order and the entry that removes something should not be the one
        // under the cursor when it opens.
        Box::new(RemoveExtension),
        Box::new(RenameClipboardEntry),
        Box::new(EditClipboardEntry),
        Box::new(CopyFontName),
        Box::new(SetDisplayMode),
        Box::new(PlaceInLayout),
        Box::new(ReadAloud),
        Box::new(StopReading),
        // Notes and reminders, both behind their own gates rather than this
        // list: a note action refuses when notes are switched off, and setting
        // a reminder is refused to a trigger by `may_schedule`. Being in the
        // registry is what makes them reachable by a key, by the model and by
        // the command line Windows starts when a timer fires.
        Box::new(OpenNote),
        Box::new(CopyNote),
        Box::new(SetReminder),
        Box::new(ShowReminder),
    ];

    ActionRegistry::new(
        core.into_iter()
            .chain(transforms())
            .chain(std::iter::once(Box::new(SwitchToTab) as Box<dyn Action>))
            .chain(std::iter::once(Box::new(PressControl) as Box<dyn Action>))
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

/// One default chord, written the way a settings row shows it.
///
/// Parsed from the accelerator rather than built by hand so that what an
/// action declares reads as the thing somebody would press, and so that these
/// go through the same bridge a recorded chord does. A string that cannot be
/// parsed leaves the action with no chord, which
/// `tests/actions.rs::every_declared_shortcut_is_the_chord_it_names` catches.
fn chord(accelerator: &str) -> Option<crate::action_keys::Shortcut> {
    crate::action_keys::Shortcut::parse(accelerator).ok()
}

// ------------------------------------------------------------------ launch

/// Opens an application, a file or a folder through the shell.
struct Launch;

#[async_trait]
impl Action for Launch {
    fn id(&self) -> &str {
        "sill.launch"
    }

    fn title(&self) -> &str {
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
        if object.target.starts_with(crate::games::GAME) {
            // A game is started by its own library rather than by the shell.
            // The target is an identifier, not a path, and `games::command` is
            // where it is checked: see that module's note on why this is not a
            // `steam://` address.
            let (exe, args) = crate::games::command(
                &object.target,
                crate::games::steam_root().as_deref(),
                crate::games::epic_launcher().as_deref(),
            )?;

            std::process::Command::new(&exe)
                .args(&args)
                .spawn()
                .map_err(|err| format!("could not launch {}: {err}", object.title))?;
        } else if let Some(app_id) = object.target.strip_prefix(crate::apps::APPS_FOLDER) {
            // A packaged app has no path to open. Explorer resolves an
            // AppUserModelID through the Apps folder, which is how the Start
            // Menu launches them too.
            std::process::Command::new("explorer.exe")
                .arg(format!("{}{}", crate::apps::APPS_FOLDER, app_id))
                .spawn()
                .map_err(|err| format!("could not launch {}: {err}", object.title))?;
        } else {
            // A record in the index is normally a path, but an extension
            // supplies its own rows and a model can name a target, so what
            // gets launched is not always something Sill put there.
            let target = crate::reach::target(&object.target)?;

            tauri_plugin_opener::open_path(&target, None::<&str>)
                .map_err(|err| format!("could not launch {}: {err}", object.title))?;
        }

        Ok(Outcome::done(format!("Opened {}", object.title)))
    }
}

/// Loads a Raycast-compatible extension command.
struct RunExtensionCommand;

#[async_trait]
impl Action for RunExtensionCommand {
    fn id(&self) -> &str {
        "sill.runExtensionCommand"
    }

    fn title(&self) -> &str {
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
        let record = extension::command_record(ctx, &object.id)?;

        // Nothing to hand it. Somebody picked this command off the root list,
        // so there is no thing it was run on, and `@sill/api` says so.
        open_extension_command(ctx, &record, &object.title, None).await
    }
}

/// One command in a worker, however it was reached.
///
/// Extracted from [`RunExtensionCommand`] rather than copied beside it, and
/// the reason is the whole of rules 14 to 16: an extension command reached
/// from the root list and the same command reached as an action on a file must
/// be the same launch. Everything here is a step somebody would forget in a
/// second copy, and the ones that fail quietly are the dangerous half: without
/// the preferences an extension runs as though every setting were cleared,
/// without the assets path it reads its own data files off an empty disk, and
/// without the capability list it can draw and nothing else.
///
/// `called` is what to call it in a message, and `on` is the only real
/// difference between the two callers.
async fn open_extension_command(
    ctx: &ActionCtx,
    record: &crate::registry::CommandRecord,
    called: &str,
    on: Option<&Object>,
) -> Result<Outcome, String> {
    // The manifest decides, and a mode this cannot name is refused rather
    // than loaded as a view. It used to fall through to `View`, so a
    // `menu-bar` command left the window waiting for a tree that never
    // arrives. Installing now refuses those, so reaching this is an index
    // written by an older Sill, and saying so is better than hanging.
    let mode = crate::exthost::CommandMode::from_manifest(&record.mode).ok_or_else(|| {
        match crate::extension_install::why_not_runnable(&record.mode) {
            Some(because) => format!("{called} cannot run here: {because}."),
            None => format!("{called} cannot run here."),
        }
    })?;

    let data_dir = crate::state::data_dir(&ctx.app);
    let declared = record.manifest.clone().unwrap_or_default();

    // What the manifest defaults to, under what somebody set in Settings.
    let held = crate::exthost::preferences::load(&data_dir);
    let preferences = crate::exthost::preferences::effective(
        &record.preferences,
        held.in_scope(&crate::exthost::preferences::extension_scope(
            &record.extension,
        )),
        held.in_scope(&crate::exthost::preferences::command_scope(
            &record.extension,
            &record.command,
        )),
    );

    // Raycast refuses to start a command whose required preference is
    // unset and names it. Sill was starting it, and the extension threw on
    // an undefined in its first line, which reads as the extension being
    // broken rather than as a setting nobody has filled in.
    let missing =
        crate::exthost::preferences::missing_required(&declared.preferences, &preferences);
    if !missing.is_empty() {
        return Err(format!(
            "{called} needs {} before it can run. Set it in Settings, under Extensions.",
            missing.join(" and ")
        ));
    }

    let hosts = ctx.app.state::<crate::state::HostState>();

    /*
     * Which kind of open this is, asked before it is made true.
     *
     * `host_of` starts the extension runtime when nothing is running, so
     * asking after it would answer "warm" every time and the cold figure
     * would never be recorded at all. A cold open pays for a Node process,
     * a worker thread and a module evaluation; a warm one pays for the
     * last of those. Reporting either as the other is the lie this split
     * exists to avoid.
     */
    let start = if crate::host::running_host(&hosts).await.is_some() {
        crate::timing::Start::Warm
    } else {
        crate::timing::Start::Cold
    };

    /*
     * The clock starts here rather than at the load.
     *
     * Everything above this line is part of the wait: the index lookup,
     * the manifest, the saved preferences, the required-preference check.
     * It is small, and it is still time somebody spends looking at a
     * launcher that has not moved.
     *
     * It is stopped by the extension's first render, over in the API
     * layer, because that is the first moment there is anything to look
     * at. This call returns long before then.
     */
    if let Some(timings) = ctx.app.try_state::<crate::timing::Timings>() {
        timings.opening_began(&record.extension, start);
    }

    let host = crate::host::host_of(&ctx.app, &hosts).await?;

    let mut opts = crate::exthost::LoadOptions::with_preferences(
        record.entrypoint.clone(),
        &record.extension,
        &record.command,
        mode,
        preferences,
    );

    // Both were the empty string, so `environment.assetsPath` pointed at
    // nothing and an extension reading an icon out of its own assets found
    // no such file. The support folder is made here rather than at install
    // because an update clears the installed directory and this must
    // survive one.
    let home = crate::store::extensions_home(&data_dir);
    let assets = home.join(&record.extension).join("assets");
    if assets.is_dir() {
        opts.assets_path = assets.to_string_lossy().replace('\\', "/");
    }

    let support = crate::exthost::preferences::support_path(&data_dir, &record.extension);
    if std::fs::create_dir_all(&support).is_ok() {
        opts.support_path = support.to_string_lossy().replace('\\', "/");
    }

    // What the command declared it wants typed. Nothing collects them yet,
    // so every one is absent, and absent is `""` rather than missing: an
    // extension destructuring `props.arguments` and passing the result to
    // a search is the ordinary shape, and `undefined` there is a crash
    // where an empty string is an empty search.
    opts.arguments = crate::exthost::LoadOptions::blank_arguments(&declared.arguments);

    /*
     * What this extension has been allowed to reach, read now.
     *
     * This one line is the whole of "an extension's action can do what the
     * extension may do and no more". A contributed action declares which
     * kinds it applies to and nothing else: it cannot ask for a permission,
     * cannot inherit one from the object it was run on, and cannot arrive
     * at a different answer from the same command started off the root
     * list, because both callers are this line. What comes back is what
     * somebody agreed to on the install card, minus anything they have
     * taken back since, and the worker's own module gate refuses the rest.
     */
    opts.capabilities = crate::exthost::grants::for_extension(&ctx.app, &record.extension);

    // The thing it was run on, when it was run on one.
    opts.on = on.cloned();

    let session = host.load(&opts).await.map_err(|e| e.to_string())?;
    Ok(Outcome::running(format!("Ran {called}"), session))
}

// ---------------------------------------------------------------- settings

/// Opens a Windows settings page or Control Panel applet.
struct OpenSystemSetting;

#[async_trait]
impl Action for OpenSystemSetting {
    fn id(&self) -> &str {
        "sill.openSystemSetting"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.openOwnSetting"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.runBuiltin"
    }

    fn title(&self) -> &str {
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
            // The overlay hands back one pixel, read once it is gone, and the
            // hex lands on the clipboard. Typing that hex into the launcher
            // then offers the other forms. The privacy check is the picture's
            // own, taken here, because the overlay only chose where to look.
            "pick-colour" => {
                use crate::commands::system::{choose_region, Purpose};

                let region = choose_region(app, Purpose::Colour).await?;
                let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;
                let shot = tokio::task::spawn_blocking(move || {
                    crate::capture::region(&allowed, region.left, region.top, 1, 1)
                })
                .await
                .map_err(|err| format!("the pixel could not be read: {err}"))??;

                let colour = crate::colour::Colour::from_bgra(&shot.pixels)
                    .ok_or_else(|| "nothing was under the pointer".to_string())?;
                let hex = colour.hex();

                return copy_with_undo(ctx, &hex, &format!("Copied {hex}"));
            }
            // A box dragged over whatever is on screen, read once the overlay
            // is gone. The privacy check is the picture's own, taken here,
            // because the overlay only chose where to look.
            "read-qr" => {
                use crate::commands::system::{choose_region, Purpose};

                let region = choose_region(app, Purpose::Qr).await?;
                let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;
                let found = tokio::task::spawn_blocking(move || {
                    let shot = crate::capture::region(
                        &allowed,
                        region.left,
                        region.top,
                        region.width,
                        region.height,
                    )?;
                    crate::qr::decode_bgra(&shot.pixels, shot.width, shot.height)
                })
                .await
                .map_err(|err| format!("reading that part of the screen failed: {err}"))??;

                return copy_codes(ctx, found);
            }
            // Asks first, because there is no undo. Every program with a
            // window gets the close its own button sends, and any with
            // unsaved work puts up its own question; nothing is terminated.
            // The count is said before anything happens, so "everything"
            // means a number rather than a surprise.
            "quit-all" => {
                let targets = crate::processes::quit_all_targets(
                    &crate::processes::running(),
                    std::process::id(),
                )
                .len();

                if targets == 0 {
                    return Ok(Outcome::done("Nothing is open to close"));
                }

                crate::dismiss_main(app);

                let asked = app.clone();
                let agreed = tokio::task::spawn_blocking(move || {
                    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

                    asked
                        .dialog()
                        .message(format!(
                            "Ask {targets} {} to close? Any with unsaved work will ask you first.",
                            if targets == 1 { "program" } else { "programs" }
                        ))
                        .title("Quit All Applications")
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            "Close them".to_string(),
                            "Cancel".to_string(),
                        ))
                        .kind(MessageDialogKind::Warning)
                        .blocking_show()
                })
                .await
                .map_err(|err| format!("the question was not asked: {err}"))?;

                if !agreed {
                    return Ok(Outcome::done("Left everything open"));
                }

                let said = tokio::task::spawn_blocking(crate::processes::quit_all)
                    .await
                    .map_err(|err| format!("could not ask programs to close: {err}"))??;

                return Ok(Outcome::done(said));
            }
            // The launcher goes away first, or the confetti lands on it.
            "confetti" => {
                crate::dismiss_main(app);
                crate::commands::system::throw_confetti(app.clone()).await?;
                return Ok(Outcome::done("Confetti"));
            }
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
                let origin = crate::store::Origin::folder(&source, crate::state::now_seconds());

                let installed = crate::extension_install::install(app, &source, &origin)?;

                return Ok(Outcome::done(format!(
                    "Installed {} ({})",
                    installed.title,
                    installed.commands.join(", ")
                )));
            }
            /*
             * Private mode, flipped through the one path that applies
             * settings.
             *
             * Not by reaching into the clipboard watcher and the dictation
             * hook from here. `set_preferences` writes the file and then
             * applies everything that can change without a restart, which
             * already includes stopping the clipboard's thread, removing the
             * keyboard hook, pointing the capture mirror at the new value and
             * putting up the standing report. Doing any of that here would be
             * a second implementation of the thing rule 14 exists to prevent,
             * and the half somebody forgot would be the half that still
             * records.
             */
            crate::registry::PRIVATE_MODE => {
                let prefs = app.state::<crate::state::PrefsState>();

                let next = {
                    let held = prefs.inner.lock().await;
                    let mut next = held.clone();
                    next.privacy.paused = !next.privacy.paused;
                    next
                };

                let paused = next.privacy.paused;
                crate::commands::settings::set_preferences(app.clone(), prefs, next).await?;

                // What it is now rather than what it was asked to do, which is
                // the rule the system switches follow: the sentence stays true
                // whatever else changed in between.
                return Ok(Outcome::done(if paused {
                    "Private mode on. The clipboard history, dictation and screen capture \
                     are paused."
                } else {
                    "Private mode off. The clipboard history, dictation and screen capture \
                     are back."
                }));
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
    fn id(&self) -> &str {
        "sill.system.run"
    }

    fn title(&self) -> &str {
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
        /*
         * Asked about first, and only these are.
         *
         * Everything else on this list is one keystroke to put back: the
         * volume goes the other way, a radio goes back on, the theme goes back
         * to light. Sleeping, signing out, restarting and switching off are
         * not, and the second of them takes whatever was unsaved with it.
         *
         * The gate is `from_id`, so a row that is not one of the five cannot
         * reach the asking and one that is cannot reach the line below it.
         */
        if let Some(power) = crate::system::Power::from_id(&object.target) {
            return once_answered(ctx, power.into(), power.question().to_string()).await;
        }

        /*
         * The bin, which asks the same way and asks with a number.
         *
         * Read before the question rather than after, so the question can name
         * what is about to go: "are you sure" is a worse thing to be asked
         * than "3.7 GB in 412 items", and the second is what somebody needs to
         * decide. An empty bin is not asked about at all, because there is
         * nothing to lose and a question about nothing trains people to answer
         * these without reading them.
         */
        if object.target == EMPTY_RECYCLE_BIN {
            let held = crate::recycle_bin::held();

            if held.is_empty() {
                return Ok(Outcome::done("The recycle bin is already empty"));
            }

            return once_answered(
                ctx,
                crate::system::Irreversible::EmptyRecycleBin,
                format!(
                    "Press Enter again to permanently delete {}",
                    held.in_words()
                ),
            )
            .await;
        }

        // Each says what it did rather than what it was asked to do, because
        // "Volume 60%" is the useful sentence and "Turned the volume up" is
        // not, and it stays true if something else changed it in between.
        let said = run_system(&object.target)?;

        // The reading is a second stale otherwise, and a second is exactly how
        // long somebody looks at a switch they just pressed.
        crate::system::forget_live(&ctx.app.state::<crate::state::Fresh<crate::system::Live>>());

        /*
         * A switch stays on screen. A one-shot gets out of the way.
         *
         * Flipping something and watching the row change is the point of
         * drawing it as a control: mute, dark mode, a radio and an output are
         * all things somebody may want to press twice, or press and then look
         * at. Locking the screen is not, and neither is nudging the volume
         * away from the launcher.
         */
        let switches = ctx.app.state::<crate::state::Fresh<crate::system::Live>>();
        if crate::system::toggle_state(&object.target, &crate::system::live(&switches)).is_none() {
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
    // Used now: the list of what is playing is a service rather than a static,
    // so nudging a volume needs the handle to say the reading is stale.
    ctx: &ActionCtx,
    object: &Object,
    to: impl Fn(f32) -> f32 + Send + 'static,
) -> Result<Outcome, String> {
    let id = object.target.clone();

    // Cloned rather than borrowed: the reading happens on a blocking thread
    // that outlives this call, so a `State` borrowed from `ctx` cannot go with
    // it.
    let app = ctx.app.clone();

    let level = tokio::task::spawn_blocking(move || {
        let now = crate::app_volume::sessions(
            &app.state::<crate::state::Fresh<Vec<crate::app_volume::Session>>>(),
        )
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

    crate::app_volume::forget_sessions(
        &ctx.app
            .state::<crate::state::Fresh<Vec<crate::app_volume::Session>>>(),
    );

    Ok(Outcome::done(format!(
        "{} at {}%",
        object.title,
        (level * 100.0).round() as i32,
    )))
}

struct ToggleSessionMute;

#[async_trait]
impl Action for ToggleSessionMute {
    fn id(&self) -> &str {
        "sill.audio.session.mute"
    }

    fn title(&self) -> &str {
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

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = object.target.clone();

        // Cloned rather than borrowed, for the reason `nudge` gives.
        let app = ctx.app.clone();

        // Read and invert rather than being told which way to go. The row was
        // drawn a moment ago and something else may have changed it since.
        let muted = tokio::task::spawn_blocking(move || {
            let now = crate::app_volume::sessions(
                &app.state::<crate::state::Fresh<Vec<crate::app_volume::Session>>>(),
            )
            .into_iter()
            .find(|session| session.id == id)
            .ok_or_else(|| "that program is not playing anything any more".to_string())?;

            crate::app_volume::set_muted(&id, !now.muted)?;
            Ok::<bool, String>(!now.muted)
        })
        .await
        .map_err(|err| format!("could not reach the sound system: {err}"))??;

        crate::app_volume::forget_sessions(
            &ctx.app
                .state::<crate::state::Fresh<Vec<crate::app_volume::Session>>>(),
        );

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
    fn id(&self) -> &str {
        "sill.audio.session.louder"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.audio.session.quieter"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.audio.session.half"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.audio.session.full"
    }

    fn title(&self) -> &str {
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

/*
 * What is playing, and the two things worth doing to it.
 *
 * Both act on **whatever Windows says the current session is**, not on the
 * session the row was drawn from. That is deliberate, and it is what the media
 * keys on a keyboard do: the current session is the one the system would send
 * a key press to, and a launcher that picked a different one would mean
 * pressing Enter here and pressing the key on the keyboard did different
 * things. The row carries the player's name so the message can say what was on
 * screen, and nothing looks a session up by it.
 *
 * Neither is destructive and neither ships with a chord. They are reached by
 * Enter and by the panel.
 */

/// Drops the reading of what is playing, after something has changed it.
///
/// Shared by both so they cannot drift on it. Without it the row would show
/// what was playing a moment ago for up to a second, which is exactly the
/// moment somebody is looking at the row they just pressed.
fn media_changed(ctx: &ActionCtx) {
    crate::media::forget(
        &ctx.app
            .state::<crate::state::Fresh<Option<crate::media::NowPlaying>>>(),
    );
}

/// Plays what is paused, and pauses what is playing.
struct PlayPause;

#[async_trait]
impl Action for PlayPause {
    fn id(&self) -> &str {
        "sill.media.playPause"
    }

    fn title(&self) -> &str {
        "Play or Pause"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::NowPlaying
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // Blocking: two calls out of this process and into a player that may
        // be busy. The same reason the volume actions are on one.
        let playing = tokio::task::spawn_blocking(crate::media::play_pause)
            .await
            .map_err(|err| format!("could not reach the player: {err}"))??;

        media_changed(ctx);

        // What it did rather than what it was asked to do, which is the rule
        // the system switches follow: the sentence stays true if something
        // else changed it in between.
        Ok(Outcome::done(format!(
            "{} is {}",
            object.title,
            if playing { "playing" } else { "paused" }
        )))
    }
}

/// Moves to the next track.
struct NextTrack;

#[async_trait]
impl Action for NextTrack {
    fn id(&self) -> &str {
        "sill.media.next"
    }

    fn title(&self) -> &str {
        "Next Track"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::NowPlaying
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        /*
         * Offered whether or not the player said it could be skipped, and
         * refused here rather than hidden there.
         *
         * `accepts` answers per kind, so there is no way to offer this on one
         * row and not another. The refusing belongs at this end anyway: a
         * hotkey, a workflow and the model all reach an action without passing
         * through a list, so a list that quietly dropped the row would protect
         * nobody. `media::next` says "there is nothing after this one" when the
         * player says no, which is the same answer with a reason.
         */
        tokio::task::spawn_blocking(crate::media::next)
            .await
            .map_err(|err| format!("could not reach the player: {err}"))??;

        media_changed(ctx);

        // The track that ended, because the one that started is not known yet:
        // a player updates its session a moment after it is told to skip, and
        // reading again straight away races that.
        Ok(Outcome::done(format!("Skipped {}", object.title)))
    }
}

/*
 * Ending a running program.
 *
 * Two actions and the difference between them is the whole feature. One asks
 * the program to close and lets it save; the other kills it and does not.
 *
 * The row carries the process id as its target and the program's name as its
 * title, and **both are needed**, which is the part that is easy to get wrong.
 * An id on its own is not an identity: Windows hands a number back out once
 * the process holding it exits, and the gap between a row being drawn and Enter
 * being pressed on it is long enough for that to happen. So the name the row
 * was drawn with goes down with the id, and the check is that the machine still
 * agrees. See `processes::may_end`.
 */

/// The process id a row carries, if it carries one.
///
/// Its own step because the failure has to be an error rather than a default.
/// A target that does not parse means the window sent something that is not a
/// process row, and picking a pid out of the air for it is how the wrong thing
/// gets ended.
fn pid_of(object: &Object) -> Result<u32, String> {
    object
        .target
        .parse::<u32>()
        .map_err(|_| format!("{} is not a running program", object.title))
}

/// Asks a program to close.
struct QuitProcess;

#[async_trait]
impl Action for QuitProcess {
    fn id(&self) -> &str {
        "sill.process.quit"
    }

    fn title(&self) -> &str {
        "Quit"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Process
    }

    fn capabilities(&self) -> &'static [Capability] {
        // Reaching into another program's windows, which is what a close
        // request is. Not `Ui`, which is Sill's own surface.
        &[Capability::WindowControl]
    }

    /// What Enter does on a process row, and the safe one of the pair.
    ///
    /// This is the ordering the panel is drawn in: the registry lifts the
    /// primary to the front and leaves the rest in registration order, so
    /// Force Quit is below this because it does not claim Enter and because it
    /// is registered after. Neither of those is a comment somebody has to
    /// keep true.
    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let pid = pid_of(object)?;
        let name = object.title.clone();

        let said = tokio::task::spawn_blocking(move || crate::processes::quit(pid, &name))
            .await
            .map_err(|err| format!("that could not be asked: {err}"))??;

        // The list on screen is about to be wrong about the row that was just
        // acted on, and a second of it saying otherwise is exactly how long
        // somebody looks at a row they pressed.
        crate::processes::forget(
            &ctx.app
                .state::<crate::state::Fresh<Vec<crate::processes::Process>>>(),
        );

        Ok(Outcome::done(said))
    }
}

/// Ends a program outright, without asking it.
struct ForceQuitProcess;

#[async_trait]
impl Action for ForceQuitProcess {
    fn id(&self) -> &str {
        "sill.process.forceQuit"
    }

    fn title(&self) -> &str {
        "Force Quit"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Process
    }

    fn capabilities(&self) -> &'static [Capability] {
        // Deliberately not `WindowControl`, which is what asking a window to
        // close is. This does not go near a window: it ends the process, and
        // a permission screen should be able to say those are different
        // things to be allowed to do.
        &[Capability::SystemControl]
    }

    /// Never Enter. Left at the default, and the default is the point:
    /// claiming the primary here would need a line of code, so the dangerous
    /// half of this pair cannot become the one Enter runs by accident.
    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let pid = pid_of(object)?;
        let name = object.title.clone();

        let said = tokio::task::spawn_blocking(move || crate::processes::force_quit(pid, &name))
            .await
            .map_err(|err| format!("that could not be run: {err}"))??;

        crate::processes::forget(
            &ctx.app
                .state::<crate::state::Fresh<Vec<crate::processes::Process>>>(),
        );

        Ok(Outcome::done(said))
    }
}

/// Runs an installed program's own uninstaller.
struct UninstallApp;

#[async_trait]
impl Action for UninstallApp {
    fn id(&self) -> &str {
        "sill.app.uninstall"
    }

    fn title(&self) -> &str {
        "Uninstall"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Application
    }

    fn capabilities(&self) -> &'static [Capability] {
        // A command line out of the registry, which is a shell and everything
        // a shell can reach, rather than "open this program".
        &[Capability::ShellExecution]
    }

    /**
    Hands over to the vendor's uninstaller rather than removing anything.

    Sill does not delete a program. It finds the command the installer left
    behind and runs it, and what happens next is the vendor's own screen with
    the vendor's own confirmation on it. That is why this needs no question of
    its own: the thing that asks is better than anything a launcher could put
    up, because it knows what it is about to remove.

    Refused rather than guessed when nothing matches. Running the wrong
    uninstaller is the same class of mistake as ending the wrong process, and
    a near match on a display name is not evidence.
    */
    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let title = object.title.clone();
        let target = object.target.clone();

        let command =
            tokio::task::spawn_blocking(move || crate::apps::uninstaller_for(&title, &target))
                .await
                .map_err(|err| format!("the registry could not be read: {err}"))?
                .ok_or_else(|| {
                    format!("Windows does not list an uninstaller for {}", object.title)
                })?;

        crate::apps::run_uninstaller(&command)?;

        // The uninstaller owns the screen from here, and it is somebody else's
        // window with somebody else's buttons on it.
        crate::dismiss_main(&ctx.app);

        Ok(Outcome::done(format!(
            "Opened the uninstaller for {}",
            object.title
        )))
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

/// The row that empties the recycle bin.
///
/// Filed under `system.` with the switches and the power commands, because
/// that is what it is: a thing that changes Windows rather than one of Sill's
/// own commands, and it wears the bin's own mark for the same reason.
pub(crate) const EMPTY_RECYCLE_BIN: &str = "system.recycle-bin.empty";

/// Does something irreversible, but never on the press that asked about it.
///
/// **The only caller of [`crate::system::Irreversible::apply`], and it calls it
/// from one arm.** That is the whole shape: everything destructive is a
/// variant of one enum, the enum has one place that runs it, and that place is
/// inside the branch that already holds the answer. There is no second route
/// to any of it, which is what makes "no single press ever means yes" a fact
/// about the code rather than about how carefully each call site was written.
///
/// The launcher deliberately stays open while a question is open. That is what
/// makes a second press possible at all, and it is also the answer to "did it
/// hear me": the row is still there with the question written under it.
///
/// The same gate covers the model, which reaches actions through this registry
/// like everything else. Being told to shut the machine down, and having asked
/// its own permission card, it still gets a question back rather than a
/// shutdown, and it has to decide to say yes a second time.
///
/// The question is passed in rather than read off the enum, because one of
/// them has to name what is about to go: "permanently delete 3.7 GB in 412
/// items" is a different sentence every time it is asked, and a question that
/// cannot say what it is about is a question nobody can answer properly.
async fn once_answered(
    ctx: &ActionCtx,
    about: crate::system::Irreversible,
    question: String,
) -> Result<Outcome, String> {
    use crate::system::Press;

    match ctx.app.state::<crate::system::Asked>().press(about) {
        // A press that came too soon is the repeat of a held key rather than
        // an answer, and repeating the question is the whole of the response
        // to it: nothing has changed, and the question is still open.
        Press::Asks | Press::TooSoon => Ok(Outcome::done(question)),

        Press::Answers => {
            // On a thread of its own, because one of these is not quick:
            // emptying a bin holding tens of gigabytes takes as long as it
            // takes, and an action that blocks the runtime while it runs takes
            // every other answer down with it.
            let said = tokio::task::spawn_blocking(move || about.apply())
                .await
                .map_err(|err| format!("that could not be started: {err}"))??;

            // The screen is about to belong to a sign-in prompt or to nothing
            // at all. Sitting on top of it until then helps nobody.
            crate::dismiss_main(&ctx.app);

            Ok(Outcome::done(said))
        }
    }
}

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

            Ok(if set {
                "Sound off".into()
            } else {
                "Sound on".into()
            })
        }

        "system.theme" => {
            let now = system::dark().unwrap_or(false);
            let set = system::set_dark(!now)?;

            Ok(if set {
                "Dark mode".into()
            } else {
                "Light mode".into()
            })
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
    fn id(&self) -> &str {
        "sill.pasteSnippet"
    }

    fn title(&self) -> &str {
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
        let expansion = crate::snippets::commands::expand_snippet(
            ctx.app.clone(),
            object.target.clone(),
            // The action registry has nowhere to ask, so a snippet with
            // fields expands with them still in it. The launcher asks
            // first and calls the command directly.
            None,
        )?;

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
    fn id(&self) -> &str {
        "sill.emoji.paste"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.openQuicklink"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.copyAnswer"
    }

    fn title(&self) -> &str {
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

        // Remembered once it has been wanted, which pressing Enter is. The
        // sum comes as the argument because the object carries only the
        // answer, and a list of answers with no questions is not a history.
        // A history that could not be written is said, not failed over: the
        // answer is already on the clipboard, which is what was asked for.
        if let Some(input) = ctx.argument() {
            let path = crate::sums::path(&crate::state::data_dir(&ctx.app));
            let sums = ctx.app.state::<crate::sums::Sums>();
            if let Err(why) =
                sums.remember(&path, input, &object.target, crate::state::now_seconds())
            {
                crate::say!("could not remember the sum: {why}");
            }
        }

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
    fn id(&self) -> &str {
        "sill.file.terminal"
    }

    fn title(&self) -> &str {
        "Open Terminal Here"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // T for terminal.
        chord("Ctrl+Shift+T")
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let here = folder_of(&object.target)?;

        /*
         * A profile, if one was asked for.
         *
         * Through `ActionCtx` rather than read from the field, so a key bound
         * to this and the model can both name a profile. An unknown name is
         * not refused here: `wt` says so itself, and a list of profiles read a
         * moment ago can be out of date by the time somebody presses Enter.
         */
        let wanted = ctx.argument().map(str::trim).filter(|one| !one.is_empty());

        // Whichever terminal the machine has, best first. `wt` is the one
        // people who have it want; `powershell` is on every Windows; `cmd` is
        // the one that cannot be missing. Only `wt` knows what a profile is,
        // so naming one and not having it is worth saying rather than quietly
        // opening the wrong shell.
        let programs: &[&str] = if wanted.is_some() {
            &["wt.exe"]
        } else {
            &["wt.exe", "powershell.exe", "cmd.exe"]
        };

        let opened = programs
            .iter()
            .find_map(|program| {
                let mut command = std::process::Command::new(program);

                // Each wants the starting folder said differently, and the
                // difference is not cosmetic: passing the wrong one silently
                // opens in the home folder.
                match *program {
                    "wt.exe" => {
                        command.args(crate::terminals::wt_arguments(wanted, &here));
                    }
                    _ => {
                        command.current_dir(&here);
                    }
                }

                command.spawn().ok().map(|_| *program)
            })
            .ok_or_else(|| match wanted {
                Some(profile) => format!(
                    "Windows Terminal would not start, so the {profile} profile \
                     could not be opened."
                ),
                None => "No terminal on this machine would start.".to_string(),
            })?;

        let _ = opened;
        Ok(Outcome::done(match wanted {
            Some(profile) => format!("Opened {profile} in {}", name_of(&here)),
            None => format!("Opened a terminal in {}", name_of(&here)),
        }))
    }
}

/// Points the open or save dialog in front at this folder.
///
/// `P8-07`, and the reason it is an action rather than a command of its own:
/// the two ways somebody wants it are "the folder I have open in Explorer,
/// right now, while this Save dialog is covering it" and "that folder I just
/// searched for". Those are one verb over two different subjects, which is
/// exactly what the registry is for. The first is a key bound to
/// `Source::ExplorerFolder`; the second is this entry in the action panel of
/// any file or folder row. Neither has its own implementation of anything.
struct JumpInDialog;

#[async_trait]
impl Action for JumpInDialog {
    fn id(&self) -> &'static str {
        "sill.dialog.jump"
    }

    fn title(&self) -> &'static str {
        "Jump To In Dialog"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    /// Reaches into another application's window, which is what
    /// `WindowControl` says.
    ///
    /// Deliberately **not** `InputInjection`. Nothing is synthesised: the
    /// path goes to one control by its handle and the accept button is told
    /// it was pressed by name. Declaring the capability that means "types
    /// wherever the keyboard is pointing" would be a false description of the
    /// mechanism, and this list exists to be read off rather than traced.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let folder = matches!(object.kind, ObjectKind::Folder);
        let target = object.target.clone();

        // Blocking, because every step waits on another process to answer a
        // window message. On a runtime worker that is a launcher that stops
        // responding for as long as somebody else's dialog is busy.
        tokio::task::spawn_blocking(move || {
            let dialog =
                crate::dialog::in_front().map_err(|refusal| refusal.reason().to_string())?;

            // Read before the plan is made, so a half-typed filename can
            // survive the navigation that is about to clear it.
            let typed = crate::dialog::typed_in(&dialog);
            let jump = crate::dialog::plan(&target, folder, &typed)?;

            crate::dialog::jump_to(&dialog, &jump)?;

            Ok(Outcome::done(format!(
                "Pointed the dialog at {}",
                name_of(&jump.folder)
            )))
        })
        .await
        .map_err(|err| format!("The jump did not finish: {err}"))?
    }
}

/// Opens one Windows Terminal profile, or one WSL distribution.
///
/// The row this acts on already knows which of the two it is, and that decides
/// the program: `wt -p` for a profile Terminal knows about, `wsl -d` for a
/// distribution it has never generated one for. Asking `wt` to open a profile
/// it does not have is not an error it reports; it opens nothing and says
/// nothing, which from the outside is indistinguishable from Sill being
/// broken.
///
/// No starting folder, deliberately. A profile carries its own
/// `startingDirectory` and somebody who wanted a particular folder has
/// "Open Terminal Here" on that folder, which is a different question with a
/// different answer.
struct OpenTerminalProfile;

#[async_trait]
impl Action for OpenTerminalProfile {
    fn id(&self) -> &'static str {
        "sill.terminal.open"
    }

    fn title(&self) -> &'static str {
        "Open Terminal"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::TerminalProfile
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ProcessLaunch]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The row carries which program to run, in its mode, because the row
        // is the only thing that knows: by the time this runs, the settings
        // file and the registry have both been left behind.
        let profile = crate::terminals::profile_from(&object.title, &object.target);
        let (program, args) = crate::terminals::opening(&profile);

        std::process::Command::new(program)
            .args(&args)
            .spawn()
            .map_err(|err| format!("{program} would not start: {err}"))?;

        Ok(Outcome::done(format!("Opened {}", object.title)))
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
    fn id(&self) -> &str {
        "sill.file.recycle"
    }

    fn title(&self) -> &str {
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
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
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
    fn id(&self) -> &str {
        "sill.copyPath"
    }

    fn title(&self) -> &str {
        "Copy Path"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // C for copy, and Shift because Ctrl+C is the clipboard's own Copy.
        chord("Ctrl+Shift+C")
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
    fn id(&self) -> &str {
        "sill.revealInFolder"
    }

    fn title(&self) -> &str {
        "Show in Folder"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // E for Explorer, which is the window this opens.
        chord("Ctrl+Shift+E")
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
    fn id(&self) -> &str {
        "sill.copyName"
    }

    fn title(&self) -> &str {
        "Copy Name"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // N for name. It sits beside Copy Path on nearly every row.
        chord("Ctrl+Shift+N")
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        // Everything has a name, including the things that have nothing else:
        // a builtin, an extension command, a setting.
        //
        // A control on somebody else's screen is the exception, and it is the
        // only one besides an answer. Its "name" is the word printed on
        // another program's button, and nobody opens a list of buttons in
        // order to put the word "Save" on their clipboard. Offering it turns
        // the one view whose whole point is that Enter presses the thing into
        // a view with a menu on it.
        !matches!(kind, ObjectKind::Answer | ObjectKind::ScreenControl)
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
    fn id(&self) -> &str {
        self.id
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.workspace.restore"
    }

    fn title(&self) -> &str {
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
            crate::commands::system::restore_workspace(ctx.app.clone(), object.target.clone())
                .await?;

        Ok(Outcome::done(match moved {
            0 => "Nothing from that workspace could be put back".to_string(),
            1 => "Put one window back".to_string(),
            many => format!("Put {many} windows back"),
        }))
    }
}

/// Rewrites a saved arrangement as named positions.
struct MakeWorkspacePortable;

#[async_trait]
impl Action for MakeWorkspacePortable {
    fn id(&self) -> &str {
        "sill.workspace.portable"
    }

    fn title(&self) -> &str {
        "Use Named Positions"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Workspace
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let named = crate::commands::system::make_workspace_portable(
            ctx.app.clone(),
            object.target.clone(),
        )?;

        Ok(Outcome::done(match named {
            0 => "None of those windows sit in a named position, so the arrangement still holds their exact sizes"
                .to_string(),
            1 => "One window now says where it goes rather than how big it was".to_string(),
            many => format!("{many} windows now say where they go rather than how big they were"),
        }))
    }
}

/// Forgets a saved arrangement. P2.12.
struct ForgetWorkspace;

#[async_trait]
impl Action for ForgetWorkspace {
    fn id(&self) -> &str {
        "sill.workspace.forget"
    }

    fn title(&self) -> &str {
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

/// Forgets one conversation.
///
/// The list of past conversations had no action panel at all: Ctrl+K there
/// said "no actions here", and Delete was the only way to remove one, wired
/// straight into the window. An action that only the page can reach is one a
/// hotkey cannot bind and the model cannot run, which is the arrangement the
/// registry exists to end.
struct ForgetConversation;

#[async_trait]
impl Action for ForgetConversation {
    fn id(&self) -> &str {
        "sill.conversation.forget"
    }

    fn title(&self) -> &str {
        "Forget"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Conversation
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    /// Enter reopens it, which the window does, so this is never the primary.
    ///
    /// Deliberately: the whole point of that list is going back to something,
    /// and an action panel whose default was "destroy this" would be a trap
    /// on a list somebody opened to read.
    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let chat = ctx.app.state::<crate::ai::chat::Chat>();

        if !chat.forget(&object.target) {
            return Err("That conversation is already gone.".to_string());
        }

        chat.save(&ctx.app);
        Ok(Outcome::done(format!("Forgot {}", object.title)))
    }
}

/// Copies what was said in one conversation.
///
/// The transcript rather than the title, because the reason to reach for a
/// conversation you have already had is usually the answer that was in it.
struct CopyConversation;

#[async_trait]
impl Action for CopyConversation {
    fn id(&self) -> &str {
        "sill.conversation.copy"
    }

    fn title(&self) -> &str {
        "Copy Transcript"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Conversation
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let said = ctx
            .app
            .state::<crate::ai::chat::Chat>()
            .said_in(&object.target)
            .ok_or_else(|| "That conversation is gone.".to_string())?;

        if said.trim().is_empty() {
            return Err("Nothing was said in that one.".to_string());
        }

        Ok(copy_with_undo(ctx, &said, self.title())?.producing(said))
    }
}

/// Copies the link to an extension's source, at the revision on the row.
///
/// The store's rows had no action panel at all, which on a shelf of code
/// somebody is deciding whether to run is the worst place for Ctrl+K to say
/// "no actions here". The one thing anybody wants from a listing they have not
/// installed is somewhere to go and read it.
///
/// The link is built from the catalogue rather than sent up by the window,
/// because it is a fact about a revision and the window's copy of it is a
/// second answer that goes stale the moment the catalogue is fetched again.
struct CopyStoreSource;

#[async_trait]
impl Action for CopyStoreSource {
    fn id(&self) -> &str {
        "sill.store.copySource"
    }

    fn title(&self) -> &str {
        "Copy Source Link"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::StoreListing
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    /// Enter installs, and installing is the store's own two screens.
    ///
    /// Deliberately not claimed here. What Enter does to a listing is fetch
    /// the source, read it, and show what it appears to be able to do before
    /// a line of it runs. That is a conversation rather than an action, and an
    /// action that skipped it would be a way to install code without the one
    /// screen written to stop exactly that.
    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // The catalogue as the store parked it. Cheap: an `Arc` clone, not a
        // copy of three thousand listings.
        let catalog = ctx
            .app
            .state::<crate::store::StoreState>()
            .held()
            .ok_or("the extension store is not open")?;

        let url = catalog
            .listings
            .iter()
            .find(|listing| listing.name == object.target)
            .ok_or_else(|| format!("the store has no extension called {}", object.target))?
            .source_url();

        Ok(copy_with_undo(ctx, &url, self.title())?.producing(url))
    }
}

/// Removes an installed extension, and everything it was allowed to reach.
///
/// It was Ctrl+Shift+X in the store view and nothing else: a removal wired
/// straight into the page, invisible to the activity log and unreachable by
/// anything that is not the page. Same shape as Delete on a conversation, and
/// the same fix.
///
/// Deliberately not the last word on whether it was there. Removing something
/// already gone is the end state somebody asked for, and the message says
/// which of the two happened.
struct RemoveExtension;

#[async_trait]
impl Action for RemoveExtension {
    fn id(&self) -> &str {
        "sill.store.remove"
    }

    fn title(&self) -> &str {
        "Remove"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::StoreListing
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let data_dir = crate::state::data_dir(&ctx.app);
        let home = crate::store::extensions_home(&data_dir);

        /*
         * The name that came in is whichever of the extension's two names the
         * screen had.
         *
         * The settings panel lists what is installed, so it sends the
         * directory. The store lists the catalogue, so it sends the slug, and
         * the two differ for every extension that has ever been renamed:
         * `translate` in the store is `google-translate` on disk. Handing the
         * slug straight on removed nothing and said "was not installed" while
         * the bundles, the index entry and every granted permission stayed.
         *
         * Resolved once, here, so the grant that is forgotten and the
         * directory that is deleted are the same extension. Doing it in two
         * places is how one of them ends up forgetting the permissions of
         * something that is still installed.
         */
        let extension = crate::store::installed_as(&home, &object.target)
            .unwrap_or_else(|| object.target.clone());

        // Before the removal rather than after, so files that refuse to go do
        // not leave permissions granted to something nobody can see any more.
        ctx.app
            .state::<std::sync::Arc<crate::exthost::grants::Granted>>()
            .forget(&extension);

        let name = extension.clone();

        // The one `LocalStorage` the application has open, so what an
        // extension saved goes with it rather than waiting on disk for the
        // next thing installed under the same name.
        let storage = ctx
            .app
            .state::<crate::state::HostState>()
            .api
            .storage()
            .clone();

        let had = tauri::async_runtime::spawn_blocking(move || {
            crate::store::install::uninstall(&data_dir, &storage, &name)
        })
        .await
        .map_err(|err| format!("the removal did not finish: {err}"))??;

        // Its commands are in the index and nothing has read it since.
        crate::reload_index(&ctx.app);

        Ok(Outcome::done(if had {
            format!("Removed {}", object.title)
        } else {
            format!("{} was not installed", object.title)
        }))
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
    fn id(&self) -> &str {
        "sill.text.readAloud"
    }

    fn title(&self) -> &str {
        "Read Aloud"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // S for speak. R is taken by nothing yet, but reads as reload.
        chord("Ctrl+Shift+S")
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
    fn id(&self) -> &str {
        "sill.text.stopReading"
    }

    fn title(&self) -> &str {
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
///
/// "Unchanged" is the whole difficulty. The object carries the row's text, and
/// **an image row's text is a caption Sill wrote**: copying a screenshot out of
/// the history put the words "Image 1920x1080" on the clipboard and lost the
/// picture. So this reads the row back by id and asks
/// [`crate::clipboard::write::payload_for`] what the entry actually is, which
/// is the same question the paste path asks.
struct CopyClipboardEntry;

#[async_trait]
impl Action for CopyClipboardEntry {
    fn id(&self) -> &str {
        "sill.clipboard.copy"
    }

    fn title(&self) -> &str {
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
        let Some(payload) = stored_payload(ctx, object) else {
            // A selection or an emoji, which is text and nothing else.
            return copy_with_undo(ctx, &object.target, "Copied");
        };

        // The same undo every other copy offers: whatever was on the clipboard
        // a moment ago, which is a string already in memory. Reading it can
        // fail perfectly normally, and that is a reason to offer no undo
        // rather than a reason to refuse the copy.
        let previous = ctx.app.clipboard().read_text().ok();

        let mut board =
            arboard::Clipboard::new().map_err(|err| format!("Could not copy: {err}"))?;

        // Sill's own write, so the watcher must not see it as a fresh copy and
        // move the row to the top of the list under the user's hands. The same
        // reservation `clipboard_paste` makes, and taken back when the write it
        // was reserved for does not happen: a reservation nothing consumes
        // swallows whatever the user really copies next.
        let history = ctx.app.try_state::<crate::clipboard::monitor::Clipboard>();
        if let Some(history) = &history {
            history.ignore_next();
        }

        if let Err(err) = crate::clipboard::write::put(&mut board, &payload) {
            if let Some(history) = &history {
                history.forget_ignored();
            }
            return Err(err);
        }

        let message = match payload {
            crate::clipboard::write::Payload::Image(_) => "Copied the picture",
            _ => "Copied",
        };

        Ok(match previous {
            Some(text) => Outcome::undoable(message, Undo::RestoreClipboard { text }),
            None => Outcome::done(message),
        })
    }
}

/// What a clipboard row holds, read back by id.
///
/// `None` when the object is not a history row at all, or when the row is gone
/// between being chosen and being acted on. Both fall back to the text the
/// window sent, which is the only thing left to copy.
fn stored_payload(ctx: &ActionCtx, object: &Object) -> Option<crate::clipboard::write::Payload> {
    if object.kind != ObjectKind::ClipboardEntry {
        return None;
    }

    let id: i64 = object.id.parse().ok()?;
    let history = ctx
        .app
        .try_state::<crate::clipboard::monitor::Clipboard>()?;
    let store = history.store();
    crate::clipboard::write::payload_for(&store, id, false).ok()?
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

/// Sends a window to a layout of the person's own, by the layout's name.
///
/// The name comes as the argument: a key records it, the panel asks for it,
/// and the model names it. The rectangle is worked out from the display the
/// window is on, so a layout means the same thing on every display.
struct PlaceInLayout;

#[async_trait]
impl Action for PlaceInLayout {
    fn id(&self) -> &str {
        "sill.window.layout"
    }

    fn title(&self) -> &str {
        "Place in Layout"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let wanted = ctx
            .argument()
            .ok_or("a layout needs a name, which the launcher asks for")?
            .to_string();

        let layouts = {
            let prefs = ctx.app.state::<crate::state::PrefsState>();
            let held = prefs.inner.lock().await;
            held.layouts.clone()
        };

        let layout = crate::layouts::find(&layouts, &wanted)
            .ok_or_else(|| format!("there is no layout called {wanted}"))?;

        let id = window_handle(object)?;
        let monitor = crate::windowing::monitor_of(id)
            .ok_or_else(|| format!("{} is on no display", object.title))?;

        // Read before the move. Afterwards the old rectangle is gone.
        let undo = window_undo(id);
        crate::windowing::place(id, crate::layouts::rect_of(layout, monitor.work))?;

        let message = format!("{} sent to {}", object.title, layout.name);
        Ok(match undo {
            Some(undo) => Outcome::undoable(message, undo),
            None => Outcome::done(message),
        })
    }
}

/// Brings a window to the front.
struct FocusWindow;

#[async_trait]
impl Action for FocusWindow {
    fn id(&self) -> &str {
        "sill.window.focus"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.window.close"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        self.id
    }

    fn title(&self) -> &str {
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

/// Pins a window above the others, or stops.
struct KeepOnTop;

#[async_trait]
impl Action for KeepOnTop {
    fn id(&self) -> &str {
        "sill.window.keepOnTop"
    }

    fn title(&self) -> &str {
        "Keep on Top"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Window
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let id = window_handle(object)?;

        // Toggled from what the window actually is rather than from anything
        // Sill remembers. Another program can pin or unpin a window at any
        // time, and a remembered answer would then send it the way it already
        // was, which reads as the key having done nothing.
        let already = crate::windowing::is_on_top(id);
        crate::windowing::set_on_top(id, !already)?;

        Ok(Outcome::done(if already {
            format!("{} no longer stays on top", object.title)
        } else {
            format!("{} stays on top", object.title)
        }))
    }
}

/// Sends a window to a named position on the display it is already on.
struct SnapWindow {
    slot: crate::windowing::Slot,
}

#[async_trait]
impl Action for SnapWindow {
    fn id(&self) -> &str {
        self.slot.action_id()
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.window.nextDisplay"
    }

    fn title(&self) -> &str {
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

/// Brings one browser tab to the front of its browser.
///
/// The only action a tab has, and that is the whole of what a tab is for in a
/// launcher. Copying its address would need the address, which a tab strip does
/// not carry: a tab exposes what it is called and nothing else, and inventing
/// an address from a title would be a row that lies.
struct SwitchToTab;

#[async_trait]
impl Action for SwitchToTab {
    fn id(&self) -> &str {
        "sill.browser.tab.focus"
    }

    fn title(&self) -> &str {
        "Switch To Tab"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::BrowserTab
    }

    /// The same capability switching to a window needs, deliberately.
    ///
    /// Going to a tab is going to the window it lives in and then changing
    /// what that window shows. A capability of its own would let something
    /// hold `WindowControl` and still be refused this, or the other way round,
    /// and neither is a distinction anybody wants to reason about.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::WindowControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    /// Reads the strip again rather than trusting what the row was built from.
    ///
    /// See `uia::pick`. The row carries a description of a tab, not a hold on
    /// one, and between the query and the Enter the strip can have changed.
    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let want = crate::uia::Where::parse(&object.target)
            .ok_or_else(|| format!("{} is not a tab", object.title))?;

        crate::uia::activate(&want)?;
        Ok(Outcome::done(format!("Switched to {}", object.title)))
    }
}

/// Presses one control of a window somebody is looking at.
///
/// The only action a control has, and it is the whole of what one is for: a
/// button exists to be pressed. There is deliberately nothing else. Copying a
/// control's name would be a row offering to put the word "Save" on the
/// clipboard, and anything that read a control's *contents* is the half of
/// `P8-04` that was refused rather than built. See [`crate::controls`].
struct PressControl;

#[async_trait]
impl Action for PressControl {
    fn id(&self) -> &str {
        "sill.control.press"
    }

    fn title(&self) -> &str {
        "Press"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ScreenControl
    }

    /// Its own capability, and the argument for that is in [`Capability`].
    ///
    /// Not `WindowControl`, which moves a window about, and not
    /// `InputInjection`, which this is carefully not: nothing is typed and no
    /// key is synthesised, so nothing here can arrive in a program that was
    /// not named.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ControlInvoke]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    /// Reads the window again rather than trusting what the row was built from.
    ///
    /// See `controls::pick`. The row carries a description of a control, not a
    /// hold on one, and between the query and the Enter a program is free to
    /// have rebuilt its toolbar. A control whose identifier or whose name has
    /// moved is refused rather than pressed.
    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let want = crate::controls::Spot::parse(&object.target)
            .ok_or_else(|| format!("{} is not a control", object.title))?;

        /*
         * Blocking, and it crosses into another program's process twice: once
         * to find the control again and once to press it. Never on an async
         * worker, which is the same rule the tab read follows.
         */
        let pressed = tokio::task::spawn_blocking(move || crate::controls::press(&want))
            .await
            .map_err(|err| format!("pressing that control failed: {err}"))?;

        pressed?;

        /*
         * Past tense and no undo, which is the honest answer.
         *
         * An `Undo` descriptor can put a file back or a window back. There is
         * no descriptor that un-presses a button: the program on the other
         * side has already done whatever it does, and offering to reverse it
         * would be the polite fiction rule 16 exists to refuse.
         */
        Ok(Outcome::done(format!("Pressed {}", object.title)))
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
        Box::new(KeepOnTop),
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
    fn id(&self) -> &str {
        "sill.markUp"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.extractText"
    }

    fn title(&self) -> &str {
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

/// Reads the QR codes in a picture already in the history.
///
/// The same shape as reading its words, and for the same reason: a picture
/// that has been copied is a picture, whether it arrived from a screenshot
/// or from somewhere else.
///
/// **What it finds is copied, never opened.** A code is put there by whoever
/// made the page it is on, so the payload is text until somebody decides
/// otherwise, and deciding otherwise goes through the ordinary rules for an
/// address a stranger wrote.
struct ReadQr;

#[async_trait]
impl Action for ReadQr {
    fn id(&self) -> &str {
        "sill.clipboard.readQr"
    }

    fn title(&self) -> &str {
        "Read QR Code"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ClipboardEntry
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
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

        // Off the async worker: decoding a picture is a solid chunk of
        // blocking work, as reading its words is.
        let found = tokio::task::spawn_blocking(move || {
            let (pixels, width, height) = crate::ocr::bgra_from_png(&png)?;
            crate::qr::decode_bgra(&pixels, width, height)
        })
        .await
        .map_err(|err| format!("reading that picture failed: {err}"))??;

        copy_codes(ctx, found)
    }
}

/// What a decoded picture amounts to, said and copied the same way wherever
/// the picture came from.
fn copy_codes(ctx: &ActionCtx, found: Vec<String>) -> Result<Outcome, String> {
    match found.len() {
        // Not an error. Most pictures have no code in them.
        0 => Ok(Outcome::done("No QR code in that picture")),
        1 => {
            let one = &found[0];
            // Named rather than pasted into the message: a payload is often a
            // long address, and a status line is one line.
            let said = if one.chars().count() > 60 {
                "Copied the code's address".to_string()
            } else {
                format!("Copied {one}")
            };
            copy_with_undo(ctx, one, &said)
        }
        many => {
            // One per line, in the order they were found, because a picture
            // with several codes in it is a page of them and the order is the
            // only thing that distinguishes one row from the next.
            let all = found.join("\n");
            copy_with_undo(ctx, &all, &format!("Copied {many} codes"))
        }
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
    fn id(&self) -> &str {
        "sill.searchWeb"
    }

    fn title(&self) -> &str {
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

        // A custom engine is a template somebody typed into settings, so the
        // scheme is theirs rather than Sill's.
        let url = crate::reach::url(&url)?;

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
    fn id(&self) -> &str {
        "sill.openUrl"
    }

    fn title(&self) -> &str {
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
        // The address arrived from somewhere: a search result, the clipboard,
        // or a model that has just read a web page telling it what to open.
        // None of those is Sill's own text.
        let address = crate::reach::url(&object.target)?;

        tauri_plugin_opener::open_url(address, None::<&str>)
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
    fn id(&self) -> &str {
        "sill.copyUrl"
    }

    fn title(&self) -> &str {
        "Copy Address"
    }

    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        // The same key as Copy Path: a web address has no path, so the two are never on one list together.
        chord("Ctrl+Shift+C")
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
    fn id(&self) -> &str {
        "sill.file.hash"
    }

    fn title(&self) -> &str {
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
    fn id(&self) -> &str {
        "sill.file.compress"
    }

    fn title(&self) -> &str {
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

/// Writes a picture out again in another format.
///
/// One struct, one instance per format, the way the fifteen window slots are
/// one struct: the difference between PNG and JPEG is a name and an encoder
/// id, and two near-identical impls would be two places to fix anything
/// learned about either.
///
/// **The original is never touched.** The new file goes beside it under a
/// free name, which is what makes deleting that file an honest undo.
struct ConvertImage {
    to: crate::images::Format,
}

#[async_trait]
impl Action for ConvertImage {
    fn id(&self) -> &str {
        match self.to {
            crate::images::Format::Png => "sill.image.toPng",
            crate::images::Format::Jpeg => "sill.image.toJpeg",
        }
    }

    fn title(&self) -> &str {
        self.to.title()
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        // A folder holds pictures rather than being one, and the kind is all
        // this can see. Whether the file really is a picture, and whether
        // converting it would only rename it, are decided in `run`.
        kind == ObjectKind::File
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileRead, Capability::FileWrite]
    }

    async fn run(&self, _ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);
        let name = crate::files_ops::name_of(&path);

        if !crate::images::is_image(&path) {
            return Err(format!("{name} is not a picture"));
        }

        if crate::images::already(&path, self.to) {
            return Err(format!("{name} is already {}", self.to.extension()));
        }

        let to = self.to;
        let made = tokio::task::spawn_blocking(move || crate::images::convert(&path, to))
            .await
            .map_err(|err| format!("could not convert that picture: {err}"))??;

        let into = crate::files_ops::name_of(&made);

        Ok(Outcome::undoable(
            format!("Wrote {into}"),
            Undo::DeleteFile {
                path: made.to_string_lossy().into_owned(),
                name: into,
            },
        ))
    }
}

/// Renames a file or folder, keeping it where it is.
///
/// The asking is a feature of the launcher, not of the rename. The window
/// takes over its own field to collect the new name, exactly as it does for a
/// quicklink with a hole in it, and hands the answer over in the context.
///
/// It used to hand the answer to a Tauri command that did the renaming itself,
/// and this action existed only to say that it refused. That made renaming
/// something **only the page could do**: no key could be bound to it, the model
/// could not run it, and it appeared in no activity log, because none of those
/// go anywhere near a command the window calls.
/// The clipboard entry an action was pointed at, and the store holding it.
///
/// The row carries its own row number in `id`; the target is the text, which
/// is what every other clipboard action wants and not what these two do.
fn clipboard_entry_of(
    ctx: &ActionCtx,
    object: &Object,
) -> Result<(i64, crate::clipboard::store::Entry), String> {
    let id: i64 = object
        .id
        .parse()
        .map_err(|_| "that clipboard row cannot be looked up".to_string())?;

    let clipboard = ctx
        .app
        .try_state::<crate::clipboard::monitor::Clipboard>()
        .ok_or_else(|| "clipboard history is not running".to_string())?;

    let entry = clipboard
        .store()
        .get(id)
        .map_err(|err| format!("could not read that entry: {err}"))?
        .ok_or_else(|| "that entry is no longer in the history".to_string())?;

    Ok((id, entry))
}

/// Gives a clipboard entry a name of its own.
///
/// The text is untouched: a name is how a row is recognised in a list of
/// four hundred, and what is pasted stays what was copied. An empty name
/// takes the name away.
struct RenameClipboardEntry;

#[async_trait]
impl Action for RenameClipboardEntry {
    fn id(&self) -> &str {
        "sill.clipboard.rename"
    }

    fn title(&self) -> &str {
        "Name This Entry"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ClipboardEntry
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let (id, entry) = clipboard_entry_of(ctx, object)?;
        let name = ctx.argument().map(str::trim).unwrap_or_default();

        let clipboard = ctx.app.state::<crate::clipboard::monitor::Clipboard>();
        clipboard
            .store()
            .set_title(id, Some(name).filter(|name| !name.is_empty()))
            .map_err(|err| format!("could not name that entry: {err}"))?;

        let said = if name.is_empty() {
            "Name taken away".to_string()
        } else {
            format!("Named it {name}")
        };

        Ok(Outcome::undoable(
            said,
            Undo::RestoreClipboardEntry {
                id,
                title: entry.title,
                text: entry.text,
            },
        ))
    }
}

/// Corrects a clipboard entry's text in place.
///
/// For the typo in the thing that was copied three times. Text only: a
/// picture's text is a caption Sill wrote, and editing it would edit a
/// description rather than the thing described. Refused when the new text is
/// already another entry, rather than merged, because the two may sit in
/// different collections with different names.
struct EditClipboardEntry;

#[async_trait]
impl Action for EditClipboardEntry {
    fn id(&self) -> &str {
        "sill.clipboard.edit"
    }

    fn title(&self) -> &str {
        "Edit Text"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ClipboardEntry
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let (id, entry) = clipboard_entry_of(ctx, object)?;

        if entry.kind == crate::clipboard::kind::Kind::Image {
            return Err("a picture has no text to edit".to_string());
        }

        let text = ctx
            .argument()
            .ok_or("editing needs the new text, which the launcher asks for")?
            .to_string();

        if text.trim().is_empty() {
            return Err("the text cannot be empty; delete the entry instead".to_string());
        }

        let clipboard = ctx.app.state::<crate::clipboard::monitor::Clipboard>();
        clipboard
            .store()
            .set_text(id, &text, crate::state::now_seconds())
            .map_err(|why| format!("could not edit that entry: {why}"))?;

        Ok(Outcome::undoable(
            "Text edited",
            Undo::RestoreClipboardEntry {
                id,
                title: entry.title,
                text: entry.text,
            },
        ))
    }
}

/// Copies an installed font's family name, which is what a stylesheet or a
/// settings page wants typed exactly.
struct CopyFontName;

#[async_trait]
impl Action for CopyFontName {
    fn id(&self) -> &str {
        "sill.font.copyName"
    }

    fn title(&self) -> &str {
        "Copy Font Name"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Font
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let outcome = copy_with_undo(ctx, &object.target, &format!("Copied {}", object.target))?;
        crate::dismiss_main(&ctx.app);
        Ok(outcome)
    }
}

/// Sets a display's resolution and refresh rate from its row.
///
/// Applied, then asked about, then put back unless kept: a mode the driver
/// accepts and the monitor does not is a black screen, and an undo nobody
/// can see is no undo. The question is a native dialog raced against
/// fifteen seconds. A "Keep" that arrives after the revert applies the mode
/// again rather than being ignored, so the button on screen never lies.
struct SetDisplayMode;

#[async_trait]
impl Action for SetDisplayMode {
    fn id(&self) -> &str {
        "sill.display.setMode"
    }

    fn title(&self) -> &str {
        "Use This Mode"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::DisplayMode
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

        let wanted = crate::displays::mode_from(&object.target)?;
        let said = crate::displays::said(&wanted);

        let applying = wanted.clone();
        let was = tokio::task::spawn_blocking(move || {
            crate::displays::set(&applying.device, &applying)
        })
        .await
        .map_err(|err| format!("could not change the display: {err}"))??;

        // The launcher goes away so the question is what is on screen.
        crate::dismiss_main(&ctx.app);

        let asked = ctx.app.clone();
        let question = said.clone();
        let (answered, answer) = tokio::sync::oneshot::channel::<bool>();
        tokio::task::spawn_blocking(move || {
            let kept = asked
                .dialog()
                .message(format!(
                    "{question}. It goes back in fifteen seconds unless you keep it."
                ))
                .title("Keep these display settings?")
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Keep".to_string(),
                    "Revert".to_string(),
                ))
                .kind(MessageDialogKind::Info)
                .blocking_show();
            let _ = answered.send(kept);
        });

        let mut answer = answer;
        let kept = match tokio::time::timeout(crate::displays::KEEP_WITHIN, &mut answer).await {
            Ok(Ok(kept)) => kept,
            Ok(Err(_)) => false,
            Err(_) => {
                // No answer in time. The dialog is still on screen, and a
                // late "Keep" is honoured by applying the mode again.
                let again = wanted.clone();
                tokio::spawn(async move {
                    if let Ok(true) = answer.await {
                        let _ = tokio::task::spawn_blocking(move || {
                            crate::displays::set(&again.device, &again)
                        })
                        .await;
                    }
                });
                false
            }
        };

        if !kept {
            let back = was.clone();
            tokio::task::spawn_blocking(move || crate::displays::set(&back.device, &back))
                .await
                .map_err(|err| format!("could not put the display back: {err}"))??;
            return Ok(Outcome::done(format!("Put back {}", crate::displays::said(&was))));
        }

        Ok(Outcome::undoable(
            format!("Set {said}"),
            Undo::RestoreDisplayMode {
                device: was.device,
                display: was.display,
                width: was.width,
                height: was.height,
                hz: was.hz,
            },
        ))
    }
}

struct RenameFile;

#[async_trait]
impl Action for RenameFile {
    fn id(&self) -> &str {
        "sill.file.rename"
    }

    fn title(&self) -> &str {
        "Rename"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::File | ObjectKind::Folder)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        // No name is not a rename to nothing, it is a caller that skipped the
        // asking. Said rather than guessed at.
        let to = ctx
            .argument()
            .ok_or("renaming needs a new name, which the launcher asks for")?
            .to_string();

        let from = std::path::PathBuf::from(&object.target);
        let was = crate::files_ops::name_of(&from);

        let landed = tokio::task::spawn_blocking(move || crate::files_ops::rename(&from, &to))
            .await
            .map_err(|err| format!("could not rename that: {err}"))??;

        // No undo. Renaming back is a second rename and nothing here can know
        // that the name it would put back is still free, so an undo token
        // would be a promise this cannot keep.
        Ok(Outcome::done(format!(
            "Renamed {was} to {}",
            crate::files_ops::name_of(&landed)
        )))
    }
}

/// Moves a file or folder into another folder.
///
/// Takes its destination the way renaming takes its new name. The window
/// borrows its whole list rather than only the field, because the answer is a
/// folder and typing only narrows which one, but what it does with the answer
/// is one call either way.
///
/// The only file action that reverses exactly, and the token is two paths
/// rather than anything copied, so undoing a move of ten gigabytes costs what
/// undoing a move of a text file costs.
struct MoveFile;

#[async_trait]
impl Action for MoveFile {
    fn id(&self) -> &str {
        "sill.file.move"
    }

    fn title(&self) -> &str {
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

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let folder = ctx
            .argument()
            .ok_or("moving needs somewhere to move to, which the launcher asks for")?
            .to_string();

        let from = std::path::PathBuf::from(&object.target);
        let into = std::path::PathBuf::from(&folder);
        let name = crate::files_ops::name_of(&from);

        // Where it came out of, read before the move, because afterwards there
        // is nothing left at the old path to ask.
        let came_from = from
            .parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .ok_or_else(|| format!("{name} has nowhere to be put back"))?;

        let landed = {
            let from = from.clone();
            let into = into.clone();

            // Blocking: between two drives this copies, and that is as slow as
            // whatever is being moved is large.
            tokio::task::spawn_blocking(move || crate::files_ops::move_to(&from, &into))
                .await
                .map_err(|err| format!("could not move that: {err}"))??
        };

        // After the move, so a folder that refused is not learned as one
        // somebody uses. The next move offers it first.
        crate::state::remember_destination(&ctx.app, &folder);

        Ok(Outcome::undoable(
            format!("Moved {name} to {}", crate::files_ops::name_of(&into)),
            Undo::MovePath {
                path: landed.to_string_lossy().to_string(),
                back_to: came_from,
                name,
            },
        ))
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
    fn id(&self) -> &str {
        "sill.file.verify"
    }

    fn title(&self) -> &str {
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
            return Err(format!("that is a {} and Sill checks SHA-256", kind.name()));
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
    fn id(&self) -> &str {
        "sill.file.lookUp"
    }

    fn title(&self) -> &str {
        "Look Up on VirusTotal"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::File
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[
            Capability::FileRead,
            Capability::Network,
            Capability::ProcessLaunch,
        ]
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

// ------------------------------------------------------------------ scripts

/// Runs a script command and hands back what it printed.
///
/// Declaring [`Capability::ShellExecution`] is what makes this safe to have at
/// all. Somebody pressing Enter on a script they put in their own folder is
/// the consent for that one run, and the capability is what stops the two
/// callers who are not that person: the model has to raise an approval card
/// before `run_action` will touch it, and an extension cannot reach the action
/// registry at all.
struct RunScript;

#[async_trait]
impl Action for RunScript {
    fn id(&self) -> &str {
        "sill.script.run"
    }

    fn title(&self) -> &str {
        "Run"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        matches!(kind, ObjectKind::Script)
    }

    fn capabilities(&self) -> &'static [Capability] {
        crate::shell::NEEDS
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        self.accepts(kind)
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let path = std::path::PathBuf::from(&object.target);

        // Read again rather than trusting what the index holds. A script's
        // header decides how its output is shown, and the file may have been
        // edited since the scan; running it under last week's mode would print
        // something somebody had since marked silent.
        let script = crate::scripts::read(&path)
            .ok_or_else(|| format!("{} is no longer a script command", object.title))?;

        let prefs = ctx
            .app
            .try_state::<crate::state::PrefsState>()
            .map(|prefs| prefs.inner.clone());

        let timeout = match &prefs {
            Some(prefs) => {
                std::time::Duration::from_secs(prefs.lock().await.scripts.timeout_seconds.max(1))
            }
            None => crate::shell::DEFAULT_TIMEOUT,
        };

        /*
         * The answer the caller gave, as the script's first argument.
         *
         * This used to refuse outright, which made every script declaring a
         * required argument reachable from the launcher window and from
         * nowhere else: no key could be bound to one, and the model could not
         * run one. Both of those carry an answer already, a `Binding` in the
         * field it was recorded with and a tool call in its `argument`, so
         * the only thing missing was somewhere here to put it.
         *
         * Still refused when nothing was given. A script that declares a
         * required argument is written expecting one, and running it with an
         * empty string is somebody else's code deciding what empty means,
         * which for a script called "Delete branch" is not a guess worth
         * making on their behalf.
         */
        let args = given(&script, ctx.argument())?;

        let allowed = match &prefs {
            Some(prefs) => prefs.lock().await.scripts.elevated.clone(),
            None => Vec::new(),
        };

        // Where it runs, what it runs with, and whether Windows is going to be
        // asked for administrator rights. One decision, shared with the
        // launcher's own path, so the two cannot answer it differently.
        let plan = crate::scripts::plan(&script, &allowed)?;

        let ran = crate::shell::run(
            &crate::shell::Setup::new(script.shell, &object.target)
                .with(&args)
                .in_folder(&plan.directory)
                .and_environment(&plan.environment)
                .within(timeout)
                .as_administrator(plan.elevated),
            &crate::shell::Stop::never(),
        )
        .await?;

        Ok(outcome_of(&script, &ran))
    }
}

/**
What an action run hands a script, from the one answer it was given.

Its own function, and that is the point of it. This is the whole of what makes
a script reachable by anything but the launcher window, and [`ActionCtx`] holds
a concrete `AppHandle`, so `RunScript::run` cannot be called from a test at
all. This can be, and the behaviour worth testing is all here.

It used to be a flat refusal: a script declaring a required argument could be
run from the window and from nowhere else, because no key could carry the
answer and the model had nowhere to put one. Both of them carry an answer
already, a `Binding` in the field it was recorded with and a tool call in its
`argument`; the only thing missing was somewhere here to receive it.

**One answer, and only the first argument.** `ActionCtx` carries one, because
renaming and moving each ask exactly one thing. So a script whose second or
third argument is required cannot be run this way, and it says so rather than
being handed one answer and two blanks, which is somebody else's code deciding
what an empty string means.
*/
fn given(script: &crate::scripts::Script, argument: Option<&str>) -> Result<Vec<String>, String> {
    let asks = crate::scripts::asks(script);

    let beyond: Vec<&str> = script
        .arguments
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, declared)| !declared.optional)
        .map(|(at, _)| asks.get(at).map(String::as_str).unwrap_or("something"))
        .collect();

    if !beyond.is_empty() {
        return Err(format!(
            "{} also asks for {}, and an action can be given one answer; run it from the launcher",
            script.title,
            beyond.join(" and "),
        ));
    }

    if let Some(said) = argument {
        return Ok(vec![said.to_string()]);
    }

    match script.arguments.first().filter(|first| !first.optional) {
        // Refused rather than run empty. A script that declares a required
        // argument is written expecting one, and "Delete branch" with an empty
        // branch is not a guess worth making on somebody's behalf.
        Some(_) => Err(format!(
            "{} needs {} to be given to it, and nothing was",
            script.title,
            asks.first().map(String::as_str).unwrap_or("something"),
        )),
        None => Ok(Vec::new()),
    }
}

/// Turns a finished run into what the window should say and show.
///
/// Split out so the wording is testable without running anything: every branch
/// here is a sentence somebody reads at the moment they are least inclined to
/// investigate, and getting "it worked" onto a failure is worse than saying
/// nothing.
fn outcome_of(script: &crate::scripts::Script, ran: &crate::shell::Ran) -> Outcome {
    use crate::scripts::Mode;
    use crate::shell::Ended;

    let title = &script.title;

    let said = match ran.ended {
        Ended::TimedOut => format!("{title} was stopped after running too long"),
        Ended::Cancelled => format!("{title} was stopped"),
        // Deliberately not "Ran". Sill handed it to Windows and has no exit
        // code, no output and no way to stop it, and a sentence in the past
        // tense would be claiming to know it worked.
        Ended::Started => {
            format!("{title} was started as administrator. Sill cannot see what it does")
        }
        Ended::Finished if ran.code != Some(0) => {
            // The last line of stderr is nearly always the actual complaint,
            // and the rest is a stack. Somebody wants the complaint.
            let complaint = ran
                .stderr
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");

            format!("{title} failed: {complaint}")
        }
        Ended::Finished => match script.mode {
            // The last line is the result for these two, which is the
            // convention the format was built around: a script prints working
            // and then the answer.
            Mode::Compact | Mode::Inline => ran
                .stdout
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Done")
                .to_string(),
            Mode::Silent => format!("Ran {title}"),
            Mode::FullOutput => format!("Ran {title}"),
        },
    };

    let outcome = Outcome::done(said);

    // Only `fullOutput` asks for the whole thing, and only a run that produced
    // something has one to give. A silent script that printed is still silent.
    match (script.mode, ran.stdout.is_empty()) {
        (Mode::FullOutput, false) => outcome.producing(ran.stdout.clone()),
        _ => outcome,
    }
}

// ------------------------------------------------------------------- notes

/**
Whether notes are switched on at all.

Read here rather than trusted from the caller, and read by both note actions,
because these are reachable by id: a key, the model, a `sill://` link and a
scheduled task all reach the registry without going anywhere near the search
that would have hidden the row. A prototype behind a switch has to be behind it
on every path, or the switch is decoration.
*/
async fn notes_are_on(ctx: &ActionCtx) -> Result<(), String> {
    let prefs = ctx.app.state::<crate::state::PrefsState>();
    let on = prefs.inner.lock().await.general.notes;

    if on {
        return Ok(());
    }

    Err("Notes are switched off. Turn them on in Settings, under General.".to_string())
}

/// Opens a note in the notes window, making one if there is none yet.
///
/// One action for both, because an empty entrypoint is what the `New Note` row
/// carries and opening a note that does not exist yet is the same act from
/// where somebody is standing: the window comes up with a cursor in it either
/// way.
struct OpenNote;

#[async_trait]
impl Action for OpenNote {
    fn id(&self) -> &str {
        "sill.note.open"
    }

    fn title(&self) -> &str {
        "Open"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Note
    }

    /// Drawing in a window Sill owns, and nothing else.
    ///
    /// Not `FileWrite`, even though a new note is eventually written to disk.
    /// The capability is about what somebody is being asked to agree to, and
    /// what this does is put a window on screen with a cursor in it; the file
    /// is Sill's own store, the same way a clipboard entry is, and every store
    /// in the application would need `FileWrite` if this one did.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Note
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        notes_are_on(ctx).await?;

        let notes = ctx.app.state::<crate::notes::Notes>();

        let note = if object.target.is_empty() {
            notes.write(&ctx.app, "", "", crate::state::now_seconds())?
        } else {
            notes
                .one(&ctx.app, &object.target)
                .ok_or_else(|| format!("There is no note called {}.", object.target))?
        };

        crate::commands::notes::show_note(&ctx.app, &note.id)?;

        Ok(Outcome::done(format!("Opened {}", note.title())))
    }
}

/// Copies what a note says.
struct CopyNote;

#[async_trait]
impl Action for CopyNote {
    fn id(&self) -> &str {
        "sill.note.copy"
    }

    fn title(&self) -> &str {
        "Copy Note"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Note
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        notes_are_on(ctx).await?;

        let note = ctx
            .app
            .state::<crate::notes::Notes>()
            .one(&ctx.app, &object.target)
            .ok_or_else(|| format!("There is no note called {}.", object.target))?;

        if note.text.trim().is_empty() {
            return Err("That note is empty.".to_string());
        }

        copy_with_undo(ctx, &note.text, &format!("Copied {}", note.title()))
    }
}

// ---------------------------------------------------------------- reminders

/// Puts a reminder on Windows' clock.
///
/// The object's target is the whole query, exactly as it was typed, and
/// [`crate::timers::matched`] reads it here for the second and last time. The
/// row that offered this read it once to say what would happen; nothing in
/// between interpreted it, so the two readings cannot disagree.
struct SetReminder;

#[async_trait]
impl Action for SetReminder {
    fn id(&self) -> &str {
        "sill.reminder.set"
    }

    fn title(&self) -> &str {
        "Set Reminder"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Reminder
    }

    /**
    Changes this machine, and that is not a formality.

    Writing a scheduled task puts something in Windows that outlives Sill's
    process, survives a reboot and survives an uninstall. `SystemControl` is
    the capability that already means exactly that, and declaring it buys two
    rules at once without either being written here: the model stops and asks
    before setting one, and `automation::may_schedule` refuses to schedule
    this, so **a trigger cannot make more triggers**.
    */
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::SystemControl]
    }

    fn is_primary(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Reminder
    }

    #[cfg(windows)]
    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let timer = crate::timers::matched(&object.target)
            .ok_or_else(|| format!("{} is not a length of time.", object.target))?;

        let at = crate::timers::fires_at(crate::timers::now(), timer.after);

        /*
         * Through the same command the automations panel writes a trigger
         * with, rather than reaching `automation::register` directly.
         *
         * That command is where `may_schedule` is consulted, where the action
         * is checked against the kind it will act on, and where the task name
         * is validated, and `verify:source` holds it to having the gate inside
         * it. A second caller of `register` would be a second place that has
         * to remember all of that, which is the shape this codebase has paid
         * for four times.
         */
        let said = crate::commands::automation::schedule(
            ctx.app.clone(),
            crate::automation::Trigger {
                // The message, repaired into something Task Scheduler takes.
                // The time is in it so two reminders with the same words at
                // different moments are two tasks rather than one replacing
                // the other.
                name: crate::automation::sanitised_name(&format!(
                    "{} at {}",
                    timer.message,
                    at.clock().replace(':', ".")
                )),
                action: ShowReminder.id().to_string(),
                target: timer.message.clone(),
                kind: Some(ObjectKind::Reminder.name().to_string()),
                argument: None,
                when: crate::automation::When::Once { at },
            },
        )
        .await?;

        crate::say!("[timers] {said}");

        Ok(Outcome::done(format!(
            "Reminder set for {}, in {}",
            at.clock(),
            crate::timers::said(timer.after)
        )))
    }

    #[cfg(not(windows))]
    async fn run(&self, _ctx: &ActionCtx, _object: &Object) -> Result<Outcome, String> {
        Err("Timers need the Windows task scheduler.".to_string())
    }
}

/**
Puts a reminder on screen, which is what a fired timer runs.

The one action a timer's task names, and the whole reason it may be scheduled
at all: it draws in a window Sill already owns and does nothing else, so it is
`Capability::Ui` and `automation::may_schedule` lets it through.

What it hands the launcher is a **piece of text**, not a reminder. Setting one
is over by the time it arrives, and the useful things to do with it now are the
things anybody does with text: copy it, have it read out, act on it. The mode
says where it came from, so the row is headed "Reminder" rather than
"Selection".
*/
struct ShowReminder;

#[async_trait]
impl Action for ShowReminder {
    fn id(&self) -> &str {
        "sill.reminder.show"
    }

    fn title(&self) -> &str {
        "Show It Now"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Reminder
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let message = if object.target.trim().is_empty() {
            "Timer".to_string()
        } else {
            object.target.clone()
        };

        crate::summon::show_actions(
            &ctx.app,
            &[Object {
                kind: ObjectKind::Text,
                id: format!("reminder:{message}"),
                target: message.clone(),
                title: message.clone(),
                mode: "reminder-shown".to_string(),
            }],
        );

        Ok(Outcome::done(format!("Reminder: {message}")))
    }
}

#[cfg(test)]
mod running_a_script {
    use super::*;
    use crate::scripts::{Mode, Script};
    use crate::shell::{Ended, Ran, Shell};

    fn script(mode: Mode) -> Script {
        Script {
            path: std::path::PathBuf::from("deploy.ps1"),
            title: "Deploy".to_string(),
            mode,
            shell: Shell::PowerShell,
            package: None,
            icon: None,
            description: None,
            author: None,
            arguments: Vec::new(),
            needs_argument: false,
            directory: None,
            environment: Vec::new(),
            wants_admin: false,
        }
    }

    fn ran(code: Option<i32>, ended: Ended, stdout: &str, stderr: &str) -> Ran {
        Ran {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            code,
            truncated: false,
            ended,
            took_ms: 1,
        }
    }

    /// A script that failed must never read as one that worked.
    ///
    /// The mode says how to show output, not whether to admit a failure, and
    /// `silent` in particular means "say nothing when it works". Somebody
    /// whose backup script exits 1 every night and reports "Ran Deploy" finds
    /// out when they need the backup.
    #[test]
    fn a_failure_is_reported_whatever_the_mode_says() {
        for mode in [Mode::Silent, Mode::Compact, Mode::Inline, Mode::FullOutput] {
            let outcome = outcome_of(
                &script(mode),
                &ran(
                    Some(1),
                    Ended::Finished,
                    "working",
                    "could not reach the server",
                ),
            );

            assert!(
                outcome.message.contains("failed"),
                "{mode:?} reported a failure as {:?}",
                outcome.message,
            );
            assert!(
                outcome.message.contains("could not reach the server"),
                "{mode:?} dropped the complaint: {:?}",
                outcome.message,
            );
        }
    }

    /// The last line is the answer, which is the convention the format is
    /// built around: a script prints its working and then its result.
    #[test]
    fn compact_says_the_last_line() {
        let outcome = outcome_of(
            &script(Mode::Compact),
            &ran(Some(0), Ended::Finished, "step one\nstep two\n42\n", ""),
        );

        assert_eq!(outcome.message, "42");
    }

    /// Only `fullOutput` hands the whole thing back.
    ///
    /// A silent script that printed is still silent, and passing its output on
    /// would put on screen exactly what its author marked as not for showing.
    #[test]
    fn only_full_output_carries_the_text() {
        let stdout = "line one\nline two\n";

        assert_eq!(
            outcome_of(
                &script(Mode::FullOutput),
                &ran(Some(0), Ended::Finished, stdout, "")
            )
            .text,
            Some(stdout.to_string()),
        );

        for quiet in [Mode::Silent, Mode::Compact, Mode::Inline] {
            assert_eq!(
                outcome_of(&script(quiet), &ran(Some(0), Ended::Finished, stdout, "")).text,
                None,
                "{quiet:?} passed output on",
            );
        }
    }

    /// An elevated start must never read as a run that finished.
    ///
    /// There is no exit code, no output and no way to stop it, so "Ran Deploy"
    /// would be claiming to know something Sill was never told.
    #[test]
    fn an_elevated_start_does_not_claim_it_worked() {
        let outcome = outcome_of(
            &script(Mode::FullOutput),
            &ran(None, Ended::Started, "", ""),
        );

        assert!(
            outcome.message.contains("started as administrator"),
            "it said {:?}",
            outcome.message,
        );
        assert_eq!(outcome.text, None, "it offered output it never had");
    }

    /// What makes a script reachable by a key and by the model.
    mod the_answer_it_is_given {
        use super::*;
        use crate::scripts::Argument;

        fn asking(arguments: Vec<Argument>) -> Script {
            Script {
                needs_argument: arguments.iter().any(|one| !one.optional),
                arguments,
                ..script(Mode::Silent)
            }
        }

        fn argument(placeholder: &str, optional: bool) -> Argument {
            Argument {
                placeholder: placeholder.to_string(),
                optional,
                percent_encoded: false,
            }
        }

        #[test]
        fn a_script_that_asks_nothing_is_run_with_nothing() {
            assert_eq!(given(&asking(Vec::new()), None), Ok(Vec::new()));
        }

        /// The change this is here for. A key recorded with an answer, or a
        /// model that gave one, now reaches a script that asks for one.
        #[test]
        fn an_answer_becomes_the_scripts_first_argument() {
            assert_eq!(
                given(&asking(vec![argument("branch", false)]), Some("main")),
                Ok(vec!["main".to_string()]),
            );
        }

        /// Still refused with nothing, and it says what it wanted in the
        /// author's own word rather than "argument 1".
        #[test]
        fn a_required_argument_with_no_answer_says_what_it_wanted() {
            let why = given(&asking(vec![argument("branch", false)]), None).expect_err("refused");

            assert!(why.contains("branch"), "it said {why}");
            assert!(why.contains("Deploy"), "it did not name the script: {why}");
        }

        #[test]
        fn an_optional_argument_does_not_stop_it() {
            assert_eq!(
                given(&asking(vec![argument("branch", true)]), None),
                Ok(Vec::new()),
            );
        }

        /// One answer is all a context carries, so a script that needs two
        /// says so rather than being handed one and a blank.
        #[test]
        fn a_second_required_argument_cannot_be_answered_this_way() {
            let script = asking(vec![argument("from", false), argument("to", false)]);

            let why = given(&script, Some("main")).expect_err("refused");

            assert!(
                why.contains("to"),
                "it did not name the one it cannot fill: {why}"
            );
            assert!(
                why.contains("launcher"),
                "it did not say where it can be run: {why}"
            );
        }
    }

    #[test]
    fn being_stopped_is_not_a_failure_and_not_a_success() {
        for (ended, expected) in [
            (Ended::TimedOut, "running too long"),
            (Ended::Cancelled, "was stopped"),
        ] {
            let outcome = outcome_of(&script(Mode::FullOutput), &ran(None, ended, "", ""));

            assert!(
                outcome.message.contains(expected),
                "{ended:?} said {:?}",
                outcome.message,
            );
        }
    }
}

/**
Which of these a trigger may name.

`P8-02` narrows automation to the actions that never stop and ask, and that
narrowing is one line reading [`Action::capabilities`]. What it does not do on
its own is say whether anything useful is left: a rule refusing the entire
registry would be green in every test written about the rule itself.

So it is checked against real actions, and the ones that have to survive it
are named rather than counted. A count stays green while the half of the list
somebody would actually use quietly leaves it.

**Not `builtins()`.** For the reason [`crate::suite`] already gives about the
`actions` integration test: building the registry retains the dialog plugin's
`TaskDialogIndirect`, which the library's own test binary has no manifest to
resolve, so the whole `--lib` run dies at load with
`STATUS_ENTRYPOINT_NOT_FOUND` before a single test starts. That was reproduced
here rather than taken on trust. The concrete types below are called directly,
so no trait object is built and none of it is linked in.
*/
#[cfg(test)]
mod what_a_trigger_may_run {
    use super::*;
    use crate::automation::may_schedule;

    #[test]
    fn the_useful_ones_survive_the_narrowing() {
        assert!(may_schedule(CopyPath.id(), CopyPath.capabilities()).is_ok());
        assert!(may_schedule(CopyName.id(), CopyName.capabilities()).is_ok());
        assert!(may_schedule(HashFile.id(), HashFile.capabilities()).is_ok());
    }

    #[test]
    fn the_ones_that_would_ask_do_not() {
        assert!(may_schedule(RunScript.id(), RunScript.capabilities()).is_err());
        assert!(may_schedule(Launch.id(), Launch.capabilities()).is_err());
        assert!(may_schedule(RecycleFile.id(), RecycleFile.capabilities()).is_err());
    }

    /// The one a timer's task names has to be schedulable, or the feature
    /// cannot exist.
    ///
    /// This is the whole of why `ShowReminder` draws and does nothing else. It
    /// fires with nobody at the machine, so it must be something that never
    /// stops to ask, and the narrowing `may_schedule` applies is the rule that
    /// decides rather than a list anybody keeps.
    #[test]
    fn showing_a_reminder_is_something_windows_may_start() {
        assert!(may_schedule(ShowReminder.id(), ShowReminder.capabilities()).is_ok());
    }

    /// And setting one is not, so a trigger cannot make more triggers.
    ///
    /// Not enforced by a rule written here. `SetReminder` declares
    /// `SystemControl` because writing a scheduled task changes the machine,
    /// and the narrowing that already exists does the rest.
    #[test]
    fn a_trigger_cannot_schedule_more_triggers() {
        assert!(may_schedule(SetReminder.id(), SetReminder.capabilities()).is_err());
    }

    /// Both note actions may be scheduled, and that is worth saying out loud.
    ///
    /// Opening one draws in a window Sill owns. Copying one goes through the
    /// deliberate `ClipboardWrite` exception `needs_asking` documents: a copy
    /// looks destructive and is not, because the history keeps what was there.
    /// So `note read this every morning` is a trigger somebody can make, and
    /// the switch in Settings is still what decides whether it does anything.
    #[test]
    fn a_trigger_may_reach_a_note() {
        assert!(may_schedule(OpenNote.id(), OpenNote.capabilities()).is_ok());
        assert!(may_schedule(CopyNote.id(), CopyNote.capabilities()).is_ok());
    }
}
