//! What sound can come out of on this machine.
//!
//! Ignored by default: it reads this machine's devices, and the switching test
//! changes which one is in use. Run it deliberately.
//!
//!     cargo test --test probe_audio -- --ignored --nocapture

use sill_lib::audio;

#[test]
#[ignore = "reads this machine's audio devices"]
fn report_outputs() {
    let found = audio::outputs();
    println!("{} output(s)", found.len());

    for output in &found {
        println!(
            "   {} {:40} {}",
            if output.current { "*" } else { " " },
            audio::short_name(&output.name),
            output.id,
        );
    }

    assert!(!found.is_empty(), "no audio outputs at all");
    assert!(
        found.iter().any(|o| o.current),
        "none of them is the one in use, so switching could never say what changed",
    );
}

/// Switches away and back, so the machine ends as it started.
#[test]
#[ignore = "changes which speakers this machine is using"]
fn switching_takes_effect_and_can_be_put_back() {
    let found = audio::outputs();
    if found.len() < 2 {
        println!("only one output, so there is nothing to switch to");
        return;
    }

    let was = found
        .iter()
        .find(|o| o.current)
        .expect("one of them is in use")
        .clone();
    let other = found
        .iter()
        .find(|o| !o.current)
        .expect("another one")
        .clone();

    println!("switching from {} to {}", audio::short_name(&was.name), audio::short_name(&other.name));
    audio::set_output(&other.id).expect("switches");

    let now = audio::outputs();
    let current = now.iter().find(|o| o.current).expect("something is in use");
    println!("now using {}", audio::short_name(&current.name));

    let landed = current.id == other.id;

    // Put back before asserting, so a failure does not leave the machine
    // playing out of the wrong speakers.
    audio::set_output(&was.id).expect("puts it back");
    let back = audio::outputs();
    let restored = back.iter().find(|o| o.current).map(|o| o.id.clone());
    println!("put back to {}", audio::short_name(&was.name));

    assert!(landed, "the switch did not take effect");
    assert_eq!(restored, Some(was.id), "it was not put back");
}
