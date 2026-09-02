//! Installing the most-installed extensions, to find out what the host cannot
//! do yet.
//!
//! Not part of `npm run verify`. It fetches from two services, runs npm once
//! per extension and takes minutes, so it is `#[ignore]` and run by name:
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml --test store_audit \
//!   -- --ignored --nocapture
//! ```
//!
//! ## Why this exists
//!
//! `gate:views` builds two extensions and proves the four view types render.
//! That answers "does the host work" and not "does the host work for the
//! things people will actually install", which is a different question and the
//! only one that matters once there is a store.
//!
//! So this takes the extensions with the most installs, puts them through the
//! **real store install path**, and writes down what came out. Running them is
//! the other half and belongs to `scripts/run-extension.mjs`, which already
//! reports every API an extension asked for that the host does not implement.
//! This writes a manifest that names each built bundle so that runner can be
//! pointed at all of them in one pass.
//!
//! ## What a failure here means
//!
//! An extension that will not install is a fact about the store; an extension
//! that installs and then needs an API nothing answers is a fact about the
//! host. Keeping them separate is the point of stopping at the bundle.

#![cfg(windows)]

use std::path::PathBuf;

use sill_lib::store::{catalog, install, Listing};

/// How many to install.
///
/// Enough to be representative and few enough to finish. These are the most
/// installed extensions there are, so they are also the ones whose failure
/// would be most visible.
const HOW_MANY: usize = 12;

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

/// The ones worth auditing: offered by default, and with something to run.
fn worth_auditing(listing: &Listing) -> bool {
    listing.blocked().is_none()
}

#[tokio::test]
#[ignore = "installs a dozen extensions from the network"]
async fn the_most_installed_extensions_install_and_say_what_they_needed() {
    let root = std::env::temp_dir().join("sill-store-audit");
    std::fs::create_dir_all(&root).expect("somewhere to install");

    // **Deliberately not wiped.** It was, and that made the skip below dead
    // code: nothing is ever already installed if the directory is emptied
    // first. It also threw away the catalogue cache, so every run refetched
    // 3.5 MB to learn the same thing.
    //
    // Delete it by hand to force a clean run:
    //   rm -rf "$TEMP/sill-store-audit"

    let catalog = catalog::load(&root, false)
        .await
        .expect("the store answers");

    let mut wanted: Vec<Listing> = catalog
        .listings
        .iter()
        .filter(|listing| worth_auditing(listing))
        .cloned()
        .collect();
    wanted.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    wanted.truncate(HOW_MANY);

    println!(
        "auditing {} of {} offered extensions\n",
        wanted.len(),
        catalog.listings.len()
    );

    let esbuild = esbuild();

    // Found once here rather than inside the install, which is where the
    // command layer does it too: finding Node means running it, and this loop
    // installs a great many extensions.
    let node = sill_lib::host::node_exe(&std::sync::Mutex::new(None))
        .expect("this audit needs Node on PATH");

    let mut built = Vec::new();
    let mut refused = Vec::new();

    let home = sill_lib::store::extensions_home(&root);

    for listing in &wanted {
        print!("{:<24}", listing.name);

        // Already here at this exact commit, so there is nothing to learn from
        // fetching it again. Skipping is not only faster: GitHub allows sixty
        // requests an hour without a token and an install spends about three,
        // so a few runs of this in an afternoon exhausts the budget and the
        // audit starts measuring the rate limit instead of the extensions.
        if let Some(origin) = sill_lib::store::origin_of(&home, &listing.name) {
            if origin.revision == listing.revision {
                println!("already installed at {}", &origin.revision[..7]);
                built.push(listing.name.clone());
                continue;
            }
        }

        let prepared = match install::prepare(&root, listing, None).await {
            Ok(prepared) => prepared,
            Err(err) => {
                println!("FETCH FAILED  {err}");
                refused.push((listing.name.clone(), format!("fetch: {err}")));
                continue;
            }
        };

        match install::finish(&root, &esbuild, &node, &listing.name) {
            Ok(done) => {
                println!(
                    "ok  {} commands, {} packages, {} capabilities",
                    done.installed.commands.len(),
                    prepared.packages.len(),
                    prepared.capabilities.len()
                );
                built.push(listing.name.clone());
            }
            Err(err) => {
                // The whole message, not a summary. npm and esbuild say
                // exactly what they could not do, and that is the useful part.
                println!("BUILD FAILED\n    {}", err.replace('\n', "\n    "));
                refused.push((listing.name.clone(), err));
            }
        }
    }

    // Every command that was built, so the runner can be pointed at them.
    let index = sill_lib::registry::load_index(&sill_lib::store::index_file(
        &sill_lib::store::extensions_home(&root),
    ));

    // What installing actually granted, in the names the worker compares
    // against. Carried into the manifest so the runner can be given the same
    // permissions the app would have, rather than none: an ungranted extension
    // dies at `require` and looks exactly like one that rendered nothing,
    // which is how a first pass counted 104 of 104 as having run.
    let manifest: Vec<serde_json::Value> = index
        .iter()
        .map(|record| {
            let granted: Vec<String> = sill_lib::store::origin_of(&home, &record.extension)
                .map(|origin| sill_lib::store::capability::granted_by(&origin.capabilities))
                .unwrap_or_default()
                .iter()
                .filter_map(|permission| {
                    serde_json::to_value(permission)
                        .ok()?
                        .as_str()
                        .map(str::to_string)
                })
                .collect();

            serde_json::json!({
                "id": record.id,
                "extension": record.extension,
                "command": record.command,
                "mode": record.mode,
                "entrypoint": record.entrypoint,
                "granted": granted,
            })
        })
        .collect();

    let listing_path = root.join("audit.json");
    std::fs::write(
        &listing_path,
        serde_json::to_string_pretty(&manifest).expect("serialises"),
    )
    .expect("the manifest is written");

    println!(
        "\n{} of {} installed, {} commands built",
        built.len(),
        wanted.len(),
        index.len()
    );

    if !refused.is_empty() {
        println!("\nrefused:");
        for (name, why) in &refused {
            println!("  {name}: {}", why.lines().next().unwrap_or(""));
        }
    }

    println!("\nmanifest: {}", listing_path.display());

    // Deliberately not an assertion on every extension installing. Some of
    // these depend on a native package, or on a CLI they expect to find, and
    // that is a fact worth reading rather than a test worth failing. What is
    // asserted is that the path works at all: if none of the twelve most
    // installed extensions can be installed, the store does not work.
    assert!(
        built.len() * 2 > wanted.len(),
        "only {} of {} installed, so this is the store rather than the extensions",
        built.len(),
        wanted.len()
    );
}
