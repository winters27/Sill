//! `ext:`, `size:` and `date:` against the index this machine really has.
//!
//! The fixtures in `catalog.rs` build four files in a temporary folder and
//! agree with the parser by construction. They cannot say what an operator
//! costs on fifty thousand entries, nor whether the numbers the walk read off
//! the disk are the numbers a person would recognise.
//!
//! Ignored, because it walks a home folder and takes seconds:
//!
//! ```text
//! cargo test --lib real_operators -- --ignored --nocapture
//! ```

#[test]
#[ignore]
#[cfg(windows)]
fn operators_on_the_real_index() {
    use std::time::Instant;

    let Some(home) = std::env::var_os("USERPROFILE") else {
        println!("no USERPROFILE");
        return;
    };

    let roots = vec![std::path::PathBuf::from(&home)];

    let began = Instant::now();
    let catalog = crate::catalog::Catalog::build(&roots);
    let walk = began.elapsed();

    let entries = catalog.len();
    if entries == 0 {
        println!("nothing indexed under {home:?}");
        return;
    }

    // What the two new fields cost, said in the units the audit uses.
    println!("walked {entries} entries in {walk:?}");
    println!("  path text: {} KB", catalog.held() / 1024);
    println!("  slots: {} KB at 24 bytes each", entries * 24 / 1024);
    println!("  of which new: {} KB at 8 bytes each", entries * 8 / 1024);

    // A query somebody would actually type, and the same query narrowed. The
    // claim under test is that the first is not slower for the second existing.
    let plain = "report";
    let queries = [
        plain,
        "report ext:pdf",
        "report size:>1mb",
        "report date:month",
        "ext:pdf",
        "size:>100mb",
        "date:today",
    ];

    for query in queries {
        // Warm, then timed. The first search of a process pages the arena in.
        let _ = catalog.search(query, 20, &[]);

        let began = Instant::now();
        let rounds = 20;
        let mut found = Vec::new();
        for _ in 0..rounds {
            found = catalog.search(query, 20, &[]);
        }
        let each = began.elapsed() / rounds;

        println!("{each:>10.2?}  {:>4} rows  {query}", found.len());
    }

    // And the parse on its own, which is what every keystroke pays whether or
    // not it used an operator.
    for typed in [
        "quarterly budget report",
        r"C:\work\notes",
        "report ext:pdf",
    ] {
        let rounds = 200_000;
        let began = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(crate::catalog::operators(std::hint::black_box(typed)));
        }
        let each = began.elapsed() / rounds;
        println!("parse {each:>10.2?}  {typed}");
    }

    // The answers have to be right as well as fast. Every row of an `ext:`
    // search really is that kind of file, on this machine's real names rather
    // than on four the test wrote itself.
    let pdfs = catalog.search("ext:pdf", 50, &[]);
    for hit in &pdfs {
        assert!(
            hit.name.to_ascii_lowercase().ends_with(".pdf"),
            "{} is not a PDF",
            hit.path
        );
    }
    println!("ext:pdf returned {} rows, all of them PDFs", pdfs.len());

    // And a size filter really does exclude the small ones. Read back off the
    // disk rather than out of the index, so this checks what the walk stored
    // rather than repeating it.
    let big = catalog.search("size:>10mb", 50, &[]);
    for hit in &big {
        let Ok(md) = std::fs::metadata(&hit.path) else {
            continue;
        };

        assert!(
            md.len() > 10 * 1024 * 1024,
            "{} is {} bytes, which is not over ten megabytes",
            hit.path,
            md.len()
        );
    }
    println!("size:>10mb returned {} rows, all of them big", big.len());
}
