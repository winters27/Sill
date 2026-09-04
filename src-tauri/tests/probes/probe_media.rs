//! What is playing on this machine, and what asking for it costs.
//!
//! Ignored by default: it reads whatever is playing right now. It **reads
//! only**. Nothing here presses play, pause or next, because a probe that
//! silently paused somebody's music would be worse than no probe.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probes probe_media -- --ignored --nocapture
//! ```

#[test]
#[ignore = "reads what is playing on this machine"]
fn report_now_playing() {
    // A reading of its own, so two probes in one run cannot be told apart by
    // whichever ran first.
    let playing = sill_lib::state::Fresh::<Option<sill_lib::media::NowPlaying>>::new(
        sill_lib::media::FRESH_FOR,
    );

    // The first call pays for the session manager, which is not what a
    // keystroke would pay, so it is timed on its own.
    let start = std::time::Instant::now();
    let first = sill_lib::media::now(&playing);
    let cold = start.elapsed();

    let mut warm = Vec::new();
    for _ in 0..10 {
        let start = std::time::Instant::now();
        let _ = sill_lib::media::now(&playing);
        warm.push(start.elapsed());
    }

    match &first {
        Some(now) => {
            println!("playing: {}", sill_lib::media::title_for(now));
            println!("     {}", sill_lib::media::subtitle_for(now));
            println!(
                "     player {} ({})",
                sill_lib::media::player_for(now),
                now.source
            );
            println!("     next enabled: {}", now.can_next);
        }
        // Not a failure. It is the ordinary state of a machine nobody is
        // playing anything on, and the row is meant to be absent for it.
        None => println!("nothing is playing, so there would be no row"),
    }

    /*
     * What the second matching keystroke actually costs.
     *
     * The number that matters, and neither of the two above is it. The first
     * call pays for the activation factory once per process, and the ten after
     * it are the one second cache answering without asking anything. This
     * waits the cache out, so it is a real reading with the factory already
     * warm: the cost of typing "pause", then "play" a moment later.
     */
    let mut again = Vec::new();
    for _ in 0..5 {
        std::thread::sleep(sill_lib::media::FRESH_FOR + std::time::Duration::from_millis(50));
        let start = std::time::Instant::now();
        let _ = sill_lib::media::now(&playing);
        again.push(start.elapsed());
    }

    let total: std::time::Duration = warm.iter().sum();
    let repeat: std::time::Duration = again.iter().sum();
    println!("\nfirst call {cold:?}");
    println!(
        "then {:?} each over ten, through the one second cache",
        total / warm.len() as u32
    );
    println!(
        "a real reading with the factory warm: {:?} each over five",
        repeat / again.len() as u32
    );
}

/// What a keystroke that is not asking about media costs.
///
/// The measurement behind the item's "costs nothing when not matched": a
/// thousand ordinary queries put through the same gate the search puts them
/// through, with a reader that would panic if it were ever called.
#[test]
#[ignore = "a measurement rather than an assertion"]
fn report_what_a_keystroke_that_is_not_asking_costs() {
    let queries = [
        "c", "ch", "chr", "chro", "chrom", "chrome", "code", "spotify", "firefox", "settings",
    ];

    let start = std::time::Instant::now();

    for _ in 0..100 {
        for query in queries {
            let row = sill_lib::media::matched(query, || {
                panic!("{query} reached the machine and it should not have")
            });
            assert!(row.is_none());
        }
    }

    let spent = start.elapsed();
    println!(
        "1000 non-matching queries through the gate: {spent:?} total, {:?} each",
        spent / 1000
    );
}
