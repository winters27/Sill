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

use serde::Serialize;
use tauri::{Emitter, Manager};

use super::openai::Message;
use super::provider::{Provider, Wire};

/// One exchange, as the window draws it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
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

/// Where they are kept.
///
/// Plain JSON beside the rest of Sill's own files, which is what the clipboard
/// and the dictation transcripts already are. Anything said to a model on this
/// machine is readable by anything running as this user, and the honest place
/// to say so is the settings window rather than an encryption scheme that only
/// looks like it helps.
const FILE: &str = "conversations.json";

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
        let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else {
            return;
        };

        let Ok(saved) = serde_json::from_str::<Vec<Conversation>>(&text) else {
            return;
        };

        if let Ok(mut held) = self.held.lock() {
            held.past = saved.into_iter().take(KEEP_PAST).collect();
            // Past whatever the file held, so a restart cannot mint an id that
            // a saved conversation already has.
            held.begun = held.past.len() as u64;
        }
    }

    /// Writes them out.
    ///
    /// Called after anything that changes them rather than on a timer. There
    /// are at most fifty and the whole file is a few kilobytes, so this costs
    /// less than deciding when to do it would.
    pub fn save(&self, dir: &std::path::Path) {
        let Ok(held) = self.held.lock() else {
            return;
        };

        let all: Vec<&Conversation> = held.open.iter().chain(held.past.iter()).collect();

        let Ok(text) = serde_json::to_string(&all) else {
            return;
        };

        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(FILE), text);
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
) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() {
        return Err("there is nothing to ask".to_string());
    }

    let chat = app.state::<Chat>();
    chat.said(Message::user(question), crate::state::now_seconds());

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
        Ok(text) => {
            chat.said(Message::assistant(&text), crate::state::now_seconds());
            let _ = app.emit(DONE, ());
            Ok(text)
        }
        Err(why) => {
            let _ = app.emit(FAILED, &why);
            Err(why)
        }
    }
}

/// Over HTTP, to anything speaking the common shape.
async fn over_http(
    app: &tauri::AppHandle,
    provider: &Provider,
    messages: &[Message],
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| format!("could not prepare the request: {err}"))?;

    let mut whole = String::new();

    super::openai::ask(&client, provider, messages, |piece| {
        whole.push_str(&piece);
        let _ = app.emit(SAID, &piece);
    })
    .await?;

    Ok(whole)
}

/// Through the Claude Code binary, on the subscription.
///
/// Only the new question is sent: the CLI holds the conversation and is asked
/// to continue the session it gave us last time.
async fn through_the_cli(
    app: &tauri::AppHandle,
    provider: &Provider,
    question: &str,
) -> Result<String, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let binary = super::claude_code::locate().ok_or_else(|| {
        "Claude Code is not installed, or not somewhere Sill can find it. \
         Install it, or choose a provider with a key instead."
            .to_string()
    })?;

    let chat = app.state::<Chat>();

    let working = super::claude_code::neutral_directory(
        &app.path()
            .app_data_dir()
            .map_err(|err| format!("no data directory: {err}"))?,
    );
    std::fs::create_dir_all(&working)
        .map_err(|err| format!("could not make a place to run from: {err}"))?;

    let mut command = tokio::process::Command::new(&binary);
    command
        .args(super::claude_code::arguments(
            chat.session().as_deref(),
            Some(&provider.model).filter(|m| !m.is_empty()).map(|m| m.as_str()),
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

    let mut whole = String::new();
    let mut failure = None;

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            match super::claude_code::parse_event(&line) {
                super::claude_code::Event::Text(piece) => {
                    whole.push_str(&piece);
                    let _ = app.emit(SAID, &piece);
                }
                super::claude_code::Event::Session(id) => chat.set_session(id),
                super::claude_code::Event::Failed(why) => failure = Some(why),
                super::claude_code::Event::Done | super::claude_code::Event::Ignored => {}
            }
        }
    }

    let _ = child.wait().await;

    match failure {
        Some(why) => Err(why),
        None if whole.trim().is_empty() => {
            Err("Claude Code answered with nothing. It may need signing in: run \
                 `claude` once in a terminal."
                .to_string())
        }
        None => Ok(whole),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(chat: &Chat, question: &str, answer: &str, at: i64) {
        chat.said(Message::user(question), at);
        chat.said(Message::assistant(answer), at);
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
            before.begin("asked yesterday", 100);
            said(&before, "asked yesterday", "answered yesterday", 100);
            before.save(&dir);

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
            before.begin("asked yesterday", 100);
            said(&before, "asked yesterday", "an answer", 100);
            before.save(&dir);

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
            before.begin("the question", 100);
            said(&before, "the question", "the answer", 100);
            before.save(&dir);

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
            before.begin("asked", 0);
            said(&before, "asked", "an answer", 0);
            before.set_session("session-abc".to_string());
            before.save(&dir);

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
            before.save(&dir);

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
}
