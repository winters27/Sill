//! What each provider has cost over its lifetime.
//!
//! A conversation knows what it cost (`chat::Spent`), and conversations are
//! forgotten: fifty are kept and the rest fall off. The bill does not fall
//! off with them, so the totals live here, per provider, in a file of their
//! own that is a few hundred bytes and grows by nothing per turn.
//!
//! ## What is kept
//!
//! Per provider: the total, a total per model (bounded, the least used
//! going first) and a total per calendar day (bounded to two months, the
//! oldest going first). That is enough to say "all time", "the last thirty
//! days" and "today" without keeping a row per answer, and a row per answer
//! is the thing this deliberately does not keep: it would be a log of when
//! somebody talks to a model, and nothing here needs one.
//!
//! Days are the machine's own calendar days, because "today" means the day
//! somebody is having, not the one in Greenwich.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::chat::{Finished, Spent};
use super::provider::{short_model, Wire};
use crate::dates::Civil;

/// Where it is kept, beside the conversations.
const FILE: &str = "ai-usage.json";

/// The one report about usage not being saved, named once so the save that
/// works withdraws the one that did not.
const TROUBLE: &str = "ai-usage";

/// Readable, because somebody checking a bill against it will open it.
const SCHEMA: crate::json_store::Schema = crate::json_store::Schema {
    version: 1,
    shape: crate::json_store::Shape::Around,
    layout: crate::json_store::Layout::Readable,
    unreadable: crate::json_store::Unreadable::KeepAside,
    what: "AI usage",
};

/// How many models one provider keeps a line for.
///
/// OpenRouter offers hundreds; somebody trying a dozen over a month should
/// see the ones they use, not a table that scrolls. The least used go first,
/// and their answers are still in the provider's total.
const KEEP_MODELS: usize = 24;

/// How many calendar days are kept, which bounds the file.
///
/// Two months, so a thirty-day window is whole whichever day it is read on.
const KEEP_DAYS: usize = 62;

/// How many days "the last month" is.
const MONTH: i64 = 30;

/// One provider's account.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    pub all: Spent,
    /// By the model that answered, as the service named it.
    pub models: BTreeMap<String, Spent>,
    /// By calendar day, `YYYY-MM-DD` on this machine's clock. Sorted by the
    /// key, so the oldest is first and the newest last.
    pub days: BTreeMap<String, Spent>,
    /// When the first and the last answer counted here landed, in seconds.
    pub first: i64,
    pub last: i64,
}

impl Account {
    fn add(&mut self, model: &str, finished: &Finished, now: i64, today: &str) {
        self.all.add(finished);
        self.models
            .entry(model.to_string())
            .or_default()
            .add(finished);
        self.days
            .entry(today.to_string())
            .or_default()
            .add(finished);

        if self.first == 0 {
            self.first = now;
        }
        self.last = now;

        self.bound(model);
    }

    /// Drops the least used model and the oldest day once there are too many.
    ///
    /// The model just answered is never the one dropped, whatever its count:
    /// a full table that dropped every newcomer at one answer would keep
    /// the first two dozen models ever tried and never learn a new name.
    fn bound(&mut self, just_used: &str) {
        while self.models.len() > KEEP_MODELS {
            let Some(fewest) = self
                .models
                .iter()
                .filter(|(model, _)| model.as_str() != just_used)
                .min_by_key(|(_, spent)| spent.answers)
                .map(|(model, _)| model.clone())
            else {
                break;
            };
            self.models.remove(&fewest);
        }

        while self.days.len() > KEEP_DAYS {
            let Some(oldest) = self.days.keys().next().cloned() else {
                break;
            };
            self.days.remove(&oldest);
        }
    }

    /// Everything from `since` onwards, `since` included.
    fn since(&self, since: &str) -> Spent {
        let mut total = Spent::default();
        for (_, spent) in self.days.range(since.to_string()..) {
            total.merge(spent);
        }
        total
    }
}

/// The file's shape.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Book {
    providers: BTreeMap<String, Account>,
}

/// Every provider's account, as managed state.
#[derive(Default)]
pub struct Ledger {
    held: Mutex<Book>,
    /// Whether what is on disk has been read. Nothing is written until it
    /// has, for the reason `chat::Chat` gives: a save before the load would
    /// replace every total with an empty file.
    read_the_file: AtomicBool,
}

/// One provider's account, as the settings window draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub provider: String,
    pub all: Window,
    pub today: Window,
    /// The last thirty days, today included.
    pub month: Window,
    /// Most answers first.
    pub models: Vec<ModelUsage>,
    pub first: i64,
    pub last: i64,
}

/// A total with its mean speed worked out, so the window draws rather than
/// divides.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub spent: Spent,
    /// Output tokens a second over everything that could be timed.
    pub mean_rate: Option<f64>,
}

impl From<Spent> for Window {
    fn from(spent: Spent) -> Self {
        Self {
            spent,
            mean_rate: mean_rate(&spent),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    /// As the service named it.
    pub model: String,
    /// As the chip would call it.
    pub label: String,
    #[serde(flatten)]
    pub window: Window,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads what was counted before Sill was last closed.
    pub fn load(&self, dir: &std::path::Path) {
        let book: Book = crate::json_store::load(&dir.join(FILE), &SCHEMA);

        if let Ok(mut held) = self.held.lock() {
            *held = book;
        }

        self.read_the_file.store(true, Ordering::Relaxed);
    }

    /// Writes it out, and says whether it worked.
    pub fn write_to(&self, dir: &std::path::Path) -> Result<(), String> {
        if !self.read_the_file.load(Ordering::Relaxed) {
            crate::say!("not saving AI usage: it was never loaded");
            return Ok(());
        }

        let held = self.held.lock().map_err(|_| "the lock was poisoned")?;
        crate::json_store::save_atomic(&dir.join(FILE), &*held, &SCHEMA)
            .map_err(|err| err.to_string())
    }

    /// Writes it out and reports it when that does not work.
    pub fn save(&self, app: &tauri::AppHandle) {
        match self.write_to(&crate::state::data_dir(app)) {
            Ok(()) => crate::status::resolved(app, TROUBLE),
            Err(err) => crate::status::report(
                app,
                TROUBLE,
                format!("Sill could not save what AI Chat has cost, so the totals in Settings will be behind: {err}"),
                Some("ai"),
            ),
        }
    }

    /// Counts one finished turn against a provider.
    pub fn record(&self, provider: &str, model: &str, finished: &Finished, now: i64, today: &str) {
        if let Ok(mut held) = self.held.lock() {
            held.providers
                .entry(provider.to_string())
                .or_default()
                .add(model, finished, now, today);
        }
    }

    /// Forgets one provider's account. Answers whether there was one.
    pub fn forget(&self, provider: &str) -> bool {
        self.held
            .lock()
            .ok()
            .map(|mut held| held.providers.remove(provider).is_some())
            .unwrap_or(false)
    }

    /// Every account, as the settings window draws them.
    pub fn report(&self, today: Civil) -> Vec<Usage> {
        let Ok(held) = self.held.lock() else {
            return Vec::new();
        };

        let today_key = day_key(today);
        let month_key = day_key(days_ago(today, MONTH - 1));

        held.providers
            .iter()
            .map(|(provider, account)| {
                let mut models: Vec<ModelUsage> = account
                    .models
                    .iter()
                    .map(|(model, spent)| ModelUsage {
                        model: model.clone(),
                        label: short_model(Wire::OpenAi, model),
                        window: Window::from(*spent),
                    })
                    .collect();
                models.sort_by(|a, b| b.window.spent.answers.cmp(&a.window.spent.answers));

                Usage {
                    provider: provider.clone(),
                    all: Window::from(account.all),
                    today: Window::from(account.days.get(&today_key).copied().unwrap_or_default()),
                    month: Window::from(account.since(&month_key)),
                    models,
                    first: account.first,
                    last: account.last,
                }
            })
            .collect()
    }
}

/// Output tokens a second over everything that was timed, or nothing.
fn mean_rate(spent: &Spent) -> Option<f64> {
    (spent.generating_ms > 0 && spent.output > 0)
        .then(|| spent.output as f64 * 1000.0 / spent.generating_ms as f64)
}

/// A calendar day as the file keys it: sortable as text.
pub fn day_key(day: Civil) -> String {
    format!("{:04}-{:02}-{:02}", day.year, day.month, day.day)
}

/// The day `ago` days before.
fn days_ago(day: Civil, ago: i64) -> Civil {
    let number = crate::timers::days_from_civil(day.year, day.month, day.day);
    let (year, month, day) = crate::timers::civil_from_days(number - ago);
    Civil { year, month, day }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::openai::Usage as Tokens;

    fn turn(output: u64, cost: Option<f64>) -> Finished {
        Finished {
            usage: Some(Tokens { input: 10, output }),
            cost,
            generating_ms: 1000,
            ..Finished::default()
        }
    }

    fn day(year: i64, month: i64, day: i64) -> Civil {
        Civil { year, month, day }
    }

    /// The whole point: every answer lands in the total, in its model's line
    /// and in its day's line, and the report reads all three back.
    #[test]
    fn an_answer_is_counted_three_ways() {
        let ledger = Ledger::new();
        ledger.record(
            "xai",
            "grok-4.6",
            &turn(100, Some(0.01)),
            1_000,
            "2026-09-05",
        );
        ledger.record(
            "xai",
            "grok-4.6",
            &turn(50, Some(0.02)),
            2_000,
            "2026-09-05",
        );
        ledger.record("xai", "grok-4", &turn(10, None), 3_000, "2026-09-04");

        let report = ledger.report(day(2026, 9, 5));
        assert_eq!(report.len(), 1);
        let xai = &report[0];

        assert_eq!(xai.provider, "xai");
        let all = xai.all.spent;
        assert_eq!((all.answers, all.output, all.unpriced), (3, 160, 1));
        assert!((all.cost.expect("priced") - 0.03).abs() < 1e-12);
        assert_eq!((xai.today.spent.answers, xai.today.spent.output), (2, 150));
        assert_eq!(xai.month.spent.answers, 3);
        assert_eq!((xai.first, xai.last), (1_000, 3_000));

        // Most answers first, named the way the chip would.
        let models: Vec<(&str, u32)> = xai
            .models
            .iter()
            .map(|one| (one.label.as_str(), one.window.spent.answers))
            .collect();
        assert_eq!(models, vec![("grok-4.6", 2), ("grok-4", 1)]);

        // 160 tokens over three timed seconds, and today's 150 over two.
        let rate = xai.all.mean_rate.expect("timed");
        assert!((rate - 160.0 / 3.0).abs() < 1e-9, "{rate}");
        assert_eq!(xai.today.mean_rate, Some(75.0));
    }

    /// Thirty days is thirty days, today included, and the day before that
    /// is not in it.
    #[test]
    fn the_month_is_the_last_thirty_days() {
        let ledger = Ledger::new();
        ledger.record("p", "m", &turn(1, None), 0, "2026-09-05");
        ledger.record("p", "m", &turn(2, None), 0, "2026-08-07");
        ledger.record("p", "m", &turn(4, None), 0, "2026-08-06");

        let report = ledger.report(day(2026, 9, 5));
        assert_eq!(report[0].month.spent.output, 3);
        assert_eq!(report[0].all.spent.output, 7);

        // Across a year end, which is where date arithmetic goes wrong: the
        // 10th of January reaches back to the 12th of December.
        let ledger = Ledger::new();
        ledger.record("p", "m", &turn(1, None), 0, "2025-12-11");
        ledger.record("p", "m", &turn(2, None), 0, "2025-12-12");
        ledger.record("p", "m", &turn(4, None), 0, "2026-01-09");
        let report = ledger.report(day(2026, 1, 10));
        assert_eq!(report[0].month.spent.output, 6);
        assert_eq!(report[0].today.spent.output, 0);
    }

    /// The file cannot grow with the models tried: the least used model
    /// goes, its answers stay in the total, and the one just answered is
    /// never the one that goes, or a full table would never learn a name.
    #[test]
    fn the_least_used_model_goes_first_but_never_the_newcomer() {
        let mut account = Account::default();
        for at in 0..KEEP_MODELS {
            account.add(&format!("model-{at}"), &turn(1, None), 0, "2026-09-05");
            account.add(&format!("model-{at}"), &turn(1, None), 0, "2026-09-05");
        }

        // A newcomer with one answer gets in, at the cost of an old model.
        account.add("once", &turn(1, None), 0, "2026-09-05");
        assert_eq!(account.models.len(), KEEP_MODELS);
        assert!(
            account.models.contains_key("once"),
            "the newcomer was dropped"
        );

        // The next newcomer pushes out the least used, which is now "once".
        account.add("newer", &turn(1, None), 0, "2026-09-05");
        account.add("newer", &turn(1, None), 0, "2026-09-05");
        assert_eq!(account.models.len(), KEEP_MODELS);
        assert!(
            !account.models.contains_key("once"),
            "the least used stayed"
        );
        assert!(account.models.contains_key("newer"));
        assert_eq!(account.all.answers as usize, KEEP_MODELS * 2 + 3);
    }

    /// Nor with the days: the oldest goes.
    #[test]
    fn the_oldest_day_goes_first() {
        let mut account = Account::default();
        for at in 0..(KEEP_DAYS + 5) {
            let key = format!("2026-{:02}-{:02}", 1 + at / 28, 1 + at % 28);
            account.add("m", &turn(1, None), 0, &key);
        }

        assert_eq!(account.days.len(), KEEP_DAYS);
        assert!(!account.days.contains_key("2026-01-01"));
        assert!(account.days.contains_key("2026-03-11"));
    }

    /// What was counted goes to disk with the app and comes back.
    #[test]
    fn what_was_counted_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("sill-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");

        let ledger = Ledger::new();
        ledger.load(&dir);
        ledger.record("openai", "gpt-5.2", &turn(100, Some(0.5)), 7, "2026-09-05");
        ledger.write_to(&dir).expect("written");

        let again = Ledger::new();
        again.load(&dir);
        let report = again.report(day(2026, 9, 5));
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].all.spent.cost, Some(0.5));
        assert_eq!(report[0].models[0].model, "gpt-5.2");

        assert!(again.forget("openai"));
        assert!(!again.forget("openai"));
        assert!(again.report(day(2026, 9, 5)).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Never over a file nobody read.
    #[test]
    fn nothing_is_written_before_the_file_was_read() {
        let dir = std::env::temp_dir().join(format!("sill-ledger-unread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");

        let ledger = Ledger::new();
        ledger.record("p", "m", &turn(1, None), 0, "2026-09-05");
        ledger.write_to(&dir).expect("refusing is not failing");
        assert!(!dir.join(FILE).exists(), "wrote over a file it never read");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_day_is_keyed_so_text_order_is_date_order() {
        assert_eq!(day_key(day(2026, 9, 5)), "2026-09-05");
        assert!(day_key(day(2026, 9, 5)) > day_key(day(2026, 8, 31)));
        assert_eq!(days_ago(day(2026, 1, 10), 29), day(2025, 12, 12));
    }
}
