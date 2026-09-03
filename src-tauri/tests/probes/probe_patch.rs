//! What a change costs: patched against re-walked, on a real folder.
//!
//! ```text
//! ROOT=C:/Users/Brandon cargo test --release --test probe_patch -- --ignored --nocapture
//! ```
#[test]
#[ignore]
fn a_change_costs() {
    use sill_lib::catalog::Catalog;
    use std::path::PathBuf;
    use std::time::Instant;

    let root = PathBuf::from(std::env::var("ROOT").expect("ROOT"));

    let began = Instant::now();
    let catalog = Catalog::build(&[root.clone()]);
    let walked = began.elapsed();

    println!(
        "walk:  {} entries, {} ms, {} KB of path text",
        catalog.len(),
        walked.as_millis(),
        catalog.held() / 1024
    );

    // One file appearing, which is the ordinary case: somebody saved something.
    let one = vec![root.join("a-file-that-was-just-written.txt")];

    let began = Instant::now();
    let patched = catalog.apply(&one, &[]).expect("a patch");
    let patch = began.elapsed();

    println!(
        "patch: 1 file, {} us, {} entries",
        patch.as_micros(),
        patched.len()
    );

    // A branch checkout: a thousand files at once, still one patch.
    let many: Vec<PathBuf> = (0..1000)
        .map(|n| root.join(format!("checked-out-{n}.txt")))
        .collect();

    let began = Instant::now();
    let bulk = catalog.apply(&many, &[]).expect("a patch");
    let thousand = began.elapsed();

    println!(
        "patch: 1000 files, {} ms, {} entries",
        thousand.as_millis(),
        bulk.len()
    );

    println!(
        "one change: {} ms walked against {} ms patched, {}x",
        walked.as_millis(),
        patch.as_micros() as f64 / 1000.0,
        walked.as_micros() / patch.as_micros().max(1)
    );
}
