//! Stopping to ask before something is changed.
//!
//! The turn genuinely pauses. The alternative was to run everything and offer
//! an undo afterwards, and it is worse for the reason undo is always worse
//! than not doing it: half the things worth asking about are things whose undo
//! is a lie. A file moved across a volume was copied and deleted, a window
//! closed took whatever was unsaved with it, and a program launched has
//! already done whatever it does on startup.
//!
//! ## Why a channel rather than a flag
//!
//! The tool loop is an ordinary async function, and what it wants is to wait.
//! A flag polled on a timer is the same wait written worse: it burns wakeups
//! while nothing is happening, which is the thing this codebase measures, and
//! it still has to decide how often to look.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tokio::sync::oneshot;

/// How long a card waits before it counts as refused.
///
/// Long enough to read a sentence and think, short enough that a conversation
/// somebody walked away from does not hold a turn open until Sill closes.
/// Refusing rather than allowing on a timeout is the only safe direction: the
/// question was "may I change this", and silence is not a yes.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(90);

/// What the window is told, when something needs deciding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asking {
    /// What the answer must name, so two cards cannot be confused.
    pub id: String,
    /// The action, as the panel would title it.
    pub title: String,
    /// What it is about to act on.
    pub subject: String,
    /// What it touches, in words somebody deciding would use.
    pub touches: String,
}

/// How it was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Allowed,
    Refused,
    /// Nobody was there. Counted as refused, said differently.
    Unanswered,
}

/// Every card waiting on somebody.
#[derive(Default)]
pub struct Pending {
    waiting: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    /// Numbers the cards, so two in one turn are told apart.
    asked: Mutex<u64>,
}

impl Pending {
    pub fn new() -> Self {
        Self::default()
    }

    /// A name for the next card.
    pub fn next_id(&self) -> String {
        let mut asked = match self.asked.lock() {
            Ok(asked) => asked,
            Err(poisoned) => poisoned.into_inner(),
        };

        *asked += 1;
        format!("ask:{asked}")
    }

    /// Waits for one to be answered.
    ///
    /// The sender is dropped when the window answers, and dropping it without
    /// sending is also an answer: a launcher that was dismissed mid-question
    /// has refused, and waiting the full ninety seconds for a window nobody is
    /// looking at helps nothing.
    pub async fn wait(&self, id: &str) -> Answer {
        self.wait_for(id, PATIENCE).await
    }

    /// The same wait, with the patience given rather than assumed.
    ///
    /// So that the one property worth proving can be proved in milliseconds:
    /// nobody answering is not permission. A test that had to sit out the real
    /// ninety seconds is a test nobody runs.
    pub async fn wait_for(&self, id: &str, patience: std::time::Duration) -> Answer {
        let (tx, rx) = oneshot::channel();

        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.insert(id.to_string(), tx);
        }

        let answered = tokio::time::timeout(patience, rx).await;

        // Whatever happened, it is not waiting any more. Left in the map it
        // would be a sender nobody sends on and an id nobody answers.
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.remove(id);
        }

        match answered {
            Ok(Ok(true)) => Answer::Allowed,
            Ok(Ok(false)) => Answer::Refused,
            // The sender went without a decision, which is the window closing.
            Ok(Err(_)) => Answer::Refused,
            Err(_) => Answer::Unanswered,
        }
    }

    /// Answers one.
    ///
    /// An id nobody is waiting on is not a failure worth reporting: a card
    /// answered twice, or answered after it timed out, is somebody pressing a
    /// key at the moment it stopped mattering.
    pub fn decide(&self, id: &str, allowed: bool) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };

        if let Some(sender) = waiting.remove(id) {
            let _ = sender.send(allowed);
        }
    }

    /// Refuses everything outstanding.
    ///
    /// What leaving a conversation means. Without it a card answered by
    /// nobody holds its turn open for a minute and a half, and the answer
    /// arrives long after somebody moved on.
    pub fn refuse_everything(&self) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };

        for (_, sender) in waiting.drain() {
            let _ = sender.send(false);
        }
    }
}

/// The windows that draw a card.
///
/// The chat window draws one whenever there is one. The launcher draws one
/// only while it is showing a conversation, which is always true when the
/// launcher itself asked the question and is why it counts here.
const SURFACES: &[&str] = &["ask", "main"];

/// Puts a card in front of somebody, and makes sure there is a front.
///
/// The event alone was enough while the only way to reach an action was to ask
/// a question in one of Sill's own windows: the window asking was on screen by
/// definition. Over MCP it is not. A card raised with every Sill window hidden
/// is a question nobody can see, and ninety seconds later it refuses itself,
/// which reads to whoever asked as the tool being broken rather than as
/// permission being withheld.
///
/// So when nothing of Sill's is on screen, the chat window is opened to hold
/// it. Only then: a window arriving unasked in front of what somebody is doing
/// is a cost, and it is worth paying exactly when the alternative is a
/// decision they never got to make.
///
/// The one case this does not cover is the launcher being open on an ordinary
/// search when a card arrives from somewhere else. Rust cannot see which mode
/// the page is in, and drawing the card there anyway would take Enter and
/// Escape away from somebody in the middle of typing.
pub fn raise(app: &tauri::AppHandle, asking: Asking) {
    use tauri::Manager;

    let _ = tauri::Emitter::emit(app, "sill://ai-asking", &asking);

    let seen = SURFACES.iter().any(|label| {
        app.get_webview_window(label)
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
    });

    if seen {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(why) = crate::commands::ai::open_ask(app).await {
            crate::log::write(&format!("[ai] nowhere to show an approval card: {why}"));
        }
    });
}

/// Whether the turn in flight has been told to stop.
///
/// A counter rather than a flag, and the difference matters: a flag set while
/// one turn is running and cleared by whoever notices would also stop the next
/// turn if the timing went badly. A turn remembers the number it started at
/// and stops when the number is no longer that, which cannot reach past the
/// turn it was meant for.
#[derive(Default)]
pub struct Halt {
    at: std::sync::atomic::AtomicU64,
}

impl Halt {
    pub fn new() -> Self {
        Self::default()
    }

    /// The number to remember, taken as a turn begins.
    pub fn mark(&self) -> u64 {
        self.at.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Stops whatever is running.
    pub fn stop(&self) {
        self.at.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether the turn that started at this number should give up.
    pub fn stopped(&self, since: u64) -> bool {
        self.at.load(std::sync::atomic::Ordering::Relaxed) != since
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_card_that_is_allowed_says_so() {
        let pending = std::sync::Arc::new(Pending::new());
        let id = pending.next_id();

        let answering = {
            let pending = pending.clone();
            let id = id.clone();
            tokio::spawn(async move {
                // Long enough that the waiter is registered before the answer.
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                pending.decide(&id, true);
            })
        };

        assert_eq!(pending.wait(&id).await, Answer::Allowed);
        answering.await.expect("the answer");
    }

    #[tokio::test]
    async fn a_card_that_is_refused_says_so() {
        let pending = std::sync::Arc::new(Pending::new());
        let id = pending.next_id();

        {
            let pending = pending.clone();
            let id = id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                pending.decide(&id, false);
            });
        }

        assert_eq!(pending.wait(&id).await, Answer::Refused);
    }

    /// Leaving a conversation answers everything it was waiting on, rather
    /// than holding a turn open for a minute and a half.
    #[tokio::test]
    async fn leaving_refuses_what_was_outstanding() {
        let pending = std::sync::Arc::new(Pending::new());
        let id = pending.next_id();

        {
            let pending = pending.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                pending.refuse_everything();
            });
        }

        assert_eq!(pending.wait(&id).await, Answer::Refused);
    }

    /// Two cards in one turn must not be confused, and the second must not be
    /// answered by the first one's decision.
    #[tokio::test]
    async fn two_cards_are_told_apart() {
        let pending = std::sync::Arc::new(Pending::new());
        let first = pending.next_id();
        let second = pending.next_id();
        assert_ne!(first, second);

        {
            let pending = pending.clone();
            let second = second.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                pending.decide(&second, true);
            });
        }

        assert_eq!(pending.wait(&second).await, Answer::Allowed);
    }

    /// Somebody pressing a key at the moment a card stopped mattering.
    #[test]
    fn answering_one_nobody_is_waiting_on_is_not_a_problem() {
        Pending::new().decide("ask:404", true);
    }

    /// Silence is not a yes. The direction of this default is the whole
    /// safety property, which is why it is proved rather than assumed.
    #[tokio::test]
    async fn nobody_answering_is_not_permission() {
        let pending = Pending::new();
        let id = pending.next_id();

        let answer = pending
            .wait_for(&id, std::time::Duration::from_millis(20))
            .await;

        assert_eq!(answer, Answer::Unanswered);
        assert_ne!(answer, Answer::Allowed);
    }

    mod stopping {
        use super::*;

        #[test]
        fn a_turn_stops_when_it_is_told_to() {
            let halt = Halt::new();
            let since = halt.mark();

            assert!(!halt.stopped(since));
            halt.stop();
            assert!(halt.stopped(since));
        }

        /// The reason it counts rather than flags. A stop pressed while one
        /// turn is running must not reach the next one, and a flag cleared by
        /// whoever notices first would.
        #[test]
        fn stopping_one_turn_does_not_stop_the_next() {
            let halt = Halt::new();

            let first = halt.mark();
            halt.stop();
            assert!(halt.stopped(first));

            let second = halt.mark();
            assert!(!halt.stopped(second), "the next turn started already stopped");
        }

        #[test]
        fn stopping_twice_is_not_a_problem() {
            let halt = Halt::new();
            let since = halt.mark();
            halt.stop();
            halt.stop();
            assert!(halt.stopped(since));
        }
    }
}
