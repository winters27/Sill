//! Against the Windows Terminal and WSL that are actually on this machine.
//!
//! The fixtures in `terminals.rs` agree with the parser by construction. They
//! cannot say whether a real settings file, written by a real installation and
//! edited by its own settings UI, reads at all.
//!
//! Ignored, because a build agent has neither:
//!
//! ```text
//! cargo test --lib real_terminals -- --ignored --nocapture
//! ```

#[test]
#[ignore]
#[cfg(windows)]
fn the_settings_file_on_this_machine_reads() {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        println!("no LOCALAPPDATA");
        return;
    };

    let looked = [
        std::path::PathBuf::from(&local)
            .join("Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json"),
        std::path::PathBuf::from(&local).join("Microsoft/Windows Terminal/settings.json"),
    ];

    let Some(path) = looked.iter().find(|one| one.is_file()) else {
        println!("Windows Terminal is not installed here");
        return;
    };

    let text = std::fs::read_to_string(path).expect("readable");
    let found = crate::terminals::profiles_in(&text);

    // The claim: a real file, with the comments and trailing commas its own
    // settings UI writes, yields profiles rather than nothing.
    assert!(
        !found.is_empty(),
        "the real settings file parsed to no profiles at all, which is what \
         happens when the comment strip is wrong"
    );

    println!(
        "profiles: {:?}",
        found.iter().map(|o| &o.name).collect::<Vec<_>>()
    );
    println!(
        "default:  {:?}",
        found.iter().find(|o| o.default).map(|o| &o.name)
    );
}

#[test]
#[ignore]
#[cfg(windows)]
fn wsl_on_this_machine_lists_its_distributions() {
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
    else {
        println!("wsl.exe would not run");
        return;
    };

    let text = crate::terminals::console_text(&out.stdout);
    let found = crate::terminals::distributions_in(&text);

    println!(
        "raw first bytes: {:?}",
        &out.stdout[..out.stdout.len().min(8)]
    );
    println!("distributions:   {found:?}");

    // Not asserted non-empty: a machine may genuinely have none installed.
    // What is asserted is that nothing came back with a NUL in it, which is
    // the failure this decoding exists to prevent and which reads as "none".
    for one in &found {
        assert!(
            !one.contains('\0'),
            "{one:?} still holds a NUL, so the UTF-16 decoding did not happen"
        );
    }
}

/// The rows this machine would actually offer, and what one costs.
///
/// `available` used to start `wsl.exe`, which measured 50 to 105 ms here. It
/// reads the registry now, and this is where that claim is checked rather than
/// asserted: the two lists have to hold the same distributions, and the
/// reading has to be quick enough to sit behind a keystroke.
#[test]
#[ignore]
#[cfg(windows)]
fn the_profiles_this_machine_offers() {
    let began = std::time::Instant::now();
    let found = crate::terminals::available();
    let cold = began.elapsed();

    let began = std::time::Instant::now();
    let again = crate::terminals::available();
    let warm = began.elapsed();

    println!("{} profiles, {cold:?} cold, {warm:?} warm", found.len());
    for one in &found {
        let (program, args) = crate::terminals::opening(one);
        println!(
            "  {:<40} {:<26} {program} {}",
            one.name,
            if one.default {
                "the default"
            } else if one.distribution {
                "WSL distribution"
            } else {
                "Terminal profile"
            },
            args.join(" ")
        );
    }

    assert_eq!(found.len(), again.len(), "two readings disagreed");

    // The point of the change. A keystroke may not wait a tenth of a second.
    assert!(
        warm < std::time::Duration::from_millis(30),
        "reading the profiles took {warm:?}, which is too long to sit behind a keystroke, \
         and is what starting wsl.exe used to cost"
    );

    // Every row has to be openable, which for a profile means Terminal knows
    // the name and for a distribution means WSL does.
    for one in &found {
        let (program, args) = crate::terminals::opening(one);
        assert!(
            matches!(program, "wt.exe" | "wsl.exe"),
            "{} would be opened by {program}",
            one.name
        );
        assert_eq!(args.len(), 2, "{} takes {} arguments", one.name, args.len());
    }
}

/// The registry and `wsl.exe` have to agree about what is installed.
///
/// If they do not, the fast path is not the same list as the slow one and the
/// rows are a different set from what the settings page shows.
#[test]
#[ignore]
#[cfg(windows)]
fn the_registry_says_what_wsl_says() {
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .output()
    else {
        println!("wsl.exe would not run");
        return;
    };

    let mut spoken =
        crate::terminals::distributions_in(&crate::terminals::console_text(&out.stdout));
    let mut listed: Vec<String> = crate::terminals::available()
        .into_iter()
        .filter(|one| one.distribution)
        .map(|one| one.name)
        .collect();

    spoken.sort();
    listed.sort();

    println!("wsl.exe says:   {spoken:?}");
    println!("the rows carry: {listed:?}");

    // Not equal: a distribution Terminal already has a profile for is offered
    // once, as that profile, so it is deliberately not in the second list.
    for one in &listed {
        assert!(
            spoken.contains(one),
            "{one} is offered as a distribution and wsl.exe has never heard of it"
        );
    }
}
