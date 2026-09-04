//! Triggers, as the settings panel needs them.
//!
//! Four adapters over [`crate::automation`], each one thin. Everything that
//! decides lives there: which actions may be scheduled, what the task's
//! command line says, and whether a task already in the folder is one Sill
//! will describe in its own words.
//!
//! Nothing here caches a list, and there is nowhere for one to live. Task
//! Scheduler is asked every time the panel opens, which is a handful of COM
//! calls a few times a day and no residency at all between them.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::action::ActionRegistry;
use crate::automation::{self, Trigger};

/// One trigger, as the panel draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// Its name in Task Scheduler, which is also how it is removed.
    pub name: String,
    pub enabled: bool,
    /// When Windows says it next runs, in Windows' own words.
    pub next: Option<String>,
    /// What it does, when Sill can still vouch for what it does.
    pub title: Option<String>,
    pub target: Option<String>,
    /// Why Sill will not vouch for it, and what it says instead.
    ///
    /// A row has one of these or the pair above, never both. Drawing a
    /// tampered task under the title Sill meant it to have would be the one
    /// way this panel could actively mislead somebody.
    pub suspect: Option<String>,
}

/// An action a trigger may name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub id: &'static str,
    pub title: &'static str,
}

/**
Every task in Sill's folder, read rather than remembered.

Whatever is in the folder is listed, including anything Sill did not put
there. A task the panel quietly skipped would be a task nobody could remove
from inside Sill, sitting in a folder with Sill's name on it.
*/
#[tauri::command]
pub(crate) async fn automations(app: AppHandle) -> Result<Vec<Row>, String> {
    let exe = std::env::current_exe().map_err(|err| format!("Sill cannot find itself: {err}"))?;

    // Blocking: a COM apartment and an enumeration of a scheduler folder.
    let held = tauri::async_runtime::spawn_blocking(automation::held)
        .await
        .map_err(|err| format!("the trigger list did not finish: {err}"))??;

    let registry = app.state::<ActionRegistry>();

    Ok(held
        .into_iter()
        .map(|task| {
            let read = automation::read_back(&exe, &task.xml).and_then(|ask| {
                let found = registry
                    .get(&ask.action)
                    .ok_or_else(|| format!("it runs {}, which Sill no longer has", ask.action))?;

                // Checked again on the way out, not only on the way in. An
                // action that grew a heavier capability since the trigger was
                // made is a trigger that would now stop and ask, and the list
                // is where somebody finds that out.
                automation::may_schedule(&ask.action, found.capabilities())?;

                Ok((found.title(), ask.target))
            });

            match read {
                Ok((title, target)) => Row {
                    name: task.name,
                    enabled: task.enabled,
                    next: task.next,
                    title: Some(title.to_string()),
                    target: Some(target),
                    suspect: None,
                },
                Err(why) => Row {
                    name: task.name,
                    enabled: task.enabled,
                    next: task.next,
                    title: None,
                    target: None,
                    suspect: Some(why),
                },
            }
        })
        .collect())
}

/// The actions a trigger may name, which is the ones that never stop to ask.
///
/// Offered as a list rather than left to a refusal after the fact, because
/// somebody filling a form in should not be able to complete it and then be
/// told the whole thing was impossible.
#[tauri::command]
pub(crate) fn schedulable(app: AppHandle) -> Vec<Offer> {
    let registry = app.state::<ActionRegistry>();

    registry
        .all()
        .into_iter()
        .filter(|(id, _, _)| {
            registry
                .get(id)
                .is_some_and(|found| automation::may_schedule(id, found.capabilities()).is_ok())
        })
        .map(|(id, title, _)| Offer { id, title })
        .collect()
}

/**
Writes one down, and says what Windows now holds.

The target is resolved here, while somebody is looking at the form. A path
with a typo in it becomes a task that fires at nine in the morning, fails, and
files a line in the status surface nobody was awake for; the same typo caught
now is a sentence under the field.
*/
#[tauri::command]
pub(crate) async fn schedule(app: AppHandle, trigger: Trigger) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|err| format!("Sill cannot find itself: {err}"))?;
    let name = automation::task_name(&trigger.name)?;

    let object = crate::ai::acting::object_for(&trigger.target, trigger.kind.as_deref())?;

    {
        let registry = app.state::<ActionRegistry>();
        let found = registry
            .get(&trigger.action)
            .ok_or_else(|| format!("Sill has no action called {}.", trigger.action))?;

        automation::may_schedule(&trigger.action, found.capabilities())?;

        if !found.accepts(object.kind) {
            return Err(format!(
                "{} cannot be done to {}.",
                found.title(),
                object.title
            ));
        }
    }

    let xml = automation::definition(&exe, &trigger)?;
    let said = trigger.when.said();

    tauri::async_runtime::spawn_blocking(move || automation::register(&name, &xml))
        .await
        .map_err(|err| format!("the trigger was not written down: {err}"))??;

    Ok(format!("Windows will run this {said}."))
}

/// Takes one out of Windows, by the name Task Scheduler knows it by.
///
/// The name goes through the same check it went through on the way in, so a
/// name that arrived from somewhere other than the list cannot name a task in
/// a different folder.
#[tauri::command]
pub(crate) async fn unschedule(name: String) -> Result<(), String> {
    let name = automation::task_name(&name)?;

    tauri::async_runtime::spawn_blocking(move || automation::forget(&name))
        .await
        .map_err(|err| format!("the trigger was not removed: {err}"))?
}
