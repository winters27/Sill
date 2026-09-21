//! Reading and replacing whatever text is selected in another application.
//!
//! Windows offers no way to ask "what is selected?" that works everywhere. UI
//! Automation answers for applications that implement `TextPattern` and stays
//! silent for the rest, which in practice means it fails in a browser text
//! box, in a terminal, and in half of Electron. So this does what every tool
//! in this space does: it presses Ctrl+C for you, reads what landed, and puts
//! the clipboard back the way it was.
//!
//! That is a borrowed clipboard, not a clobbered one, and the borrowing has to
//! be invisible:
//!
//! - The clipboard history must not record any of it, or every transformation
//!   would leave entries behind.
//! - The original contents go back afterwards, so the thing the user copied
//!   ten minutes ago is still there.
//! - The keystroke is marked as ours, so the dictation hook does not read
//!   Sill's own Ctrl+C as a trigger.
//!
//! **The borrow is held for the whole operation, by one owner.** It used to be
//! two nested save-and-restore pairs, one in the capture and one in the
//! replace, each reserving the changes it expected to make. That is wrong for
//! a reason no amount of reading the code showed: the action running in
//! between writes its own result to the clipboard, so by the time the replace
//! read "the previous contents" it was reading the action's output and
//! faithfully restored that. Measured on a real desktop: a 5,011 character
//! clipboard came back as 14 characters. See [`Held`].
//!
//! **The borrow never empties the clipboard, and never writes over somebody
//! else's copy.** Windows numbers every clipboard change, and the borrow keeps
//! the number of the last change that was Sill's doing. Giving the clipboard
//! back compares that number with the current one: equal means the last thing
//! written was ours and the previous text goes back; different means somebody
//! copied something while Sill held the borrow, and theirs stays. When there
//! was no text to begin with (a picture, a list of files, nothing) Sill's
//! result is left where it is rather than the clipboard being emptied, which
//! is what "restoring" a picture used to mean. See [`Borrow`].
//!
//! Every write and every give-back is said in the log with the sequence
//! numbers around it, so a copy that goes missing on a real desktop can be
//! placed against what Sill was doing at that moment.
//!
//! Sill must not be the foreground window when any of this runs. Everything
//! here is reached from a global shortcut with the launcher hidden, which is
//! the only arrangement where "the selection" means anything.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::clipboard::monitor::{Clipboard, CLIPBOARD_ATTEMPTS, RETRY_DELAY};

/// How long to wait for the target application to answer Ctrl+C.
///
/// Generous, because the wait ends the moment the clipboard actually changes
/// and only runs out when there was no selection to copy. A tighter bound
/// makes an ordinary slow application look like an empty selection.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(400);

/// How often the clipboard is asked whether it has changed yet.
const POLL: Duration = Duration::from_millis(10);

/// Long enough for the target to have read the clipboard before it is put back.
///
/// The same race every paste in Sill has to lose deliberately: writing and
/// immediately restoring means the application pastes the restored value.
const SETTLE: Duration = Duration::from_millis(80);

/// A borrow held longer than this is named in the log when it ends.
///
/// Reading a selection is a few hundred milliseconds. Reading text aloud
/// holds the borrow for as long as the speech takes, and a copy made during
/// that is not recorded in the history; the line says which key did it.
const LONG_BORROW: Duration = Duration::from_secs(1);

/// A count of clipboard changes, which Windows increments for every one.
///
/// Comparing contents instead cannot tell "the copy produced the same text
/// that was already there" from "nothing was copied", and the first is
/// perfectly ordinary: selecting a word you just copied and running a
/// transform on it.
#[cfg(windows)]
pub(crate) fn sequence() -> u32 {
    // SAFETY: takes nothing, returns a counter, dereferences nothing.
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
pub(crate) fn sequence() -> u32 {
    0
}

/// The application in front, for a log line.
fn front() -> String {
    crate::dictation::context::foreground_app_full()
        .map(|app| app.name)
        .unwrap_or_default()
}

/// A Sill write to the clipboard, said out loud with the numbers that place
/// it against the user's own copies.
///
/// For writes that are not part of a borrow: a copy action, a screenshot,
/// a dictated transcript. Lengths and counts only; nothing that was written
/// passes through the log.
pub(crate) fn traced<T, E: std::fmt::Display>(
    reason: &str,
    write: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
    let before = sequence();
    let result = write();
    let who = front();
    match &result {
        Ok(_) => crate::say!(
            "clipboard write ({reason}): seq {before} -> {}, front {who}",
            sequence()
        ),
        Err(err) => crate::say!(
            "clipboard write ({reason}) failed at seq {before}, front {who}: {err}"
        ),
    }
    result
}

/// What a borrow does to the clipboard, and nothing else.
///
/// One real implementation and one in the tests. **There is no clear on
/// purpose**: emptying the clipboard is never restoring anything, and this is
/// the trait that makes it impossible to write by accident.
pub(crate) trait TextBoard {
    /// `Ok(None)` when there is no text (a picture, files, or nothing).
    /// `Err` when the clipboard could not be opened, which is worth retrying.
    fn text(&self) -> Result<Option<String>, String>;
    fn write_text(&self, text: &str) -> Result<(), String>;
    fn write_html(&self, html: &str, text: &str) -> Result<(), String>;
    fn sequence(&self) -> u32;
}

/// The real clipboard. `arboard` opens it per call and closes it when the
/// call returns, so nothing here holds a lock between calls.
pub(crate) struct SystemBoard;

impl TextBoard for SystemBoard {
    fn text(&self) -> Result<Option<String>, String> {
        let mut board = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        match board.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    fn write_text(&self, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut board| board.set_text(text.to_string()))
            .map_err(|e| e.to_string())
    }

    fn write_html(&self, html: &str, text: &str) -> Result<(), String> {
        arboard::Clipboard::new()
            .and_then(|mut board| board.set().html(html.to_string(), Some(text.to_string())))
            .map_err(|e| e.to_string())
    }

    fn sequence(&self) -> u32 {
        sequence()
    }
}

/// What giving the clipboard back actually did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Returned {
    /// The previous text is back.
    Restored,
    /// Nothing changed the clipboard during the borrow, so there was nothing
    /// to undo.
    NothingWritten,
    /// Somebody else wrote after Sill's last change. Theirs stays; Sill's
    /// result and the previous text are both gone, which is what they asked
    /// for by copying.
    LeftTheirs { theirs: u32 },
    /// There was no text before (a picture, files, nothing). Sill's result
    /// stays rather than the clipboard being emptied.
    NoTextBefore,
    /// The previous text could not be written back.
    CouldNotWrite,
}

/// The borrow, with no handle on the app: the part a test can drive.
///
/// `taken_at` is the sequence number when the borrow began and `ours` the
/// number after the last change that was Sill's doing, which includes the copy
/// a target application makes when Sill presses Ctrl+C in it. Both are what
/// [`Self::give_back`] reads to decide whether the previous text may go back.
pub(crate) struct Borrow {
    /// What was on the clipboard before Sill touched it, when it was text.
    previous: Option<String>,
    taken_at: u32,
    ours: AtomicU32,
}

impl Borrow {
    /// Reads what is there, waiting out a locked clipboard.
    ///
    /// Waited out rather than read once: a read that lost the lock used to
    /// come back as "nothing was there", and nothing was then "restored" by
    /// emptying the clipboard.
    pub(crate) fn take(board: &dyn TextBoard) -> Self {
        let mut previous = None;
        for attempt in 0..CLIPBOARD_ATTEMPTS {
            match board.text() {
                Ok(text) => {
                    previous = text;
                    break;
                }
                Err(err) if attempt + 1 == CLIPBOARD_ATTEMPTS => {
                    crate::say!(
                        "clipboard borrow: could not read what was there after \
                         {CLIPBOARD_ATTEMPTS} tries: {err}"
                    );
                }
                Err(_) => std::thread::sleep(RETRY_DELAY),
            }
        }

        let taken_at = board.sequence();
        Self {
            previous,
            taken_at,
            ours: AtomicU32::new(taken_at),
        }
    }

    pub(crate) fn previous(&self) -> Option<&str> {
        self.previous.as_deref()
    }

    pub(crate) fn taken_at(&self) -> u32 {
        self.taken_at
    }

    /// Records that the clipboard's current state is Sill's doing.
    ///
    /// Called after every write of ours and after a target application
    /// answers Sill's Ctrl+C. Without the second, every selection borrow
    /// would end in [`Returned::LeftTheirs`] and the person's text would
    /// never come back.
    pub(crate) fn mark(&self, board: &dyn TextBoard) {
        self.ours.store(board.sequence(), Ordering::SeqCst);
    }

    /// Writes, retrying the lock, and remembers the number the write left.
    pub(crate) fn write(&self, board: &dyn TextBoard, text: &str) -> bool {
        self.write_with(board, |board| board.write_text(text))
    }

    pub(crate) fn write_html(&self, board: &dyn TextBoard, html: &str, text: &str) -> bool {
        self.write_with(board, |board| board.write_html(html, text))
    }

    fn write_with(
        &self,
        board: &dyn TextBoard,
        write: impl Fn(&dyn TextBoard) -> Result<(), String>,
    ) -> bool {
        for attempt in 0..CLIPBOARD_ATTEMPTS {
            match write(board) {
                Ok(()) => {
                    self.mark(board);
                    return true;
                }
                Err(err) if attempt + 1 == CLIPBOARD_ATTEMPTS => {
                    crate::say!(
                        "could not write to the clipboard after {CLIPBOARD_ATTEMPTS} tries: {err}"
                    );
                    return false;
                }
                Err(_) => std::thread::sleep(RETRY_DELAY),
            }
        }
        false
    }

    /// Whether anything, by anybody, changed the clipboard since the borrow.
    pub(crate) fn touched(&self, board: &dyn TextBoard) -> bool {
        board.sequence() != self.taken_at
    }

    /// Puts the previous text back, when that is the right thing to do.
    pub(crate) fn give_back(self, board: &dyn TextBoard) -> Returned {
        let now = board.sequence();
        if now == self.taken_at {
            return Returned::NothingWritten;
        }
        if now != self.ours.load(Ordering::SeqCst) {
            return Returned::LeftTheirs { theirs: now };
        }
        let Some(previous) = self.previous.as_deref() else {
            return Returned::NoTextBefore;
        };

        // Not `write`: the restore is the end of the borrow, and nothing is
        // marked after it.
        let mut last = String::new();
        for attempt in 0..CLIPBOARD_ATTEMPTS {
            match board.write_text(previous) {
                Ok(()) => return Returned::Restored,
                Err(err) => last = err,
            }
            if attempt + 1 < CLIPBOARD_ATTEMPTS {
                std::thread::sleep(RETRY_DELAY);
            }
        }

        crate::say!("could not put the clipboard back after {CLIPBOARD_ATTEMPTS} tries: {last}");
        Returned::CouldNotWrite
    }
}

/// The user's clipboard, borrowed for the length of one operation.
///
/// Taken before anything runs and given back after everything has, so the
/// value that goes back is the one the user actually had rather than whatever
/// the last step happened to leave behind. Nothing in between is recorded in
/// the history, however many writes it takes, because the history is suspended
/// rather than told to expect a number.
///
/// A bound transform's result therefore does not appear in clipboard history.
/// That is deliberate: pressing a transform key twenty times should not leave
/// twenty entries nobody asked for.
///
/// **What a borrow cannot give back.** Only text is read and only text goes
/// back. When the clipboard held a picture or a list of files, the result of
/// the operation is left on it rather than the clipboard being emptied, and
/// the log says `NoTextBefore`. Formatting that sat beside the previous text
/// (HTML, RTF) is not restored either; the text is. And a copy the user makes
/// while the borrow is held stays on the clipboard but is not recorded in the
/// history, because the history is suspended for the borrow.
pub struct Held {
    app: AppHandle,
    borrow: Borrow,
    board: SystemBoard,
    since: Instant,
}

impl Held {
    /// Takes the clipboard, stopping the history until it is given back.
    pub fn take(app: &AppHandle) -> Self {
        // Before the read, because the listener fires on its own thread the
        // moment the contents change and a flag set afterwards is set too late.
        if let Some(history) = app.try_state::<Clipboard>() {
            history.suspend();
        }

        let board = SystemBoard;
        let borrow = Borrow::take(&board);
        crate::say!(
            "clipboard borrowed: seq {}, previous {}, front {}",
            borrow.taken_at(),
            borrow
                .previous()
                .map(|text| format!("{} chars", text.chars().count()))
                .unwrap_or_else(|| "none".to_string()),
            front()
        );

        Self {
            app: app.clone(),
            borrow,
            board,
            since: Instant::now(),
        }
    }

    /// What was on the clipboard before Sill touched it, when it was text.
    pub fn previous(&self) -> Option<&str> {
        self.borrow.previous()
    }

    /// Writes text, as part of the borrow.
    pub fn write(&self, text: &str, reason: &str) -> bool {
        let before = sequence();
        let ok = self.borrow.write(&self.board, text);
        crate::say!(
            "clipboard write ({reason}) while borrowed: seq {before} -> {}, {} chars, ok {ok}",
            sequence(),
            text.chars().count()
        );
        ok
    }

    /// Writes formatted text with its plain alternative, as part of the borrow.
    pub fn write_html(&self, html: &str, text: &str, reason: &str) -> bool {
        let before = sequence();
        let ok = self.borrow.write_html(&self.board, html, text);
        crate::say!(
            "clipboard write ({reason}) while borrowed: seq {before} -> {}, {} chars, ok {ok}",
            sequence(),
            text.chars().count()
        );
        ok
    }

    /// Whatever is selected in the foreground application.
    ///
    /// `None` when nothing is selected, when the application does not answer
    /// Ctrl+C, or when it answers with something that is not text.
    ///
    /// Leaves its copy on the clipboard, marked as Sill's doing: the target
    /// wrote it, but only because Sill asked, and the borrow's give-back has
    /// to know that or it would leave the copy there and never restore.
    pub fn capture(&self) -> Option<String> {
        let before = sequence();
        let who = front();

        if !crate::input::ctrl(crate::input::VK_C) {
            return None;
        }

        let deadline = Instant::now() + ANSWER_TIMEOUT;
        while sequence() == before {
            if Instant::now() >= deadline {
                // Nothing was selected, or the application ignores Ctrl+C.
                // Either way there is nothing to act on, and the clipboard
                // is untouched.
                crate::say!(
                    "selection copy: no answer from {who} in {} ms, seq {before} unchanged",
                    ANSWER_TIMEOUT.as_millis()
                );
                return None;
            }
            std::thread::sleep(POLL);
        }

        self.borrow.mark(&self.board);

        // The application that just answered is frequently still holding the
        // clipboard open, so this read is waited out like the one in `take`.
        let mut text = None;
        for attempt in 0..CLIPBOARD_ATTEMPTS {
            match self.board.text() {
                Ok(read) => {
                    text = read;
                    break;
                }
                Err(_) if attempt + 1 == CLIPBOARD_ATTEMPTS => {}
                Err(_) => std::thread::sleep(RETRY_DELAY),
            }
        }
        let text = text.filter(|text| !text.is_empty());

        crate::say!(
            "selection copy from {who}: seq {before} -> {}, {} chars",
            sequence(),
            text.as_ref().map(|text| text.chars().count()).unwrap_or(0)
        );
        text
    }

    /// Puts the original contents back and starts recording again.
    pub fn give_back(self) {
        let taken_at = self.borrow.taken_at();
        self.note_length();
        let returned = self.borrow.give_back(&self.board);
        crate::say!(
            "clipboard given back: {returned:?}, seq {taken_at} -> {}, front {}",
            sequence(),
            front()
        );
        finish(&self.app, matches!(returned, Returned::NothingWritten));
    }

    /// Starts recording again, leaving on the clipboard whatever is there.
    ///
    /// For an operation whose whole point was to put its result there.
    pub fn keep_result(self) {
        self.note_length();
        let untouched = !self.borrow.touched(&self.board);
        finish(&self.app, untouched);
    }

    fn note_length(&self) {
        let held = self.since.elapsed();
        if held >= LONG_BORROW {
            crate::say!("clipboard borrowed for {} ms", held.as_millis());
        }
    }
}

/// Ends the borrow's hold on the history.
///
/// The settle exists so the listener's late report of Sill's last write is
/// still swallowed. When nothing changed the clipboard there is nothing
/// late, and a typed snippet should not pay 80 ms for a write it never made.
fn finish(app: &AppHandle, untouched: bool) {
    if !untouched {
        std::thread::sleep(SETTLE);
    }

    if let Some(history) = app.try_state::<Clipboard>() {
        history.resume();
    }
}

/// Replaces the selection with `text`.
///
/// Leaves the result on the clipboard. The caller holds the borrow and is what
/// puts the original back; see [`Held`].
pub fn replace(held: &Held, text: &str) -> Result<(), String> {
    if !held.write(text, "transform result") {
        return Err("could not put the result on the clipboard".to_string());
    }

    std::thread::sleep(SETTLE);

    if !crate::input::ctrl(crate::input::VK_V) {
        return Err("that application would not accept the paste".to_string());
    }

    std::thread::sleep(SETTLE);
    Ok(())
}

/// Text a keyboard-driven action is about to run against.
///
/// Captured when a shortcut fires rather than held continuously, because the
/// only cheap way to read a selection is to press Ctrl+C in somebody else's
/// window, and doing that on a timer would be indefensible.
pub struct Captured {
    pub text: String,
    /// Where the text came from, which is not the same as where it was asked
    /// for. This is the only thing that distinguishes "the selection said
    /// HELLO" from "there was no selection so here is the clipboard", and the
    /// caller has to know which, because only one of them can be pasted back.
    pub from: Origin,
}

/// Where captured text actually came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Selection,
    Clipboard,
}

impl Captured {
    /// The selection, or the clipboard when there is no selection.
    ///
    /// The fallback is what makes a shortcut usable rather than fussy: a
    /// transform bound to a key should do something sensible when you press it
    /// with nothing highlighted, and the thing you last copied is the obvious
    /// candidate.
    ///
    /// **The fallback is only safe because [`Origin`] comes back with it.**
    /// Falling back and then pasting the result over the selection would take
    /// text the user never chose and write it into their document, destroying
    /// what was highlighted. That is not hypothetical: it is what happened the
    /// first time this ran against a real editor, before the capture worked.
    ///
    /// The fallback reads `held` rather than the clipboard, and has to: by
    /// this point Sill's own copy is sitting on the clipboard, so reading it
    /// again would return the selection rather than what the user last copied.
    pub fn selection_or_clipboard(held: &Held) -> Option<Self> {
        if let Some(text) = held.capture() {
            return Some(Self {
                text,
                from: Origin::Selection,
            });
        }

        Self::clipboard(held)
    }

    /// What is on the clipboard, whatever is selected.
    pub fn clipboard(held: &Held) -> Option<Self> {
        let text = held
            .previous()
            .map(str::to_owned)
            .filter(|text| !text.is_empty())?;

        Some(Self {
            text,
            from: Origin::Clipboard,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A clipboard that remembers what was done to it.
    struct Fake(Mutex<FakeState>);

    struct FakeState {
        text: Option<String>,
        seq: u32,
        writes: Vec<String>,
        read_failures: u32,
    }

    impl Fake {
        fn holding(text: Option<&str>) -> Self {
            Self(Mutex::new(FakeState {
                text: text.map(str::to_owned),
                seq: 10,
                writes: Vec::new(),
                read_failures: 0,
            }))
        }

        fn locked_for(text: Option<&str>, reads: u32) -> Self {
            let fake = Self::holding(text);
            fake.0.lock().unwrap().read_failures = reads;
            fake
        }

        /// Somebody other than Sill copies something.
        fn user_copies(&self, text: &str) {
            let mut state = self.0.lock().unwrap();
            state.seq += 1;
            state.text = Some(text.to_string());
        }

        fn writes(&self) -> Vec<String> {
            self.0.lock().unwrap().writes.clone()
        }

        fn text_now(&self) -> Option<String> {
            self.0.lock().unwrap().text.clone()
        }
    }

    impl TextBoard for Fake {
        fn text(&self) -> Result<Option<String>, String> {
            let mut state = self.0.lock().unwrap();
            if state.read_failures > 0 {
                state.read_failures -= 1;
                return Err("locked".to_string());
            }
            Ok(state.text.clone())
        }

        fn write_text(&self, text: &str) -> Result<(), String> {
            let mut state = self.0.lock().unwrap();
            state.seq += 1;
            state.text = Some(text.to_string());
            state.writes.push(text.to_string());
            Ok(())
        }

        fn write_html(&self, _html: &str, text: &str) -> Result<(), String> {
            self.write_text(text)
        }

        fn sequence(&self) -> u32 {
            self.0.lock().unwrap().seq
        }
    }

    #[test]
    fn giving_back_restores_the_previous_text_after_sills_own_write() {
        let fake = Fake::holding(Some("old"));
        let borrow = Borrow::take(&fake);

        assert!(borrow.write(&fake, "RESULT"));

        assert_eq!(borrow.give_back(&fake), Returned::Restored);
        assert_eq!(fake.text_now().as_deref(), Some("old"));
        assert_eq!(fake.writes(), vec!["RESULT", "old"]);
    }

    #[test]
    fn a_copy_somebody_else_made_during_the_borrow_is_left_alone() {
        let fake = Fake::holding(Some("old"));
        let borrow = Borrow::take(&fake);

        assert!(borrow.write(&fake, "RESULT"));
        fake.user_copies("theirs");

        assert_eq!(
            borrow.give_back(&fake),
            Returned::LeftTheirs { theirs: 12 },
            "the person copied something after Sill wrote, and that copy is theirs to keep"
        );
        assert_eq!(fake.text_now().as_deref(), Some("theirs"));
        assert_eq!(fake.writes(), vec!["RESULT"]);
    }

    #[test]
    fn no_text_before_means_nothing_is_written_back_and_nothing_is_emptied() {
        // A picture or a file list was on the clipboard. The old code called
        // this "restoring" and emptied the clipboard.
        let fake = Fake::holding(None);
        let borrow = Borrow::take(&fake);

        assert!(borrow.write(&fake, "RESULT"));

        assert_eq!(borrow.give_back(&fake), Returned::NoTextBefore);
        assert_eq!(fake.text_now().as_deref(), Some("RESULT"));
        assert_eq!(fake.writes(), vec!["RESULT"]);
    }

    #[test]
    fn nothing_written_means_nothing_is_restored() {
        let fake = Fake::holding(Some("old"));
        let borrow = Borrow::take(&fake);

        assert_eq!(borrow.give_back(&fake), Returned::NothingWritten);
        assert!(fake.writes().is_empty());
    }

    #[test]
    fn a_locked_clipboard_at_take_is_waited_out_rather_than_read_as_empty() {
        let fake = Fake::locked_for(Some("old"), 2);
        let borrow = Borrow::take(&fake);

        assert_eq!(borrow.previous(), Some("old"));
    }

    #[test]
    fn each_write_moves_the_mark() {
        let fake = Fake::holding(Some("old"));
        let borrow = Borrow::take(&fake);

        assert!(borrow.write(&fake, "a"));
        assert!(borrow.write(&fake, "b"));

        assert_eq!(borrow.give_back(&fake), Returned::Restored);
        assert_eq!(fake.text_now().as_deref(), Some("old"));
    }

    #[test]
    fn a_synthetic_copy_counts_as_ours_so_the_previous_text_still_comes_back() {
        // Sill presses Ctrl+C and the target application writes the
        // selection. That is the target's write and Sill's doing, and
        // `Held::capture` marks it. Without the mark every selection binding
        // would end in `LeftTheirs` and the person's text would never return.
        let fake = Fake::holding(Some("old"));
        let borrow = Borrow::take(&fake);

        fake.user_copies("selected");
        borrow.mark(&fake);
        assert!(borrow.write(&fake, "RESULT"));

        assert_eq!(borrow.give_back(&fake), Returned::Restored);
        assert_eq!(fake.text_now().as_deref(), Some("old"));
    }
}
