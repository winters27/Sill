//! Does this machine actually speak?
//!
//! Ignored by default: it makes a noise. Run it deliberately.
//!
//!     cargo test --test probe_tts -- --ignored --nocapture
use sill_lib::tts::sapi::Sapi;

#[test]
#[ignore]
fn it_says_something_out_loud() {
    let voice = Sapi::default();
    let start = std::time::Instant::now();
    voice.aloud("Sill can read this out loud.").expect("spoke");
    println!(
        "  Speak returned in {} ms (async, so it is still talking)",
        start.elapsed().as_millis()
    );
    std::thread::sleep(std::time::Duration::from_millis(2500));
    voice.stop().expect("stopped");
    println!("  stopped cleanly");
}
