//! Does this machine actually speak? Ignored: it makes a noise.
use sill_lib::speech::Speech;

#[test]
#[ignore]
fn it_says_something_out_loud() {
    let speech = Speech::default();
    let start = std::time::Instant::now();
    speech.aloud("Sill can read this out loud.").expect("spoke");
    println!("  Speak returned in {} ms (async, so it is still talking)", start.elapsed().as_millis());
    std::thread::sleep(std::time::Duration::from_millis(2500));
    speech.stop().expect("stopped");
    println!("  stopped cleanly");
}
