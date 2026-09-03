//! How big a home folder actually is once the noise is left out.
#[test]
#[ignore]
fn measure() {
    let root = std::env::var("ROOT").unwrap();
    let skip = [
        "node_modules",
        "target",
        ".git",
        "dist",
        "build",
        ".svelte-kit",
        "__pycache__",
        ".next",
        ".cargo",
        ".rustup",
        ".gradle",
        ".m2",
        "AppData",
        ".venv",
        "venv",
        "vendor",
        ".pnpm-store",
        ".cache",
    ];

    for (label, respect) in [("raw", false), ("gitignore + noise", true)] {
        let start = std::time::Instant::now();
        let mut files = 0usize;
        let mut bytes = 0usize;

        let mut builder = ignore::WalkBuilder::new(&root);
        builder
            .hidden(respect)
            .git_ignore(respect)
            .git_global(respect)
            .git_exclude(respect)
            .ignore(respect)
            .follow_links(false)
            .threads(std::thread::available_parallelism().map_or(4, |n| n.get().min(8)));

        if respect {
            builder.filter_entry(move |e| {
                !e.file_name()
                    .to_str()
                    .is_some_and(|name| skip.contains(&name))
            });
        }

        for entry in builder.build().flatten() {
            if entry.file_type().is_some_and(|t| t.is_file()) {
                files += 1;
                bytes += entry.path().as_os_str().len();
            }
        }

        println!(
            "{label:>18}: {files:>9} files  {:>7.1} MB of paths  {:>6} ms",
            bytes as f64 / 1_048_576.0,
            start.elapsed().as_millis()
        );
    }
}

/// Can the existing matcher rank a whole home folder per keystroke?
#[test]
#[ignore]
fn ranking_cost() {
    use sill_lib::registry::{self, Aliases, CommandRecord, Excluded, Frecency};

    let root = std::env::var("ROOT").unwrap();
    let skip = [
        "node_modules",
        "target",
        ".git",
        "dist",
        "build",
        ".svelte-kit",
        "__pycache__",
        ".next",
        ".cargo",
        ".rustup",
        ".gradle",
        ".m2",
        "AppData",
        ".venv",
        "venv",
        "vendor",
        ".pnpm-store",
        ".cache",
    ];

    let mut builder = ignore::WalkBuilder::new(&root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .follow_links(false)
        .threads(8)
        .filter_entry(move |e| !e.file_name().to_str().is_some_and(|n| skip.contains(&n)));

    let records: Vec<CommandRecord> = builder
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| {
            let path = e.path().to_string_lossy().to_string();
            let name = e.file_name().to_string_lossy().to_string();
            CommandRecord {
                id: path.clone(),
                extension: "file".into(),
                extension_title: "Files".into(),
                command: name.clone(),
                title: name,
                subtitle: path.clone(),
                description: String::new(),
                mode: "file".into(),
                entrypoint: path,
                keywords: Vec::new(),
                icon: None,
                panel: None,
                preferences: serde_json::Value::Null,
                toggle: None,
            }
        })
        .collect();

    println!("corpus: {} files", records.len());

    for query in ["r", "re", "reg", "regi", "registry", "registry.rs"] {
        let start = std::time::Instant::now();
        let found = registry::search_excluding(
            records.iter(),
            query,
            &Frecency::default(),
            &Aliases::default(),
            1_756_000_000,
            60,
            Excluded::none(),
            // Nothing pinned. Added when search_excluding grew the
            // parameter; this file was one of forty-five separate binaries
            // and had not compiled since.
            &[],
        );
        println!(
            "  {query:>12} -> {:>5} hits in {:>5} ms",
            found.len(),
            start.elapsed().as_millis()
        );
    }
}

/// How much of the corpus a first-character bucket rules out.
#[test]
#[ignore]
fn bucket_selectivity() {
    use std::collections::HashMap;

    let root = std::env::var("ROOT").unwrap();
    let skip = [
        "node_modules",
        "target",
        ".git",
        "dist",
        "build",
        ".svelte-kit",
        "__pycache__",
        ".next",
        ".cargo",
        ".rustup",
        ".gradle",
        ".m2",
        "AppData",
        ".venv",
        "venv",
        "vendor",
        ".pnpm-store",
        ".cache",
    ];
    let mut builder = ignore::WalkBuilder::new(&root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .follow_links(false)
        .threads(8)
        .filter_entry(move |e| !e.file_name().to_str().is_some_and(|n| skip.contains(&n)));

    let names: Vec<String> = builder
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();

    // Every character that begins a word, which is where a match may start.
    let mut buckets: HashMap<char, usize> = HashMap::new();
    for name in &names {
        let mut seen: Vec<char> = Vec::new();
        let chars: Vec<char> = name.chars().collect();
        for (at, &ch) in chars.iter().enumerate() {
            let starts = at == 0 || matches!(chars[at - 1], ' ' | '-' | '_' | '.' | '/' | ':');
            if starts && !seen.contains(&ch) {
                seen.push(ch);
            }
        }
        for ch in seen {
            *buckets.entry(ch).or_default() += 1;
        }
    }

    let total = names.len();
    let mut rows: Vec<(usize, char)> = buckets.iter().map(|(c, n)| (*n, *c)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));

    println!("corpus {total} files. Worst first characters:");
    for (n, ch) in rows.iter().take(8) {
        println!(
            "  {ch:?} -> {n:>6} candidates ({:>4.1}% of the corpus)",
            *n as f64 * 100.0 / total as f64
        );
    }
    let sum: usize = rows.iter().map(|(n, _)| n).sum();
    println!(
        "  average bucket {:.0} ({:.1}%)",
        sum as f64 / rows.len() as f64,
        sum as f64 * 100.0 / (rows.len() * total) as f64
    );
}
