//! Whether formatting survives the trip through the clipboard.
//!
//! This is the half of a formatted snippet that can fail quietly. The paste
//! itself is a Ctrl+V, which the long-snippet path has been sending for as
//! long as there have been long snippets; what is new is writing two formats
//! in one go and expecting the receiving application to take its pick.
//!
//! Ignored by default: it writes to this machine's clipboard. It puts back
//! what it found.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_rich_snippet -- --ignored --nocapture
//! ```

#[test]
#[ignore = "writes to this machine's clipboard"]
fn both_formats_land_and_the_clipboard_is_put_back() {
    let mut board = arboard::Clipboard::new().expect("a clipboard");

    // Whatever is there now, so it can go back. Text only, which is what
    // every borrow in Sill puts back.
    let before = board.get_text().ok();
    println!(
        "clipboard held {:?} before",
        before.as_deref().map(|t| &t[..t.len().min(40)])
    );

    let html = "Heads up: <b>this is important</b>";
    let plain = "Heads up: this is important";

    board
        .set()
        .html(html.to_string(), Some(plain.to_string()))
        .expect("writes both formats");

    // The plain half, which is what a plain field receives. Read from a second
    // handle, because reading through the one that wrote is not the same as
    // another application reading it.
    let mut reader = arboard::Clipboard::new().expect("a second handle");
    let text = reader.get_text().expect("plain text is there");
    assert_eq!(
        text, plain,
        "the plain half is not what a plain field would get"
    );

    let markup = reader.get().html().expect("markup is there");
    println!("markup came back as {markup:?}");
    assert!(
        markup.contains("<b>this is important</b>"),
        "the formatting did not survive: {markup}",
    );

    // Put it back, the way `Held` does.
    match before {
        Some(text) => board.set_text(text).expect("puts it back"),
        // Nothing was there. Clearing is closer to that than leaving a
        // snippet somebody did not copy.
        None => board.clear().expect("clears"),
    }

    println!("clipboard put back");
}
