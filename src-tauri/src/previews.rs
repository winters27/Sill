//! Pictures of the windows you are switching between, and a look inside the
//! file under the cursor.
//!
//! A list of window titles tells you which application, and often not which
//! window: four browser windows are four rows reading almost the same. A
//! picture answers in one glance what a title cannot answer at all.
//!
//! ## What this costs, and when
//!
//! Nothing at rest, and nothing at all unless the switcher is open. A window
//! is captured when it is the one selected, never in a batch: opening the
//! switcher on twenty windows must not photograph twenty windows.
//!
//! Each picture is made small **before** it is encoded, because encoding is
//! the expensive half and a full-size window is four million pixels nobody is
//! going to look at.
//!
//! ## Why they are kept, and why not for long
//!
//! Arrowing down a list and back up again asks for the same window twice a
//! second, and capturing it twice is work for a picture that has not changed.
//! So a handful are kept, and the whole lot is dropped when the switcher
//! closes: a preview is a picture of a moment, and the moment ends.

use std::collections::HashMap;
use std::sync::Mutex;

/// How many pictures are kept while the switcher is open.
///
/// Enough that arrowing up and down a list does not re-photograph anything,
/// few enough that the memory is a few megabytes at most. Each is a small
/// PNG, not a window's worth of pixels.
const KEPT: usize = 12;

/// The longer side of a preview, in pixels.
///
/// The strip it is drawn in is a few hundred pixels wide and this is drawn
/// into it, so there is room for a display that is not at 100% without there
/// being room for a megabyte of picture nobody sees.
const LONGEST_SIDE: i32 = 480;

/// How many file previews are kept, on the same reasoning as [`KEPT`].
const KEPT_FILES: usize = 12;

/// The largest picture worth showing in a strip a few hundred pixels wide.
///
/// The bytes are handed through as they are, because there is no image decoder
/// in this build and adding one to shrink a picture the webview is about to
/// shrink anyway would be a dependency for nothing. That makes the file's own
/// size the IPC payload, plus a third for the base64, so the ceiling is the
/// payload ceiling: **two megabytes on the wire is already more than a preview
/// is worth**, and a photograph straight off a camera is past it.
const MOST_PICTURE: u64 = 2 * 1024 * 1024;

/// How much of a text file is read.
///
/// Enough to fill a strip several times over and nothing like enough to matter.
/// A log file is gigabytes and a preview of it is the first screenful; reading
/// the rest would be reading somebody's whole disk to show them eight lines.
const MOST_TEXT: usize = 8 * 1024;

/// Kinds of file that never have anything to show.
///
/// An exception list rather than a list of what may be previewed, because a
/// file with no extension at all, a `.gitignore`, a `.conf` and a `.editorconfig`
/// are all text worth a glance and no list of permitted extensions would have
/// all of them. What is written down here is the opposite: the kinds that are
/// certainly not text and certainly not a picture the strip can draw.
///
/// **This is a measurement, not tidiness.** Measured over a hundred real files
/// of each kind on this machine, a binary that shows nothing cost a mean of 22
/// ms and a worst of 374 ms, all of it a cold read of a file Sill then threw
/// away. Everything on this list is refused before anything is opened, so that
/// whole class costs one look at the name. It also means the launcher does not
/// open somebody's archives and videos at all.
///
/// `pdf` is here. A first page would be the one genuinely useful preview on
/// the list, and rendering one means a PDF engine: `pdfium` is a ten megabyte
/// native library to ship and load, and the pure-Rust readers parse the file
/// structure without rasterising a page. Ten megabytes and a second dynamic
/// library, in a launcher whose whole argument is that it costs nothing at
/// rest, is not worth a thumbnail of a first page. Refused with a reason
/// rather than half-built.
const NOTHING_TO_SHOW: &[&str] = &[
    // Programs and their parts.
    "exe", "dll", "sys", "msi", "pdb", "obj", "lib", "so", "dylib", "class", "pyc", "wasm",
    // Archives and disk images.
    "zip", "7z", "rar", "gz", "bz2", "xz", "tar", "cab", "iso", "vhd", "vhdx", "dmg", "pkg", "jar",
    "nupkg", "whl", "crx", "appx", "msix", // Sound and moving pictures.
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "mp4", "mkv", "mov", "avi", "webm", "wmv",
    "m4v", // Documents that are archives or containers rather than text.
    "pdf", "docx", "xlsx", "pptx", "doc", "xls", "ppt", "odt", "ods", "odp", "epub",
    // Databases and other opaque blobs.
    "sqlite", "sqlite3", "db", "mdb", "accdb", "bin", "dat", "pack", "idx", "ttf", "otf", "woff",
    "woff2", "psd", "ai", "blend", "fbx",
];

/// Whether a name says outright that there is nothing to show.
fn plainly_nothing(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let ext = ext.to_ascii_lowercase();
            NOTHING_TO_SHOW.contains(&ext.as_str())
        })
}

/// What a file turned out to look like.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    /// `image` or `text`.
    pub kind: &'static str,
    /// A data URI for a picture, the text itself for text.
    pub body: String,
    /// Whether there is more of it than this.
    pub more: bool,
}

/// Pictures of windows, for as long as the switcher is open, and looks inside
/// files, for as long as the list showing them is.
#[derive(Default)]
pub struct Previews {
    inner: Mutex<HashMap<isize, String>>,
    /// What was found inside a file, by its path.
    ///
    /// `None` is remembered as well as a look, so a file that has nothing to
    /// show is not opened again every time the cursor passes back over it.
    /// Arrowing down a list and back up is the case this whole cache exists
    /// for, and it is the case where the answer is most often nothing.
    files: Mutex<HashMap<String, Option<Look>>>,
}

impl Previews {
    pub fn new() -> Self {
        Self::default()
    }

    /// A look inside one file, reading it if there is no look already.
    ///
    /// `None` when there is nothing worth showing, which is most files: an
    /// executable, an archive, a picture too big to be worth sending, a folder,
    /// or a file whose bytes are still in somebody's cloud. None of those are
    /// errors and none of them deserve a message.
    pub fn of_file(&self, path: &str) -> Option<Look> {
        if let Ok(kept) = self.files.lock() {
            if let Some(already) = kept.get(path) {
                return already.clone();
            }
        }

        let found = look_at(std::path::Path::new(path));

        if let Ok(mut kept) = self.files.lock() {
            // Bounded the same crude way, and for the same reason: the list
            // closes in seconds and takes the whole lot with it, so an eviction
            // policy would be machinery for a case that does not arise.
            if kept.len() >= KEPT_FILES {
                kept.clear();
            }

            kept.insert(path.to_string(), found.clone());
        }

        found
    }

    /// Drops every look inside a file.
    ///
    /// Called when the list showing them goes away, which includes the window
    /// being hidden. A preview holds up to two megabytes of somebody's picture,
    /// and a launcher sitting hidden with twelve of those resident is the
    /// finding `P2-06` closed arriving again by another door.
    pub fn forget_files(&self) {
        if let Ok(mut kept) = self.files.lock() {
            kept.clear();
        }
    }

    /// A picture of one window, as a data URI, taking one if there is none.
    ///
    /// `None` when the window has closed or refuses to be photographed, which
    /// is not an error: a switcher with no picture is a switcher, and a
    /// message about it would be about Sill rather than about the window.
    pub fn of(&self, allowed: &crate::privacy::Allowed, id: isize) -> Option<String> {
        if let Ok(kept) = self.inner.lock() {
            if let Some(already) = kept.get(&id) {
                return Some(already.clone());
            }
        }

        let taken = take(allowed, id)?;

        if let Ok(mut kept) = self.inner.lock() {
            // Bounded, and crudely. There is no order worth keeping here: the
            // switcher closes in seconds and takes the whole lot with it, so
            // an eviction policy would be machinery for a case that does not
            // arise.
            if kept.len() >= KEPT {
                kept.clear();
            }

            kept.insert(id, taken.clone());
        }

        Some(taken)
    }

    /// Drops every picture.
    ///
    /// Called when the switcher closes. A preview is a picture of a moment,
    /// and keeping them would mean showing somebody a window as it was the
    /// last time they looked rather than as it is.
    pub fn forget(&self) {
        if let Ok(mut kept) = self.inner.lock() {
            kept.clear();
        }
    }
}

/// Reads enough of a file to show what it is.
///
/// # What it is allowed to open
///
/// The path that arrived, and nothing else. This is somebody's own file and
/// the only reason to be reading it is that they put the cursor on it.
///
/// A **placeholder is refused before anything is opened**. OneDrive and every
/// other provider on the same Windows feature leave an entry that looks like a
/// file and holds none of the bytes, and reading one downloads it. Doing that
/// for a preview would spend somebody's connection on a picture they get a
/// glance at, for a file they only moved the cursor past. The rule is
/// `icons::wants_recall`, which is the same rule the icon path uses, asked of
/// metadata this already has rather than fetched a second time.
fn look_at(path: &std::path::Path) -> Option<Look> {
    look_unless(path, is_elsewhere)
}

/// The same, with the cloud question handed in.
///
/// Split out because the attribute that says a file's bytes are somewhere else
/// cannot be set on a temporary file without a cloud provider signed in on the
/// machine, so a test of the real thing would be a test that runs nowhere and
/// proves nothing. Asking the question through a parameter means a test can
/// hand over a perfectly ordinary readable file, say it is a placeholder, and
/// find out whether this really refuses to open it.
///
/// The same shape, and for the same reason, as `catalog::answers_within`: its
/// own note says a test that cannot fail is worse than no test.
fn look_unless(
    path: &std::path::Path,
    held_elsewhere: impl Fn(&std::fs::Metadata) -> bool,
) -> Option<Look> {
    use std::io::Read;

    // Before the disk is touched at all. See [`NOTHING_TO_SHOW`]: this is the
    // difference between a class of file costing a look at its name and a
    // class of file costing a cold read that is then thrown away.
    if plainly_nothing(path) {
        return None;
    }

    let about = std::fs::metadata(path).ok()?;

    // A folder has no contents to show that the list is not already showing,
    // and a file whose bytes are in somebody's cloud is not opened at all.
    if about.is_dir() || held_elsewhere(&about) {
        return None;
    }

    // The head only. A picture is read whole below, once its type is known and
    // its size has been found acceptable; everything else stops here, so a two
    // gigabyte log costs eight kilobytes.
    let mut head = vec![0u8; MOST_TEXT];
    let mut file = std::fs::File::open(path).ok()?;
    let read = file.read(&mut head).ok()?;
    head.truncate(read);

    if head.is_empty() {
        return None;
    }

    if let Some(mime) = crate::ai::files::image_type(&head) {
        if about.len() > MOST_PICTURE {
            return None;
        }

        // Read again from the start rather than continued, because the head is
        // already in hand and joining two buffers to save one small read of a
        // file the operating system has just cached is not worth the code.
        let whole = std::fs::read(path).ok()?;

        return Some(Look {
            kind: "image",
            body: format!("data:{mime};base64,{}", base64_of(&whole)),
            more: false,
        });
    }

    let text = String::from_utf8_lossy(&head);

    if !reads_as_text(&text) {
        return None;
    }

    // Cut at a line ending, so the last line of a preview is a line rather
    // than however many characters happened to fit.
    let body = match text.rfind('\n') {
        Some(end) if read as u64 == MOST_TEXT as u64 => text[..end].to_string(),
        _ => text.into_owned(),
    };

    Some(Look {
        kind: "text",
        body,
        more: about.len() > read as u64,
    })
}

/// Whether the bytes read are text rather than something that is not.
///
/// The same test the model attachments use, plus a NUL. A run of replacement
/// characters means the bytes were never text; a NUL means it outright, and an
/// executable's first eight kilobytes hold plenty of both while still holding
/// enough ASCII to slip past the ratio on its own.
fn reads_as_text(text: &str) -> bool {
    if text.contains('\0') {
        return false;
    }

    let noise = text
        .chars()
        .filter(|c| *c == char::REPLACEMENT_CHARACTER)
        .count();

    noise <= text.chars().count() / 20
}

/// Whether the bytes are in somebody's cloud rather than here.
#[cfg(windows)]
fn is_elsewhere(about: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    crate::icons::wants_recall(about.file_attributes())
}

#[cfg(not(windows))]
fn is_elsewhere(_about: &std::fs::Metadata) -> bool {
    false
}

/// Photographs one window, small, as a data URI.
///
/// Takes the permission rather than asking for it, because it is called from a
/// blocking task with no `AppHandle` to hand. The asking is at the top of
/// `preview`, once, which is also where it belongs: a switcher in private mode
/// should show no pictures at all rather than one per arrow key that each
/// separately fails.
fn take(allowed: &crate::privacy::Allowed, id: isize) -> Option<String> {
    // Revalidated rather than trusted. A handle can be reused by a different
    // window once the first one closes, and photographing a stranger is worse
    // than showing nothing.
    let window = crate::windowing::find(id)?;

    // A minimized window has nothing on screen to photograph and `PrintWindow`
    // on one gives back an empty rectangle or the desktop behind it. Saying
    // there is no picture is honest; showing a grey box is not.
    if window.minimized {
        return None;
    }

    let shot = crate::capture::window(
        allowed,
        id,
        (
            window.rect.x,
            window.rect.y,
            window.rect.width,
            window.rect.height,
        ),
    )
    .ok()?;

    let png = crate::capture::thumbnail(&shot, LONGEST_SIDE)
        .to_png()
        .ok()?;

    Some(format!("data:image/png;base64,{}", base64_of(&png),))
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight bytes that make a file a PNG, and a little after them.
    fn a_png() -> Vec<u8> {
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        out.extend_from_slice(b"not really, but the type is read from the head");
        out
    }

    #[test]
    fn a_text_file_becomes_its_first_lines() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("notes.md");
        std::fs::write(&at, "# A heading\n\nAnd a paragraph under it.\n").expect("write");

        let look = look_at(&at).expect("a text file has something to show");

        assert_eq!(look.kind, "text");
        assert!(look.body.starts_with("# A heading"), "{look:?}");
        assert!(!look.more, "the whole file was read");
    }

    #[test]
    fn a_long_text_file_is_cut_rather_than_read_whole() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("server.log");

        // Well past the ceiling, in lines, so the cut lands on a line ending.
        let line = "2026-09-03 12:00:00 something happened again\n";
        std::fs::write(&at, line.repeat(2_000)).expect("write");

        let look = look_at(&at).expect("a log file has something to show");

        assert!(
            look.body.len() <= MOST_TEXT,
            "read {} bytes, which is past the ceiling",
            look.body.len()
        );
        assert!(look.more, "a file that was cut should say so");
        assert!(
            !look.body.ends_with('\n') && look.body.ends_with("again"),
            "the preview should end on a whole line, not mid-way through one"
        );
    }

    #[test]
    fn a_picture_becomes_a_data_uri() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("holiday.png");
        std::fs::write(&at, a_png()).expect("write");

        let look = look_at(&at).expect("a picture has something to show");

        assert_eq!(look.kind, "image");
        assert!(look.body.starts_with("data:image/png;base64,"), "{look:?}");
    }

    /// The bytes decide, not the name.
    ///
    /// A JPEG called `.png` is common enough, and the strip handed the wrong
    /// type draws a broken image rather than a picture.
    #[test]
    fn the_kind_comes_from_the_bytes_not_the_name() {
        let dir = tempfile::tempdir().expect("temp dir");

        let lying = dir.path().join("holiday.png");
        std::fs::write(&lying, b"\xff\xd8\xffand then some").expect("write");
        assert_eq!(
            look_at(&lying).expect("a picture").body[..15].to_string(),
            "data:image/jpeg"
        );

        let plain = dir.path().join("picture.png");
        std::fs::write(&plain, "this is text with a picture's name\n").expect("write");
        assert_eq!(look_at(&plain).expect("text").kind, "text");
    }

    #[test]
    fn a_picture_too_big_to_send_is_not_sent() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("enormous.png");

        let mut bytes = a_png();
        bytes.resize(MOST_PICTURE as usize + 1, 0);
        std::fs::write(&at, bytes).expect("write");

        assert!(
            look_at(&at).is_none(),
            "a picture past the ceiling was read and encoded anyway"
        );
    }

    /// A kind that can never show anything is refused without being opened.
    ///
    /// The measurement that put this here: over a hundred real binaries on this
    /// machine, opening each to discover it was binary cost a mean of 22 ms and
    /// a worst of 374 ms, entirely in cold reads that were then thrown away.
    /// Sabotage this by taking the name check out of `look_at` and the cost
    /// comes straight back.
    #[test]
    fn a_kind_that_can_never_show_anything_is_not_opened() {
        for name in [
            "setup.exe",
            "library.DLL",
            "photos.zip",
            "manual.pdf",
            "report.docx",
            "music.flac",
            "clip.mp4",
            "font.woff2",
        ] {
            assert!(
                plainly_nothing(std::path::Path::new(name)),
                "{name} would have been opened to find out it shows nothing"
            );
        }

        // And the list must not swallow anything that is worth a glance. A
        // file with no extension at all and a name that is nothing but one are
        // both text people keep.
        for name in [
            "notes.md",
            "holiday.png",
            "Makefile",
            ".gitignore",
            "server.log",
            "config.toml",
            "index.ts",
        ] {
            assert!(
                !plainly_nothing(std::path::Path::new(name)),
                "{name} was refused a preview by its name"
            );
        }

        /*
         * And the rule is actually consulted, not merely written down.
         *
         * A file of plain text called `manual.pdf`. The bytes say text and the
         * name says PDF, and the name has to win: if `look_at` reached the byte
         * check at all it would show the text, which means it opened the file,
         * which is the whole cost this list exists to avoid. Taking the name
         * check out of `look_at` fails here and nowhere else.
         */
        let dir = tempfile::tempdir().expect("temp dir");
        let lying = dir.path().join("manual.pdf");
        std::fs::write(&lying, "this is plain text wearing a PDF's name\n").expect("write");

        assert!(
            look_at(&lying).is_none(),
            "a file the name said had nothing to show was opened anyway"
        );
    }

    #[test]
    fn something_that_is_neither_shows_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");

        /*
         * A NUL is the giveaway a program's pages always carry, and the one a
         * ratio of replacement characters misses because the rest of the bytes
         * are ASCII enough to pass it.
         *
         * **Not called `.exe`.** It was, and that made this test unable to
         * fail: the name list refuses an executable before a byte is read, so
         * removing the NUL rule entirely changed nothing here. A sabotage of
         * the rule found that rather than reading finding it. A blob whose
         * extension nothing recognises is the only kind of file that reaches
         * the rule at all.
         */
        let blob = dir.path().join("dump.qqq");
        std::fs::write(&blob, b"HEADER\x00\x00\x00\x01record one\x00record two\x00")
            .expect("write");
        assert!(look_at(&blob).is_none(), "a binary blob was previewed");

        let empty = dir.path().join("empty.txt");
        std::fs::write(&empty, b"").expect("write");
        assert!(look_at(&empty).is_none(), "an empty file was previewed");

        assert!(
            look_at(dir.path()).is_none(),
            "a folder was opened as if it were a file"
        );

        assert!(look_at(&dir.path().join("not-there.txt")).is_none());
    }

    /// A file whose bytes are still in somebody's cloud is not touched.
    ///
    /// Reading a placeholder downloads it, over a connection nobody chose to
    /// spend, for a glance at a row somebody arrowed past.
    ///
    /// The file here is an ordinary readable one and the answer is handed in,
    /// which is the only way to make this test able to fail: the attribute
    /// cannot be set on a temporary file without a cloud provider signed in,
    /// and a test that only runs on a machine with OneDrive is a test that runs
    /// nowhere. Saying yes about a file that plainly has something to show
    /// proves the guard is consulted rather than merely written.
    #[test]
    fn a_file_that_is_not_really_here_is_not_opened() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("notes.md");
        std::fs::write(&at, "a file with plenty to show\n").expect("write");

        assert!(
            look_unless(&at, |_| false).is_some(),
            "the file has something to show when it is really here"
        );

        assert!(
            look_unless(&at, |_| true).is_none(),
            "a placeholder was opened, which downloads it"
        );
    }

    /// And the rule itself is the one the icon path already uses.
    #[test]
    fn the_two_attributes_that_mean_it_is_not_here() {
        const RECALL_ON_OPEN: u32 = 0x0004_0000;
        const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
        const ORDINARY: u32 = 0x0000_0080;

        assert!(crate::icons::wants_recall(RECALL_ON_OPEN));
        assert!(crate::icons::wants_recall(RECALL_ON_DATA_ACCESS));
        assert!(!crate::icons::wants_recall(ORDINARY));
    }

    /// The look is kept, so arrowing back onto a row does not open it again.
    #[test]
    fn a_file_is_opened_once_however_often_the_cursor_returns() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("notes.md");
        std::fs::write(&at, "the first reading\n").expect("write");

        let previews = Previews::new();
        let path = at.to_string_lossy().into_owned();

        let first = previews.of_file(&path).expect("a look");

        // Changed underneath, which the cache must not notice: it is a picture
        // of a moment, and the moment is the one the cursor arrived at.
        std::fs::write(&at, "a second reading\n").expect("write");
        assert_eq!(previews.of_file(&path), Some(first));

        // And the whole lot goes when the list does.
        previews.forget_files();
        assert_eq!(
            previews.of_file(&path).map(|look| look.body),
            Some("a second reading\n".to_string()),
            "the cache was not dropped"
        );
    }

    /// A file with nothing to show is remembered as having nothing.
    ///
    /// Otherwise arrowing up and down a list of them opens every one again on
    /// every pass, which is the case the cache exists for and the case where
    /// the answer is most often nothing.
    ///
    /// The fixture is deliberately not called `.exe`. It was, and that made
    /// this test unable to fail: the name list refuses an executable without
    /// ever consulting the cache, so the second answer was nothing whether or
    /// not the first had been remembered.
    #[test]
    fn a_file_with_nothing_to_show_is_not_opened_twice() {
        let dir = tempfile::tempdir().expect("temp dir");
        let at = dir.path().join("dump.qqq");
        std::fs::write(&at, b"HEADER\x00\x00\x00\x01record\x00").expect("write");

        let previews = Previews::new();
        let path = at.to_string_lossy().into_owned();

        assert!(previews.of_file(&path).is_none());

        // Now readable, and it must still answer nothing: the answer was kept.
        std::fs::write(&at, "plain text now\n").expect("write");
        assert!(
            previews.of_file(&path).is_none(),
            "the file was opened a second time"
        );
    }

    #[test]
    fn the_cache_is_bounded() {
        let dir = tempfile::tempdir().expect("temp dir");
        let previews = Previews::new();

        for at in 0..KEPT_FILES * 3 {
            let file = dir.path().join(format!("note{at}.md"));
            std::fs::write(&file, format!("file number {at}\n")).expect("write");
            previews.of_file(&file.to_string_lossy());
        }

        let held = previews.files.lock().expect("the cache").len();

        assert!(
            held <= KEPT_FILES,
            "{held} previews are being held, which is past the bound"
        );
    }
}
