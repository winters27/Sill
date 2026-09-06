//! Installing a real extension from the store, end to end.
//!
//! Not part of `npm run verify`. It fetches from two services and runs npm, so
//! it is slow, it needs a network, and it can fail for reasons that are not
//! about this code. `#[ignore]` keeps it out of the suite and reachable by
//! name:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test store_install -- --ignored --nocapture
//! ```
//!
//! What it proves is the part nothing else can. Every other test here is a
//! function over values; this is the one that says the catalogue really
//! answers, the commit really resolves, the source really arrives, npm really
//! installs, esbuild really builds it and the index really lists it. It prints
//! the bundle it produced so `scripts/run-extension.mjs` can be pointed at it,
//! which is the last step and the only one that runs the extension.

#![cfg(windows)]

use std::path::PathBuf;

use sill_lib::store::{catalog, install, Origin};

/// The extension this installs.
///
/// `uuid-generator` on purpose: it is the one the view gate already builds, and
/// it is the reason third party dependencies are not optional. Three of its
/// imports (`uuid`, `typeid-js`, `ulidx`) are packages, so a store that fetched
/// source and skipped npm would build one of its nine commands and fail on the
/// rest.
const EXTENSION: &str = "uuid-generator";

/// esbuild, found the way a development build finds it.
fn esbuild() -> PathBuf {
    if let Some(named) = std::env::var_os("SILL_ESBUILD") {
        return PathBuf::from(named);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join("host")
        .join("node_modules")
        .join("@esbuild")
        .join("win32-x64")
        .join("esbuild.exe")
}

#[tokio::test]
#[ignore = "reaches the network and runs npm"]
async fn a_real_extension_installs_from_the_store() {
    let root = std::env::temp_dir().join("sill-store-e2e");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a place to install into");

    // ---------------------------------------------------------- the catalogue
    let catalog = catalog::load(&root, true).await.expect("the store answers");

    assert!(
        catalog.listings.len() > 500,
        "only {} listings came back, which is not the catalogue",
        catalog.listings.len()
    );

    assert!(
        catalog
            .listings
            .iter()
            .all(|listing| { listing.platforms.is_empty() || listing.declares_windows() }),
        "an extension that names macOS and not Windows reached the catalogue"
    );

    // The icon, which the first version of the reduce dropped. A store of
    // lettered tiles is the failure with no error in it, so this asserts a
    // real proportion rather than merely that the field parses.
    let with_icons = catalog
        .listings
        .iter()
        .filter(|listing| listing.icon.starts_with("https://"))
        .count();

    println!(
        "catalogue: {} listings, {} of them naming Windows, {} with icons",
        catalog.listings.len(),
        catalog
            .listings
            .iter()
            .filter(|listing| listing.declares_windows())
            .count(),
        with_icons,
    );

    assert!(
        with_icons * 2 > catalog.listings.len(),
        "only {with_icons} of {} listings carried an icon, so the store would draw \
         mostly lettered tiles",
        catalog.listings.len()
    );

    // The cache is what a second open reads instead of fetching again.
    assert!(
        catalog::cache_path(&root).is_file(),
        "the catalogue was fetched and not written, so every open would refetch"
    );

    let listing = catalog
        .listings
        .iter()
        .find(|listing| listing.name == EXTENSION)
        .unwrap_or_else(|| panic!("{EXTENSION} is not in the store"))
        .clone();

    println!(
        "installing {} from {} at {}",
        listing.title, listing.folder, listing.revision
    );

    // ------------------------------------------------------------ step one
    //
    // The progress reports go to the console rather than nowhere. A fetch
    // that stalls looks identical to one that is slow, and this test waits on
    // a network, so the one thing worth having in its output is how far it
    // got before it stopped.
    let watching = |progress: sill_lib::extension_install::Progress| println!("  {progress:?}");
    let prepared = install::prepare(&root, &listing, None, &watching)
        .await
        .expect("the source is fetched");

    assert_eq!(prepared.name, EXTENSION);
    assert!(prepared.files > 5, "only {} files arrived", prepared.files);
    assert_eq!(prepared.revision, listing.revision, "installed at the pin");

    // The three packages the view gate's workflow says are not optional.
    for package in ["uuid", "typeid-js", "ulidx"] {
        assert!(
            prepared.packages.iter().any(|it| it == package),
            "{package} is missing from the packages this reports, and esbuild \
             fails on an unresolved import rather than warning"
        );
    }

    assert!(
        prepared.commands.iter().any(|command| command.runnable),
        "nothing in it can run"
    );

    println!(
        "prepared: {} files, {} bytes, {} packages, {} capabilities",
        prepared.files,
        prepared.bytes,
        prepared.packages.len(),
        prepared.capabilities.len()
    );
    for reach in &prepared.capabilities {
        println!(
            "  {} {} ({})",
            if reach.mediated {
                "via Sill  "
            } else {
                "direct    "
            },
            reach.title,
            reach.seen_in.join(", ")
        );
    }

    // Nothing has been installed yet, which is the whole reason for two steps.
    assert!(
        !root.join("extensions").join(EXTENSION).exists(),
        "preparing installed something, so the screen that asks would be asking \
         about a thing that had already happened"
    );

    // ------------------------------------------------------------ step two
    let node = sill_lib::host::node_exe(&std::sync::Mutex::new(None), None)
        .expect("this test needs Node on PATH");
    let done = install::finish(&root, &esbuild(), &node, EXTENSION).expect("it builds");

    println!(
        "installed {} at {}: {}",
        done.installed.title,
        done.revision,
        done.installed.commands.join(", ")
    );

    let home = root.join("extensions");
    let installed = home.join(EXTENSION);

    // The bundles.
    for command in ["generateV7", "viewHistory"] {
        let bundle = installed.join(format!("{command}.js"));
        assert!(bundle.is_file(), "{} was not built", bundle.display());
        assert!(
            bundle.metadata().expect("readable").len() > 500,
            "{command} built to almost nothing"
        );
    }

    // The marker without which Node loads a CommonJS bundle as an ES module.
    assert!(installed.join("package.json").is_file());

    // The index, which is what the launcher searches.
    let index = sill_lib::registry::load_index(&home.join("index.json"));
    assert!(
        index
            .iter()
            .any(|record| record.id == "uuid-generator:generateV7"),
        "the index does not list the command that was built"
    );

    // The pin, which is what makes "out of date" a comparison.
    let origin = sill_lib::store::origin_of(&home, EXTENSION).expect("an origin was recorded");
    assert_eq!(origin.source, "store");
    assert_eq!(origin.revision, listing.revision);
    assert_eq!(origin.listing, EXTENSION);
    assert!(
        !origin.outdated_against(&listing.revision),
        "it was installed from this exact commit"
    );
    assert!(
        origin.outdated_against("something-else"),
        "and a different published commit is an update"
    );

    // The staging area, which held 45 MB of node_modules a moment ago.
    assert!(
        !install::staging_home(&root).exists(),
        "the staged source was left behind, which is 45 MB per extension at rest"
    );

    // ---------------------------------------------------------- and removal
    let pins = sill_lib::store::pins(&home);
    assert!(
        pins.contains_key(EXTENSION),
        "the pin is found by the name the store uses"
    );

    println!(
        "\nrun it with:\n  node scripts/run-extension.mjs {} {EXTENSION} --no-view\n",
        installed
            .join("generateV7.js")
            .display()
            .to_string()
            .replace('\\', "/")
    );
}

/// Removing takes the directory and the index entries together.
///
/// Its own test, and it builds nothing: the thing worth checking is that both
/// halves go, and a hand-made directory with a hand-made index proves that
/// without a network.
#[test]
fn removing_an_extension_takes_its_commands_out_of_the_index() {
    // A directory of its own. This one runs in an ordinary `cargo test`, and
    // named after the test it would be wiped and rebuilt by every run on the
    // machine at once, which is a second run deleting the bundles this one is
    // about to assert are there.
    let scratch = tempfile::tempdir().expect("a temp directory");
    let root = scratch.path();

    let home = root.join("extensions");
    std::fs::create_dir_all(home.join("demo")).expect("a directory to remove");
    std::fs::create_dir_all(home.join("other")).expect("one to leave alone");

    sill_lib::store::write_origin(
        &home,
        "demo",
        &Origin::store("demo", "extensions/demo", "sha", Vec::new(), 0),
    )
    .expect("an origin");

    let index = r#"[
        {"id":"demo:run","extension":"demo","extensionTitle":"Demo","command":"run",
         "title":"Run","mode":"view","entrypoint":"demo/run.js"},
        {"id":"other:go","extension":"other","extensionTitle":"Other","command":"go",
         "title":"Go","mode":"view","entrypoint":"other/go.js"}
    ]"#;
    std::fs::write(home.join("index.json"), index).expect("an index");

    // A store of its own, because removal empties an extension's
    // `LocalStorage` and the one the application holds is not this test's.
    let storage = sill_lib::exthost::Storage::memory().expect("a store");
    let had = install::uninstall(root, &storage, "demo").expect("it is removed");

    assert!(had);
    assert!(!home.join("demo").exists(), "the bundles are gone");
    assert!(home.join("other").exists(), "and nothing else is");

    let kept = sill_lib::registry::load_index(&home.join("index.json"));
    let ids: Vec<&str> = kept.iter().map(|record| record.id.as_str()).collect();
    assert_eq!(ids, ["other:go"], "its commands are out of the index too");
}
