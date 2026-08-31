//! Taking a picture of the screen.
//!
//! Ported from AuraKey's `ocr_watcher.rs`, which is Brandon's own: the BitBlt
//! into a memory device context and the `GetDIBits` read-back with a negative
//! height are its work. What is added here is saying when it failed, cleaning
//! up on every path out of it, and knowing where the screen actually is when
//! there is more than one of them.
//!
//! ## Nothing runs until it is asked for
//!
//! There is no capture thread, no timer and nothing watching a screen. A
//! picture is taken when something asks for one and the device contexts are
//! released before the call returns, so this costs exactly what it is used.

/// A picture of part of the screen.
///
/// Its `Debug` says the size rather than the pixels: a failed assertion
/// printing thirty megabytes of them helps nobody.
pub struct Shot {
    /// Bgra8, top row first, which is what both the recogniser and PNG want.
    pub pixels: Vec<u8>,
    pub width: i32,
    pub height: i32,
}

impl std::fmt::Debug for Shot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Shot {}x{} ({} bytes)",
            self.width,
            self.height,
            self.pixels.len()
        )
    }
}

impl Shot {
    /// The picture as a PNG, which is how the clipboard and disk want it.
    ///
    /// Bgra to Rgba on the way out, the same swap the recogniser needs in the
    /// other direction.
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        let mut rgba = self.pixels.clone();
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            // The screen has no transparency, and BitBlt leaves this channel
            // as whatever was in the buffer. Left alone, a capture can come
            // out fully transparent.
            pixel[3] = 255;
        }

        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|err| format!("could not write that picture: {err}"))?;
            writer
                .write_image_data(&rgba)
                .map_err(|err| format!("could not write that picture: {err}"))?;
        }

        Ok(out)
    }
}

/// A smaller copy, no larger than `most` on its longer side.
///
/// A preview of a window is looked at in a strip a few hundred pixels wide,
/// and the window itself can be four million pixels. Sending the original
/// would be a multi-megabyte picture per press of an arrow key, so it is made
/// small **before** it is encoded rather than after: the encoding is the
/// expensive half and there is no reason to spend it on pixels nobody sees.
///
/// Averaged rather than sampled. Taking every eighth pixel of a window full of
/// one-pixel text turns it into noise, which reads as a broken preview rather
/// than a small one; averaging the block each output pixel covers gives
/// something that still looks like the window.
///
/// Returns the shot untouched when it is already small enough, so nothing is
/// paid to shrink something that does not need it.
pub fn thumbnail(shot: &Shot, most: i32) -> Shot {
    let longest = shot.width.max(shot.height);

    if most <= 0 || longest <= most || shot.width <= 0 || shot.height <= 0 {
        return Shot {
            pixels: shot.pixels.clone(),
            width: shot.width,
            height: shot.height,
        };
    }

    // At least one pixel each way. A window one pixel tall is silly but it is
    // not a reason to produce a picture with no rows in it.
    let width = ((shot.width * most) / longest).max(1);
    let height = ((shot.height * most) / longest).max(1);

    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];

    for y in 0..height {
        // The block of source rows this output row covers.
        let from_y = (y * shot.height) / height;
        let to_y = (((y + 1) * shot.height) / height).max(from_y + 1).min(shot.height);

        for x in 0..width {
            let from_x = (x * shot.width) / width;
            let to_x = (((x + 1) * shot.width) / width).max(from_x + 1).min(shot.width);

            let mut totals = [0u32; 4];
            let mut counted = 0u32;

            for sy in from_y..to_y {
                for sx in from_x..to_x {
                    let at = ((sy as usize) * (shot.width as usize) + sx as usize) * 4;
                    if at + 3 >= shot.pixels.len() {
                        continue;
                    }

                    for channel in 0..4 {
                        totals[channel] += u32::from(shot.pixels[at + channel]);
                    }
                    counted += 1;
                }
            }

            if counted == 0 {
                continue;
            }

            let out = ((y as usize) * (width as usize) + x as usize) * 4;
            for channel in 0..4 {
                pixels[out + channel] = (totals[channel] / counted) as u8;
            }
        }
    }

    Shot {
        pixels,
        width,
        height,
    }
}

/// Everything the screens cover, as one rectangle.
///
/// With more than one display this is bigger than any of them, and its origin
/// can be negative: a monitor to the left of the primary one starts at a
/// negative x. Anything positioning an overlay over "the whole screen" needs
/// this rather than the primary display's size.
#[cfg(windows)]
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    // SAFETY: reads four documented system metrics and returns integers.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

#[cfg(not(windows))]
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    (0, 0, 0, 0)
}

/// A picture of one rectangle of the screen.
///
/// Coordinates are the screen's own, so they can be negative on a multi-display
/// desk. They are physical pixels rather than the ones a window reports, which
/// differ by the display's scaling.
#[cfg(windows)]
pub fn region(left: i32, top: i32, width: i32, height: i32) -> Result<Shot, String> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        HGDIOBJ, SRCCOPY,
    };

    if width <= 0 || height <= 0 {
        return Err("that is not an area".to_string());
    }

    // A drag across two 4K displays is already 33 megabytes of pixels. Beyond
    // this something has gone wrong with the coordinates rather than somebody
    // having asked for it.
    const TOO_BIG: i64 = 64 * 1024 * 1024;
    if (width as i64) * (height as i64) * 4 > TOO_BIG {
        return Err("that area is too large to capture".to_string());
    }

    // SAFETY: every handle created below is released on every path out,
    // including the early returns, which is what the nested blocks are for.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("the screen could not be read".to_string());
        }

        let memory = CreateCompatibleDC(Some(screen));
        if memory.is_invalid() {
            ReleaseDC(None, screen);
            return Err("the screen could not be read".to_string());
        }

        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if bitmap.is_invalid() {
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            return Err("there was no room for a picture that size".to_string());
        }

        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));

        let copied = BitBlt(memory, 0, 0, width, height, Some(screen), left, top, SRCCOPY).is_ok();

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];

        // A negative height asks for the rows top down. Without it the picture
        // comes back upside down, which is the classic form of this bug.
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        if !copied {
            return Err("that part of the screen could not be copied".to_string());
        }
        // AuraKey ignores this, which is fine for a watcher that will look
        // again in a moment. A picture somebody asked for should say so
        // instead of coming back blank.
        if rows == 0 {
            return Err("that picture came back empty".to_string());
        }

        Ok(Shot {
            pixels,
            width,
            height,
        })
    }
}

#[cfg(not(windows))]
pub fn region(_left: i32, _top: i32, _width: i32, _height: i32) -> Result<Shot, String> {
    Err("screen capture needs Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture of one flat colour, for the tests that only care about size.
    fn flat(width: i32, height: i32, colour: [u8; 4]) -> Shot {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&colour);
        }

        Shot {
            pixels,
            width,
            height,
        }
    }

    mod making_it_small {
        use super::*;

        #[test]
        fn the_longer_side_becomes_the_limit() {
            let small = thumbnail(&flat(1600, 900, [1, 2, 3, 255]), 400);

            assert_eq!(small.width, 400);
            assert_eq!(small.height, 225);
            assert_eq!(small.pixels.len(), 400 * 225 * 4);
        }

        /// A tall window is limited by its height, not by its width.
        #[test]
        fn a_tall_picture_is_limited_the_same_way() {
            let small = thumbnail(&flat(600, 1200, [0, 0, 0, 255]), 300);

            assert_eq!(small.height, 300);
            assert_eq!(small.width, 150);
        }

        /// Nothing is paid to shrink something that does not need it.
        #[test]
        fn something_already_small_is_left_exactly_alone() {
            let already = flat(120, 80, [9, 9, 9, 255]);
            let same = thumbnail(&already, 400);

            assert_eq!(same.width, 120);
            assert_eq!(same.height, 80);
            assert_eq!(same.pixels, already.pixels);
        }

        /// Averaged rather than sampled: half black and half white makes grey,
        /// where taking every other pixel would make one or the other.
        #[test]
        fn the_block_is_averaged_and_not_sampled() {
            // Two pixels wide, one tall: black then white.
            let pair = shot_from(2, 1, &[[0, 0, 0, 255], [255, 255, 255, 255]]);
            let one = thumbnail(&pair, 1);

            assert_eq!(one.width, 1);
            assert_eq!(one.height, 1);
            assert_eq!(&one.pixels[..3], &[127, 127, 127]);
        }

        /// Taking every eighth pixel of a window full of one-pixel text turns
        /// it into noise, which reads as a broken preview rather than a small
        /// one. This is that case: a single bright pixel in a dark field must
        /// still lighten the block it lands in.
        #[test]
        fn one_bright_pixel_still_shows_in_its_block() {
            let mut field = flat(4, 1, [0, 0, 0, 255]);
            field.pixels[8] = 255;
            field.pixels[9] = 255;
            field.pixels[10] = 255;

            let one = thumbnail(&field, 1);
            assert_eq!(one.pixels[0], 63, "the bright pixel was sampled away");
        }

        /// A window one pixel tall is silly, and not a reason to produce a
        /// picture with no rows in it.
        #[test]
        fn nothing_ever_comes_back_with_no_pixels() {
            let sliver = thumbnail(&flat(2000, 1, [5, 5, 5, 255]), 100);

            assert_eq!(sliver.width, 100);
            assert_eq!(sliver.height, 1);
            assert!(!sliver.pixels.is_empty());
        }

        #[test]
        fn a_limit_of_nothing_changes_nothing() {
            let original = flat(40, 40, [1, 1, 1, 255]);
            assert_eq!(thumbnail(&original, 0).width, 40);
        }
    }

    /// A picture built pixel by pixel, for the tests that care about colour.
    fn shot_from(width: i32, height: i32, pixels: &[[u8; 4]]) -> Shot {
        Shot {
            pixels: pixels.iter().flatten().copied().collect(),
            width,
            height,
        }
    }

    fn shot(width: i32, height: i32, pixels: Vec<u8>) -> Shot {
        Shot {
            pixels,
            width,
            height,
        }
    }

    /// The screen has no transparency, and BitBlt does not set that channel.
    ///
    /// Left as it came out of the buffer, a capture encodes as fully
    /// transparent and every viewer shows nothing at all. This is the bug that
    /// makes a screenshot look like it failed when the pixels are all there.
    #[test]
    fn a_capture_is_opaque_however_the_buffer_arrived() {
        // Bgra with a zero alpha, which is what an uninitialised buffer gives.
        let png = shot(1, 1, vec![30, 20, 10, 0]).to_png().expect("encodes");

        let (rgba, _, _) = crate::ocr::bgra_from_png(&png).expect("decodes");

        // Read back as Bgra, so this is the alpha.
        assert_eq!(rgba[3], 255, "the picture came out transparent");
    }

    /// Bgra in, Rgba out, which is the swap PNG needs.
    #[test]
    fn the_channels_are_put_the_way_a_png_wants_them() {
        let png = shot(1, 1, vec![30, 20, 10, 255]).to_png().expect("encodes");

        // Decoding swaps them back, so what comes out should match what went in.
        let (bgra, width, height) = crate::ocr::bgra_from_png(&png).expect("decodes");

        assert_eq!((width, height), (1, 1));
        assert_eq!(&bgra[..3], &[30, 20, 10]);
    }

    #[test]
    fn the_size_survives_the_trip_through_a_png() {
        let png = shot(4, 3, vec![0u8; 4 * 3 * 4]).to_png().expect("encodes");

        let (_, width, height) = crate::ocr::bgra_from_png(&png).expect("decodes");

        assert_eq!((width, height), (4, 3));
    }

    #[test]
    fn an_empty_area_is_refused_rather_than_captured() {
        assert!(region(0, 0, 0, 100).is_err());
        assert!(region(0, 0, 100, 0).is_err());
        assert!(region(0, 0, -5, -5).is_err());
    }

    /// Coordinates that have gone wrong ask for something enormous, and the
    /// allocation is what would be noticed rather than the mistake.
    #[test]
    fn an_absurd_area_is_refused_before_anything_is_allocated() {
        let err = region(0, 0, 100_000, 100_000).expect_err("should refuse");

        assert!(err.contains("too large"), "{err}");
    }
}

/// A picture of one window, even where something is sitting on top of it.
///
/// `PrintWindow` with `PW_RENDERFULLCONTENT` asks the window to draw itself
/// rather than copying what is on screen, so an overlapped window still comes
/// out whole. Reading the screen instead would capture whatever is covering
/// it, which is the version of this bug people notice immediately.
///
/// It is not universal: a window drawing through a path that will not
/// re-render on demand comes back blank or partly blank. So the result is
/// checked, and a blank one falls back to reading the screen, which is at
/// least what somebody can see.
#[cfg(windows)]
pub fn window(handle: isize, fallback: (i32, i32, i32, i32)) -> Result<Shot, String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    // Under Storage::Xps, not WindowsAndMessaging where the Win32 docs group
    // it. The crate files it by the header it is declared in.
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};

    let (left, top, width, height) = fallback;
    if width <= 0 || height <= 0 {
        return Err("that window has no size".to_string());
    }

    // PW_RENDERFULLCONTENT. Not named in the crate's constants, and it is what
    // makes this work on a composited window at all.
    const FULL_CONTENT: u32 = 0x00000002;

    // SAFETY: every handle is released on each path out, and the window handle
    // is revalidated by the caller before it gets here.
    let taken = unsafe {
        let hwnd = HWND(handle as *mut core::ffi::c_void);

        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));

        let drew = PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(FULL_CONTENT)).as_bool();

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let rows = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);

        (drew && rows != 0).then_some(pixels)
    };

    match taken {
        Some(pixels) if !blank(&pixels) => Ok(Shot {
            pixels,
            width,
            height,
        }),
        // Either it refused to draw, or it drew nothing. Reading the screen
        // gets whatever is actually there, which is worse than the window on
        // its own and much better than an empty rectangle.
        _ => region(left, top, width, height),
    }
}

#[cfg(not(windows))]
pub fn window(_handle: isize, _fallback: (i32, i32, i32, i32)) -> Result<Shot, String> {
    Err("window capture needs Windows".to_string())
}

/// Whether a picture is one flat colour, which is what a refusal looks like.
///
/// `PrintWindow` can report success and produce nothing at all, so the result
/// has to be looked at rather than trusted. Sampled rather than scanned in
/// full: a window is hundreds of thousands of pixels and the question is only
/// whether any two of them differ.
fn blank(pixels: &[u8]) -> bool {
    if pixels.len() < 8 {
        return true;
    }

    let first = &pixels[..4];
    // Every four hundredth pixel, which finds any real content immediately and
    // costs nothing on a window that genuinely is empty.
    pixels
        .chunks_exact(4)
        .step_by(97)
        .all(|pixel| pixel[..3] == first[..3])
}
