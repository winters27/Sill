//! What the real index costs, on a real home folder.
#[test]
#[ignore]
fn cost() {
    use sill_lib::catalog::Catalog;
    use std::path::PathBuf;

    let root = PathBuf::from(std::env::var("ROOT").unwrap());

    let start = std::time::Instant::now();
    let catalog = Catalog::build(&[root]);
    let built = start.elapsed();

    println!("built {} entries in {} ms", catalog.len(), built.as_millis());

    for query in ["r", "re", "reg", "registry", "registry.rs", ".rs", "package.json", "m", "s"] {
        // Ten runs, because one is noise at this scale.
        let start = std::time::Instant::now();
        let mut hits = 0;
        for _ in 0..10 {
            hits = catalog.search(query, 60).len();
        }
        println!(
            "  {query:>14} -> {hits:>3} hits, {:>6.2} ms each",
            start.elapsed().as_secs_f64() * 100.0
        );
    }

    let found = catalog.search("registry.rs", 5);
    println!("\n  registry.rs resolves to:");
    for hit in found.iter().take(3) {
        println!("    {}", hit.path);
    }
}

/// How far apart the letters of a scattered match actually sit.
#[test]
#[ignore]
fn gaps() {
    use sill_lib::registry::{self, MatchClass};

    fn report(label: &str, query: &str, text: &str) {
        let needle: Vec<char> = query.to_lowercase().chars().collect();
        match registry::match_name(&needle, text) {
            Some((MatchClass::TitleSubsequence, at)) => {
                let widest = at.windows(2).map(|w| w[1] - w[0] - 1).max().unwrap_or(0);
                let span = at.last().unwrap_or(&0) - at.first().unwrap_or(&0) + 1;
                println!(
                    "  {label:<8} {query:>12} -> widest gap {widest:>3}, span {span:>3}, name {:>3} chars",
                    text.chars().count()
                );
            }
            Some((class, _)) => println!("  {label:<8} {query:>12} -> {class:?}, not scattered"),
            None => println!("  {label:<8} {query:>12} -> no match"),
        }
    }

    println!("WANTED:");
    report("keep", "steam", "StreamNook");
    report("keep", "strm", "StreamNook");
    report("keep", "disc", "Disk Cleanup");

    println!("JUNK:");
    report("drop", "registry.rs", "An app that allows anyone to be a radio dj. Like you start playing music from your library, but can be live to where others can listen to what you are playing as well. A sort of social personal radio.md");
    report("drop", "tada", "Team Device Management");
    report("drop", "note", "Node.js website");
    report("drop", "package.json", "Presentation And Compatibility Knowledge Assistant Guide Everyone Justifies Simply Or Not");
}
