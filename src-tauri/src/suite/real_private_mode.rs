//! Private mode against this machine, rather than against a value.
//!
//! The tests in `private_mode.rs` ask each gate what it answers. These two ask
//! the machine: one actually photographs this screen and then shows that the
//! same call cannot be made with private mode on, and one writes a preferences
//! file and reads it back to show the mode survives a restart.
//!
//! Ignored, because the first opens a real device context and the second
//! writes a file. Never Sill's own preferences: a temporary path, deleted at
//! the end.
//!
//! ```text
//! cargo test --lib real_private_mode -- --ignored --nocapture
//! ```

/// The screen is really photographed, and then really is not.
///
/// The capture here is the same function every screenshot in Sill goes
/// through, called the same way, so "private mode stops screen capture" is
/// demonstrated rather than asserted about a flag.
#[test]
#[ignore]
#[cfg(windows)]
fn a_screenshot_is_taken_and_then_refused() {
    let privacy = crate::privacy::Privacy::default();

    let allowed = crate::privacy::allow(&privacy).expect("an ordinary Sill may photograph");
    let shot = crate::capture::region(&allowed, 0, 0, 320, 200).expect("the screen is readable");

    println!(
        "with private mode off: a {}x{} picture, {} bytes",
        shot.width,
        shot.height,
        shot.pixels.len()
    );
    assert_eq!(shot.pixels.len(), 320 * 200 * 4);
    assert!(
        shot.pixels.iter().any(|&byte| byte != 0),
        "the capture came back entirely black, so this proves nothing"
    );

    privacy.set(true);

    let refused = crate::privacy::allow(&privacy)
        .expect_err("private mode allowed the screen to be photographed");
    println!("with private mode on:  {refused}");

    /*
     * And there is no second way round it.
     *
     * `capture::region` takes a `privacy::Allowed`, whose only field is
     * private to `privacy`, so the line below cannot be written without the
     * permission this call just refused. That is a compile-time fact and this
     * comment is the only place a probe can point at it; `verify-source` is
     * what keeps `allowed_regardless` out of the paths a person's screen goes
     * through.
     */
    assert_eq!(refused, crate::privacy::REFUSED);
}

/// The mode is still on after Sill is restarted.
///
/// The failure this exists to prevent is somebody switching private mode on,
/// Sill restarting for any reason, and everything they paused quietly coming
/// back. So it is written to a preferences file and read out of one, through
/// the same two functions the application uses.
#[test]
#[ignore]
fn private_mode_survives_a_restart() {
    let path = std::env::temp_dir().join(format!(
        "sill-private-mode-{}-{:?}.json",
        std::process::id(),
        std::thread::current().id()
    ));

    let mut prefs = crate::preferences::Preferences::default();
    prefs.clipboard.enabled = true;
    prefs.privacy.paused = true;
    prefs.save(&path).expect("the file writes");

    let read = crate::preferences::Preferences::load(&path);

    println!("wrote {}", path.display());
    println!(
        "read back: privacy.paused = {}, clipboard.enabled = {}",
        read.privacy.paused, read.clipboard.enabled
    );

    assert!(
        read.privacy.paused,
        "private mode was forgotten across a restart, which is the failure it exists to prevent"
    );
    // The setting it overrides is untouched, so switching private mode off
    // gives back exactly what was there before.
    assert!(read.clipboard.enabled);
    assert!(
        !crate::clipboard::monitor::records(&crate::privacy::clipboard_rules(&read), None),
        "a Sill started with private mode on would record the clipboard"
    );

    let _ = std::fs::remove_file(&path);
}
