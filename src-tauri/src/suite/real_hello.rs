//! Against the Windows Hello this machine actually has, or has not.
//!
//! `crate::ai::acting::gate` is a function over values and its fixtures cover
//! every case. What they cannot say is what Windows answers here: whether the
//! WinRT call resolves at all, and which of the five availabilities this
//! machine is in. That decides whether the AI panel's heavy actions ask for a
//! fingerprint or fall back to the approval card, and it is worth being able
//! to read off a machine rather than guessed at.
//!
//! Ignored, because a build agent has no Hello and because the second one puts
//! a dialog on somebody's screen:
//!
//! ```text
//! cargo test --lib real_hello -- --ignored --nocapture
//! ```
//!
//! **The second test is not automated and must not be.** It waits for a real
//! person to touch a real sensor. Run it by hand, look at the prompt, and read
//! the sentence on it: that sentence is the only thing somebody has to decide
//! on, and no fixture can tell you whether it reads like a warning or like
//! noise.

/// What Windows says about this machine, and nothing else.
///
/// Safe to run anywhere: it puts nothing on screen and asks nobody anything.
/// It passes whatever the answer is, because there is no right answer for a
/// machine to give; what it is for is printing the one this machine gives.
#[test]
#[ignore]
#[cfg(windows)]
fn what_this_machine_can_prove() {
    let had = crate::hello::available();

    println!("availability: {had:?}");
    println!("ready:        {}", had.ready());
    println!("says:         {:?}", had.why());

    // The claim, and it is the only one a machine-independent test can make:
    // whatever Windows answered, Sill has words for it. A state with no
    // sentence is a card that downgrades silently.
    assert!(
        had.ready() || had.why().is_some(),
        "Windows answered something Sill cannot explain to anybody",
    );

    let gate = crate::ai::acting::gate(&[crate::action::Capability::ShellExecution], Some(had));
    println!("running a command here would: {gate:?}");
}

/**
A real prompt, answered by a real person.

**Do not automate this and do not send it any input.** It is here so that the
one path no fixture reaches can be walked by hand on a machine that has Hello.

What to do, and what to look for:

1. Enrol a face, a fingerprint or a Hello PIN, then run the command in the
   module note above.
2. A Windows credential prompt appears. **Read it.** It should name the action,
   name the thing being acted on, and say what that does, in one sentence.
3. Answer it. The test prints the verdict and asserts nothing about which way
   it went, because both ways are correct answers to a question about consent.
4. Run it once more and let it sit. It must give up after ninety seconds and
   take its own dialog down, rather than waiting forever on somebody who has
   gone to lunch.

On a machine with no Hello it prints why and stops, which is not a failure:
that machine's behaviour is the fallback, and the fallback is covered by the
fixtures in `ai::acting`.
*/
#[test]
#[ignore]
#[cfg(windows)]
fn a_real_prompt_answered_by_a_real_person() {
    let had = crate::hello::available();

    if !had.ready() {
        println!("nothing to prompt: {:?}", had.why());
        return;
    }

    let message = crate::ai::acting::hello_message(
        "Run",
        "deploy.ps1",
        crate::ai::acting::what_it_touches(&[crate::action::Capability::ShellExecution]),
    );

    println!("the prompt says: {message}");

    // Zero rather than a window handle, because there is no Tauri app here.
    // Windows accepts the desktop for a console process; if it refuses, the
    // verdict is `Trouble` and that is worth seeing too.
    let verdict = crate::hello::verify(0, &message, crate::hello::PATIENCE);

    println!("verdict: {verdict:?}");
}
