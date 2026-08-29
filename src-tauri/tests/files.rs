//! File search against the real Everything instance.

#[test]
#[cfg(windows)]
fn everything_ipc_returns_results() {
    if !sill_lib::everything_ipc::available() {
        eprintln!("Everything is not running; skipping");
        return;
    }

    let start = std::time::Instant::now();
    let hits = sill_lib::everything_ipc::search("*.md", 20);
    let elapsed = start.elapsed();

    eprintln!("IPC returned {} hits in {:?}", hits.len(), elapsed);

    assert!(
        !hits.is_empty(),
        "the IPC query returned nothing; the protocol or the reply parse is wrong"
    );

    for hit in hits.iter().take(3) {
        eprintln!("  {} <- {}", hit.name, hit.path);
    }

    // A parsed reply must give absolute paths and matching names.
    assert!(
        hits.iter()
            .all(|h| h.path.contains(std::path::MAIN_SEPARATOR)),
        "results should be full paths"
    );
    assert!(
        hits.iter().all(|h| !h.name.is_empty()),
        "every result needs a display name"
    );
    assert!(
        hits.iter()
            .all(|h| h.path.to_lowercase().ends_with(&h.name.to_lowercase())),
        "the name must be the last segment of its own path"
    );
}

#[test]
#[cfg(windows)]
fn ipc_is_faster_than_spawning_the_client() {
    if !sill_lib::everything_ipc::available() {
        return;
    }

    // Warm both paths first so this measures the query, not first-use setup.
    let _ = sill_lib::everything_ipc::search("*.txt", 5);
    let _ = sill_lib::files::search("*.txt", 5);

    let ipc = {
        let start = std::time::Instant::now();
        for _ in 0..5 {
            let _ = sill_lib::everything_ipc::search("*.txt", 20);
        }
        start.elapsed() / 5
    };

    eprintln!("IPC average: {ipc:?}");

    // The whole reason for the protocol work: a query must be cheap enough to
    // run on a keystroke.
    assert!(
        ipc < std::time::Duration::from_millis(150),
        "an IPC query took {ipc:?}, which is too slow to run per keystroke"
    );
}

#[test]
#[cfg(windows)]
fn a_directory_is_marked_as_one() {
    if !sill_lib::everything_ipc::available() {
        return;
    }

    let hits = sill_lib::everything_ipc::search("folder:Windows", 10);
    if hits.is_empty() {
        return;
    }

    assert!(
        hits.iter().any(|h| h.is_dir),
        "a folder: query should return directories flagged as such"
    );
}

#[test]
fn scoping_wraps_the_folder_clause_so_it_applies_to_the_whole_query() {
    let scoped = sill_lib::files::scope("report", &[r"C:\work".to_string()]);

    assert_eq!(
        scoped, r#"report <path:"C:\work">"#,
        "the folder clause has to be grouped, or it binds to the last term only"
    );
}

#[test]
fn scoping_joins_several_folders_as_alternatives() {
    let scoped =
        sill_lib::files::scope("notes", &[r"C:\a".to_string(), r"D:\my files".to_string()]);

    assert_eq!(scoped, r#"notes <path:"C:\a"|path:"D:\my files">"#);
}

#[test]
fn no_folders_leaves_the_query_untouched() {
    // The common case, and it must not pay anything for the feature.
    assert_eq!(sill_lib::files::scope("plain", &[]), "plain");
    assert_eq!(
        sill_lib::files::scope("plain", &["".to_string(), "  ".to_string()]),
        "plain"
    );
}
