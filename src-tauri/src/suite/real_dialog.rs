//! Against a real save dialog, in a real other process, opened by this test.
//!
//! The fixtures in `dialog.rs` decide whether a set of controls is a file
//! dialog and what a jump into one would do. They cannot say whether a real
//! `IFileDialog` window is built the way they assume, whether `WM_SETTEXT`
//! reaches a control in another process, which message actually makes a dialog
//! move, or what any of it costs. All four are decided by Windows, and three
//! of the four turned out differently from the obvious guess.
//!
//! Ignored, because it puts a window on the screen and takes the foreground:
//!
//! ```text
//! cargo test --lib real_dialog -- --ignored --nocapture
//! ```
//!
//! **The dialog belongs to this test.** It is opened here, in a PowerShell
//! this test spawns, and killed here. Nothing goes near a dialog somebody else
//! had open, which is the whole reason this is a probe and not something that
//! runs in CI: the failure mode of getting `P8-07` wrong is text in a
//! stranger's Save As box, and a test that reaches for whatever dialog happens
//! to be in front is the same mistake in a different costume.
//!
//! ## What makes the navigation provable
//!
//! A Save dialog resolves a bare file name against **the folder it is
//! currently showing**, and hands back the whole path. So the probe opens one
//! in a temporary folder of its own, jumps it to a different temporary folder,
//! and then makes it resolve a bare name. The path it reports names the folder
//! it was in, so a dialog that only looked like it moved cannot pass.
//!
//! Nothing is written to either folder by the dialog: it reports the name it
//! would have saved under and closes.

#![cfg(windows)]

use std::time::{Duration, Instant};

/// How long the dialog gets to appear before the probe gives up on it.
const APPEARS_WITHIN: Duration = Duration::from_secs(15);

/// How long it gets to close after being told its file was chosen.
const CLOSES_WITHIN: Duration = Duration::from_secs(10);

/// The bare name the dialog is asked to resolve at the end.
///
/// Bare on purpose: it is the question "where are you?" asked in the only
/// language a file dialog answers in.
const PROOF: &str = "sill-jump-proof.txt";

/// A folder of our own, with something in it, and somewhere to put the answer.
fn somewhere() -> tempfile::TempDir {
    let folder = tempfile::tempdir().expect("a temporary folder");
    std::fs::write(folder.path().join("a-file.txt"), b"probe").expect("a file in it");

    folder
}

/// A Save dialog in another process, showing the folder it is given.
///
/// PowerShell rather than a dialog put up by this test, and that is the point:
/// `dialog::in_front` refuses Sill's own windows, exactly as every other
/// foreground read here does, so a dialog owned by the test process would be
/// skipped and the probe would prove nothing.
///
/// It starts in a writable folder rather than at the root of a drive. A dialog
/// asked to save into `C:\` puts a permission prompt up instead of closing,
/// which reads exactly like a jump that did not work and cost an hour once.
fn open_dialog_in(answer: &std::path::Path, start: &str) -> std::process::Child {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; \
         $d = New-Object System.Windows.Forms.SaveFileDialog; \
         $d.InitialDirectory = '{start}'; \
         $d.OverwritePrompt = $false; \
         $d.Title = 'Sill dialog jump probe'; \
         if ($d.ShowDialog() -eq 'OK') {{ \
             Set-Content -LiteralPath '{}' -Value $d.FileName \
         }}",
        answer.display()
    );

    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .spawn()
        .expect("powershell starts")
}

/// The dialog this test opened, once it is on screen.
fn wait_for_it(child: &mut std::process::Child) -> crate::dialog::Dialog {
    let waiting = Instant::now();

    loop {
        match crate::dialog::in_front() {
            Ok(dialog) => return dialog,
            Err(refusal) => {
                if waiting.elapsed() > APPEARS_WITHIN {
                    let _ = child.kill();
                    panic!(
                        "no dialog appeared within {APPEARS_WITHIN:?}: {}",
                        refusal.reason()
                    );
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Makes the dialog say where it is, by resolving a name with no folder in it.
fn where_is_it(dialog: &crate::dialog::Dialog, answer: &std::path::Path) -> String {
    // Built by hand rather than by `plan`, which refuses to accept anything on
    // somebody's behalf and is right to. The probe wants exactly the thing
    // production never does.
    let _ = crate::dialog::jump_to(
        dialog,
        &crate::dialog::Jump {
            folder: PROOF.to_string(),
            name: String::new(),
        },
    );

    let closing = Instant::now();
    while closing.elapsed() < CLOSES_WITHIN && !answer.exists() {
        std::thread::sleep(Duration::from_millis(100));
    }

    std::fs::read_to_string(answer)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The whole of `P8-07`, end to end, against a dialog this test opened.
///
/// The output of a passing run:
///
/// ```text
/// the dialog appeared after 804.2113ms
/// window 0xad09d8, file name box 0x750890, accept button 0xcf04ba
/// 47 controls, of which the dialog's own children are:
///   ...
/// worst of five identifications: 1.2703ms
/// the box said "" before the jump
/// jumped to C:\Users\Brandon\AppData\Local\Temp\.tmpMg7I0M in 235.7709ms
/// the dialog resolved sill-jump-proof.txt to
///   C:\Users\Brandon\AppData\Local\Temp\.tmpMg7I0M\sill-jump-proof.txt
/// ```
#[test]
#[ignore]
fn a_real_save_dialog_is_found_and_pointed_somewhere_else() {
    let start = somewhere();
    let target = somewhere();
    let answer = start.path().join("chosen.txt");
    let mut child = open_dialog_in(&answer, &start.path().to_string_lossy());

    // Everything after this has to leave the machine tidy, so the failure is
    // collected and the child is killed before it is reported.
    let outcome = drive_the_dialog(&mut child, target.path(), &answer);

    let _ = child.kill();
    let _ = child.wait();

    if let Err(reason) = outcome {
        panic!("{reason}");
    }
}

fn drive_the_dialog(
    child: &mut std::process::Child,
    target: &std::path::Path,
    answer: &std::path::Path,
) -> Result<(), String> {
    let waiting = Instant::now();
    let found = wait_for_it(child);

    println!("the dialog appeared after {:?}", waiting.elapsed());
    println!(
        "window {:#x}, file name box {:#x}, accept button {:#x}",
        found.window, found.fields.edit, found.fields.accept
    );

    // What it is made of, which is the thing the fixtures claim to describe.
    let controls = crate::dialog::controls_of(found.window);
    println!(
        "{} controls, of which the dialog's own children are:",
        controls.len()
    );

    for control in controls.iter().filter(|c| c.parent == found.window) {
        println!("  id {:>6} {}", control.id, control.class);
    }

    // What finding one costs when there is one, which is the rarer half.
    let mut worst = Duration::ZERO;
    for _ in 0..5 {
        let started = Instant::now();
        let _ = crate::dialog::in_front();
        worst = worst.max(started.elapsed());
    }
    println!("worst of five identifications: {worst:?}");

    let typed = crate::dialog::typed_in(&found);
    println!("the box said {typed:?} before the jump");

    let jump = crate::dialog::plan(&target.to_string_lossy(), true, &typed)?;
    let started = Instant::now();
    crate::dialog::jump_to(&found, &jump)?;
    println!("jumped to {} in {:?}", jump.folder, started.elapsed());

    let chosen = where_is_it(&found, answer);
    println!("the dialog resolved {PROOF} to {chosen:?}");

    let should_be = target.join(PROOF);
    if chosen.eq_ignore_ascii_case(&should_be.to_string_lossy()) {
        return Ok(());
    }

    Err(format!(
        "the dialog was showing somewhere else: it resolved {PROOF} to {chosen:?} \
         rather than {}",
        should_be.display()
    ))
}

/**
Which message, applied to a folder path in the box, actually navigates.

The bench behind the choice of `BM_CLICK` in `dialog.rs`, kept so the claim is
checkable rather than folklore. One fresh dialog per strategy, the folder put
in the box raw, the strategy applied, and then the dialog asked where it is.
Measured:

```text
bm_click:       chose ...\.tmpRNM8sG\sill-jump-proof.txt (wanted ...\.tmpRNM8sG)
sent_command:   chose ...\.tmpPjKuc5\sill-jump-proof.txt (wanted ...\.tmpPjKuc5)
posted_command: chose ...\.tmpAmjLxj\sill-jump-proof.txt (wanted ...\.tmpAmjLxj)
posted_return:  chose ...\.tmpcW3QMO\sill-jump-proof.txt (wanted ...\.tmpcW3QMO)
```

All four move it, which is worth knowing in itself: none of them is a trick
that only works on one generation of dialog. `BM_CLICK` is the one production
uses because it is *sent* to one button handle rather than posted to a thread
queue, so a delivery that fails is visible to the caller.
*/
#[test]
#[ignore]
fn which_message_navigates() {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        PostMessageW, SendMessageTimeoutW, BM_CLICK, SMTO_ABORTIFHUNG, WM_COMMAND, WM_KEYDOWN,
        WM_KEYUP, WM_SETTEXT,
    };

    for strategy in [
        "bm_click",
        "sent_command",
        "posted_command",
        "posted_return",
    ] {
        let start = somewhere();
        let target = somewhere();
        let answer = start.path().join("chosen.txt");
        let here = target.path().to_string_lossy().into_owned();
        let mut child = open_dialog_in(&answer, &start.path().to_string_lossy());
        let found = wait_for_it(&mut child);

        let wide: Vec<u16> = here.encode_utf16().chain(std::iter::once(0)).collect();
        let edit = HWND(found.fields.edit as *mut core::ffi::c_void);
        let dialog = HWND(found.window as *mut core::ffi::c_void);
        let mut answered = 0usize;

        // SAFETY: every handle came from this dialog's own enumeration, the
        // string outlives the synchronous call, and `WM_SETTEXT` is marshalled
        // between processes by the window manager.
        unsafe {
            SendMessageTimeoutW(
                edit,
                WM_SETTEXT,
                WPARAM(0),
                LPARAM(wide.as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                3000,
                Some(&mut answered),
            );

            match strategy {
                "bm_click" => {
                    SendMessageTimeoutW(
                        HWND(found.fields.accept as *mut core::ffi::c_void),
                        BM_CLICK,
                        WPARAM(0),
                        LPARAM(0),
                        SMTO_ABORTIFHUNG,
                        3000,
                        Some(&mut answered),
                    );
                }
                "sent_command" => {
                    SendMessageTimeoutW(
                        dialog,
                        WM_COMMAND,
                        WPARAM(1),
                        LPARAM(found.fields.accept),
                        SMTO_ABORTIFHUNG,
                        3000,
                        Some(&mut answered),
                    );
                }
                "posted_command" => {
                    let _ = PostMessageW(
                        Some(dialog),
                        WM_COMMAND,
                        WPARAM(1),
                        LPARAM(found.fields.accept),
                    );
                }
                _ => {
                    let _ = PostMessageW(Some(edit), WM_KEYDOWN, WPARAM(0x0D), LPARAM(0x001C0001));
                    let _ = PostMessageW(
                        Some(edit),
                        WM_KEYUP,
                        WPARAM(0x0D),
                        LPARAM(0xC01C0001u32 as isize),
                    );
                }
            }
        }

        std::thread::sleep(Duration::from_millis(400));
        let chosen = where_is_it(&found, &answer);

        println!("{strategy}: chose {chosen:?} (wanted {here})");

        let _ = child.kill();
        let _ = child.wait();
    }
}

/// What a press costs when there is no dialog anywhere.
///
/// The case that runs on every press of the key in an editor, a browser or a
/// terminal, which is nearly all of them. It reads one handle and one class
/// name and stops; nothing is enumerated and no control is touched. Measured
/// at **82 microseconds for the first answer and 244 for the worst of a
/// thousand**, against 1.3 milliseconds for the hit, where 47 controls are
/// enumerated across a process boundary.
#[test]
#[ignore]
fn what_the_miss_costs() {
    let mut worst = Duration::ZERO;

    for run in 0..1000 {
        let started = Instant::now();
        let answer = crate::dialog::in_front();
        let took = started.elapsed();
        worst = worst.max(took);

        if run == 0 {
            println!("the first answer was {answer:?} in {took:?}");
        }
    }

    println!("worst of a thousand: {worst:?}");

    // Generous by three orders of magnitude, because a probe that fails when
    // the machine is busy teaches nothing. What it holds is the shape: this
    // must not be doing work.
    assert!(worst < Duration::from_millis(1), "{worst:?}");
}
