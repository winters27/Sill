//! What a row *is*, rather than what it opens.
//!
//! Every other action in this registry does something to the thing behind a
//! row: it launches it, copies it, closes it. These six do something to the
//! row itself.
//!
//! They exist because the capability was already here and reachable from
//! nowhere. Ranking, pins and per-command keys were all settings somebody had
//! to go and find, never things done to the row in front of them.
//!
//! Naming was the exception until it moved here. It lived in `panel.ts` as a
//! pair of hand-written entries, which meant the one thing you could do to a
//! row that the panel offered and a bound key, the model and an automation
//! could not. Moving it cost `describe_for`: an entry that retitles itself and
//! hides itself needs to see the row, and the registry could only see the
//! kind. Every action gains that now, rather than naming keeping a private
//! arrangement.
//!
//! ## Why these are keyed on the id
//!
//! All six take `object.id` rather than `object.target`. The id is what
//! ranking, pinning and aliasing are stored against; the target is what the
//! action opens. Two rows routinely share a target and never share an id: a
//! quicklink and the application it points at are one path and two rows, and
//! pinning one must not pin the other.
//!
//! ## What they do not accept
//!
//! Files and folders are absent. Those arrive from a filesystem search rather
//! than the index, they are identified by a path that moves, and there is
//! nothing to reset because nothing was ever learned about them.

use async_trait::async_trait;
use tauri::Manager;

use crate::action::{Action, ActionCtx, ActionRegistry, Capability, Outcome, Row};
use crate::object::{Object, ObjectKind};
use crate::state::{PrefsState, RegistryState};

/// Rows that came out of the index, and so have a stable id to act on.
fn indexed(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Application
            | ObjectKind::ExtensionCommand
            | ObjectKind::SystemSetting
            | ObjectKind::Setting
            | ObjectKind::Builtin
            | ObjectKind::SystemControl
            | ObjectKind::Snippet
            | ObjectKind::Quicklink
            | ObjectKind::Script
    )
}

/// Percent-encodes one field of a `sill://` link.
///
/// The pair of [`crate::reach::decoded`], which is hand-written for the same
/// reason this is: the whole need is one field of one scheme, and a URL crate
/// is a dependency and a parser for a job that is a loop over bytes.
///
/// Encodes everything outside RFC 3986's unreserved set. That is stricter
/// than it has to be for a path, and deliberately so: a target is arbitrary
/// text somebody else wrote, and the escapes that matter here are `&` and `=`,
/// which would otherwise end the field early and put the rest of a filename
/// into a parameter of its own.
fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }

    out
}

// ------------------------------------------------------------------ deeplink

pub(super) struct CopyDeeplink;

#[async_trait]
impl Action for CopyDeeplink {
    fn id(&self) -> &str {
        "sill.row.deeplink"
    }

    fn title(&self) -> &str {
        "Copy Deeplink"
    }

    fn icon(&self) -> &'static str {
        "Link"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::ClipboardRead, Capability::ClipboardWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        /*
         * The link names the action Enter runs, asked of the registry rather
         * than decided here.
         *
         * A table in this file mapping kind to action id would be a second
         * answer to a question the registry already answers, and the one that
         * would quietly stop agreeing the first time a primary moved.
         */
        let actions = ctx.app.state::<ActionRegistry>();
        let primary = actions
            .primary(object.kind)
            .ok_or_else(|| format!("nothing is bound to Enter for {}", object.title))?;

        let link = format!(
            "sill://run/{}?target={}",
            encoded(primary.id()),
            encoded(&object.target)
        );

        super::copy_with_undo(ctx, &link, "Copied the link to")
    }
}

// ------------------------------------------------------------------- ranking

pub(super) struct ResetRanking;

#[async_trait]
impl Action for ResetRanking {
    fn id(&self) -> &str {
        "sill.row.forget"
    }

    fn title(&self) -> &str {
        "Reset Ranking"
    }

    fn icon(&self) -> &'static str {
        "ArrowCounterClockwise"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    /// Writes to disk: the ranking history for this one, `preferences.json`
    /// for the other two. An empty list here would be a lie, and the
    /// permission model reads exactly this.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let state = ctx.app.state::<RegistryState>();
        let id = object.id.clone();

        // Copy, change, swap, and write outside the lock, which is the rule
        // every other writer of this file follows: the next keystroke is
        // waiting on the lock and not on the disk.
        let (path, text) = state.record(move |ranking| {
            ranking.frecency.forget(&id);
            ranking.path.clone()
        });

        crate::state::save_ranking_soon(&path, text);

        Ok(Outcome::done(format!(
            "{} is ranked as though it had never been opened.",
            object.title
        )))
    }
}

// ----------------------------------------------------------------- favourite

pub(super) struct ToggleFavourite;

#[async_trait]
impl Action for ToggleFavourite {
    fn id(&self) -> &str {
        "sill.row.favourite"
    }

    /*
     * One action rather than an Add and a Remove, because `title` is asked of
     * the action and not of the object: two actions would both be offered on
     * every row, and one of them would always be the wrong one. The switches
     * Sill already ships answer this the same way, by saying afterwards which
     * way it went.
     */
    fn title(&self) -> &str {
        "Toggle Favourite"
    }

    fn icon(&self) -> &'static str {
        "Star"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    /// Writes to disk: the ranking history for this one, `preferences.json`
    /// for the other two. An empty list here would be a lie, and the
    /// permission model reads exactly this.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let prefs = ctx.app.state::<PrefsState>();

        let (next, pinned) = {
            let held = prefs.inner.lock().await;
            let mut next = held.clone();

            let was = next.sources.pinned.iter().position(|id| id == &object.id);
            match was {
                Some(at) => {
                    next.sources.pinned.remove(at);
                }
                None => next.sources.pinned.push(object.id.clone()),
            }

            let pinned = was.is_none();
            (next, pinned)
        };

        crate::commands::settings::set_preferences(ctx.app.clone(), prefs, next).await?;

        Ok(Outcome::done(if pinned {
            format!("{} leads the list now.", object.title)
        } else {
            format!("{} is no longer a favourite.", object.title)
        }))
    }
}

// --------------------------------------------------------------------- alias

/// Rows whose name would not still mean something tomorrow.
///
/// An alias points at a command id and is matched against the index, so it is
/// only worth offering on a row whose id survives a restart.
///
/// - A calculator **answer** exists for as long as it is on screen.
/// - A **window**'s id is a handle that stops being valid when it closes, and a
///   **browser tab**'s holds that handle plus the browser's own identifier for
///   the tab.
/// - An **audio session** carries the process number, so naming one would be
///   naming this morning's copy of that program, and a **process** row is that
///   exactly: its id *is* the process number.
/// - A **control** is the shorter-lived version again, a window handle plus one
///   button, both meaningless the moment that window redraws.
/// - **Media** has a fixed id, so a name would outlive the music and point at
///   nothing.
/// - A **conversation** is not in the index, so a name would find nothing
///   however carefully chosen, and a **store listing** may not even be
///   installed. Once it is, its commands are in the index and each can be
///   named there.
///
/// Kept as the modes that cannot rather than the modes that can, so a mode
/// added later is namable by default and reads oddly rather than vanishing.
///
/// By mode rather than by kind because that is the distinction it was written
/// in, and `accepts` already refuses most of these on kind alone. Both is
/// deliberate: the kind filter is the coarse one, and this is the argument.
const UNNAMABLE: &[&str] = &[
    "answer",
    "window",
    "browser-tab",
    "audio-session",
    "process",
    "control",
    "media",
    "conversation",
    "past-conversation",
    "store-listing",
];

fn namable(row: &Row<'_>) -> bool {
    !UNNAMABLE.contains(&row.object.mode.as_str())
}

pub(super) struct SetAlias;

#[async_trait]
impl Action for SetAlias {
    fn id(&self) -> &str {
        "sill.row.alias"
    }

    /// The wording when there is no alias yet. See `title_for` for the other
    /// half: setting one and changing one are the same action, and reading the
    /// same word for both is how somebody overwrites an alias they meant to
    /// keep.
    fn title(&self) -> &str {
        "Set Alias"
    }

    fn title_for(&self, row: &Row<'_>) -> Option<String> {
        row.alias.map(|alias| format!("Change Alias \"{alias}\""))
    }

    fn icon(&self) -> &'static str {
        "Pencil"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    fn shown(&self, row: &Row<'_>) -> bool {
        namable(row)
    }

    /// Writes `preferences.json`. An empty list here would be a lie, and the
    /// permission model reads exactly this.
    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let wanted = ctx
            .argument()
            .ok_or("a name is needed, which the launcher asks for")?
            // Stored lowercased, because that is how `Aliases` compares them.
            .trim()
            .to_lowercase();

        if wanted.is_empty() {
            return Err("a name of nothing would find nothing".into());
        }

        let prefs = ctx.app.state::<PrefsState>();

        let next = {
            let held = prefs.inner.lock().await;
            let mut next = held.clone();

            // One name per row, and one row per name. Both directions are
            // cleared first: the same word pointing at two rows makes typing
            // it ambiguous, and a row carrying two names means one of them is
            // unreachable.
            next.aliases
                .retain(|a| a.command != object.id && a.alias != wanted);
            next.aliases.push(crate::registry::Alias {
                alias: wanted.clone(),
                command: object.id.clone(),
            });

            next
        };

        crate::commands::settings::set_preferences(ctx.app.clone(), prefs, next).await?;

        Ok(Outcome::done(format!(
            "Typing {wanted} now finds {}.",
            object.title
        )))
    }
}

pub(super) struct ClearAlias;

#[async_trait]
impl Action for ClearAlias {
    fn id(&self) -> &str {
        "sill.row.alias.clear"
    }

    fn title(&self) -> &str {
        "Clear Alias"
    }

    fn title_for(&self, row: &Row<'_>) -> Option<String> {
        row.alias.map(|alias| format!("Clear Alias \"{alias}\""))
    }

    fn icon(&self) -> &'static str {
        "XMarkCircle"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    /// Only where there is one to forget. This is the case `shown` exists for:
    /// on the great majority of rows this action has nothing to do, and an
    /// entry that does nothing is worse than no entry, because it has to be
    /// read before it can be skipped.
    fn shown(&self, row: &Row<'_>) -> bool {
        namable(row) && row.alias.is_some()
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::FileWrite]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        let prefs = ctx.app.state::<PrefsState>();

        let (next, had) = {
            let held = prefs.inner.lock().await;
            let mut next = held.clone();
            let before = next.aliases.len();
            next.aliases.retain(|a| a.command != object.id);
            let had = next.aliases.len() != before;
            (next, had)
        };

        if !had {
            return Ok(Outcome::done(format!("{} had no name.", object.title)));
        }

        crate::commands::settings::set_preferences(ctx.app.clone(), prefs, next).await?;

        Ok(Outcome::done(format!(
            "{} answers to its own name again.",
            object.title
        )))
    }
}

// --------------------------------------------------------------- preferences

pub(super) struct OpenCommandPreferences;

#[async_trait]
impl Action for OpenCommandPreferences {
    fn id(&self) -> &str {
        "sill.row.preferences"
    }

    fn title(&self) -> &str {
        "Open Preferences"
    }

    fn icon(&self) -> &'static str {
        "Gear"
    }

    /// Only an extension command has any. Everything else in the index is
    /// configured by Sill's own settings or by nothing at all.
    fn accepts(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::ExtensionCommand
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::commands::settings::open_settings(ctx.app.clone(), Some("extensions".into()))
            .await?;

        Ok(Outcome::done(format!(
            "Opened the settings for {}.",
            object.title
        )))
    }
}

// ------------------------------------------------------------------ shortcut

pub(super) struct SetGlobalShortcut;

#[async_trait]
impl Action for SetGlobalShortcut {
    fn id(&self) -> &str {
        "sill.row.shortcut"
    }

    fn title(&self) -> &str {
        "Set Global Shortcut"
    }

    fn icon(&self) -> &'static str {
        "Keyboard"
    }

    fn accepts(&self, kind: ObjectKind) -> bool {
        indexed(kind)
    }

    fn capabilities(&self) -> &'static [Capability] {
        &[Capability::Ui]
    }

    /*
     * Opens the recorder rather than taking a typed accelerator.
     *
     * A key is not a string somebody should have to spell. The settings panel
     * already owns a recorder that watches a real keypress and writes what it
     * saw, and asking for "Ctrl+Shift+K" in a text field would be a second
     * spelling of the same fact, wrong in a different way on every layout.
     */
    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String> {
        crate::commands::settings::open_settings(ctx.app.clone(), Some("shortcuts".into()))
            .await?;

        Ok(Outcome::done(format!(
            "Pick a key for {} in Shortcuts.",
            object.title
        )))
    }
}
