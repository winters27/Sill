//! Moving between two real drives, which is the half a same-drive test cannot
//! reach.
//!
//! `fs::rename` cannot cross a volume: Windows answers `ERROR_NOT_SAME_DEVICE`
//! and nothing moves. The fallback copies and then removes, and whether that
//! actually happens can only be seen with two drives in front of it.
//!
//! Ignored by default: it writes to a second drive on this machine. It only
//! ever touches files it made itself, under a folder named after this test,
//! and it removes them.
//!
//! ```text
//! SILL_OTHER_DRIVE=P: cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_move_across_drives -- --ignored --nocapture
//! ```

use sill_lib::files_ops::move_to;

#[test]
#[ignore = "writes to a second drive on this machine"]
fn a_file_moves_between_two_drives() {
    let Ok(other) = std::env::var("SILL_OTHER_DRIVE") else {
        println!("set SILL_OTHER_DRIVE to a drive letter to run this");
        return;
    };

    let here = std::env::temp_dir().join("sill-move-across");
    let there = std::path::PathBuf::from(format!(r"{other}\sill-move-across"));

    let _ = std::fs::remove_dir_all(&here);
    let _ = std::fs::remove_dir_all(&there);
    std::fs::create_dir_all(&here).expect("a folder here");
    std::fs::create_dir_all(&there).expect("a folder there");

    let from = here.join("crossing.txt");
    std::fs::write(&from, b"across").expect("written");

    println!("moving {} to {}", from.display(), there.display());
    let landed = move_to(&from, &there).expect("moves across drives");

    println!("landed at {}", landed.display());
    assert!(landed.exists(), "it is not on the other drive");
    assert!(!from.exists(), "the original is still on this one");
    assert_eq!(std::fs::read(&landed).expect("read"), b"across");

    // And back, which is the same code in the other direction.
    let home = move_to(&landed, &here).expect("moves back");
    assert!(home.exists());
    assert!(!landed.exists());
    println!("and back to {}", home.display());

    let _ = std::fs::remove_dir_all(&here);
    let _ = std::fs::remove_dir_all(&there);
    println!("cleaned up both");
}
