//! Against the fonts this machine actually has installed.
//!
//! The fixtures in `fonts.rs` cover the gate, the narrowing and the tidying.
//! What they cannot say is whether GDI answers at all, which is what this
//! asks: one enumeration, a few milliseconds, and a face every Windows since
//! 7 has shipped.

#[cfg(windows)]
#[test]
fn segoe_ui_is_installed_on_every_windows() {
    let installed = crate::fonts::installed();

    assert!(
        installed.iter().any(|name| name == "Segoe UI"),
        "Segoe UI is not among the {} families GDI listed",
        installed.len()
    );

    // Tidied: no vertical faces, no duplicates, in order.
    assert!(installed.iter().all(|name| !name.starts_with('@')));
    let mut sorted = installed.clone();
    sorted.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    assert_eq!(sorted.len(), installed.len(), "a family was listed twice");
}
