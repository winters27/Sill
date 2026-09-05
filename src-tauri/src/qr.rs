//! Reading a QR code off a picture.
//!
//! A code arrives on the screen far more often than anybody wants to reach
//! for a phone: a login page, a Wi-Fi card, a link in a slide somebody is
//! presenting. Sill already has both halves of the answer, a picture on the
//! clipboard and a rectangle of the screen, so this is the piece in between.
//!
//! ## What it does not do
//!
//! **Nothing is opened.** A code found on a screen was put there by whoever
//! made the page, which is exactly the shape of a link nobody chose to click,
//! so the payload is copied and named and goes no further. Opening it is a
//! second, deliberate keystroke through the ordinary rules in
//! [`crate::reach`], which is where an address anybody else wrote belongs.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. Every entry point here is called from an action somebody ran.

/// How much of one payload is worth keeping.
///
/// A QR code holds at most about 2,950 bytes, so this is a bound on a
/// mistake rather than on a real code.
const MOST_BYTES: usize = 4 * 1024;

/// The codes in a picture, in the order they were found.
///
/// The pixels are BGRA8 with the top row first, which is what
/// [`crate::capture::Shot`] holds and what [`crate::ocr::bgra_from_png`]
/// hands back, so a screen region and a clipboard picture both reach here
/// without a conversion in between.
///
/// An empty answer means no code, which is an ordinary outcome rather than a
/// failure: most pictures have no code in them.
pub fn decode_bgra(pixels: &[u8], width: i32, height: i32) -> Result<Vec<String>, String> {
    let (w, h) = usable(pixels, width, height)?;

    // Luminance by the usual weights. The closure is called once per pixel
    // by the preparer, which is why the picture is not copied first.
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
        let at = (y * w + x) * 4;
        let (b, g, r) = (
            f32::from(pixels[at]),
            f32::from(pixels[at + 1]),
            f32::from(pixels[at + 2]),
        );
        (0.114 * b + 0.587 * g + 0.299 * r) as u8
    });

    let mut found = Vec::new();

    for grid in prepared.detect_grids() {
        // A grid that will not decode is a shape that looked like a code and
        // was not, or one too damaged to read. Both are ordinary in a
        // photograph of a screen, and neither is worth failing the others for.
        if let Ok((_, content)) = grid.decode() {
            let content: String = content.chars().take(MOST_BYTES).collect();
            if !content.is_empty() && !found.contains(&content) {
                found.push(content);
            }
        }
    }

    Ok(found)
}

/// The picture's size, if it is one this can read.
///
/// Checked rather than trusted: the buffer and the dimensions arrive from
/// two different places, and reading past the end of one because the other
/// was wrong is the kind of mistake that only shows up on somebody else's
/// machine.
fn usable(pixels: &[u8], width: i32, height: i32) -> Result<(usize, usize), String> {
    if width <= 0 || height <= 0 {
        return Err("that picture has no size".to_string());
    }

    let (w, h) = (width as usize, height as usize);
    let wanted = w
        .checked_mul(h)
        .and_then(|count| count.checked_mul(4))
        .ok_or_else(|| "that picture is too large to read".to_string())?;

    if pixels.len() < wanted {
        return Err("that picture is smaller than it says it is".to_string());
    }

    Ok((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real code, as its modules, for `https://sill.winters.app`.
    ///
    /// The matrix rather than a PNG on purpose: a fixture somebody can read
    /// in the diff is worth more than a binary blob, and decoding is what is
    /// being tested rather than PNG parsing.
    const CODE: &[&str] = &[
        "#######...##.##...#######",
        "#.....#.#..#..#...#.....#",
        "#.###.#.###.#.###.#.###.#",
        "#.###.#.#.####....#.###.#",
        "#.###.#..#.#..#.#.#.###.#",
        "#.....#..#..##....#.....#",
        "#######.#.#.#.#.#.#######",
        "........#.#....##........",
        "#.....#.#.#####..##..###.",
        "#...#..##..#...#...#####.",
        "#.....#...#.##.##..###.##",
        "####......##..#..##..#..#",
        "....#######.#.#.#.#.....#",
        "#.#.#.....#..#.##..#...#.",
        "#.#..#############.###.##",
        "#..##..##.##..##..##.##.#",
        "#.#.#.#...###########.#..",
        "........##..#..##...#....",
        "#######..######.#.#.#...#",
        "#.....#.....#.###...#...#",
        "#.###.#...#.#########.#..",
        "#.###.#..#..#....##....##",
        "#.###.#..##.#...#....##.#",
        "#.....#.....#...##.##...#",
        "#######.##.#...####..#..#",
    ];

    const PAYLOAD: &str = "https://sill.winters.app";

    /// Modules to pixels: each module a square, with the quiet zone a reader
    /// needs around it, drawn as BGRA the way a capture holds it.
    fn picture(modules: &[&str], scale: usize, quiet: usize) -> (Vec<u8>, i32, i32) {
        let side = modules.len();
        let pixels_side = (side + quiet * 2) * scale;
        let mut out = vec![255u8; pixels_side * pixels_side * 4];

        for (row, line) in modules.iter().enumerate() {
            for (column, module) in line.chars().enumerate() {
                if module != '#' {
                    continue;
                }

                for y in 0..scale {
                    for x in 0..scale {
                        let px = (quiet + column) * scale + x;
                        let py = (quiet + row) * scale + y;
                        let at = (py * pixels_side + px) * 4;
                        // Black, opaque. Alpha is ignored by the reader and
                        // set anyway so the buffer is a real picture.
                        out[at] = 0;
                        out[at + 1] = 0;
                        out[at + 2] = 0;
                        out[at + 3] = 255;
                    }
                }
            }
        }

        (out, pixels_side as i32, pixels_side as i32)
    }

    #[test]
    fn a_known_code_decodes() {
        let (pixels, width, height) = picture(CODE, 4, 4);
        let found = decode_bgra(&pixels, width, height).expect("reads the picture");

        assert_eq!(found, vec![PAYLOAD.to_string()]);
    }

    /// The same code drawn larger, which is what a screenshot of one is.
    #[test]
    fn a_bigger_drawing_of_the_same_code_reads_the_same() {
        let (pixels, width, height) = picture(CODE, 9, 6);
        let found = decode_bgra(&pixels, width, height).expect("reads the picture");

        assert_eq!(found, vec![PAYLOAD.to_string()]);
    }

    #[test]
    fn a_picture_with_no_code_says_so() {
        // Plain white, which is a picture and holds nothing.
        let blank = vec![255u8; 64 * 64 * 4];
        assert_eq!(decode_bgra(&blank, 64, 64), Ok(Vec::new()));

        // Stripes: plenty of edges, no code.
        let mut stripes = vec![255u8; 64 * 64 * 4];
        for (at, byte) in stripes.iter_mut().enumerate() {
            if (at / 4 / 64) % 2 == 0 {
                *byte = 0;
            }
        }
        assert_eq!(decode_bgra(&stripes, 64, 64), Ok(Vec::new()));
    }

    /// The buffer and the dimensions come from different places, so a
    /// mismatch is refused rather than read past the end of.
    #[test]
    fn a_picture_smaller_than_it_claims_is_refused() {
        let small = vec![255u8; 10 * 10 * 4];

        assert!(decode_bgra(&small, 100, 100).is_err());
        assert!(decode_bgra(&small, 0, 10).is_err());
        assert!(decode_bgra(&small, 10, -1).is_err());
        assert!(decode_bgra(&small, i32::MAX, i32::MAX).is_err());
    }
}
