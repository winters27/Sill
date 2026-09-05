//! Against the imaging codecs this machine actually has.
//!
//! The fixtures in `images.rs` cover the naming and the two refusals. What
//! they cannot say is whether Windows accepts the pipeline: a decoder built
//! from a stream, a bitmap converted to a format the encoder takes, and
//! bytes that come back out as a real file. That is what this asks, with a
//! picture it draws itself so nothing has to be committed.

use crate::images::{convert, Format};

/// A tiny PNG, written with the encoder the clipboard already uses.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();

    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header().expect("writes a header");

        // A diagonal, so the picture has something in it that survives a
        // round trip and is not all one colour.
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let on = x == y;
                pixels.extend_from_slice(&[
                    if on { 0 } else { 255 },
                    if on { 0 } else { 255 },
                    if on { 0 } else { 255 },
                    255,
                ]);
            }
        }

        writer.write_image_data(&pixels).expect("writes the pixels");
    }

    out
}

#[cfg(windows)]
#[test]
fn a_png_becomes_a_jpeg_and_a_jpeg_becomes_a_png() {
    let dir = tempfile::tempdir().expect("a temp directory");
    let source = dir.path().join("diagonal.png");
    std::fs::write(&source, png(64, 64)).expect("writes the fixture");

    // Out to JPEG.
    let jpeg = convert(&source, Format::Jpeg).expect("converts to JPEG");
    assert_eq!(jpeg.extension().and_then(|e| e.to_str()), Some("jpg"));
    assert!(jpeg.is_file(), "the JPEG was not written");
    assert!(source.is_file(), "the original was not left alone");

    // A real JPEG, by its own first bytes rather than by its name.
    let bytes = std::fs::read(&jpeg).expect("reads it back");
    assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF], "that is not a JPEG");

    // And back to PNG, which proves the decoder reads what the encoder wrote.
    let back = convert(&jpeg, Format::Png).expect("converts back to PNG");
    let bytes = std::fs::read(&back).expect("reads it back");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "that is not a PNG");
}

/// Converting the same file twice keeps both, rather than one silently
/// replacing the other.
#[cfg(windows)]
#[test]
fn a_second_conversion_lands_beside_the_first() {
    let dir = tempfile::tempdir().expect("a temp directory");
    let source = dir.path().join("twice.png");
    std::fs::write(&source, png(32, 32)).expect("writes the fixture");

    let first = convert(&source, Format::Jpeg).expect("converts once");
    let second = convert(&source, Format::Jpeg).expect("converts twice");

    assert_ne!(first, second, "the second conversion replaced the first");
    assert!(first.is_file() && second.is_file());
}
