//! What each program's own volume is, and what asking costs.
//!
//! Ignored by default: it reads whatever is playing on this machine right now.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_app_volume -- --ignored --nocapture
//! ```

#[test]
#[ignore = "reads what is playing on this machine"]
fn report_sessions() {
    // A reading of its own. It used to reach for a process-wide cache,
    // so two probes in one run could not be told apart.
    let playing = sill_lib::state::Fresh::<Vec<sill_lib::app_volume::Session>>::new(
        sill_lib::app_volume::FRESH_FOR,
    );
    // The first call pays for the COM apartment and the device, which is not
    // what a keystroke would pay, so it is timed separately from the rest.
    let start = std::time::Instant::now();
    let first = sill_lib::app_volume::sessions(&playing);
    let cold = start.elapsed();

    let mut warm = Vec::new();
    for _ in 0..10 {
        let start = std::time::Instant::now();
        let _ = sill_lib::app_volume::sessions(&playing);
        warm.push(start.elapsed());
    }

    println!("{} session(s)", first.len());
    for session in &first {
        println!(
            "   {:<22} {:>4}%{}  {}",
            session.name,
            (session.volume * 100.0).round() as i32,
            if session.muted { "  muted" } else { "       " },
            session.id,
        );
    }

    let total: std::time::Duration = warm.iter().sum();
    println!("\nfirst call {cold:?}");
    println!("then {:?} each over ten", total / warm.len() as u32);
}
