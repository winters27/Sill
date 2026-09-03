//! Putting an entry back on the clipboard.
//!
//! The monitor reads; this is the other direction. It exists as its own module
//! because deciding *what* an entry is when it goes back was being done twice
//! and disagreed with itself: the paste path looked for a picture, and the
//! registry's `Copy` action did not. An image row's text is a caption, so
//! `Copy` on a screenshot put the words "Image 1920x1080" on the clipboard and
//! the picture somebody had saved was gone.
//!
//! The decision and the writing are separate on purpose. The system clipboard
//! is one machine-wide resource, so a test that asserted what reached it would
//! be writing over whatever the person running the tests had copied. Splitting
//! them lets the assertion be made against a recorder instead, which is what
//! `an_image_row_puts_the_picture_on_the_clipboard` does.

use crate::clipboard::kind::Kind;
use crate::clipboard::store::Store;

/// What goes on the clipboard for a history entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Text(String),
    /// Formatted, with the plain text offered as the alternative.
    ///
    /// One write, not two: whichever the target understands it takes, and a
    /// plain field still gets sensible text rather than markup as literal
    /// characters.
    Rich {
        html: String,
        text: String,
    },
    /// PNG bytes.
    Image(Vec<u8>),
}

/// Anywhere a payload can be written.
///
/// A trait with one real implementation, for one reason: see the module note.
/// It is not an abstraction over clipboards, and nothing else should implement
/// it outside a test.
pub trait Board {
    fn put_text(&mut self, text: &str) -> Result<(), String>;
    fn put_html(&mut self, html: &str, text: &str) -> Result<(), String>;
    fn put_image(&mut self, png: &[u8]) -> Result<(), String>;
}

impl Board for arboard::Clipboard {
    fn put_text(&mut self, text: &str) -> Result<(), String> {
        self.set_text(text.to_string()).map_err(|e| e.to_string())
    }

    fn put_html(&mut self, html: &str, text: &str) -> Result<(), String> {
        self.set()
            .html(html.to_string(), Some(text.to_string()))
            .map_err(|e| e.to_string())
    }

    fn put_image(&mut self, png: &[u8]) -> Result<(), String> {
        self.set_image(decode_png(png)?).map_err(|e| e.to_string())
    }
}

/// What belongs on the clipboard for a history row.
///
/// `None` when the row is gone, which happens perfectly normally between
/// choosing it and acting on it.
///
/// `plain` drops the formatted alternative. It never drops the picture: "paste
/// as plain text" is about markup, and an image has none.
pub fn payload_for(store: &Store, id: i64, plain: bool) -> rusqlite::Result<Option<Payload>> {
    let Some(entry) = store.get(id)? else {
        return Ok(None);
    };

    if entry.kind == Kind::Image {
        // Before anything looks at the text. The text of an image row is a
        // caption Sill wrote, not something anybody copied.
        if let Some(png) = store.blob(id)? {
            return Ok(Some(Payload::Image(png)));
        }
        // No blob, so the picture was never kept: over the size limit, or the
        // write failed. The caption is then the only honest thing to hand
        // back, and the row already says so.
    }

    // Read only when it is going to be used. Markup is routinely several times
    // the size of the text it formats.
    if !plain && entry.rich {
        if let Some(html) = store.html(id)? {
            return Ok(Some(Payload::Rich {
                html,
                text: entry.text,
            }));
        }
    }

    Ok(Some(Payload::Text(entry.text)))
}

/// Writes a payload, whatever it turned out to be.
pub fn put(board: &mut dyn Board, payload: &Payload) -> Result<(), String> {
    match payload {
        Payload::Image(png) => board.put_image(png),
        Payload::Rich { html, text } => board.put_html(html, text),
        Payload::Text(text) => board.put_text(text),
    }
}

/// PNG bytes for a clipboard image.
pub fn encode_png(image: &arboard::ImageData<'_>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.bytes).ok()?;
    }
    Some(out)
}

/// The pixels behind a stored PNG.
fn decode_png(png: &[u8]) -> Result<arboard::ImageData<'static>, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(info.buffer_size());

    Ok(arboard::ImageData {
        width: info.width as usize,
        height: info.height as usize,
        bytes: buffer.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::store::Recording;

    const NOW: i64 = 1_700_000_000;

    /// A clipboard that remembers rather than one that exists.
    #[derive(Default)]
    struct Recorder {
        wrote: Vec<Payload>,
    }

    impl Board for Recorder {
        fn put_text(&mut self, text: &str) -> Result<(), String> {
            self.wrote.push(Payload::Text(text.to_string()));
            Ok(())
        }

        fn put_html(&mut self, html: &str, text: &str) -> Result<(), String> {
            self.wrote.push(Payload::Rich {
                html: html.to_string(),
                text: text.to_string(),
            });
            Ok(())
        }

        fn put_image(&mut self, png: &[u8]) -> Result<(), String> {
            self.wrote.push(Payload::Image(png.to_vec()));
            Ok(())
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(&dir.path().join("clipboard.db")).expect("opens");
        (dir, store)
    }

    /// A four-pixel picture, as the clipboard would hand one over.
    fn a_picture() -> Vec<u8> {
        encode_png(&arboard::ImageData {
            width: 2,
            height: 2,
            bytes: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255].into(),
        })
        .expect("encodes")
    }

    fn an_image_row(store: &Store, caption: &str, png: &[u8]) -> i64 {
        let id = store
            .record(Recording {
                hash: "pixels",
                kind: Kind::Image,
                text: caption,
                html: None,
                app: None,
                app_path: None,
                bytes: png.len() as i64,
                now: NOW,
            })
            .expect("records");
        store.put_blob(id, png).expect("stores");
        id
    }

    /// The bug this module exists for.
    ///
    /// Somebody screenshots something, opens the history, presses Copy, and
    /// pastes the word "Image". The row's text is a caption Sill wrote.
    #[test]
    fn an_image_row_puts_the_picture_on_the_clipboard() {
        let (_dir, store) = store();
        let png = a_picture();
        let id = an_image_row(&store, "Image 2x2", &png);

        let payload = payload_for(&store, id, false)
            .expect("reads")
            .expect("the row is there");
        let mut board = Recorder::default();
        put(&mut board, &payload).expect("writes");

        assert_eq!(
            board.wrote,
            vec![Payload::Image(png)],
            "the clipboard got something other than the picture"
        );
    }

    /// And nothing resembling the caption reaches it.
    ///
    /// Separate from the assertion above because a future payload that carried
    /// both would still satisfy it, and pasting a caption beside a picture is
    /// the same complaint.
    #[test]
    fn the_caption_of_an_image_row_never_reaches_the_clipboard() {
        let (_dir, store) = store();
        let id = an_image_row(&store, "Image 2x2", &a_picture());

        let payload = payload_for(&store, id, false)
            .expect("reads")
            .expect("the row is there");
        let mut board = Recorder::default();
        put(&mut board, &payload).expect("writes");

        for written in &board.wrote {
            let text = match written {
                Payload::Text(text) => text.clone(),
                Payload::Rich { text, .. } => text.clone(),
                Payload::Image(_) => continue,
            };
            assert!(
                !text.contains("Image"),
                "the caption was written as text: {text:?}"
            );
        }
    }

    /// An image whose pixels were never kept has only its caption left.
    ///
    /// Over the size limit, or a write that failed. The row already says so,
    /// and refusing to copy anything would be worse than copying the label.
    #[test]
    fn an_image_row_with_no_picture_behind_it_falls_back_to_its_caption() {
        let (_dir, store) = store();
        let id = store
            .record(Recording {
                hash: "too-big",
                kind: Kind::Image,
                text: "Image 9000x9000",
                html: None,
                app: None,
                app_path: None,
                bytes: 400_000_000,
                now: NOW,
            })
            .expect("records");

        assert_eq!(
            payload_for(&store, id, false).expect("reads"),
            Some(Payload::Text("Image 9000x9000".to_string()))
        );
    }

    /// Text still goes back as text, and formatting still goes with it.
    #[test]
    fn text_goes_back_as_itself_and_keeps_its_formatting() {
        let (_dir, store) = store();
        let id = store
            .record(Recording {
                hash: "words",
                kind: Kind::Text,
                text: "hello world",
                html: Some("<b>hello world</b>"),
                app: None,
                app_path: None,
                bytes: 11,
                now: NOW,
            })
            .expect("records");

        assert_eq!(
            payload_for(&store, id, false).expect("reads"),
            Some(Payload::Rich {
                html: "<b>hello world</b>".to_string(),
                text: "hello world".to_string(),
            })
        );

        assert_eq!(
            payload_for(&store, id, true).expect("reads"),
            Some(Payload::Text("hello world".to_string())),
            "asking for plain text still has to drop the markup"
        );
    }

    /// "Paste as plain text" is about markup and must not drop the picture.
    #[test]
    fn asking_for_plain_text_does_not_turn_a_picture_into_its_caption() {
        let (_dir, store) = store();
        let png = a_picture();
        let id = an_image_row(&store, "Image 2x2", &png);

        assert_eq!(
            payload_for(&store, id, true).expect("reads"),
            Some(Payload::Image(png))
        );
    }

    /// A row deleted between being chosen and being acted on.
    #[test]
    fn a_row_that_is_gone_says_so_rather_than_erroring() {
        let (_dir, store) = store();
        assert_eq!(payload_for(&store, 404, false).expect("reads"), None);
    }

    /// The pixels survive the round trip through storage.
    #[test]
    fn the_pixels_come_back_the_way_they_went_in() {
        let bytes = vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255];
        let png = encode_png(&arboard::ImageData {
            width: 2,
            height: 2,
            bytes: bytes.clone().into(),
        })
        .expect("encodes");

        let back = decode_png(&png).expect("decodes");
        assert_eq!(back.width, 2);
        assert_eq!(back.height, 2);
        assert_eq!(back.bytes.as_ref(), bytes.as_slice());
    }
}
