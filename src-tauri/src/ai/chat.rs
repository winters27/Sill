//! Conversations, and asking the next thing in one.
//!
//! Held here rather than in the window. The window is closed most of the time
//! and reloaded whenever the page does; a conversation that lived there would
//! be lost every time somebody pressed Escape, which is the opposite of what a
//! follow-up is for.
//!
//! ## One conversation per question asked from the root list
//!
//! Every press of Tab begins a new one. There used to be exactly one
//! conversation for the life of the process and every question joined it,
//! which is a mode the launcher carried between summons with nothing on screen
//! saying so. The one just left is offered back as a single row that expires,
//! so returning to it is something you choose rather than something that
//! happens to you.
//!
//! ## What goes over the wire
//!
//! The whole conversation, every time, because that is how these services
//! work: they hold no state and the context is the request. The exception is
//! Claude Code, which holds the session itself and is handed only the new
//! question plus the id of the session to continue. That session belongs to
//! the conversation, so a new one cannot continue the old one's.
//!
//! ## Why the answer arrives as events
//!
//! A launcher that shows nothing for four seconds and then a paragraph feels
//! broken even when it is not. Each piece is emitted as it arrives and the
//! window appends it, so the first words are on screen while the rest is still
//! being written.

use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::openai::{Attached, Message, Part, Usage};
use super::provider::{Provider, Wire};

/// One tool being used, as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// The call's own id, which its result is labelled with.
    pub id: String,
    pub tool: String,
    /// What it is being used on. Empty when the tool takes no arguments.
    pub subject: String,
}

/// One tool finished, and whether it managed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Used {
    pub id: String,
    pub ok: bool,
}

/// What a turn cost, said once it is over.
///
/// Every field can be missing: a local model names no cost, a service that
/// was not asked names no usage, and the turn is no less finished for it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    /// Which model actually answered. Empty when the service did not say.
    pub model: String,
    pub usage: Option<Usage>,
    pub duration_ms: u64,
    /// From the first piece to the last, over every request in the turn.
    /// Zero when the transport could not say. What a local model's speed is
    /// read from, since the wait before the first piece is the prompt being
    /// read rather than the answer being written.
    pub generating_ms: u64,
    /// In dollars. Named by Claude Code and OpenRouter; worked out from the
    /// published rate for everybody else with a known model, and absent for
    /// a model on this machine or one nobody has priced.
    pub cost: Option<f64>,
    /// The conversation's running total once this turn is counted in.
    pub spent: Spent,
}

/// What a conversation has cost so far.
///
/// Kept with the conversation rather than added up by the window, so a
/// reopened conversation still knows and two windows drawing it agree. Every
/// answer is counted; what could not be priced is counted as tokens alone and
/// said so, because a total that quietly left a turn out would read as
/// smaller than the bill.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spent {
    pub input: u64,
    pub output: u64,
    /// Dollars over the answers that could be priced. Absent until one could.
    pub cost: Option<f64>,
    /// Answers whose tokens were counted but which no rate was known for.
    pub unpriced: u32,
    /// Output tokens a second over the last answer, when it could be timed.
    /// The number a model on this machine is judged by.
    pub rate: Option<f64>,
    /// How many answers this counts.
    pub answers: u32,
    /// Every timed millisecond, so a total can also say its mean speed.
    pub generating_ms: u64,
}

impl Spent {
    /// Counts one more answer in.
    pub fn add(&mut self, finished: &Finished) {
        self.answers += 1;
        self.generating_ms += finished.generating_ms;

        if let Some(cost) = finished.cost {
            self.cost = Some(self.cost.unwrap_or(0.0) + cost);
        }

        match finished.usage {
            Some(usage) => {
                self.input += usage.input;
                self.output += usage.output;
                if finished.cost.is_none() {
                    self.unpriced += 1;
                }
                self.rate = (finished.generating_ms > 0 && usage.output > 0)
                    .then(|| usage.output as f64 * 1000.0 / finished.generating_ms as f64);
            }
            // Nothing to time it by, and the last rate is not this one.
            None => self.rate = None,
        }
    }

    /// Adds another total into this one, for summing days into a month.
    ///
    /// The rate is the other's when it has one, so summing in date order
    /// leaves the newest day's speed, which is the one worth showing.
    pub fn merge(&mut self, other: &Spent) {
        self.input += other.input;
        self.output += other.output;
        self.unpriced += other.unpriced;
        self.answers += other.answers;
        self.generating_ms += other.generating_ms;
        if let Some(cost) = other.cost {
            self.cost = Some(self.cost.unwrap_or(0.0) + cost);
        }
        if other.rate.is_some() {
            self.rate = other.rate;
        }
    }
}

/// Prices a turn the service did not price.
///
/// A model on this machine costs nothing, whatever it is called: somebody
/// running gpt-oss under Ollama is not paying OpenAI for it. Everybody else
/// is priced from the model that answered, or the one asked for when the
/// service did not say which.
pub fn price(finished: &mut Finished, asked_for: &str, local: bool) {
    if local || finished.cost.is_some() {
        return;
    }

    let Some(usage) = finished.usage else {
        return;
    };

    let model = if finished.model.is_empty() {
        asked_for
    } else {
        &finished.model
    };

    finished.cost = super::pricing::cost(model, usage);
}

/// One exchange, as the window draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
    /// What was handed over with it, so a reopened conversation still shows
    /// the picture that was asked about rather than a question with no
    /// subject.
    pub attachments: Vec<Attached>,
    /// How an answer came about, in order. Empty on a question.
    pub parts: Vec<Part>,
}

/// How much thinking is kept with one answer, in characters.
///
/// Some models think for pages before answering a question about the
/// clipboard. The window shows it folded and the file holds it forever, so
/// the first few thousand characters are the part worth keeping: enough to
/// see what it was weighing, not the whole of it.
const KEEP_THINKING: usize = 4 * 1024;

/// How many parts one answer keeps.
///
/// A loop bounded at six rounds cannot produce this many, so it is a bound on
/// a mistake rather than a target. Steps go first; the words never do.
const KEEP_PARTS: usize = 64;

/// One thing that happened on the way to an answer, as the loop saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Told {
    Text(String),
    Thinking(String),
    Using(Step),
    Used(Used),
}

/// What one turn has recorded so far.
///
/// Both transports record through this, so an answer is the same shape
/// whichever way it came and the window learns nothing about which it was.
/// Recording and telling the window are separate so the recording can be
/// proved without a window to tell.
#[derive(Debug, Default)]
pub struct Telling {
    pub parts: Vec<Part>,
    /// Every word, in one string, which is what the conversation keeps as
    /// the message and what a follow-up is answered from.
    pub whole: String,
    /// When the thinking part now being written began, so it can be stamped
    /// with how long it took once something else follows.
    thinking_began: Option<Instant>,
}

impl Telling {
    pub fn record(&mut self, told: Told) {
        match told {
            Told::Text(piece) => {
                self.whole.push_str(&piece);
                self.close_thinking();
                match self.parts.last_mut() {
                    Some(Part::Text { text }) => text.push_str(&piece),
                    // Whitespace on its own starts nothing. A model that
                    // sends a newline between two tool calls would otherwise
                    // leave an empty paragraph between two steps.
                    _ if piece.trim().is_empty() => {}
                    _ => self.parts.push(Part::Text { text: piece }),
                }
            }
            Told::Thinking(piece) => match self.parts.last_mut() {
                Some(Part::Thinking { text, .. }) => {
                    let room = KEEP_THINKING.saturating_sub(text.len());
                    text.push_str(&piece[..cut_at(&piece, room)]);
                }
                _ => {
                    self.thinking_began = Some(Instant::now());
                    let kept = &piece[..cut_at(&piece, KEEP_THINKING)];
                    self.parts.push(Part::Thinking {
                        text: kept.to_string(),
                        ms: None,
                    });
                }
            },
            Told::Using(step) => {
                self.close_thinking();
                self.parts.push(Part::Step {
                    id: step.id,
                    tool: step.tool,
                    subject: step.subject,
                    ok: None,
                });
                self.bound();
            }
            Told::Used(used) => {
                for part in self.parts.iter_mut().rev() {
                    if let Part::Step { id, ok, .. } = part {
                        if *id == used.id {
                            *ok = Some(used.ok);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Stamps the thinking part being written with how long it took.
    fn close_thinking(&mut self) {
        let Some(began) = self.thinking_began.take() else {
            return;
        };
        if let Some(Part::Thinking { ms, .. }) = self.parts.last_mut() {
            *ms = Some(began.elapsed().as_millis() as u64);
        }
    }

    /// Drops the oldest steps once there are too many parts. Words stay.
    fn bound(&mut self) {
        while self.parts.len() > KEEP_PARTS {
            let Some(at) = self
                .parts
                .iter()
                .position(|part| matches!(part, Part::Step { .. }))
            else {
                return;
            };
            self.parts.remove(at);
        }
    }

    pub fn finish(mut self) -> Vec<Part> {
        self.close_thinking();
        self.parts
    }
}

/// The largest byte index at or below `most` that is a character boundary.
fn cut_at(text: &str, most: usize) -> usize {
    let mut at = most.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Records one thing and tells the window about it.
fn tell(app: &tauri::AppHandle, telling: &mut Telling, told: Told) {
    let _ = match &told {
        Told::Text(piece) => app.emit(SAID, piece),
        Told::Thinking(piece) => app.emit(THINKING, piece),
        Told::Using(step) => app.emit(USING, step),
        Told::Used(used) => app.emit(USED, used),
    };
    telling.record(told);
}

/// What one turn produced, whichever way it came.
struct Answer {
    text: String,
    parts: Vec<Part>,
    finished: Finished,
}

/// How long the offer to go back to a conversation lasts, in seconds.
///
/// Long enough that stepping out to check something is not punished, short
/// enough that it is not permanent furniture in the root list. It is the only
/// row there that is about where you were rather than about what exists, and a
/// row like that has to expire by itself or it becomes something to dismiss.
const KEEP_OFFERING: i64 = 10 * 60;

/// How many finished conversations are kept.
///
/// Written to disk now that there is a list somebody can open, resume from and
/// delete out of. Bounded because the file is read whole at startup and because
/// nobody scrolls past fifty of anything.
const KEEP_PAST: usize = 50;

/// How many messages of one conversation are kept.
///
/// Twenty exchanges, which is far more than anybody has with a launcher and
/// still a bound. Without one a conversation somebody keeps returning to grows
/// for as long as Sill runs, is held in memory in full, is written to disk in
/// full, and is **cloned in full for every question asked**, because the whole
/// history is what a service is sent.
const KEEP_TURNS: usize = 40;

/// How many bytes of attached pictures are carried forward.
///
/// An attachment is a data URI, so a screenshot handed over is a couple of
/// megabytes of base64 sitting in the message it came with. A few of those and
/// the conversation is larger than everything else Sill holds put together.
///
/// The newest are kept, because a follow-up is nearly always about the picture
/// just handed over, and the ones that fall out of the budget keep their name
/// and their size so the chip still reads correctly. What is dropped is the
/// body, which is the part that is large.
const KEEP_ATTACHED_BYTES: usize = 4 * 1024 * 1024;

/// Bounds one conversation, in place.
///
/// Its own function so the rule can be stated in a test rather than reasoned
/// about: this runs after every message, and getting it wrong either loses
/// what somebody said or keeps everything, and both are quiet.
fn trim(messages: &mut Vec<Message>) {
    /*
     * The system message stays wherever it is.
     *
     * It is the instructions, not a turn, and dropping it because it happens
     * to be oldest would change how the model answers halfway through a
     * conversation, which is the sort of thing that reads as the model
     * getting worse the longer you talk to it.
     */
    if messages.len() > KEEP_TURNS {
        let over = messages.len() - KEEP_TURNS;
        let mut dropped = 0;

        messages.retain(|message| {
            if dropped >= over || message.role == "system" {
                return true;
            }

            dropped += 1;
            false
        });
    }

    // Then the attachment bodies, newest first until the budget is spent.
    let mut spent = 0usize;

    for message in messages.iter_mut().rev() {
        for attached in message.attachments.iter_mut() {
            if attached.body.is_empty() {
                continue;
            }

            spent += attached.body.len();

            if spent > KEEP_ATTACHED_BYTES {
                // The name and the size stay, so the chip that names it still
                // reads correctly. The body is what was large.
                attached.body.clear();
            }
        }
    }
}

/// Where they are kept.
///
/// Plain JSON beside the rest of Sill's own files, which is what the clipboard
/// and the dictation transcripts already are. Anything said to a model on this
/// machine is readable by anything running as this user, and the honest place
/// to say so is the settings window rather than an encryption scheme that only
/// looks like it helps.
const FILE: &str = "conversations.json";

/// Where a file that could not be read is put instead of being lost.
///
/// Loading tolerates a file it cannot parse, which is right: refusing to start
/// because of one bad byte helps nobody. But saving then wrote what was in
/// memory over the top, and for a failed load that is nothing at all, so one
/// unreadable byte silently replaced every conversation with an empty list.
/// Moving it aside means the next save writes a clean file and what could not
/// be read is still on disk to look at.
///
/// Named by `json_store`, which puts every store's aside file beside the
/// original under the same rule, so there is one place to look rather than one
/// convention per store. Only the test that proves it needs the name; nothing
/// in the store writes it any more.
#[cfg(test)]
const BROKEN: &str = "conversations.json.broken";

/// How the file is kept. See `json_store` for what each part buys.
///
/// Compact, because nobody reads a transcript in a text editor and the
/// indentation would be most of the bytes.
///
/// The gain that matters here is `load_list`. `Conversation` has required
/// fields, so a single entry serde could not read failed the whole file and
/// took every other conversation with it; now that one is dropped and named in
/// the log and the rest survive. The staged write is the other: this was
/// written in place, so being killed mid-save left a truncated file that reads
/// as no history at all.
const SCHEMA: crate::json_store::Schema = crate::json_store::Schema {
    version: 1,
    shape: crate::json_store::Shape::Around,
    layout: crate::json_store::Layout::Compact,
    unreadable: crate::json_store::Unreadable::KeepAside,
    what: "conversations",
};

/// The one report about conversations not being saved, named once so the save
/// that works withdraws the one that did not.
const TROUBLE: &str = "conversations";

/// The longest a question may be before it is shortened into a name.
const TITLE: usize = 80;

/// One conversation, from its first question until another is begun.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    /// Unique for as long as Sill runs, which is as long as these live.
    pub id: String,
    /// The first question, which is what it is called.
    pub title: String,
    /// When it was last spoken to.
    pub last: i64,
    messages: Vec<Message>,
    /// What it has cost so far. A file from before this was counted reads
    /// as nothing spent, which is the only honest number for it.
    #[serde(default)]
    spent: Spent,
    /// Claude Code's own session, when that is who is answering.
    ///
    /// Not written out. A session id belongs to a running CLI and means
    /// nothing after a restart; keeping one would make the first follow-up
    /// after reopening Sill fail with somebody else's error about a session
    /// that is not there.
    #[serde(skip)]
    session: Option<String>,
}

impl Conversation {
    /// How many answers it holds, which is what makes it worth going back to.
    fn replies(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| message.role == "assistant")
            .count()
    }
}

/// One conversation, as the list of them needs to draw it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub replies: usize,
    /// Seconds since it was last spoken to.
    pub age: i64,
    /// Whether this is the one currently open.
    pub open: bool,
}

/// A conversation offered back, as the root list needs to draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub id: String,
    pub title: String,
    /// So the row can say more than what was asked.
    pub replies: usize,
    /// Seconds since it was last spoken to.
    pub age: i64,
}

/// Every conversation, as managed state rather than a static.
#[derive(Default)]
pub struct Chat {
    held: Mutex<Held>,
    /// Whether what is on disk has been read.
    ///
    /// Nothing is written until it has. Saving is writing what is in memory
    /// over the file, and before a load there is nothing in memory: one save
    /// that got in first would replace every conversation with an empty list,
    /// and nothing about that failure looks like a failure.
    read_the_file: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Held {
    /// The one being had. `None` before the first question of the run, and
    /// again after starting a fresh one from inside a conversation.
    open: Option<Conversation>,
    /// Finished ones, newest first.
    past: Vec<Conversation>,
    /// Counts conversations, so two begun in the same second differ.
    begun: u64,
}

impl Held {
    fn file_the_open_one(&mut self) {
        if let Some(finished) = self.open.take() {
            self.past.insert(0, finished);
            self.past.truncate(KEEP_PAST);
        }
    }

    fn open_a_new_one(&mut self, title: &str, now: i64) {
        self.begun += 1;
        self.open = Some(Conversation {
            id: format!("chat:{}", self.begun),
            title: shorten(title),
            last: now,
            messages: Vec::new(),
            spent: Spent::default(),
            session: None,
        });
    }
}

impl Chat {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a new conversation, filing whatever was open.
    ///
    /// Every press of Tab lands here.
    pub fn begin(&self, question: &str, now: i64) {
        if let Ok(mut held) = self.held.lock() {
            held.file_the_open_one();
            held.open_a_new_one(question, now);
        }
    }

    /// Files the open one without beginning another.
    ///
    /// What the key for a fresh conversation does from inside one: the next
    /// question begins its own, and the one just left is still offered back.
    pub fn set_aside(&self) {
        if let Ok(mut held) = self.held.lock() {
            held.file_the_open_one();
        }
    }

    /// Reopens one, wherever it currently is.
    ///
    /// Answering `true` for the one already open is not a special case worth
    /// avoiding: the row offering it does not know which it is, and neither
    /// should the window that pressed it.
    pub fn resume(&self, id: &str, now: i64) -> bool {
        let Ok(mut held) = self.held.lock() else {
            return false;
        };

        if held.open.as_ref().is_some_and(|open| open.id == id) {
            return true;
        }

        let Some(at) = held.past.iter().position(|past| past.id == id) else {
            return false;
        };

        // Taken out before the open one is filed, and that order is the whole
        // of it. Filing first pushes a conversation onto the front of the same
        // list, which shifts every index in it, so the position found a moment
        // earlier then names the wrong one. Taking it out first also means the
        // failure above changes nothing.
        let mut found = held.past.remove(at);
        held.file_the_open_one();

        found.last = now;
        held.open = Some(found);
        true
    }

    /// The conversation worth offering to go back to, if there is one.
    ///
    /// The most recently spoken to, open or filed, because the root list is
    /// only ever drawn when nobody is looking at a conversation. One with no
    /// answer in it is not offered: a question that failed is not a place to
    /// return to.
    pub fn offer(&self, now: i64) -> Option<Offer> {
        let held = self.held.lock().ok()?;

        // Ranked by when it was last spoken to, and then by whether it is the
        // open one. The tiebreak is not decoration: two conversations in the
        // same second is ordinary, and without it the older of the two wins,
        // which offers a way back to the one before the one you were just in.
        held.past
            .iter()
            .map(|conversation| (conversation, 0u8))
            .chain(held.open.iter().map(|conversation| (conversation, 1u8)))
            .filter(|(conversation, _)| conversation.replies() > 0)
            .max_by_key(|(conversation, open)| (conversation.last, *open))
            .map(|(conversation, _)| conversation)
            .filter(|conversation| now.saturating_sub(conversation.last) <= KEEP_OFFERING)
            .map(|conversation| Offer {
                id: conversation.id.clone(),
                title: conversation.title.clone(),
                replies: conversation.replies(),
                age: now.saturating_sub(conversation.last).max(0),
            })
    }

    /// Everything said in the open one, for a window that has just opened.
    pub fn transcript(&self) -> Vec<Turn> {
        let Ok(held) = self.held.lock() else {
            return Vec::new();
        };

        let Some(open) = held.open.as_ref() else {
            return Vec::new();
        };

        open.messages
            .iter()
            .filter(|message| message.role != "system")
            .map(|message| Turn {
                role: message.role.clone(),
                text: message.content.clone(),
                attachments: message.attachments.clone(),
                parts: message.parts.clone(),
            })
            .collect()
    }

    /// Every conversation, newest first.
    ///
    /// The open one included, marked as open. A list that hid it would be
    /// missing the one somebody is most likely to be looking for, and the mark
    /// is what stops the row offering to resume something already resumed.
    pub fn summaries(&self, now: i64) -> Vec<Summary> {
        let Ok(held) = self.held.lock() else {
            return Vec::new();
        };

        let mut all: Vec<Summary> = held
            .open
            .iter()
            .map(|one| (one, true))
            .chain(held.past.iter().map(|one| (one, false)))
            .map(|(one, open)| Summary {
                id: one.id.clone(),
                title: one.title.clone(),
                replies: one.replies(),
                age: now.saturating_sub(one.last).max(0),
                open,
            })
            .collect();

        all.sort_by(|a, b| a.age.cmp(&b.age));
        all
    }

    /// What was said in one, whichever one it is, as plain text.
    ///
    /// For the action that copies a conversation. `transcript` reads only the
    /// open one, because that is what the view showing it needs; this one is
    /// asked about a row in the list of past conversations, and that row is
    /// usually not the open one.
    ///
    /// Roles are written out rather than left implicit. A transcript pasted
    /// somewhere else has lost the layout that told the two speakers apart,
    /// and a wall of alternating paragraphs with nothing saying who said what
    /// is not worth pasting.
    pub fn said_in(&self, id: &str) -> Option<String> {
        let held = self.held.lock().ok()?;

        let found = held
            .open
            .iter()
            .chain(held.past.iter())
            .find(|one| one.id == id)?;

        let said = found
            .messages
            .iter()
            .filter(|message| message.role != "system")
            .map(|message| {
                let who = if message.role == "user" {
                    "You"
                } else {
                    "Sill"
                };
                format!("{who}: {}", message.content.trim())
            })
            .collect::<Vec<_>>()
            .join(
                "

",
            );

        Some(said)
    }

    /// Forgets one, wherever it is.
    pub fn forget(&self, id: &str) -> bool {
        let Ok(mut held) = self.held.lock() else {
            return false;
        };

        if held.open.as_ref().is_some_and(|open| open.id == id) {
            held.open = None;
            return true;
        }

        let before = held.past.len();
        held.past.retain(|past| past.id != id);
        held.past.len() != before
    }

    /// Forgets all of them.
    pub fn clear(&self) {
        if let Ok(mut held) = self.held.lock() {
            held.open = None;
            held.past.clear();
        }
    }

    /// Reads what was said before Sill was last closed.
    ///
    /// Nothing is opened: a conversation from yesterday is somewhere to go
    /// back to, not somewhere to be when the launcher appears.
    pub fn load(&self, dir: &std::path::Path) {
        // Nothing there is a thing that was read: an empty history is the
        // ordinary state of a machine nobody has asked anything on yet, and a
        // file that could not be read has been moved aside by the time this
        // returns, so a fresh file destroys nothing either way.
        let saved: Vec<Conversation> = crate::json_store::load_list(&dir.join(FILE), &SCHEMA);

        if let Ok(mut held) = self.held.lock() {
            held.past = saved.into_iter().take(KEEP_PAST).collect();
            // Past whatever the file held, so a restart cannot mint an id that
            // a saved conversation already has.
            held.begun = held.past.len() as u64;
        }

        self.read_the_file
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Writes them out.
    ///
    /// Called after anything that changes them rather than on a timer. There
    /// are at most fifty and the whole file is a few kilobytes, so this costs
    /// less than deciding when to do it would.
    /// Writes them out, and says whether it worked.
    ///
    /// Separate from `save` so the behaviour can be tested against a temporary
    /// directory without standing up an application to report to, which is
    /// rule 20: the brain is testable without the frontend.
    pub fn write_to(&self, dir: &std::path::Path) -> Result<(), String> {
        // Never over a file nobody read. See the field this reads. Not a
        // failure to report: refusing to write is the correct outcome here,
        // and the load that did not happen is what would be worth knowing.
        if !self
            .read_the_file
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            crate::say!("not saving conversations: they were never loaded");
            return Ok(());
        }

        let held = self.held.lock().map_err(|_| "the lock was poisoned")?;

        let all: Vec<&Conversation> = held.open.iter().chain(held.past.iter()).collect();

        crate::json_store::save_atomic(&dir.join(FILE), &all, &SCHEMA)
            .map_err(|err| err.to_string())
    }

    /// Writes them out and reports it when that does not work.
    ///
    /// What everything outside this file calls. It takes the application
    /// rather than a directory because the directory is derived from it and
    /// every caller was already deriving it, and because a failure has
    /// somewhere to go now.
    ///
    /// Worth reporting because there is no other sign. Every conversation from
    /// this session is on screen and reads as saved; the loss happens at the
    /// next start, when the list of past conversations is simply short, and
    /// nothing connects that to the disk having been full an hour earlier.
    pub fn save(&self, app: &tauri::AppHandle) {
        match self.write_to(&crate::state::data_dir(app)) {
            Ok(()) => crate::status::resolved(app, TROUBLE),
            Err(err) => crate::status::report(
                app,
                TROUBLE,
                format!(
                    "Sill could not save your conversations, so anything said since it \
                     started will be gone when it next opens: {err}"
                ),
                Some("ai"),
            ),
        }
    }

    /// Remembers something said, and marks the conversation as spoken to.
    ///
    /// Begins one when nothing is open, so a message can never be said into
    /// nowhere. A fallback rather than the way in: `begin` is what Tab calls,
    /// and it is what decides the name.
    fn said(&self, message: Message, now: i64) {
        let Ok(mut held) = self.held.lock() else {
            return;
        };

        if held.open.is_none() {
            held.open_a_new_one(&message.content, now);
        }

        if let Some(open) = held.open.as_mut() {
            open.messages.push(message);
            open.last = now;

            // Bounded here rather than when it is sent, because the cost is
            // holding it: a conversation is kept in memory in full, written to
            // disk in full, and cloned in full for every question asked.
            trim(&mut open.messages);
        }
    }

    fn context(&self) -> Vec<Message> {
        self.held
            .lock()
            .ok()
            .and_then(|held| held.open.as_ref().map(|open| open.messages.clone()))
            .unwrap_or_default()
    }

    fn session(&self) -> Option<String> {
        self.held
            .lock()
            .ok()
            .and_then(|held| held.open.as_ref().and_then(|open| open.session.clone()))
    }

    fn set_session(&self, session: String) {
        if let Ok(mut held) = self.held.lock() {
            if let Some(open) = held.open.as_mut() {
                open.session = Some(session);
            }
        }
    }

    /// What the open conversation has cost so far. Nothing, when none is.
    pub fn spent(&self) -> Spent {
        self.held
            .lock()
            .ok()
            .and_then(|held| held.open.as_ref().map(|open| open.spent))
            .unwrap_or_default()
    }

    /// Counts a finished turn into the open conversation, and answers with
    /// the total once it is in.
    fn spend(&self, finished: &Finished) -> Spent {
        let Ok(mut held) = self.held.lock() else {
            return Spent::default();
        };

        match held.open.as_mut() {
            Some(open) => {
                open.spent.add(finished);
                open.spent
            }
            None => Spent::default(),
        }
    }
}

/// A question, cut down to something a row can carry.
///
/// Counted and cut in characters rather than bytes: a question can hold
/// anything, and slicing a String in the middle of a character panics.
fn shorten(question: &str) -> String {
    let question = question.trim();

    if question.chars().count() <= TITLE {
        return question.to_string();
    }

    let kept: String = question.chars().take(TITLE - 1).collect();
    let mut out = kept.trim_end().to_string();
    out.push('\u{2026}');
    out
}

/// What the window is told while an answer is being written.
const SAID: &str = "sill://ai-said";
/// What the model is thinking before it writes, for services that say.
const THINKING: &str = "sill://ai-thinking";
/// One tool being reached for, so the window can say what is happening.
const USING: &str = "sill://ai-using";
/// That tool finished, and whether it managed.
const USED: &str = "sill://ai-used";
/// The turn is over, with what it cost.
const DONE: &str = "sill://ai-done";
const FAILED: &str = "sill://ai-failed";

/// Asks the next thing, and streams the answer to the window.
///
/// The question is remembered before the request rather than after, so a
/// conversation that fails half way still shows what was asked. The answer is
/// remembered when it is finished, because half an answer is not a turn.
pub async fn ask(
    app: &tauri::AppHandle,
    provider: &Provider,
    question: &str,
    attachments: Vec<Attached>,
) -> Result<String, String> {
    let question = question.trim();

    // Something handed over is a question in itself. "What is this" with a
    // screenshot attached and nothing typed is the most ordinary thing
    // somebody does with a picture.
    if question.is_empty() && attachments.is_empty() {
        return Err("there is nothing to ask".to_string());
    }

    let chat = app.state::<Chat>();
    chat.said(
        Message::with(question, attachments),
        crate::state::now_seconds(),
    );

    let began = Instant::now();

    let answer = match provider.wire {
        Wire::ClaudeCode => through_the_cli(app, provider, question).await,
        Wire::OpenAi => over_http(app, provider, &chat.context()).await,
        // Anthropic's own format is not written yet. Saying so is better than
        // sending it a request shaped for somebody else and reporting whatever
        // it says about that.
        Wire::Anthropic => Err(
            "Sill cannot speak to that one yet. Reach Anthropic through Claude \
             Code, or through a gateway that speaks the common format."
                .to_string(),
        ),
    };

    match answer {
        Ok(mut answer) => {
            // Measured here rather than by either transport, so the two
            // cannot disagree about what a turn's length means. Claude Code
            // says how long it took as well, and this one includes starting
            // it, which is the number somebody waited through.
            answer.finished.duration_ms = began.elapsed().as_millis() as u64;

            // Priced here, once, whichever way it came; then counted into the
            // conversation, so what the window is told is the total and not
            // something it has to add up for itself.
            price(
                &mut answer.finished,
                &provider.model,
                super::provider::is_on_this_network(&provider.base_url),
            );

            chat.said(
                Message::assistant(&answer.text).with_parts(answer.parts),
                crate::state::now_seconds(),
            );
            answer.finished.spent = chat.spend(&answer.finished);

            // And against the provider, which outlives the conversation.
            let answered_by = if answer.finished.model.is_empty() {
                provider.model.as_str()
            } else {
                answer.finished.model.as_str()
            };
            let ledger = app.state::<super::ledger::Ledger>();
            ledger.record(
                &provider.id,
                answered_by,
                &answer.finished,
                crate::state::now_seconds(),
                &super::ledger::day_key(crate::dates::today()),
            );
            ledger.save(app);

            let _ = app.emit(DONE, &answer.finished);
            Ok(answer.text)
        }
        Err(why) => {
            let _ = app.emit(FAILED, &why);
            Err(why)
        }
    }
}

/// How many times one question may go round before the answer is written.
///
/// Each round is a request paid for in full, so this is a ceiling on a
/// mistake rather than a target: a model that keeps calling tools without ever
/// answering is the failure this bounds, and six is more steps than any
/// question a launcher is asked has needed.
const MOST_STEPS: usize = 6;

/// Over HTTP, to anything speaking the common shape.
///
/// A loop rather than one request, because a tool call is the model asking to
/// be told something before it answers. Each round: ask, run whatever was
/// asked for, put the results in and ask again. It ends the first time a round
/// comes back with words and no calls, which is the ordinary case on the very
/// first round when nothing needed looking up.
async fn over_http(
    app: &tauri::AppHandle,
    provider: &Provider,
    messages: &[Message],
) -> Result<Answer, String> {
    use super::openai::Piece;

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| format!("could not prepare the request: {err}"))?;

    let tools = super::tools::as_request();
    let mut conversation = messages.to_vec();
    let mut telling = Telling::default();
    let mut finished = Finished::default();

    // The number this turn started at. Everything below asks whether it has
    // moved, which is the only thing "stop" means.
    // Read and dropped on the same line rather than bound. A managed state
    // guard held across an await makes the whole future not `Send`, and Tauri
    // needs it to be; the error says nothing about which line did it.
    let since = app.state::<super::approval::Halt>().mark();
    let give_up = || app.state::<super::approval::Halt>().stopped(since);

    let heard = |telling: &mut Telling, piece: Piece| match piece {
        Piece::Text(text) => tell(app, telling, Told::Text(text)),
        Piece::Thinking(thought) => tell(app, telling, Told::Thinking(thought)),
    };

    // The cost of a turn is the cost of every request in it.
    let add_up = |finished: &mut Finished, said: &super::openai::Said| {
        if !said.model.is_empty() {
            finished.model = said.model.clone();
        }
        if let Some(usage) = said.usage {
            let so_far = finished.usage.unwrap_or(Usage {
                input: 0,
                output: 0,
            });
            finished.usage = Some(Usage {
                input: so_far.input + usage.input,
                output: so_far.output + usage.output,
            });
        }
        if let Some(cost) = said.cost {
            finished.cost = Some(finished.cost.unwrap_or(0.0) + cost);
        }
        finished.generating_ms += said.generating_ms;
    };

    let done = |telling: Telling, finished: Finished| Answer {
        text: telling.whole.clone(),
        parts: telling.finish(),
        finished,
    };

    for step in 0..MOST_STEPS {
        let said = super::openai::ask(
            &client,
            provider,
            &conversation,
            Some(&tools),
            &give_up,
            |piece| heard(&mut telling, piece),
        )
        .await?;

        add_up(&mut finished, &said);

        // What arrived is still an answer. Somebody who stops a reply has
        // usually read enough of it, and throwing it away would be its own
        // small betrayal.
        if said.stopped || said.calls.is_empty() {
            return Ok(done(telling, finished));
        }

        // Checked again between steps: a stop pressed while a tool was running
        // should not be followed by another request.
        if give_up() {
            return Ok(done(telling, finished));
        }

        // The turn back in full: what it said, then the calls it asked for.
        // Sending only the results earns a complaint about a tool message with
        // no call before it.
        conversation.push(Message::calling(said.text.clone(), said.calls.clone()));

        for call in &said.calls {
            let name = call.function.name.clone();
            let arguments: serde_json::Value =
                serde_json::from_str(&call.function.arguments).unwrap_or(serde_json::Value::Null);

            // Said before the tool runs rather than after. Reading the screen
            // takes a moment, and a window showing nothing during it looks
            // like a window that has stopped.
            tell(
                app,
                &mut telling,
                Told::Using(Step {
                    id: call.id.clone(),
                    tool: name.clone(),
                    subject: subject_in(&arguments),
                }),
            );

            let found = super::tools::run(app, &name, &arguments).await;

            // A tool never fails as an `Err`; it answers with an `error`
            // field, which is what a failure looks like from here.
            tell(
                app,
                &mut telling,
                Told::Used(Used {
                    id: call.id.clone(),
                    ok: found.get("error").is_none(),
                }),
            );

            conversation.push(Message::answered(&call.id, found.to_string()));
        }

        // Every round after the first is one the person is waiting through, so
        // the last is spent answering rather than looking something else up.
        if step + 1 == MOST_STEPS {
            conversation.push(Message::system(
                "You have used every step available. Answer now with what you have, \
                 and say plainly what you could not find out.",
            ));
        }
    }

    if give_up() {
        return Ok(done(telling, finished));
    }

    // One more, with no tools, so a model that kept calling them still ends
    // with words rather than with nothing.
    let said = super::openai::ask(&client, provider, &conversation, None, &give_up, |piece| {
        heard(&mut telling, piece)
    })
    .await?;

    add_up(&mut finished, &said);

    Ok(done(telling, finished))
}

/// What a tool is about to be used on, for the line that says so.
///
/// The argument a person would recognise, which is nearly always the first
/// string in it. Nothing at all for the tools that take no arguments, where
/// the name already says everything.
fn subject_in(arguments: &serde_json::Value) -> String {
    // In the order a person would read them. `target` is what an action acts
    // on, and it is the only one of these that is worth saying twice.
    for key in ["query", "path", "target"] {
        if let Some(found) = arguments.get(key).and_then(|value| value.as_str()) {
            if !found.trim().is_empty() {
                return found.trim().to_string();
            }
        }
    }

    String::new()
}

/// The MCP config naming Sill's own server, when one can be made.
///
/// Everything here can fail without the question failing. A port that would
/// not open or a config that would not write costs the tools, and a Claude
/// Code that answers from what it knows is far better than one that refuses to
/// answer at all. The reason is written to the log rather than dropped,
/// because "it stopped using the tools" is otherwise a silent change in
/// behaviour with nowhere to look.
async fn the_toolset(
    app: &tauri::AppHandle,
    data_dir: &std::path::Path,
) -> Option<super::mcp::Config> {
    let reachable = match app.state::<super::mcp::link::Link>().reachable(app).await {
        Ok(reachable) => reachable,
        Err(why) => {
            crate::log::write(&format!("[ai] no tools for Claude Code: {why}"));
            return None;
        }
    };

    let bridge = match super::mcp::link::this_program() {
        Ok(bridge) => bridge,
        Err(why) => {
            crate::log::write(&format!("[ai] no tools for Claude Code: {why}"));
            return None;
        }
    };

    match super::mcp::write_config(data_dir, &bridge, reachable.port, &reachable.token) {
        Ok(path) => Some(path),
        Err(why) => {
            crate::log::write(&format!("[ai] could not write the MCP config: {why}"));
            None
        }
    }
}

/// Through the Claude Code binary, on the subscription.
///
/// Only the new question is sent: the CLI holds the conversation and is asked
/// to continue the session it gave us last time.
async fn through_the_cli(
    app: &tauri::AppHandle,
    provider: &Provider,
    question: &str,
) -> Result<Answer, String> {
    use super::claude_code::Event;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let binary = super::claude_code::locate().ok_or_else(|| {
        "Claude Code is not installed, or not somewhere Sill can find it. \
         Install it, or choose a provider with a key instead."
            .to_string()
    })?;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("no data directory: {err}"))?;

    let working = super::claude_code::neutral_directory(&data_dir);
    std::fs::create_dir_all(&working)
        .map_err(|err| format!("could not make a place to run from: {err}"))?;

    // Sill's own tools, over MCP, because this CLI has no other way to be
    // handed any. Awaited before the chat state is borrowed below: the port is
    // opened once and this is where that happens.
    let tools = the_toolset(app, &data_dir).await;

    let chat = app.state::<Chat>();

    let mut command = tokio::process::Command::new(&binary);
    command
        .args(super::claude_code::arguments(
            chat.session().as_deref(),
            Some(&provider.model)
                .filter(|m| !m.is_empty())
                .map(|m| m.as_str()),
            tools.as_ref().map(super::mcp::Config::path),
        ))
        // The reason is in `claude_code.rs`: a session that is not bare runs
        // the hooks and servers of wherever it starts.
        .current_dir(&working)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    for (name, value) in super::claude_code::environment(provider) {
        command.env(name, value);
    }

    #[cfg(windows)]
    {
        // No console window. Without this a black box flashes on screen every
        // time somebody asks a question.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("could not start Claude Code: {err}"))?;

    // The question goes in on stdin, so a long one cannot run into a
    // command-line length limit and nothing has to be quoted.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(question.as_bytes()).await;
        // Closed, or it waits for more.
        drop(stdin);
    }

    let mut telling = Telling::default();
    let mut finished = Finished::default();
    let mut failure = None;
    let mut stopped = false;

    // Calls being assembled, by the position the stream numbers them with.
    // A position means something only until its block stops, and positions
    // start again from zero with each message, so a finished block leaves
    // the map at once.
    let mut calls: std::collections::HashMap<usize, (String, String, String)> =
        std::collections::HashMap::new();

    // Same rule as over HTTP: stop is the counter having moved. Checked per
    // line received, which is where this loop wakes up anyway; a stop pressed
    // while the CLI is silent takes effect on its next line.
    let since = app.state::<super::approval::Halt>().mark();
    let give_up = || app.state::<super::approval::Halt>().stopped(since);

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if give_up() {
                // Killed rather than left to finish. A stopped turn that kept
                // its process would keep spending on an answer nobody reads.
                let _ = child.kill().await;
                stopped = true;
                break;
            }

            match super::claude_code::parse_event(&line) {
                Event::Text(piece) => tell(app, &mut telling, Told::Text(piece)),
                Event::Thinking(piece) => tell(app, &mut telling, Told::Thinking(piece)),
                Event::Model(model) => finished.model = model,
                Event::CallBegun { at, id, name } => {
                    calls.insert(at, (id, name, String::new()));
                }
                Event::CallInput { at, json } => {
                    if let Some((_, _, arguments)) = calls.get_mut(&at) {
                        arguments.push_str(&json);
                    }
                }
                Event::BlockDone { at } => {
                    if let Some((id, name, arguments)) = calls.remove(&at) {
                        let arguments: serde_json::Value =
                            serde_json::from_str(&arguments).unwrap_or(serde_json::Value::Null);
                        tell(
                            app,
                            &mut telling,
                            Told::Using(Step {
                                id,
                                tool: super::mcp::short_name(&name).to_string(),
                                subject: subject_in(&arguments),
                            }),
                        );
                    }
                }
                Event::CallAnswered { id, failed } => {
                    tell(app, &mut telling, Told::Used(Used { id, ok: !failed }))
                }
                Event::Session(id) => chat.set_session(id),
                Event::Failed(why) => failure = Some(why),
                Event::Done => {
                    let cost = super::claude_code::outcome(&line);
                    if !cost.model.is_empty() {
                        finished.model = cost.model;
                    }
                    finished.usage = cost.usage;
                    finished.cost = cost.cost;
                }
                Event::Ignored => {}
            }
        }
    }

    let _ = child.wait().await;

    let answer = Answer {
        text: telling.whole.clone(),
        parts: telling.finish(),
        finished,
    };

    match failure {
        Some(why) => Err(why),
        // What arrived before the stop is still an answer, however little.
        None if stopped => Ok(answer),
        None if answer.text.trim().is_empty() => Err(
            "Claude Code answered with nothing. It may need signing in: run \
                 `claude` once in a terminal."
                .to_string(),
        ),
        None => Ok(answer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(chat: &Chat, question: &str, answer: &str, at: i64) {
        chat.said(Message::user(question), at);
        chat.said(Message::assistant(answer), at);
    }

    mod what_it_costs {
        use super::*;

        fn turn(input: u64, output: u64, cost: Option<f64>, generating_ms: u64) -> Finished {
            Finished {
                usage: Some(Usage { input, output }),
                cost,
                generating_ms,
                ..Finished::default()
            }
        }

        /// Every answer counts, and the ones that could not be priced are
        /// counted as tokens and named, so the total never reads as smaller
        /// than the bill.
        #[test]
        fn answers_add_up_and_the_unpriced_are_named() {
            let mut spent = Spent::default();
            spent.add(&turn(100, 50, Some(0.01), 1000));
            spent.add(&turn(200, 25, None, 500));
            spent.add(&turn(10, 10, Some(0.02), 0));

            assert_eq!((spent.input, spent.output), (310, 85));
            assert_eq!(spent.answers, 3);
            assert_eq!(spent.unpriced, 1);
            assert!((spent.cost.expect("priced") - 0.03).abs() < 1e-12);
        }

        /// The rate is the last answer's, not an average: the number a local
        /// model is judged by is how fast it is writing now.
        #[test]
        fn the_rate_is_the_last_answers() {
            let mut spent = Spent::default();
            spent.add(&turn(0, 100, None, 2000));
            assert_eq!(spent.rate, Some(50.0));

            spent.add(&turn(0, 30, None, 1000));
            assert_eq!(spent.rate, Some(30.0));

            // Untimed, or unanswered, and there is no rate to show.
            spent.add(&turn(0, 30, None, 0));
            assert_eq!(spent.rate, None);
            spent.add(&Finished::default());
            assert_eq!(spent.rate, None);
        }

        /// Days summed into a month keep every count and the newest speed.
        #[test]
        fn totals_merge_and_keep_the_newest_rate() {
            let mut month = Spent::default();
            let mut monday = Spent::default();
            monday.add(&turn(10, 100, Some(0.01), 2000));
            let mut tuesday = Spent::default();
            tuesday.add(&turn(10, 30, None, 1000));

            month.merge(&monday);
            month.merge(&tuesday);

            assert_eq!((month.input, month.output, month.answers), (20, 130, 2));
            assert_eq!(month.unpriced, 1);
            assert_eq!(month.cost, Some(0.01));
            assert_eq!(month.generating_ms, 3000);
            assert_eq!(month.rate, Some(30.0));

            // A day with no rate does not erase the one before it.
            month.merge(&Spent::default());
            assert_eq!(month.rate, Some(30.0));
        }

        /// Nothing counted is still an answer counted.
        #[test]
        fn an_answer_with_no_numbers_still_counts_as_one() {
            let mut spent = Spent::default();
            spent.add(&Finished::default());
            assert_eq!(spent.answers, 1);
            assert_eq!(spent.unpriced, 0);
            assert_eq!(spent.cost, None);
        }

        /// A service that named the dollars is believed over the table.
        #[test]
        fn a_price_the_service_named_is_kept() {
            let mut finished = turn(1_000_000, 0, Some(0.25), 0);
            finished.model = "gpt-5.2".to_string();
            price(&mut finished, "gpt-5.2", false);
            assert_eq!(finished.cost, Some(0.25));
        }

        /// The model that answered is priced, and the one asked for is only
        /// a fallback: a gateway asked for an alias answers with the real
        /// one, and that is what it bills.
        #[test]
        fn the_model_that_answered_is_the_one_priced() {
            let mut finished = turn(1_000_000, 0, None, 0);
            finished.model = "gpt-5-nano".to_string();
            price(&mut finished, "gpt-5.2", false);
            assert_eq!(finished.cost, Some(0.05));

            let mut finished = turn(1_000_000, 0, None, 0);
            price(&mut finished, "gpt-5.2", false);
            assert_eq!(finished.cost, Some(1.75));
        }

        /// A model on this machine costs nothing whatever it is called.
        #[test]
        fn a_local_model_is_never_priced() {
            let mut finished = turn(1_000_000, 1_000_000, None, 0);
            finished.model = "gpt-5.2".to_string();
            price(&mut finished, "gpt-5.2", true);
            assert_eq!(finished.cost, None);
        }

        /// A model nobody has priced is left unpriced rather than guessed.
        #[test]
        fn an_unknown_model_is_left_unpriced() {
            let mut finished = turn(1_000, 1_000, None, 0);
            finished.model = "mystery-9b".to_string();
            price(&mut finished, "mystery-9b", false);
            assert_eq!(finished.cost, None);
        }

        /// Counting goes into the open conversation and nowhere else.
        #[test]
        fn spending_lands_on_the_open_conversation() {
            let chat = Chat::new();
            assert_eq!(chat.spend(&turn(1, 1, None, 0)), Spent::default());

            chat.begin("first", 0);
            let total = chat.spend(&turn(10, 5, Some(0.001), 0));
            assert_eq!((total.input, total.output, total.answers), (10, 5, 1));
            assert_eq!(chat.spent(), total);

            chat.begin("second", 1);
            assert_eq!(chat.spent(), Spent::default());
        }
    }

    mod one_conversation_per_question {
        use super::*;

        /// The whole point of the change. Tab used to join whatever came
        /// before, forever, with nothing on screen saying so.
        #[test]
        fn beginning_one_does_not_carry_the_last_one_into_it() {
            let chat = Chat::new();

            chat.begin("first", 0);
            said(&chat, "first", "an answer", 0);

            chat.begin("second", 10);

            let turns = chat.transcript();
            assert!(
                turns.is_empty(),
                "a new conversation started holding {} turns",
                turns.len(),
            );
        }

        #[test]
        fn a_follow_up_stays_in_the_conversation_it_was_asked_in() {
            let chat = Chat::new();

            chat.begin("first", 0);
            said(&chat, "first", "an answer", 0);
            said(&chat, "and another thing", "a second answer", 5);

            assert_eq!(chat.transcript().len(), 4);
        }

        /// Claude Code holds the conversation itself, so a session carried
        /// into a new one would continue the old conversation on its side
        /// while this side showed an empty transcript.
        #[test]
        fn a_new_conversation_does_not_inherit_the_last_ones_session() {
            let chat = Chat::new();

            chat.begin("first", 0);
            chat.set_session("session-abc".to_string());
            assert_eq!(chat.session().as_deref(), Some("session-abc"));

            chat.begin("second", 10);
            assert_eq!(chat.session(), None);
        }
    }

    mod what_is_offered_back {
        use super::*;

        #[test]
        fn the_one_just_left_is_offered() {
            let chat = Chat::new();
            chat.begin("what is a launcher", 100);
            said(&chat, "what is a launcher", "a thing", 100);

            let offer = chat.offer(160).expect("something to go back to");
            assert_eq!(offer.title, "what is a launcher");
            assert_eq!(offer.replies, 1);
            assert_eq!(offer.age, 60);
        }

        /// A question whose answer never arrived is not a place to return to.
        #[test]
        fn one_that_never_got_an_answer_is_not_offered() {
            let chat = Chat::new();
            chat.begin("this one failed", 100);
            chat.said(Message::user("this one failed"), 100);

            assert_eq!(chat.offer(120), None);
        }

        #[test]
        fn nothing_is_offered_before_anything_is_asked() {
            assert_eq!(Chat::new().offer(0), None);
        }

        /// It has to expire by itself. It is the only row in the root list
        /// that is about the past, and a row like that becomes furniture.
        #[test]
        fn it_stops_being_offered_once_it_is_stale() {
            let chat = Chat::new();
            chat.begin("ages ago", 0);
            said(&chat, "ages ago", "an answer", 0);

            assert!(chat.offer(KEEP_OFFERING).is_some(), "still fresh");
            assert_eq!(chat.offer(KEEP_OFFERING + 1), None, "a second past it");
        }

        /// Set aside is not deleted. The conversation you were just in is
        /// still the one you are most likely to want back.
        #[test]
        fn one_set_aside_is_still_offered() {
            let chat = Chat::new();
            chat.begin("still wanted", 0);
            said(&chat, "still wanted", "an answer", 0);

            chat.set_aside();

            assert_eq!(
                chat.offer(10).map(|offer| offer.title),
                Some("still wanted".to_string()),
            );
            assert!(chat.transcript().is_empty(), "and nothing is open");
        }

        /// The most recently spoken to, which is not the most recently begun:
        /// reopening an old one makes it the recent one again.
        #[test]
        fn the_most_recently_spoken_to_is_the_one_offered() {
            let chat = Chat::new();

            chat.begin("older", 0);
            said(&chat, "older", "an answer", 0);

            chat.begin("newer", 50);
            said(&chat, "newer", "an answer", 50);

            assert_eq!(
                chat.offer(60).map(|offer| offer.title),
                Some("newer".to_string()),
            );
        }
    }

    mod going_back {
        use super::*;

        #[test]
        fn a_filed_conversation_comes_back_whole() {
            let chat = Chat::new();

            chat.begin("the first thing", 0);
            said(&chat, "the first thing", "the first answer", 0);
            let id = chat.offer(1).expect("an offer").id;

            chat.begin("something else", 10);
            said(&chat, "something else", "another answer", 10);

            assert!(chat.resume(&id, 20), "it should still be here");

            let turns = chat.transcript();
            assert_eq!(turns.len(), 2);
            assert_eq!(turns[0].text, "the first thing");
            assert_eq!(turns[1].text, "the first answer");
        }

        /// The row offering one does not know whether it is open or filed, so
        /// resuming the open one has to work rather than fail.
        #[test]
        fn resuming_the_one_already_open_is_not_an_error() {
            let chat = Chat::new();
            chat.begin("open right now", 0);
            said(&chat, "open right now", "an answer", 0);

            let id = chat.offer(1).expect("an offer").id;

            assert!(chat.resume(&id, 2));
            assert_eq!(chat.transcript().len(), 2);
        }

        /// Going back must not throw away what you were in the middle of.
        #[test]
        fn what_was_open_is_filed_rather_than_lost() {
            let chat = Chat::new();

            chat.begin("the old one", 0);
            said(&chat, "the old one", "an answer", 0);
            let old = chat.offer(1).expect("an offer").id;

            chat.begin("the one in progress", 10);
            said(&chat, "the one in progress", "an answer", 10);
            let current = chat.offer(11).expect("an offer").id;

            chat.resume(&old, 20);
            assert!(chat.resume(&current, 30), "the interrupted one survived");
            assert_eq!(chat.transcript()[0].text, "the one in progress");
        }

        #[test]
        fn a_conversation_that_is_not_here_says_so() {
            assert!(!Chat::new().resume("chat:404", 0));
        }

        /// Ids have to be unique or resuming picks the wrong one, and two
        /// conversations begun in the same second is an ordinary thing.
        #[test]
        fn two_begun_in_the_same_second_are_still_told_apart() {
            let chat = Chat::new();

            chat.begin("one", 7);
            said(&chat, "one", "an answer", 7);
            let first = chat.offer(7).expect("an offer").id;

            chat.begin("two", 7);
            said(&chat, "two", "an answer", 7);
            let second = chat.offer(7).expect("an offer").id;

            assert_ne!(first, second);
        }

        /// Bounded, because they are held in memory for as long as Sill runs.
        #[test]
        fn only_so_many_are_kept() {
            let chat = Chat::new();

            for n in 0..(KEEP_PAST as i64 + 5) {
                chat.begin(&format!("question {n}"), n);
                said(&chat, "q", "an answer", n);
            }

            let held = chat.held.lock().expect("the lock");
            assert_eq!(held.past.len(), KEEP_PAST);
        }
    }

    mod the_list_of_them {
        use super::*;

        #[test]
        fn the_open_one_is_listed_and_marked() {
            let chat = Chat::new();
            chat.begin("what is open", 0);
            said(&chat, "what is open", "an answer", 0);

            let all = chat.summaries(30);
            assert_eq!(all.len(), 1);
            assert_eq!(all[0].title, "what is open");
            assert_eq!(all[0].age, 30);
            assert!(all[0].open, "the one being had is the one to say so about");
        }

        /// Newest first, which is not the order they were begun in: reopening
        /// an old one makes it the recent one again.
        #[test]
        fn they_are_ordered_by_when_each_was_last_spoken_to() {
            let chat = Chat::new();

            chat.begin("oldest", 0);
            said(&chat, "oldest", "a", 0);
            chat.begin("middle", 10);
            said(&chat, "middle", "a", 10);
            chat.begin("newest", 20);
            said(&chat, "newest", "a", 20);

            let titles: Vec<String> = chat.summaries(30).into_iter().map(|s| s.title).collect();
            assert_eq!(titles, vec!["newest", "middle", "oldest"]);
        }

        /// Unlike the row offering one back, this list holds everything. A
        /// question that failed is still something somebody asked, and hiding
        /// it from the only place it can be deleted would strand it.
        #[test]
        fn one_with_no_answer_is_still_listed() {
            let chat = Chat::new();
            chat.begin("this failed", 0);
            chat.said(Message::user("this failed"), 0);

            assert_eq!(chat.summaries(1).len(), 1);
            assert_eq!(chat.offer(1), None, "but it is not offered back");
        }
    }

    mod forgetting {
        use super::*;

        #[test]
        fn one_that_was_filed_goes() {
            let chat = Chat::new();
            chat.begin("keep me", 0);
            said(&chat, "keep me", "a", 0);
            chat.begin("delete me", 10);
            said(&chat, "delete me", "a", 10);

            let doomed = chat.summaries(11)[0].id.clone();
            assert!(chat.forget(&doomed));

            let titles: Vec<String> = chat.summaries(11).into_iter().map(|s| s.title).collect();
            assert_eq!(titles, vec!["keep me"]);
        }

        /// Deleting the one being had closes it rather than leaving a
        /// conversation on screen that nothing holds any more.
        #[test]
        fn the_open_one_goes_and_leaves_nothing_open() {
            let chat = Chat::new();
            chat.begin("the open one", 0);
            said(&chat, "the open one", "a", 0);

            let open = chat.summaries(1)[0].id.clone();
            assert!(chat.forget(&open));

            assert!(chat.summaries(1).is_empty());
            assert!(chat.transcript().is_empty());
        }

        #[test]
        fn forgetting_one_that_is_not_here_says_so() {
            assert!(!Chat::new().forget("chat:404"));
        }
    }

    mod across_a_restart {
        use super::*;

        fn a_directory(name: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!("sill-chat-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a directory");
            dir
        }

        #[test]
        fn what_was_said_survives() {
            let dir = a_directory("survives");

            let before = Chat::new();
            // What the app does before anything else, and what the guard on
            // `save` now requires: never write over a file nobody read.
            before.load(&dir);
            before.begin("asked yesterday", 100);
            said(&before, "asked yesterday", "answered yesterday", 100);
            before.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);

            let all = after.summaries(200);
            assert_eq!(all.len(), 1);
            assert_eq!(all[0].title, "asked yesterday");
            assert_eq!(all[0].replies, 1);
        }

        /// Yesterday's conversation is somewhere to go back to, not somewhere
        /// to be when the launcher next appears.
        #[test]
        fn nothing_is_open_after_a_restart() {
            let dir = a_directory("nothing-open");

            let before = Chat::new();
            // What the app does before anything else, and what the guard on
            // `save` now requires: never write over a file nobody read.
            before.load(&dir);
            before.begin("asked yesterday", 100);
            said(&before, "asked yesterday", "an answer", 100);
            before.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);

            assert!(after.transcript().is_empty());
            assert!(!after.summaries(200)[0].open);
        }

        /// The whole point of saving them: reopening one from a previous run
        /// has to give back what was said, not an empty conversation.
        #[test]
        fn one_from_before_can_be_reopened_whole() {
            let dir = a_directory("reopened");

            let before = Chat::new();
            // What the app does before anything else, and what the guard on
            // `save` now requires: never write over a file nobody read.
            before.load(&dir);
            before.begin("the question", 100);
            said(&before, "the question", "the answer", 100);
            before.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);

            let id = after.summaries(200)[0].id.clone();
            assert!(after.resume(&id, 200));

            let turns = after.transcript();
            assert_eq!(turns.len(), 2);
            assert_eq!(turns[1].text, "the answer");
        }

        /// A session id belongs to a running CLI. Kept across a restart it
        /// would make the first follow-up fail with somebody else's error
        /// about a session that is not there.
        #[test]
        fn a_cli_session_is_not_carried_across() {
            let dir = a_directory("no-session");

            let before = Chat::new();
            // What the app does before anything else, and what the guard on
            // `save` now requires: never write over a file nobody read.
            before.load(&dir);
            before.begin("asked", 0);
            said(&before, "asked", "an answer", 0);
            before.set_session("session-abc".to_string());
            before.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);
            let id = after.summaries(1)[0].id.clone();
            after.resume(&id, 1);

            assert_eq!(after.session(), None);
        }

        /// A restart must not mint an id that a saved conversation already
        /// has, or resuming picks whichever the search finds first.
        #[test]
        fn a_new_conversation_after_a_restart_does_not_reuse_an_id() {
            let dir = a_directory("ids");

            let before = Chat::new();
            for n in 0..3 {
                before.begin(&format!("question {n}"), n);
                said(&before, "q", "a", n);
            }
            before.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);
            after.begin("brand new", 100);
            said(&after, "brand new", "a", 100);

            let ids: Vec<String> = after.summaries(100).into_iter().map(|s| s.id).collect();
            let mut unique = ids.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(ids.len(), unique.len(), "an id came back twice: {ids:?}");
        }

        /// A file written before attachments existed.
        ///
        /// Exactly the shape that was on disk, keys and all. If this stops
        /// parsing, every conversation somebody has ever had is silently
        /// replaced by an empty list the next time anything is asked.
        #[test]
        fn a_file_from_an_older_build_still_reads() {
            let dir = a_directory("older");
            std::fs::write(
                dir.join(FILE),
                r#"[{"id":"chat:1","title":"what windows are open","last":1788201736,
                     "messages":[{"role":"user","content":"what windows are open"},
                                 {"role":"assistant","content":"eleven"}]}]"#,
            )
            .expect("written");

            let chat = Chat::new();
            chat.load(&dir);

            let all = chat.summaries(1788201800);
            assert_eq!(all.len(), 1, "an older file was thrown away");
            assert_eq!(all[0].title, "what windows are open");
            assert_eq!(all[0].replies, 1);

            // Nothing was counted back then, and nothing is the number.
            assert!(chat.resume("chat:1", 1788201800));
            assert_eq!(chat.spent(), Spent::default());
        }

        /// What a conversation cost goes to disk with it, so reopening one
        /// tomorrow still says what it came to.
        #[test]
        fn what_was_spent_survives_a_restart() {
            let dir = a_directory("spent");
            let chat = Chat::new();
            chat.load(&dir);
            chat.begin("first", 0);
            said(&chat, "first", "an answer", 0);
            chat.spend(&Finished {
                usage: Some(Usage {
                    input: 100,
                    output: 40,
                }),
                cost: Some(0.5),
                generating_ms: 2000,
                ..Finished::default()
            });
            chat.write_to(&dir).expect("written");

            let again = Chat::new();
            again.load(&dir);
            assert!(again.resume("chat:1", 10));

            let spent = again.spent();
            assert_eq!((spent.input, spent.output, spent.answers), (100, 40, 1));
            assert_eq!(spent.cost, Some(0.5));
            assert_eq!(spent.rate, Some(20.0));
        }

        /*
         * The one that turns a bad read into a permanent loss.
         *
         * Loading tolerates a file it cannot parse, which is right. Saving
         * then wrote whatever was in memory over the top, which for a failed
         * load is nothing at all: one unreadable byte and every conversation
         * is gone, with no way back and nothing said about it.
         */
        #[test]
        fn a_file_that_could_not_be_read_is_not_overwritten() {
            let dir = a_directory("kept");
            let nonsense = "{not json at all";
            std::fs::write(dir.join(FILE), nonsense).expect("written");

            let chat = Chat::new();
            chat.load(&dir);
            chat.begin("something new", 0);
            said(&chat, "something new", "an answer", 0);
            chat.write_to(&dir).expect("written");

            let aside = std::fs::read_to_string(dir.join(BROKEN)).expect("kept aside");
            assert_eq!(aside, nonsense, "what could not be read was thrown away");

            // Read back through the store rather than off the bytes, because
            // what the bytes look like is `json_store`'s business and this is
            // asking whether the conversation survived.
            let after = Chat::new();
            after.load(&dir);
            assert_eq!(
                after.summaries(0).len(),
                1,
                "the new conversation was not saved"
            );
        }

        /*
         * The failure that has no symptom.
         *
         * Saving writes what is in memory over the file, and before a load
         * there is nothing in memory. One save that got in first would replace
         * every conversation with an empty list, and nothing about that looks
         * wrong at the time: the window simply says nothing has been asked.
         */
        #[test]
        fn nothing_is_written_over_a_file_that_was_never_read() {
            let dir = a_directory("unread");

            let first = Chat::new();
            first.begin("worth keeping", 0);
            said(&first, "worth keeping", "an answer", 0);
            first.load(&dir);
            first.write_to(&dir).expect("written");

            // A second `Chat` that never loaded, doing what it does.
            let careless = Chat::new();
            careless.begin("brand new", 10);
            said(&careless, "brand new", "an answer", 10);
            let _ = careless.write_to(&dir);

            let after = Chat::new();
            after.load(&dir);

            let titles: Vec<String> = after.summaries(20).into_iter().map(|s| s.title).collect();
            assert_eq!(titles, vec!["worth keeping"], "the file was written over");
        }

        /// Having read an empty machine counts as having read it, or the very
        /// first conversation anybody has could never be saved.
        #[test]
        fn a_machine_with_no_history_can_still_save_its_first_conversation() {
            let dir = a_directory("first-ever");

            let chat = Chat::new();
            chat.load(&dir);
            chat.begin("the first thing", 0);
            said(&chat, "the first thing", "an answer", 0);
            chat.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);
            assert_eq!(after.summaries(1).len(), 1);
        }

        /// A save that does not happen says so rather than returning quietly.
        ///
        /// This used to be `let _ = std::fs::write(...)`, so a full disk, a
        /// read-only data directory or a file another process was holding
        /// meant every conversation from that session was gone at the next
        /// start with nothing at any point suggesting it. Nothing on screen
        /// changes when a save fails: the conversation is still there, in
        /// memory, looking saved.
        ///
        /// A directory standing where the file goes is the failure that can be
        /// arranged on any machine. What it stands in for is the ones that
        /// cannot: the disk being full, and the folder being denied.
        #[test]
        fn a_save_that_cannot_be_written_says_so() {
            let dir = a_directory("unwritable");

            // Something that is not a file, exactly where the file belongs.
            std::fs::create_dir_all(dir.join(FILE)).expect("a directory in the way");

            let chat = Chat::new();
            chat.load(&dir);
            chat.begin("worth keeping", 0);
            said(&chat, "worth keeping", "an answer", 0);

            assert!(
                chat.write_to(&dir).is_err(),
                "a save that did not happen reported success, so the loss is silent"
            );
        }

        #[test]
        fn a_missing_file_is_not_a_problem() {
            let chat = Chat::new();
            chat.load(&std::env::temp_dir().join("sill-chat-nothing-here"));
            assert!(chat.summaries(0).is_empty());
        }

        /// A file somebody edited, or one written by a version that shaped
        /// them differently. Starting empty beats refusing to start.
        #[test]
        fn a_file_full_of_nonsense_is_not_a_problem() {
            let dir = a_directory("nonsense");
            std::fs::write(dir.join(FILE), "{not json at all").expect("written");

            let chat = Chat::new();
            chat.load(&dir);
            assert!(chat.summaries(0).is_empty());
        }
    }

    mod naming_one {
        use super::*;

        #[test]
        fn a_short_question_is_its_own_name() {
            assert_eq!(shorten("  what time is it  "), "what time is it");
        }

        #[test]
        fn a_long_one_is_cut_and_says_it_was() {
            let name = shorten(&"a".repeat(200));
            assert_eq!(name.chars().count(), TITLE);
            assert!(name.ends_with('\u{2026}'));
        }

        /// A question can hold anything, and cutting a String by bytes in the
        /// middle of a character panics rather than truncating.
        #[test]
        fn cutting_one_full_of_wide_characters_does_not_panic() {
            let name = shorten(&"\u{1f600}".repeat(120));
            assert_eq!(name.chars().count(), TITLE);
        }
    }

    mod what_a_turn_records {
        use super::super::{Step, Telling, Told, Used, KEEP_PARTS, KEEP_THINKING};
        use super::*;
        use crate::ai::openai::Part;

        fn a_step(id: &str) -> Told {
            Told::Using(Step {
                id: id.to_string(),
                tool: "read_file".to_string(),
                subject: "notes.txt".to_string(),
            })
        }

        /// Words arrive a few characters at a time and are one paragraph,
        /// not one part per delta.
        #[test]
        fn text_that_arrives_in_pieces_is_one_part() {
            let mut telling = Telling::default();
            telling.record(Told::Text("Hel".into()));
            telling.record(Told::Text("lo".into()));

            assert_eq!(telling.whole, "Hello");
            assert_eq!(
                telling.finish(),
                vec![Part::Text {
                    text: "Hello".into()
                }]
            );
        }

        /// A newline between two calls is not a paragraph.
        #[test]
        fn whitespace_on_its_own_starts_nothing() {
            let mut telling = Telling::default();
            telling.record(a_step("a"));
            telling.record(Told::Text("\n".into()));
            telling.record(a_step("b"));

            let parts = telling.finish();
            assert_eq!(parts.len(), 2, "{parts:?}");
            assert!(parts.iter().all(|part| matches!(part, Part::Step { .. })));
        }

        /// A step after words starts a new paragraph after it, so the order
        /// on screen is the order it happened.
        #[test]
        fn a_step_between_words_keeps_them_apart() {
            let mut telling = Telling::default();
            telling.record(Told::Text("Looking.".into()));
            telling.record(a_step("a"));
            telling.record(Told::Text("Found it.".into()));

            let parts = telling.finish();
            assert_eq!(parts.len(), 3, "{parts:?}");
            assert!(matches!(&parts[0], Part::Text { text } if text == "Looking."));
            assert!(matches!(&parts[1], Part::Step { .. }));
            assert!(matches!(&parts[2], Part::Text { text } if text == "Found it."));
        }

        #[test]
        fn thinking_is_capped() {
            let mut telling = Telling::default();
            for _ in 0..(KEEP_THINKING / 100 + 2) {
                telling.record(Told::Thinking("x".repeat(100)));
            }

            let parts = telling.finish();
            let Some(Part::Thinking { text, .. }) = parts.first() else {
                panic!("no thinking: {parts:?}");
            };
            assert_eq!(text.len(), KEEP_THINKING);
            assert_eq!(parts.len(), 1);
        }

        /// The cap must not land inside a character.
        #[test]
        fn the_cap_lands_on_a_character_boundary() {
            let mut telling = Telling::default();
            telling.record(Told::Thinking("é".repeat(KEEP_THINKING)));

            let parts = telling.finish();
            let Some(Part::Thinking { text, .. }) = parts.first() else {
                panic!("no thinking: {parts:?}");
            };
            assert!(text.len() <= KEEP_THINKING);
            assert!(text.chars().all(|c| c == 'é'));
        }

        /// Thinking is timed from its first piece to whatever follows it.
        #[test]
        fn thinking_is_timed_once_something_follows() {
            let mut telling = Telling::default();
            telling.record(Told::Thinking("hmm".into()));
            telling.record(Told::Text("eleven".into()));

            let parts = telling.finish();
            assert!(
                matches!(&parts[0], Part::Thinking { ms: Some(_), .. }),
                "{parts:?}"
            );
        }

        #[test]
        fn a_step_is_marked_finished_by_its_id() {
            let mut telling = Telling::default();
            telling.record(a_step("a"));
            telling.record(a_step("b"));
            telling.record(Told::Used(Used {
                id: "a".into(),
                ok: false,
            }));

            let parts = telling.finish();
            assert!(matches!(&parts[0], Part::Step { ok: Some(false), .. }), "{parts:?}");
            assert!(matches!(&parts[1], Part::Step { ok: None, .. }), "{parts:?}");
        }

        /// A result for a call nobody recorded is not a problem.
        #[test]
        fn a_result_for_an_unknown_call_is_ignored() {
            let mut telling = Telling::default();
            telling.record(Told::Used(Used {
                id: "nobody".into(),
                ok: true,
            }));
            assert!(telling.finish().is_empty());
        }

        /// Too many steps drop the oldest steps. The words stay.
        #[test]
        fn too_many_parts_drop_the_oldest_steps_first() {
            let mut telling = Telling::default();
            telling.record(Told::Text("first".into()));
            for n in 0..(KEEP_PARTS + 5) {
                telling.record(a_step(&n.to_string()));
            }

            let parts = telling.finish();
            assert_eq!(parts.len(), KEEP_PARTS);
            assert!(matches!(&parts[0], Part::Text { text } if text == "first"));
            assert!(matches!(&parts[1], Part::Step { id, .. } if id == "6"), "{:?}", parts[1]);
        }

        /// The working comes back with the answer after a restart, which is
        /// the whole reason it is stored rather than held by the window.
        #[test]
        fn parts_survive_a_restart() {
            let dir = std::env::temp_dir().join(format!(
                "sill-chat-parts-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a directory");

            let mut telling = Telling::default();
            telling.record(Told::Thinking("hmm".into()));
            telling.record(a_step("a"));
            telling.record(Told::Used(Used {
                id: "a".into(),
                ok: true,
            }));
            telling.record(Told::Text("eleven".into()));

            let chat = Chat::new();
            chat.load(&dir);
            chat.begin("what is in notes", 0);
            chat.said(Message::user("what is in notes"), 0);
            chat.said(Message::assistant("eleven").with_parts(telling.finish()), 0);
            chat.write_to(&dir).expect("written");

            let after = Chat::new();
            after.load(&dir);
            let id = after.summaries(1)[0].id.clone();
            assert!(after.resume(&id, 1));

            let turns = after.transcript();
            assert_eq!(turns.len(), 2);
            assert_eq!(turns[0].parts.len(), 0, "a question has no parts");
            assert_eq!(turns[1].parts.len(), 3, "{:?}", turns[1].parts);
            assert!(matches!(&turns[1].parts[1], Part::Step { ok: Some(true), .. }));

            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    mod bounding_a_conversation {
        use super::super::{trim, KEEP_ATTACHED_BYTES, KEEP_TURNS};
        use crate::ai::openai::{Attached, Message};

        fn said(role: &str, text: &str) -> Message {
            Message {
                role: role.to_string(),
                content: text.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: None,
                attachments: Vec::new(),
                parts: Vec::new(),
            }
        }

        fn with_picture(bytes: usize) -> Message {
            let mut message = said("user", "what is this");
            message.attachments.push(Attached {
                name: "screenshot.png".to_string(),
                kind: "image".to_string(),
                body: "x".repeat(bytes),
                bytes,
            });
            message
        }

        /// A conversation somebody keeps returning to does not grow forever.
        ///
        /// It is held in memory in full, written to disk in full, and cloned
        /// in full for every question asked, because the whole history is
        /// what a service is sent.
        #[test]
        fn the_oldest_turns_fall_out() {
            let mut messages: Vec<Message> = (0..KEEP_TURNS + 10)
                .map(|n| said("user", &format!("question {n}")))
                .collect();

            trim(&mut messages);

            assert_eq!(messages.len(), KEEP_TURNS);
            assert_eq!(
                messages[0].content, "question 10",
                "the wrong end was dropped"
            );
        }

        /// The instructions are not a turn and do not fall out with them.
        ///
        /// Dropping them because they happen to be oldest would change how the
        /// model answers halfway through a conversation, which reads as it
        /// getting worse the longer you talk to it.
        #[test]
        fn the_system_message_stays_wherever_it_is() {
            let mut messages = vec![said("system", "you are Sill")];
            messages.extend((0..KEEP_TURNS + 10).map(|n| said("user", &format!("q{n}"))));

            trim(&mut messages);

            assert_eq!(messages[0].role, "system", "the instructions were dropped");
        }

        /// The newest pictures are kept and the older bodies are let go.
        ///
        /// An attachment is a data URI, so a screenshot handed over is a
        /// couple of megabytes of base64 in the message it came with.
        #[test]
        fn old_picture_bodies_are_dropped_and_the_newest_kept() {
            let big = KEEP_ATTACHED_BYTES / 2 + 1;
            let mut messages = vec![with_picture(big), with_picture(big), with_picture(big)];

            trim(&mut messages);

            assert!(
                messages[2].attachments[0].body.is_empty() == false,
                "the newest picture was dropped"
            );
            assert!(
                messages[0].attachments[0].body.is_empty(),
                "an old picture body was kept and the budget means nothing"
            );
        }

        /// What is dropped still says what it was.
        #[test]
        fn a_dropped_picture_keeps_its_name_and_size() {
            let big = KEEP_ATTACHED_BYTES + 1;
            let mut messages = vec![with_picture(big), with_picture(big)];

            trim(&mut messages);

            let gone = &messages[0].attachments[0];
            assert!(gone.body.is_empty());
            assert_eq!(gone.name, "screenshot.png");
            assert_eq!(gone.bytes, big, "the chip would show the wrong size");
        }

        /// A short conversation is left exactly as it is.
        #[test]
        fn an_ordinary_conversation_is_untouched() {
            let mut messages = vec![said("user", "hello"), said("assistant", "hello")];
            let before = messages.clone();

            trim(&mut messages);

            assert_eq!(messages.len(), before.len());
            assert_eq!(messages[0].content, "hello");
        }
    }
}
