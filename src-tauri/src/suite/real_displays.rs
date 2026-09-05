//! Against the displays this machine actually has.
//!
//! Nothing here changes a mode. The fixtures in `displays.rs` cover the
//! gate, the ordering and the row's target; this asks the one thing they
//! cannot, whether the enumeration answers and agrees with itself: the mode a
//! display is in has to be among the modes it lists.

#[cfg(windows)]
#[test]
fn the_current_mode_is_among_the_listed_ones() {
    let devices = crate::displays::devices();
    assert!(!devices.is_empty(), "no attached display was found");

    for (index, device) in &devices {
        let modes = crate::displays::modes(device, *index);
        assert!(!modes.is_empty(), "{device} listed no 32-bit modes");

        let current: Vec<_> = modes.iter().filter(|mode| mode.current).collect();
        assert_eq!(
            current.len(),
            1,
            "{device} should be in exactly one of the modes it lists, found {}",
            current.len()
        );

        // Widest first, which is the order the rows are read in.
        let widths: Vec<u32> = modes.iter().map(|mode| mode.width).collect();
        let mut sorted = widths.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(widths, sorted, "{device} modes are not widest first");
    }
}
