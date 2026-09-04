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
