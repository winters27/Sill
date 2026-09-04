//! What a window preview costs.
//!
//! It is taken while somebody arrows down a list, so it has to be cheap enough
//! that holding the key does not stutter. Ignored by default: it photographs
//! whatever is open on this machine.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_preview_cost -- --ignored --nocapture
//! ```

#[test]
#[ignore = "photographs windows open on this machine"]
fn a_preview_is_cheap_enough_to_take_while_arrowing() {
    let windows = sill_lib::windowing::list();
    let visible: Vec<_> = windows
        .into_iter()
        .filter(|w| !w.minimized)
        .take(5)
        .collect();

    if visible.is_empty() {
        println!("nothing open to photograph");
        return;
    }

    println!(
        "{:<34} {:>7} {:>7} {:>9}",
        "window", "capture", "shrink", "encode"
    );

    for window in &visible {
        let start = std::time::Instant::now();
        let Ok(shot) = sill_lib::capture::window(
            &sill_lib::privacy::allowed_regardless(),
            window.id,
            (
                window.rect.x,
                window.rect.y,
                window.rect.width,
                window.rect.height,
            ),
        ) else {
            println!("{:<34} refused", &window.app);
            continue;
        };
        let captured = start.elapsed();

        let start = std::time::Instant::now();
        let small = sill_lib::capture::thumbnail(&shot, 480);
        let shrunk = start.elapsed();

        let start = std::time::Instant::now();
        let png = small.to_png().expect("encodes");
        let encoded = start.elapsed();

        println!(
            "{:<34} {:>5} ms {:>5} ms {:>7} ms   {}x{} -> {}x{}, {} KB",
            window.app.chars().take(32).collect::<String>(),
            captured.as_millis(),
            shrunk.as_millis(),
            encoded.as_millis(),
            shot.width,
            shot.height,
            small.width,
            small.height,
            png.len() / 1024,
        );
    }
}
