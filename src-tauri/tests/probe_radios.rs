//! Which radios this machine has, and whether Sill can switch them.
//!
//!     cargo test --test probe_radios -- --ignored --nocapture

#[test]
#[ignore = "reads this machine's radios"]
fn report_radios() {
    let found = sill_lib::radios::radios();
    println!("{} radio(s)", found.len());

    for radio in &found {
        println!(
            "   {:10} {}",
            radio.name,
            if radio.on { "on" } else { "off" }
        );
    }
}

/// Switches one off and straight back on, so the machine ends as it started.
///
/// Bluetooth rather than wifi on purpose: turning wifi off on a machine that
/// is using it drops whatever is downloading.
#[test]
#[ignore = "turns this machine's Bluetooth off and on again"]
fn switching_takes_effect_and_can_be_put_back() {
    let before = sill_lib::radios::radios();
    let Some(radio) = before.iter().find(|r| r.kind == "bluetooth") else {
        println!("no Bluetooth in this machine, so there is nothing to switch");
        return;
    };

    let was = radio.on;
    println!("Bluetooth is {}", if was { "on" } else { "off" });

    sill_lib::radios::set_radio("bluetooth", !was).expect("switches");
    let during = sill_lib::radios::radios();
    let now = during
        .iter()
        .find(|r| r.kind == "bluetooth")
        .expect("still there")
        .on;
    println!("now {}", if now { "on" } else { "off" });

    // Put back before asserting, so a failure does not leave it off.
    sill_lib::radios::set_radio("bluetooth", was).expect("puts it back");
    let after = sill_lib::radios::radios();
    let restored = after
        .iter()
        .find(|r| r.kind == "bluetooth")
        .expect("still there")
        .on;
    println!("put back to {}", if restored { "on" } else { "off" });

    assert_eq!(now, !was, "the switch did not take effect");
    assert_eq!(restored, was, "it was not put back");
}
