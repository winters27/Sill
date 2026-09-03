//! Two sources of file results, shown as one list.

/// Reaches the merge through the command module, which is where it lives.
///
/// Declared here rather than reaching into private code: the behaviour worth
/// pinning is what somebody sees, and what they see is one list with nothing
/// in it twice.
use crate::files::FileHit;

fn hit(path: &str) -> FileHit {
    FileHit {
        name: path.rsplit('\\').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        is_dir: false,
    }
}

#[test]
fn the_same_file_from_both_sources_is_listed_once() {
    // A file under an indexed folder is found by both, and a whole-volume
    // indexer reports paths with whatever capitalisation the disk has.
    let ours = vec![hit(r"C:\Sill\src\registry.rs")];
    let theirs = vec![
        hit(r"c:\sill\src\registry.rs"),
        hit(r"C:\Other\registry.rs"),
    ];

    let merged = crate::commands::search::merge(ours, theirs, 10);
    let paths: Vec<&str> = merged.iter().map(|h| h.path.as_str()).collect();

    assert_eq!(
        paths,
        vec![r"C:\Sill\src\registry.rs", r"C:\Other\registry.rs"]
    );
}

#[test]
fn our_own_results_come_first() {
    // Ours is ranked by the same code as every other row. A whole-volume
    // indexer has its own idea of relevance, and interleaving the two would
    // mean neither order survives.
    let ours = vec![hit(r"C:\work\notes.md")];
    let theirs = vec![hit(r"C:\Windows\other.md")];

    let merged = crate::commands::search::merge(ours, theirs, 10);

    assert_eq!(merged[0].path, r"C:\work\notes.md");
}

#[test]
fn the_limit_is_the_limit_however_many_arrive() {
    let ours = vec![hit(r"C:\a.md"), hit(r"C:\b.md")];
    let theirs = vec![hit(r"C:\c.md"), hit(r"C:\d.md")];

    assert_eq!(crate::commands::search::merge(ours, theirs, 3).len(), 3);
}

#[test]
fn one_source_answering_nothing_is_not_a_problem() {
    // The ordinary case on a machine with no whole-volume indexer, and the
    // ordinary case before our own index has finished its first walk.
    let only_ours = crate::commands::search::merge(vec![hit(r"C:\a.md")], Vec::new(), 10);
    assert_eq!(only_ours.len(), 1);

    let only_theirs = crate::commands::search::merge(Vec::new(), vec![hit(r"C:\b.md")], 10);
    assert_eq!(only_theirs.len(), 1);

    assert!(crate::commands::search::merge(Vec::new(), Vec::new(), 10).is_empty());
}
