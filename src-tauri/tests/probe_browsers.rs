//! What browser search actually finds on this machine.
//!
//! Ignored by default, like the other probes: it reads whatever browsers are
//! installed here, so what it prints is a fact about this computer rather than
//! about the code. Run it deliberately.
//!
//!     cargo test --test probe_browsers -- --ignored --nocapture

use std::time::Instant;

use sill_lib::browsers::{self, Want};

fn scratch() -> std::path::PathBuf {
    std::env::temp_dir().join("sill-browser-probe")
}

#[test]
#[ignore = "reads the browsers installed on this machine"]
fn report_profiles() {
    let profiles = browsers::profiles();
    println!("{} profile(s)", profiles.len());

    for profile in &profiles {
        let size = profile
            .history
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| format!("{:.1} MB", m.len() as f64 / 1e6))
            .unwrap_or_else(|| "-".into());

        println!(
            "   {:8} {:34} {:?}  history {:>8}  bookmarks {}",
            profile.browser,
            profile.name,
            profile.family,
            size,
            profile.bookmarks.is_some(),
        );
    }

    assert!(!profiles.is_empty(), "no browser profiles found at all");
}

#[test]
#[ignore = "reads the browsers installed on this machine"]
fn report_search() {
    let scratch = scratch();

    for query in ["github", "docs", "a"] {
        // Twice: the first pays for the copy, the second is what somebody
        // typing actually waits for.
        let cold = Instant::now();
        let hits = browsers::search(query, 8, Want::default(), &scratch);
        let cold = cold.elapsed();

        let warm = Instant::now();
        let again = browsers::search(query, 8, Want::default(), &scratch);
        let warm = warm.elapsed();

        println!();
        println!(
            "{query:?}: {} hit(s), {} ms cold, {} ms warm",
            hits.len(),
            cold.as_millis(),
            warm.as_millis(),
        );
        assert_eq!(hits.len(), again.len(), "two identical searches disagreed");

        for hit in hits.iter().take(6) {
            let kind = if hit.bookmark { "saved" } else { "visited" };
            println!(
                "   {:7} {:8} {:3} {}",
                kind,
                hit.browser,
                hit.visits,
                truncate(&hit.title, 58),
            );
        }
    }
}

fn truncate(text: &str, at: usize) -> String {
    if text.chars().count() <= at {
        return text.to_string();
    }

    text.chars().take(at.saturating_sub(1)).collect::<String>() + "…"
}

#[test]
#[ignore = "reads the browsers installed on this machine"]
fn report_programs_behind_the_browsers() {
    println!("default browser: {:?}", browsers::default_browser());
    println!();

    println!("registered with Windows:");
    for (name, path) in browsers::installed_browsers() {
        println!("   {name:24} {}", path.display());
    }

    println!();
    println!("matched to the profiles that were found:");
    for profile in browsers::profiles() {
        println!(
            "   {:8} -> {:?}",
            profile.browser,
            browsers::browser_exe(&profile.browser),
        );
    }
}

#[test]
#[ignore = "reads the browsers installed on this machine"]
fn report_icons_resolve() {
    // One cache for this probe, rather than the process-wide one this used
    // to reach for.
    let icons = sill_lib::icons::Icons::new(None);

    let mut checked = 0;

    for (name, path) in browsers::installed_browsers() {
        let icon = icons.data_uri(&path.to_string_lossy());
        println!(
            "   {name:24} {}",
            icon.as_ref()
                .map(|uri| format!("{} bytes", uri.len()))
                .unwrap_or_else(|| "NO ICON".into()),
        );
        checked += 1;
    }

    assert!(checked > 0, "no browsers to check");

    let default = browsers::default_browser().expect("a default browser");
    let icon = icons.data_uri(&default.to_string_lossy());
    println!();
    println!("default browser icon: {:?}", icon.map(|u| u.len()));
}
