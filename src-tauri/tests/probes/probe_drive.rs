//! What a whole drive costs to index.
#[test]
#[ignore]
fn measure() {
    let root = std::env::var("ROOT").unwrap();
    let noise = sill_lib::catalog::NOISE;
    // Skipped only where a drive begins, not everywhere.
    let system = [
        "Windows",
        "Program Files",
        "Program Files (x86)",
        "ProgramData",
        "$Recycle.Bin",
        "System Volume Information",
        "Recovery",
        "PerfLogs",
        "Config.Msi",
        "Documents and Settings",
        "inetpub",
        "MSOCache",
        "$WinREAgent",
        "OneDriveTemp",
        "Intel",
        "PerfBoost",
    ];

    let start = std::time::Instant::now();
    let mut files = 0usize;
    let mut bytes = 0usize;

    let mut builder = ignore::WalkBuilder::new(&root);
    builder
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .hidden(true)
        .follow_links(false)
        .threads(6)
        .filter_entry(move |e| {
            let Some(name) = e.file_name().to_str() else {
                return false;
            };
            if noise.contains(&name) {
                return false;
            }
            // Depth 1 is a direct child of the drive.
            if e.depth() == 1 && system.contains(&name) {
                return false;
            }
            true
        });

    for entry in builder.build().flatten() {
        if entry.file_type().is_some_and(|t| t.is_file()) {
            files += 1;
            bytes += entry.path().as_os_str().len();
        }
    }

    println!(
        "{root}: {files} files, {:.1} MB of paths, {} ms",
        bytes as f64 / 1_048_576.0,
        start.elapsed().as_millis()
    );
}
