//! What Sill will do on an extension's behalf.
//!
//! Extensions do not touch the clipboard, launch applications or raise
//! dialogs. They ask, and this is the whole list of things they may ask for.
//!
//! A trait rather than an `AppHandle` for two reasons. The API layer stays
//! testable without a running application, which is how the protocol was
//! written in the first place. And every capability an extension can reach is
//! named in one place: when permissions arrive, this is the list they get
//! declared against, rather than a search through the codebase for everything
//! an extension turned out to be able to do.

use serde::Serialize;

/// Something an extension put on, or wants from, the clipboard.
///
/// Raycast's `Clipboard.Content` is a string or an object with `text`, `html`
/// or `file`, and the caller may set any combination. Modelled as options
/// rather than an enum because a copy legitimately carries both plain text and
/// its HTML form, and the receiving application picks.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Clip {
    pub text: Option<String>,
    pub html: Option<String>,
    pub file: Option<String>,
    /// Raycast's `concealed`. A password or token that clipboard history must
    /// not keep.
    ///
    /// Honouring this is not politeness. An extension that copies a secret and
    /// says so, into a launcher that records everything copied, has written
    /// that secret to disk in plain text unless something acts on this flag.
    pub concealed: bool,
}

impl Clip {
    /// The plain text form, whatever the caller supplied it as.
    pub fn as_text(&self) -> Option<&str> {
        self.text
            .as_deref()
            .or(self.html.as_deref())
            .or(self.file.as_deref())
    }
}

/// An installed application, shaped as `@raycast/api` documents it.
///
/// `bundle_id` is a macOS concept with no Windows equivalent, so it is always
/// absent. Present in the shape because extensions read it, and a missing key
/// is what their `?.` guards expect; inventing a value would be worse.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub path: String,
    pub bundle_id: Option<String>,
}

/// A question an extension wants answered before it does something.
#[derive(Debug, Clone)]
pub struct Alert {
    pub title: String,
    pub message: Option<String>,
    /// Label for the button that means yes.
    pub primary: Option<String>,
    /// Label for the button that means no.
    pub dismiss: Option<String>,
    /// Whether the action being confirmed destroys something.
    pub destructive: bool,
}

/// The capabilities an extension can reach, and its only route to them.
pub trait Bridge: Send + Sync {
    fn clipboard_write(&self, clip: &Clip) -> Result<(), String>;
    fn clipboard_read(&self) -> Result<Clip, String>;
    fn clipboard_clear(&self) -> Result<(), String>;
    /// Writes, then puts the launcher away and sends the paste chord.
    fn clipboard_paste(&self, clip: &Clip) -> Result<(), String>;
    /// Opens a URL or a path, optionally in a named application.
    fn open(&self, target: &str, with: Option<&str>) -> Result<(), String>;
    /// Installed applications, from the index the launcher already holds.
    fn applications(&self) -> Result<Vec<AppInfo>, String>;
    /// Blocks until the user answers.
    fn confirm(&self, alert: &Alert) -> Result<bool, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_prefers_plain_text_but_will_settle_for_what_it_has() {
        // An extension that copies only HTML still has something a text-only
        // consumer can use, and refusing it would be worse than degrading.
        let html = Clip {
            html: Some("<b>hi</b>".into()),
            ..Clip::default()
        };
        assert_eq!(html.as_text(), Some("<b>hi</b>"));

        let both = Clip {
            text: Some("hi".into()),
            html: Some("<b>hi</b>".into()),
            ..Clip::default()
        };
        assert_eq!(both.as_text(), Some("hi"), "plain text wins when present");

        assert_eq!(Clip::default().as_text(), None);
    }
}
