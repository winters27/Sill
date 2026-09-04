//! How long a preview of a real file takes, on real files.
//!
//! The item's claim is that a preview appears within 100 ms of the selection
//! settling. Everything but the read is fixed: the settle is a 90 ms timer in
//! the strip, the round trip is one `invoke`, and what is left to measure is
//! the part that touches somebody's disk and can be any size at all.
//!
//! So this points the reader at whatever is really in a home folder, found by
//! the index rather than by a list written here, and reports the median, the
//! 95th and the worst rather than an average that hides both ends.
//!
//! It has already earned its keep. The first run said a binary that shows
//! nothing cost a **mean of 22 ms and a worst of 374 ms**, all of it a cold
//! read of a file that was then thrown away, which is what put the exception
//! list in `previews`. Ignored, because it walks a home folder:
//!
//! ```text
//! cargo test --lib real_previews -- --ignored --nocapture
//! ```

#[test]
#[ignore]
#[cfg(windows)]
fn a_preview_of_a_real_file_is_well_inside_the_budget() {
    use std::time::{Duration, Instant};

    let Some(home) = std::env::var_os("USERPROFILE") else {
        println!("no USERPROFILE");
        return;
    };

    let catalog = crate::catalog::Catalog::build(&[std::path::PathBuf::from(&home)]);
    if catalog.is_empty() {
        println!("nothing indexed under {home:?}");
        return;
    }

    let previews = crate::previews::Previews::new();

    // Found with the operators this item also added, which is the only way to
    // ask an index for "a hundred files of that kind" without a second walk.
    for kind in [
        "ext:md,txt,json,rs,ts,log",
        "ext:png,jpg,jpeg,gif,webp",
        "ext:exe,dll,zip,7z",
        "ext:pdf",
    ] {
        let files = catalog.search(kind, 100, &[]);

        if files.is_empty() {
            println!("{kind}: nothing on this machine");
            continue;
        }

        let mut took: Vec<(Duration, &str)> = Vec::new();
        let mut shown = 0usize;

        for hit in &files {
            // Fresh each time. The cache is what makes arrowing back cheap and
            // it would make this measure a `HashMap` lookup.
            previews.forget_files();

            let began = Instant::now();
            let look = previews.of_file(&hit.path);
            took.push((began.elapsed(), hit.path.as_str()));

            if look.is_some() {
                shown += 1;
            }
        }

        took.sort_unstable();

        let middle = took[took.len() / 2];
        let ninety_five = took[took.len() * 95 / 100];
        let worst = *took.last().expect("at least one");

        println!(
            "{kind}\n  {} files, {shown} with something to show\n  \
             median {:?}, 95th {:?}, worst {:?}\n  worst was {}",
            files.len(),
            middle.0,
            ninety_five.0,
            worst.0,
            worst.1,
        );

        /*
         * The claim, on the half that is not a fixed timer.
         *
         * The median rather than the worst, and deliberately. What is being
         * measured past the first few is **the disk**, not Sill: the worst file
         * of a hundred is one nothing has touched since it was written, and no
         * amount of care here makes a cold seek faster. What the median says is
         * that a preview of a file somebody is plausibly looking for arrives
         * well inside the budget, and what the 95th says is how bad the tail
         * gets. Both are printed so a regression in either is visible.
         *
         * A debug build reading off whatever disk is under it, so the ceiling
         * is generous. It is still tight enough that a preview which decoded or
         * re-encoded a picture would not fit.
         */
        assert!(
            middle.0 < Duration::from_millis(100),
            "the median preview of {} took {:?}, which is the whole budget",
            kind,
            middle.0
        );
    }

    // And the second look at the same file, which is what arrowing down a list
    // and back up again costs.
    let some = catalog.search("ext:md,txt,json", 20, &[]);
    if let Some(hit) = some.first() {
        previews.forget_files();
        let _ = previews.of_file(&hit.path);

        let began = Instant::now();
        for _ in 0..100 {
            std::hint::black_box(previews.of_file(&hit.path));
        }
        println!("second look: {:?} each", began.elapsed() / 100);
    }
}
