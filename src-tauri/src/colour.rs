//! Colours typed into the search field, and the pixel under the pointer.
//!
//! `#ff8800` typed into a launcher is somebody who wants it in another form:
//! the same colour as `rgb()` for a stylesheet, as `hsl()` for a design
//! tool, or the other way round. So a colour in any of the written forms is
//! answered with the forms it was not written in, each a row that copies.
//!
//! The picker is the other half: a pixel chosen on the screen through the
//! capture overlay, read once, and copied as hex.
//!
//! Pure, apart from nothing. The gate is the first character and a handful
//! of comparisons, so a search that is not a colour costs what it always did.

/// A colour, with the alpha kept only when one was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: Option<u8>,
}

/// Reads a colour in any of the written forms, or `None` for anything else.
///
/// `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(255, 136, 0)`, `rgba(255, 136, 0, 0.5)`,
/// `hsl(32, 100%, 50%)`, `hsla(32, 100%, 50%, 0.5)`. A hash followed by
/// anything that is not hex digits is a tag or a heading, and stays a search.
pub fn parse(input: &str) -> Option<Colour> {
    let lower = input.trim().to_ascii_lowercase();

    if let Some(digits) = lower.strip_prefix('#') {
        return from_hex(digits);
    }

    let (name, inside) = lower.split_once('(')?;
    let inside = inside.strip_suffix(')')?;
    let parts: Vec<&str> = inside
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();

    match (name, parts.as_slice()) {
        ("rgb", [r, g, b]) => Some(Colour {
            r: channel(r)?,
            g: channel(g)?,
            b: channel(b)?,
            a: None,
        }),
        ("rgba", [r, g, b, a]) => Some(Colour {
            r: channel(r)?,
            g: channel(g)?,
            b: channel(b)?,
            a: Some(alpha(a)?),
        }),
        ("hsl", [h, s, l]) => Some(from_hsl(degrees(h)?, percent(s)?, percent(l)?, None)),
        ("hsla", [h, s, l, a]) => Some(from_hsl(
            degrees(h)?,
            percent(s)?,
            percent(l)?,
            Some(alpha(a)?),
        )),
        _ => None,
    }
}

impl Colour {
    /// `#ff8800`, or `#ff880080` when an alpha was given.
    pub fn hex(&self) -> String {
        match self.a {
            Some(a) => format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, a),
            None => format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b),
        }
    }

    /// `rgb(255, 136, 0)`, or `rgba(...)` with the alpha as a fraction.
    pub fn rgb(&self) -> String {
        match self.a {
            Some(a) => format!(
                "rgba({}, {}, {}, {})",
                self.r,
                self.g,
                self.b,
                fraction(a)
            ),
            None => format!("rgb({}, {}, {})", self.r, self.g, self.b),
        }
    }

    /// `hsl(32, 100%, 50%)`, with each part rounded to a whole number.
    pub fn hsl(&self) -> String {
        let (h, s, l) = to_hsl(*self);

        match self.a {
            Some(a) => format!("hsla({h}, {s}%, {l}%, {})", fraction(a)),
            None => format!("hsl({h}, {s}%, {l}%)"),
        }
    }

    /// The colour of the first pixel of a capture, which is BGRA8.
    pub fn from_bgra(pixels: &[u8]) -> Option<Self> {
        match pixels {
            [b, g, r, ..] => Some(Colour {
                r: *r,
                g: *g,
                b: *b,
                a: None,
            }),
            _ => None,
        }
    }

    /// Every written form, named, in the order the rows show them.
    pub fn formats(&self) -> [(&'static str, String); 3] {
        [("hex", self.hex()), ("rgb", self.rgb()), ("hsl", self.hsl())]
    }

    /// The forms other than the one that was typed.
    ///
    /// A row repeating the question is noise, which is the same rule the
    /// calculator's `is_useful` applies. Sameness ignores case and spacing.
    pub fn other_forms(&self, typed: &str) -> Vec<(&'static str, String)> {
        let typed = squash(typed);
        self.formats()
            .into_iter()
            .filter(|(_, text)| squash(text) != typed)
            .collect()
    }
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn from_hex(digits: &str) -> Option<Colour> {
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let pair = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    let single = |at: usize| {
        u8::from_str_radix(&digits[at..at + 1], 16)
            .ok()
            .map(|n| n * 17)
    };

    match digits.len() {
        3 => Some(Colour {
            r: single(0)?,
            g: single(1)?,
            b: single(2)?,
            a: None,
        }),
        4 => Some(Colour {
            r: single(0)?,
            g: single(1)?,
            b: single(2)?,
            a: Some(single(3)?),
        }),
        6 => Some(Colour {
            r: pair(0)?,
            g: pair(2)?,
            b: pair(4)?,
            a: None,
        }),
        8 => Some(Colour {
            r: pair(0)?,
            g: pair(2)?,
            b: pair(4)?,
            a: Some(pair(6)?),
        }),
        _ => None,
    }
}

/// `0` to `255`, or a percentage.
fn channel(text: &str) -> Option<u8> {
    if let Some(percent) = text.strip_suffix('%') {
        let value: f64 = percent.parse().ok()?;
        return (0.0..=100.0)
            .contains(&value)
            .then(|| (value / 100.0 * 255.0).round() as u8);
    }

    let value: f64 = text.parse().ok()?;
    (0.0..=255.0).contains(&value).then(|| value.round() as u8)
}

/// `0` to `1`, or a percentage, as the byte it is stored as.
fn alpha(text: &str) -> Option<u8> {
    let value: f64 = match text.strip_suffix('%') {
        Some(percent) => percent.parse::<f64>().ok()? / 100.0,
        None => text.parse().ok()?,
    };

    (0.0..=1.0)
        .contains(&value)
        .then(|| (value * 255.0).round() as u8)
}

fn fraction(a: u8) -> String {
    let value = f64::from(a) / 255.0;
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Degrees, with or without the unit, wrapped into one turn.
fn degrees(text: &str) -> Option<f64> {
    let value: f64 = text.trim_end_matches("deg").parse().ok()?;
    Some(value.rem_euclid(360.0))
}

fn percent(text: &str) -> Option<f64> {
    let value: f64 = text.strip_suffix('%').unwrap_or(text).parse().ok()?;
    (0.0..=100.0).contains(&value).then_some(value / 100.0)
}

fn from_hsl(h: f64, s: f64, l: f64, a: Option<u8>) -> Colour {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Colour {
        r: byte(r),
        g: byte(g),
        b: byte(b),
        a,
    }
}

fn to_hsl(colour: Colour) -> (i64, i64, i64) {
    let r = f64::from(colour.r) / 255.0;
    let g = f64::from(colour.g) / 255.0;
    let b = f64::from(colour.b) / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;

    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as i64);
    }

    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };

    (
        h.round() as i64 % 360,
        (s * 100.0).round() as i64,
        (l * 100.0).round() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: Colour = Colour {
        r: 255,
        g: 136,
        b: 0,
        a: None,
    };

    #[test]
    fn hex_short_and_long_agree() {
        assert_eq!(parse("#f80"), Some(ORANGE));
        assert_eq!(parse("#FF8800"), Some(ORANGE));
        assert_eq!(parse("  #ff8800  "), Some(ORANGE));
    }

    #[test]
    fn the_written_forms_read_and_write_the_same_colour() {
        assert_eq!(parse("rgb(255, 136, 0)"), Some(ORANGE));
        assert_eq!(parse("rgb(255 136 0)"), Some(ORANGE));
        assert_eq!(parse("hsl(32, 100%, 50%)"), Some(ORANGE));

        assert_eq!(ORANGE.hex(), "#ff8800");
        assert_eq!(ORANGE.rgb(), "rgb(255, 136, 0)");
        assert_eq!(ORANGE.hsl(), "hsl(32, 100%, 50%)");
    }

    #[test]
    fn hsl_round_trips_through_rgb() {
        for text in ["hsl(0, 100%, 50%)", "hsl(120, 50%, 25%)", "hsl(300, 20%, 80%)"] {
            let colour = parse(text).unwrap();
            assert_eq!(colour.hsl(), text, "{text} did not come back as itself");
        }

        // Grey has no hue, and says so with a zero rather than a guess.
        assert_eq!(parse("#808080").unwrap().hsl(), "hsl(0, 0%, 50%)");
    }

    #[test]
    fn alpha_is_kept_where_it_was_given() {
        let half = parse("rgba(255, 136, 0, 0.5)").unwrap();
        assert_eq!(half.a, Some(128));
        assert_eq!(half.hex(), "#ff880080");
        assert_eq!(half.rgb(), "rgba(255, 136, 0, 0.5)");
        assert_eq!(half.hsl(), "hsla(32, 100%, 50%, 0.5)");

        assert_eq!(parse("#ff880080"), Some(half));
        assert_eq!(parse("#f808").unwrap().a, Some(136));
        assert_eq!(parse("hsla(32, 100%, 50%, 50%)").unwrap().a, Some(128));
    }

    #[test]
    fn a_file_name_with_a_hash_is_not_a_colour() {
        for not in [
            "#readme",
            "#",
            "#ff",
            "#ff880",
            "#ff88001",
            "#tag",
            "rgb(256, 0, 0)",
            "rgb(1, 2)",
            "hsl(10, 200%, 50%)",
            "notepad",
            "",
            "rgb",
            "(1, 2, 3)",
        ] {
            assert_eq!(parse(not), None, "{not:?} was read as a colour");
        }
    }

    #[test]
    fn the_form_that_was_typed_is_not_offered_back() {
        let offered = ORANGE.other_forms("#FF8800");
        assert_eq!(offered.len(), 2);
        assert_eq!(offered[0].0, "rgb");
        assert_eq!(offered[1].0, "hsl");

        // Short hex is not the same text, so the long form is worth a row.
        assert_eq!(ORANGE.other_forms("#f80").len(), 3);
        assert_eq!(ORANGE.other_forms("rgb(255,136,0)").len(), 2);
    }

    #[test]
    fn a_captured_pixel_reads_as_the_colour_it_is() {
        // BGRA, as `capture::region` hands it over.
        assert_eq!(Colour::from_bgra(&[0, 136, 255, 255]), Some(ORANGE));
        assert_eq!(Colour::from_bgra(&[0, 136]), None);
    }
}
