//! Against the jump lists this machine has actually accumulated.
//!
//! The fixtures in `jumplists.rs` are built by the same understanding of the
//! format that reads them, so they agree by construction. They cannot say
//! whether a file written by Explorer, by an editor, and by whatever else has
//! opened a document on this machine reads at all.
//!
//! Ignored, because a build agent has no jump lists and a fresh account has
//! almost none:
//!
//! ```text
//! cargo test --lib real_jumplists -- --ignored --nocapture
//! ```

#[test]
#[ignore]
#[cfg(windows)]
fn the_jump_lists_on_this_machine_read() {
    let Some(folder) = crate::jumplists::folder() else {
        println!("no AutomaticDestinations folder");
        return;
    };

    let listing: Vec<std::path::PathBuf> = std::fs::read_dir(&folder)
        .expect("the folder lists")
        .flatten()
        .map(|one| one.path())
        .filter(|one| {
            one.extension().and_then(|one| one.to_str()) == Some("automaticDestinations-ms")
        })
        .collect();

    let bytes: u64 = listing
        .iter()
        .filter_map(|one| one.metadata().ok())
        .map(|one| one.len())
        .sum();

    println!("{} jump lists, {bytes} bytes on disk", listing.len());

    let began = std::time::Instant::now();

    let mut read = 0usize;
    let mut refused = 0usize;
    let mut documents = 0usize;
    let mut widest = 0usize;
    let mut versions: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();

    for path in &listing {
        let named = path
            .file_stem()
            .map(|one| one.to_string_lossy().into_owned())
            .unwrap_or_default();

        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut file = std::io::Cursor::new(bytes);

        // Which version of the entry layout this machine writes. It is the one
        // fact a fixture could never have supplied: every jump list here is
        // version 6, which no description of the format names, and read as the
        // version 4 that is documented they yield nothing at all.
        if let Ok(document) = crate::jumplists::Compound::open(&mut file) {
            if let Ok(Some(stream)) = document.stream(&mut file, "DestList") {
                if stream.len() >= 4 {
                    let version = u32::from_le_bytes([stream[0], stream[1], stream[2], stream[3]]);
                    *versions.entry(version).or_insert(0usize) += 1;
                }
            }
        }

        match crate::jumplists::documents_in(&mut file, &named) {
            Ok(found) => {
                read += 1;
                documents += found.len();
                widest = widest.max(found.len());
            }
            Err(err) => {
                refused += 1;
                println!("  refused {named}: {err}");
            }
        }
    }

    let took = began.elapsed();

    println!("read {read}, refused {refused}, {documents} documents, {took:?}");
    println!("DestList versions: {versions:?}");
    println!("the fullest single list held {widest}");

    // The claim this exists to check. A parser that is wrong about the
    // container or the entry layout does not produce a few odd rows, it
    // produces none at all, and a fixture would never say so.
    assert!(
        read > 0 && documents > 0,
        "not one jump list on a machine with {} of them produced a document",
        listing.len()
    );

    /*
     * And not one of them was given up on.
     *
     * A refusal here is the container reader failing on a file Windows wrote,
     * and it is silent in ordinary use: the other two hundred still answer, so
     * the list looks fine while the jump lists with the most history in them
     * are missing from it. Both of the files that did this were the two
     * largest, and both end part way through a sector.
     */
    assert_eq!(
        refused, 0,
        "{refused} jump lists could not be read at all, and the largest ones are \
         exactly the ones a container bug loses"
    );
}

#[test]
#[ignore]
#[cfg(windows)]
fn what_the_rows_would_say() {
    let began = std::time::Instant::now();
    let found = crate::jumplists::recent();
    let took = began.elapsed();

    println!("{} documents kept, read in {took:?}", found.len());

    // Through the same call the search makes, so what is printed is the rows
    // themselves: filtered, checked for existing, and told which are folders.
    let rows = crate::jumplists::matched(
        "recent",
        || found.clone(),
        |path| std::fs::metadata(path).ok().map(|found| found.is_dir()),
    );

    let gone = found
        .iter()
        .take(25)
        .filter(|one| !std::path::Path::new(&one.path).exists())
        .count();

    for one in &rows {
        println!(
            "  {:<44}  {:<56}  {}",
            crate::jumplists::title_for(one),
            crate::jumplists::subtitle_for(one),
            one.source
        );
    }

    println!("{gone} of the newest 25 no longer exist and are not offered");

    // Asserted, because a parser that has stopped working produces an empty
    // list rather than a wrong one, and an empty list prints tidily.
    assert!(
        !rows.is_empty(),
        "a machine with {} remembered documents offered no rows",
        found.len()
    );
    assert!(
        rows.iter()
            .all(|one| std::path::Path::new(&one.path).exists()),
        "a row was offered for something that is not there"
    );

    // Ordering is the whole of the list's usefulness: the newest first.
    for pair in found.windows(2) {
        assert!(
            pair[0].at >= pair[1].at,
            "the list is not newest first: {} before {}",
            pair[0].path,
            pair[1].path
        );
    }
}

/// What one query costs, warm, on this machine.
#[test]
#[ignore]
#[cfg(windows)]
fn what_asking_costs() {
    // Warm, so the number is the parsing rather than the disk.
    let _ = crate::jumplists::recent();

    let began = std::time::Instant::now();
    let found = crate::jumplists::recent();
    let warm = began.elapsed();

    let began = std::time::Instant::now();
    for query in ["chrome", "notepad", "2+2", "rec", "recycle", "sill", "code"] {
        assert!(crate::jumplists::matched(query, Vec::new, |_| Some(false)).is_empty());
    }
    let sixteen = began.elapsed();

    println!("warm reading:      {warm:?} for {} documents", found.len());
    println!("seven non-matches: {sixteen:?}");
}
