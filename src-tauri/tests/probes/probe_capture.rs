//! Whether screen capture actually takes a picture on this machine.
//!
//!     cargo test --test probe_capture -- --ignored --nocapture

use sill_lib::capture;

#[test]
#[ignore = "reads this machine's screens"]
fn takes_a_picture_of_the_screen() {
    let (left, top, width, height) = capture::virtual_screen();
    println!("the screens cover {width}x{height} from ({left}, {top})");
    assert!(width > 0 && height > 0, "no screen area was reported");

    let started = std::time::Instant::now();
    let shot = capture::region(left, top, width, height).expect("captures");
    let took = started.elapsed();

    println!(
        "captured {}x{} in {} ms, {} bytes",
        shot.width,
        shot.height,
        took.as_millis(),
        shot.pixels.len()
    );

    // A picture of a desk is never one flat colour. This is what catches a
    // capture that "worked" and came back as an empty buffer.
    let first = &shot.pixels[..4];
    let varied = shot.pixels.chunks_exact(4).any(|pixel| pixel != first);
    assert!(
        varied,
        "every pixel is identical, so nothing was actually copied"
    );

    let png = shot.to_png().expect("encodes");
    let out = std::env::temp_dir().join("sill-capture-probe.png");
    std::fs::write(&out, &png).expect("writes");
    println!("wrote {} ({} KB)", out.display(), png.len() / 1024);
}

/// The two halves together: capture the screen, then read the words on it.
#[test]
#[ignore = "reads this machine's screens"]
fn the_words_on_the_screen_can_be_read_back() {
    let (left, top, width, height) = capture::virtual_screen();
    let shot = capture::region(left, top, width, height).expect("captures");

    let started = std::time::Instant::now();
    let text = sill_lib::ocr::read_bgra(&shot.pixels, shot.width, shot.height).expect("reads");
    println!(
        "read the whole screen in {} ms",
        started.elapsed().as_millis()
    );

    let words: Vec<&str> = text.split_whitespace().take(12).collect();
    println!("first words found: {words:?}");

    assert!(
        !text.trim().is_empty(),
        "nothing was read off a screen that certainly has words on it",
    );
}
