//! Reading the words out of a picture.
//!
//! Ported from AuraKey's `ocr_watcher.rs`, which shares this author and had
//! already solved this. The recognition itself, the pre-multiplied Bgra8
//! `SoftwareBitmap`, the `IMemoryBufferByteAccess` cast to reach its pixels,
//! and the upscale for small captures are all its work; what is new here is
//! only getting a clipboard image into the shape it wants.
//!
//! ## Why this costs nothing at rest
//!
//! `Windows.Media.Ocr` is part of Windows. There is no model to download, no
//! engine to keep warm and nothing resident: the engine is built when a
//! picture is handed to it and dropped afterwards, so this feature is worth
//! exactly what it is used and nothing when it is not. That is the only reason
//! rule 23 allows it at all.
//!
//! It is never automatic. Recognising every image that passes through the
//! clipboard would be a background transcription service nobody asked for,
//! running over whatever happened to be copied.

/// The words in a picture, given its pixels.
///
/// `pixels` is Bgra8, which is what both Windows' own capture and a decoded
/// clipboard image can be arranged into cheaply.
///
/// Returns nothing when there is nothing to read, which is not an error: a
/// screenshot of a photograph is a perfectly ordinary thing to try this on.
#[cfg(windows)]
pub fn read_bgra(pixels: &[u8], width: i32, height: i32) -> Result<String, String> {
    if width <= 0 || height <= 0 {
        return Err("that picture has no size".to_string());
    }

    let wanted = (width as usize) * (height as usize) * 4;
    if pixels.len() < wanted {
        return Err(format!(
            "that picture is {} bytes short of the {wanted} its size claims",
            wanted - pixels.len()
        ));
    }

    let (pixels, width, height) = enlarged(pixels, width, height);

    recognise(&pixels, width, height)
}

#[cfg(not(windows))]
pub fn read_bgra(_pixels: &[u8], _width: i32, _height: i32) -> Result<String, String> {
    Err("text recognition needs Windows".to_string())
}

/// The size below which a picture is enlarged before being read.
///
/// AuraKey's number, and its reasoning: recognition is unreliable on a small
/// capture, and a nearest-neighbour enlargement is enough to fix it. A tooltip
/// or a single line grabbed out of a screenshot lands well under this.
const TOO_SMALL: i32 = 80;

/// How much smaller pictures are enlarged by.
const SCALE: i32 = 3;

/// A picture big enough to be read, enlarging it if it is not.
///
/// Nearest neighbour on purpose. Smoothing the edges of text is the opposite
/// of helpful here, and it costs more than repeating pixels does.
fn enlarged(pixels: &[u8], width: i32, height: i32) -> (Vec<u8>, i32, i32) {
    if width >= TOO_SMALL && height >= TOO_SMALL {
        return (pixels.to_vec(), width, height);
    }

    let wide = width * SCALE;
    let tall = height * SCALE;
    let mut out = vec![0u8; (wide as usize) * (tall as usize) * 4];

    for y in 0..tall {
        for x in 0..wide {
            let from = (((y / SCALE) * width + (x / SCALE)) * 4) as usize;
            let to = ((y * wide + x) * 4) as usize;
            out[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
        }
    }

    (out, wide, tall)
}

/// Turns the bytes a PNG holds into the ones the recogniser wants.
///
/// Clipboard images are stored as RGBA PNG, and Windows wants Bgra8, so the
/// red and blue channels swap. Everything else is already in the right order.
pub fn bgra_from_png(png: &[u8]) -> Result<(Vec<u8>, i32, i32), String> {
    // Wrapped because this version of the decoder seeks, and a byte slice on
    // its own cannot.
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder
        .read_info()
        .map_err(|err| format!("that image could not be read: {err}"))?;

    let mut buffer = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|err| format!("that image could not be read: {err}"))?;

    let width = info.width as i32;
    let height = info.height as i32;

    let mut pixels = match info.color_type {
        png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
        // Everything Sill writes is Rgba, but a blob is only bytes and a
        // future version writing something else should say so rather than
        // hand the recogniser a picture with the channels in the wrong places.
        other => return Err(format!("{other:?} images are not read yet")),
    };

    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    Ok((pixels, width, height))
}

/// Hands the picture to Windows and takes the words back.
#[cfg(windows)]
fn recognise(pixels: &[u8], width: i32, height: i32) -> Result<String, String> {
    use windows::Graphics::Imaging::{BitmapBufferAccessMode, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Win32::System::WinRT::IMemoryBufferByteAccess;

    let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, width, height)
        .map_err(|err| format!("could not make a bitmap: {err}"))?;

    {
        let buffer = bitmap
            .LockBuffer(BitmapBufferAccessMode::Write)
            .map_err(|err| format!("could not open the bitmap: {err}"))?;
        let reference = buffer
            .CreateReference()
            .map_err(|err| format!("could not reach the bitmap: {err}"))?;

        // SAFETY: the pointer and its length come from the buffer that owns
        // them, and are used only until the reference is dropped below. The
        // copy is bounded by the smaller of the two lengths, so a bitmap with
        // row padding cannot be overrun.
        unsafe {
            let access: IMemoryBufferByteAccess = windows::core::Interface::cast(&reference)
                .map_err(|err| format!("could not reach the bitmap's bytes: {err}"))?;

            let mut at = std::ptr::null_mut();
            let mut room = 0u32;
            access
                .GetBuffer(&mut at, &mut room)
                .map_err(|err| format!("could not reach the bitmap's bytes: {err}"))?;

            let into = std::slice::from_raw_parts_mut(at, room as usize);
            let len = into.len().min(pixels.len());
            into[..len].copy_from_slice(&pixels[..len]);
        }

        drop(reference);
        drop(buffer);
    }

    // Built here and dropped at the end of this call. Nothing is kept warm:
    // the whole point is that this costs nothing until it is asked for.
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|err| format!("no text recognition is installed for your languages: {err}"))?;

    let text = engine
        .RecognizeAsync(&bitmap)
        .map_err(|err| format!("could not read that picture: {err}"))?
        .join()
        .map_err(|err| format!("could not read that picture: {err}"))?
        .Text()
        .map_err(|err| format!("could not read that picture: {err}"))?
        .to_string_lossy();

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture, as a PNG the clipboard would have stored.
    fn png_of(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(rgba).expect("pixels");
        }
        out
    }

    /// Red and blue swap, and nothing else moves.
    ///
    /// Getting this backwards does not fail, it quietly recognises worse: the
    /// words come out of a picture whose colours are wrong, which is exactly
    /// the kind of bug that survives a demo.
    #[test]
    fn a_stored_image_comes_back_with_its_channels_in_the_order_windows_wants() {
        // One pixel: red 10, green 20, blue 30, opaque.
        let png = png_of(1, 1, &[10, 20, 30, 255]);

        let (pixels, width, height) = bgra_from_png(&png).expect("decodes");

        assert_eq!((width, height), (1, 1));
        assert_eq!(pixels, vec![30, 20, 10, 255], "the channels are not in Bgra order");
    }

    #[test]
    fn the_size_comes_back_with_it() {
        let png = png_of(3, 2, &[0u8; 3 * 2 * 4]);

        let (pixels, width, height) = bgra_from_png(&png).expect("decodes");

        assert_eq!((width, height), (3, 2));
        assert_eq!(pixels.len(), 3 * 2 * 4);
    }

    #[test]
    fn something_that_is_not_a_picture_says_so() {
        assert!(bgra_from_png(b"not a png at all").is_err());
    }

    /// A small picture is enlarged, because recognition is unreliable on one.
    #[test]
    fn a_small_picture_is_enlarged_before_it_is_read() {
        let pixels = vec![7u8; 4 * 4 * 4];

        let (out, width, height) = enlarged(&pixels, 4, 4);

        assert_eq!((width, height), (12, 12));
        assert_eq!(out.len(), 12 * 12 * 4);
        assert!(out.iter().all(|byte| *byte == 7), "the pixels changed value");
    }

    /// A big enough one is left exactly as it is.
    #[test]
    fn a_large_enough_picture_is_left_alone() {
        let pixels = vec![3u8; 100 * 90 * 4];

        let (out, width, height) = enlarged(&pixels, 100, 90);

        assert_eq!((width, height), (100, 90));
        assert_eq!(out, pixels);
    }

    /// Enlarging repeats pixels rather than blending them, so a hard edge
    /// stays hard. Smoothing the edges of text is the opposite of helpful.
    #[test]
    fn enlarging_repeats_pixels_rather_than_blending_them() {
        // Two pixels side by side, black then white.
        let pixels = vec![0, 0, 0, 255, 255, 255, 255, 255];

        let (out, width, height) = enlarged(&pixels, 2, 1);

        assert_eq!((width, height), (6, 3));
        // The first row: three black, then three white, nothing in between.
        let row: Vec<u8> = out
            .chunks_exact(4)
            .take(width as usize)
            .map(|pixel| pixel[0])
            .collect();
        assert_eq!(row, vec![0, 0, 0, 255, 255, 255]);
    }

    /// A picture whose bytes do not match its stated size is rejected rather
    /// than read past the end of.
    #[test]
    fn a_picture_shorter_than_it_claims_is_refused() {
        let err = read_bgra(&[0u8; 16], 100, 100).expect_err("should refuse");

        assert!(err.contains("short"), "{err}");
    }

    #[test]
    fn a_picture_with_no_size_is_refused() {
        assert!(read_bgra(&[], 0, 0).is_err());
    }
}
