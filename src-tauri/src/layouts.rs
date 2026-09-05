//! Window positions of your own.
//!
//! The fifteen built-in slots cover halves, thirds, quarters and the centre.
//! A layout is the sixteenth and onward: a rectangle somebody drew, kept as
//! fractions of the display's work area so it means the same thing on every
//! display, applied by one action that any key, the panel or the model can
//! reach with the layout's name.
//!
//! Pure. The fractions are clamped here and the move is `windowing`'s.

use serde::{Deserialize, Serialize};

use crate::windowing::Rect;

/// One saved position, as fractions of the work area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    /// Stable across renames, because a key may refer to it.
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        }
    }
}

/// The smallest a layout may be, as a fraction of the work area.
///
/// A window five percent wide is still one somebody can grab. One zero
/// pixels wide is a window that has gone missing.
const AT_LEAST: f64 = 0.05;

/// A layout by its id, or failing that by its name, case not mattering.
///
/// A key carries the name, because a name is what somebody typed and can
/// read back in the settings row; the id is what survives a rename.
pub fn find<'a>(layouts: &'a [Layout], wanted: &str) -> Option<&'a Layout> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return None;
    }

    layouts
        .iter()
        .find(|layout| layout.id == wanted)
        .or_else(|| {
            layouts
                .iter()
                .find(|layout| layout.name.trim().eq_ignore_ascii_case(wanted))
        })
}

/// Where a layout puts a window on a given work area.
///
/// Fractions are clamped into the area first, then each edge is rounded to
/// a whole pixel by the same rule, so two layouts that share an edge, the
/// left half and the right half, meet exactly rather than overlapping by a
/// pixel or leaving a gap of one.
pub fn rect_of(layout: &Layout, work: Rect) -> Rect {
    let unit = |value: f64| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };

    let left = unit(layout.x);
    let top = unit(layout.y);
    let width = unit(layout.width).max(AT_LEAST).min(1.0 - left).max(AT_LEAST);
    let height = unit(layout.height).max(AT_LEAST).min(1.0 - top).max(AT_LEAST);

    // A layout pushed off the edge is pulled back rather than cut down.
    let left = left.min(1.0 - width);
    let top = top.min(1.0 - height);

    let edge = |fraction: f64, size: i32| (fraction * f64::from(size)).round() as i32;

    let x0 = work.x + edge(left, work.width);
    let x1 = work.x + edge(left + width, work.width);
    let y0 = work.y + edge(top, work.height);
    let y1 = work.y + edge(top + height, work.height);

    Rect {
        x: x0,
        y: y0,
        width: (x1 - x0).max(1),
        height: (y1 - y0).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(name: &str, x: f64, y: f64, width: f64, height: f64) -> Layout {
        Layout {
            id: format!("id-{name}"),
            name: name.to_string(),
            x,
            y,
            width,
            height,
        }
    }

    const WORK: Rect = Rect {
        x: 10,
        y: 20,
        width: 1001,
        height: 801,
    };

    #[test]
    fn two_half_layouts_tile_the_work_area_exactly() {
        let left = rect_of(&layout("left", 0.0, 0.0, 0.5, 1.0), WORK);
        let right = rect_of(&layout("right", 0.5, 0.0, 0.5, 1.0), WORK);

        assert_eq!(left.x, WORK.x);
        assert_eq!(left.x + left.width, right.x, "the halves overlap or gap");
        assert_eq!(right.x + right.width, WORK.x + WORK.width);
        assert_eq!(left.height, WORK.height);
    }

    #[test]
    fn a_layout_never_leaves_the_work_area() {
        let off = rect_of(&layout("off", 0.9, 0.9, 0.5, 0.5), WORK);
        assert_eq!(off.x + off.width, WORK.x + WORK.width);
        assert_eq!(off.y + off.height, WORK.y + WORK.height);

        let wild = rect_of(&layout("wild", -3.0, 7.0, 40.0, f64::NAN), WORK);
        assert!(wild.x >= WORK.x && wild.x + wild.width <= WORK.x + WORK.width);
        assert!(wild.y >= WORK.y && wild.y + wild.height <= WORK.y + WORK.height);
    }

    #[test]
    fn a_layout_too_small_to_see_is_widened() {
        let sliver = rect_of(&layout("sliver", 0.0, 0.0, 0.0, 0.0), WORK);
        assert!(sliver.width >= (AT_LEAST * f64::from(WORK.width)) as i32 - 1);
        assert!(sliver.height >= (AT_LEAST * f64::from(WORK.height)) as i32 - 1);
    }

    #[test]
    fn a_layout_is_found_by_id_or_by_name() {
        let all = vec![layout("Left third", 0.0, 0.0, 0.33, 1.0), layout("Reading", 0.25, 0.1, 0.5, 0.8)];

        assert_eq!(find(&all, "id-Reading").map(|l| l.name.as_str()), Some("Reading"));
        assert_eq!(find(&all, "reading").map(|l| l.name.as_str()), Some("Reading"));
        assert_eq!(find(&all, "  LEFT THIRD ").map(|l| l.name.as_str()), Some("Left third"));
        assert_eq!(find(&all, "nowhere"), None);
        assert_eq!(find(&all, ""), None);
    }
}
