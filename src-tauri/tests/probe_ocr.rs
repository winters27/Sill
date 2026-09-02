//! Whether text recognition actually reads words on this machine.
//!
//! Ignored by default: it needs a picture made outside the test, and it leans
//! on whatever recognition Windows has installed for the current languages.
//!
//!     pwsh -File scripts/make-ocr-fixture.ps1
//!     cargo test --test probe_ocr -- --ignored --nocapture

#[test]
#[ignore = "needs a fixture image and Windows text recognition"]
fn reads_the_words_it_was_given() {
    let path = std::env::temp_dir().join("sill-ocr-fixture.png");
    let png = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "run scripts/make-ocr-fixture.ps1 first ({}): {err}",
            path.display()
        )
    });

    let (pixels, width, height) = sill_lib::ocr::bgra_from_png(&png).expect("decodes");
    println!(
        "picture: {width}x{height}, {} bytes of pixels",
        pixels.len()
    );

    let started = std::time::Instant::now();
    let text = sill_lib::ocr::read_bgra(&pixels, width, height).expect("recognises");
    println!("read in {} ms", started.elapsed().as_millis());
    println!("got: {text:?}");

    let flat = text.to_lowercase().replace(' ', "");

    for wanted in ["quick", "brown", "fox", "12345"] {
        assert!(
            flat.contains(wanted),
            "{wanted:?} was not read out of the picture, got {text:?}",
        );
    }
}
