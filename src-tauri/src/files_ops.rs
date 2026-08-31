//! Doing things to files, as opposed to finding them.
//!
//! The arithmetic and the filesystem work behind the file actions, kept apart
//! from the actions themselves so it can be tested without an application, a
//! window or a clipboard. Every function here takes paths and returns a
//! result; none of them know what a launcher is.

use std::path::{Path, PathBuf};

/// What a file is called, for saying what happened to it.
pub fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// A file's SHA-256, lower case hex.
///
/// Read in blocks. An installer is hundreds of megabytes and there is no
/// reason for any of it to be resident at once, let alone all of it.
pub fn sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).map_err(|err| format!("could not open that file: {err}"))?;

    let mut hasher = Sha256::new();
    let mut block = vec![0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut block)
            .map_err(|err| format!("could not read that file: {err}"))?;

        if read == 0 {
            break;
        }

        hasher.update(&block[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// A name nothing is using yet, given one somebody wanted.
///
/// `report.zip` becomes `report (2).zip`, then `report (3).zip`. The suffix
/// goes before the extension rather than after, because `report.zip (2)` is
/// not a zip file as far as anything else on the machine is concerned.
///
/// Bounded. A directory with a thousand of them is somebody's mistake rather
/// than a case to serve, and an unbounded loop on a filesystem that reports
/// every name as taken would spin for ever.
pub fn free_name(wanted: &Path) -> Option<PathBuf> {
    if !wanted.exists() {
        return Some(wanted.to_path_buf());
    }

    let parent = wanted.parent()?;
    let stem = wanted.file_stem()?.to_string_lossy().into_owned();
    let extension = wanted
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    (2..1000).find_map(|n| {
        let candidate = parent.join(format!("{stem} ({n}){extension}"));
        (!candidate.exists()).then_some(candidate)
    })
}

/// Whether a name is one a file can actually have on this platform.
///
/// Checked before touching anything, so a rename fails with a sentence
/// somebody can act on rather than an error number from the filesystem.
///
/// The reserved names are the ones Windows has kept since DOS. `CON.txt` is
/// still `CON`, so the extension is not what saves it, and creating one
/// produces an error nobody would connect to the name they typed.
pub fn usable_name(name: &str) -> Result<(), String> {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let trimmed = name.trim();

    if trimmed.is_empty() {
        return Err("a file needs a name".to_string());
    }

    if let Some(bad) = trimmed.chars().find(|c| r#"\/:*?"<>|"#.contains(*c)) {
        return Err(format!("a name cannot contain {bad}"));
    }

    if trimmed.chars().any(|c| (c as u32) < 32) {
        return Err("a name cannot contain control characters".to_string());
    }

    // Windows silently drops a trailing dot, so a file named "report."
    // becomes "report" and the rename appears to have half worked.
    //
    // A trailing space is dropped by Windows too and is not refused here: it
    // has already been trimmed off above, because nobody types one on purpose
    // and a stray one is not worth an error. `rename` trims to match.
    if trimmed.ends_with('.') {
        return Err("a name cannot end with a dot".to_string());
    }

    let stem = trimmed
        .split('.')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_uppercase();

    if RESERVED.contains(&stem.as_str()) {
        return Err(format!("{stem} is a name Windows keeps for itself"));
    }

    Ok(())
}

/// Renames a file or folder, keeping it where it is.
///
/// Returns where it ended up. The name is checked first and an existing name
/// is refused rather than overwritten: renaming one file onto another is how
/// somebody loses the other one, and no undo would bring it back.
pub fn rename(path: &Path, to: &str) -> Result<PathBuf, String> {
    usable_name(to)?;

    let parent = path
        .parent()
        .ok_or_else(|| "that has nowhere to be renamed in".to_string())?;
    let target = parent.join(to.trim());

    if target == path {
        return Err("that is already its name".to_string());
    }

    // `exists` follows links and is a moment out of date by the time the
    // rename runs, so this is a courtesy rather than a guarantee. It turns the
    // common case into a sentence instead of an overwrite.
    if target.exists() {
        return Err(format!("{to} is already there"));
    }

    std::fs::rename(path, &target).map_err(|err| format!("could not rename that: {err}"))?;

    Ok(target)
}

/// Puts a file or folder into a zip beside it, and says where.
pub fn compress(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "that has nowhere to be compressed into".to_string())?;
    let stem = path
        .file_stem()
        .ok_or_else(|| "that has no name to use".to_string())?
        .to_string_lossy()
        .into_owned();

    let into = free_name(&parent.join(format!("{stem}.zip")))
        .ok_or_else(|| "there is no free name for the archive".to_string())?;

    let file =
        std::fs::File::create(&into).map_err(|err| format!("could not make the archive: {err}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let result = if path.is_dir() {
        add_dir(&mut zip, path, path, &options)
    } else {
        add_file(&mut zip, path, &name_of(path), &options)
    };

    // Finished on every path, including the failing one: a half-written zip
    // left open is a file nothing can read and nothing will clean up.
    let finish = zip.finish();

    if let Err(reason) = result {
        let _ = std::fs::remove_file(&into);
        return Err(reason);
    }

    finish.map_err(|err| format!("could not finish the archive: {err}"))?;

    Ok(into)
}

fn add_file(
    zip: &mut zip::ZipWriter<std::fs::File>,
    path: &Path,
    name: &str,
    options: &zip::write::FileOptions<'_, ()>,
) -> Result<(), String> {
    use std::io::{Read, Write};

    zip.start_file(name, *options)
        .map_err(|err| format!("could not add {name}: {err}"))?;

    let mut file =
        std::fs::File::open(path).map_err(|err| format!("could not read {name}: {err}"))?;
    let mut block = vec![0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut block)
            .map_err(|err| format!("could not read {name}: {err}"))?;

        if read == 0 {
            break;
        }

        zip.write_all(&block[..read])
            .map_err(|err| format!("could not write {name}: {err}"))?;
    }

    Ok(())
}

fn add_dir(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    at: &Path,
    options: &zip::write::FileOptions<'_, ()>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(at).map_err(|err| format!("could not read that folder: {err}"))?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Paths inside a zip are relative to what was compressed, and use
        // forward slashes whatever the platform: a backslash is a legal
        // character in a name on the other side of the archive.
        let inside = path
            .strip_prefix(root)
            .map_err(|_| "a path escaped the folder being compressed".to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_dir() {
            add_dir(zip, root, &path, options)?;
        } else {
            add_file(zip, &path, &inside, options)?;
        }
    }

    Ok(())
}

/// Which checksum a piece of text looks like, if any.
///
/// Length is the whole of it: these are hex strings and nothing else
/// distinguishes them. Knowing which kind matters because comparing a SHA-256
/// against a SHA-1 is not a mismatch, it is the wrong question, and reporting
/// it as "does not match" would send somebody off to re-download a file that
/// was fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Checksum {
    pub fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
        }
    }
}

/// The checksum a piece of text is, if it is one at all.
///
/// Whitespace either side is expected: a hash copied off a page usually brings
/// some with it, and some tools print it in spaced groups, which is why the
/// spaces inside are taken out rather than being treated as a refusal.
pub fn looks_like_checksum(text: &str) -> Option<Checksum> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();

    if cleaned.is_empty() || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    match cleaned.len() {
        32 => Some(Checksum::Md5),
        40 => Some(Checksum::Sha1),
        64 => Some(Checksum::Sha256),
        128 => Some(Checksum::Sha512),
        _ => None,
    }
}

/// Whether two checksums are the same, ignoring how they were written.
///
/// Case and spacing differ between the tools that print them, and neither is
/// part of the value. A comparison that says no because one side is upper case
/// is worse than no comparison.
pub fn same_checksum(a: &str, b: &str) -> bool {
    let tidy = |text: &str| -> String {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(|c| c.to_lowercase())
            .collect()
    };

    let (a, b) = (tidy(a), tidy(b));

    !a.is_empty() && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sill-file-ops-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    mod hashing {
        use super::*;

        /// The published answer for an empty file, so this is checked against
        /// the world rather than against itself.
        #[test]
        fn an_empty_file_hashes_to_the_known_value() {
            let dir = scratch("hash-empty");
            let path = dir.join("nothing");
            std::fs::write(&path, b"").expect("written");

            assert_eq!(
                sha256(&path).expect("hashes"),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            );
        }

        /// Likewise: "abc" is the canonical SHA-256 test vector.
        #[test]
        fn a_known_string_hashes_to_the_known_value() {
            let dir = scratch("hash-abc");
            let path = dir.join("abc");
            std::fs::write(&path, b"abc").expect("written");

            assert_eq!(
                sha256(&path).expect("hashes"),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            );
        }

        /// Bigger than the read block, so the loop is actually exercised.
        #[test]
        fn a_file_larger_than_one_block_hashes_the_whole_of_it() {
            let dir = scratch("hash-big");
            let one = dir.join("one");
            let two = dir.join("two");

            let mut body = vec![7u8; 200 * 1024];
            std::fs::write(&one, &body).expect("written");
            // One byte different, right at the end, past several blocks.
            *body.last_mut().expect("not empty") = 8;
            std::fs::write(&two, &body).expect("written");

            assert_ne!(
                sha256(&one).expect("hashes"),
                sha256(&two).expect("hashes"),
                "a change in the last block did not change the hash",
            );
        }

        #[test]
        fn a_file_that_is_not_there_says_so() {
            let dir = scratch("hash-absent");

            assert!(sha256(&dir.join("nothing")).is_err());
        }
    }

    mod names {
        use super::*;

        #[test]
        fn an_ordinary_name_is_fine() {
            assert!(usable_name("report.txt").is_ok());
            assert!(usable_name("a name with spaces.md").is_ok());
        }

        #[test]
        fn nothing_is_not_a_name() {
            assert!(usable_name("").is_err());
            assert!(usable_name("   ").is_err());
        }

        #[test]
        fn the_characters_a_path_is_made_of_are_refused() {
            for bad in [r"a\b", "a/b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
                assert!(usable_name(bad).is_err(), "{bad:?} was allowed");
            }
        }

        /// Windows drops a trailing dot silently, so the file ends up with a
        /// different name than the one that was typed and the rename looks
        /// half broken. Saying so beats letting it happen.
        #[test]
        fn a_trailing_dot_is_refused() {
            assert!(usable_name("report.").is_err());
            assert!(usable_name("report..").is_err());
        }

        /// A trailing space is tidied rather than refused. Windows would drop
        /// it too, but nobody types one on purpose and a stray one at the end
        /// of a name is not worth an error. `rename` trims to match.
        #[test]
        fn a_trailing_space_is_tidied_rather_than_refused() {
            assert!(usable_name("report.txt ").is_ok());
        }

        /// `CON.txt` is still `CON`, so the extension does not save it.
        #[test]
        fn the_names_windows_keeps_are_refused_with_or_without_an_extension() {
            for bad in ["CON", "con", "NUL", "COM1", "LPT9", "CON.txt", "nul.log"] {
                assert!(usable_name(bad).is_err(), "{bad:?} was allowed");
            }
        }

        #[test]
        fn a_name_that_merely_contains_a_reserved_word_is_fine() {
            assert!(usable_name("console.log").is_ok());
            assert!(usable_name("recon.txt").is_ok());
        }
    }

    mod free_names {
        use super::*;

        #[test]
        fn a_name_nothing_uses_is_the_name() {
            let dir = scratch("free-clear");
            let wanted = dir.join("report.zip");

            assert_eq!(free_name(&wanted), Some(wanted));
        }

        /// The number goes before the extension: `report.zip (2)` is not a zip
        /// file as far as anything else on the machine is concerned.
        #[test]
        fn a_taken_name_gets_a_number_before_the_extension() {
            let dir = scratch("free-taken");
            std::fs::write(dir.join("report.zip"), b"x").expect("written");

            assert_eq!(
                free_name(&dir.join("report.zip")),
                Some(dir.join("report (2).zip")),
            );
        }

        #[test]
        fn it_keeps_counting_past_the_second() {
            let dir = scratch("free-many");
            std::fs::write(dir.join("report.zip"), b"x").expect("written");
            std::fs::write(dir.join("report (2).zip"), b"x").expect("written");

            assert_eq!(
                free_name(&dir.join("report.zip")),
                Some(dir.join("report (3).zip")),
            );
        }

        #[test]
        fn a_name_with_no_extension_still_works() {
            let dir = scratch("free-bare");
            std::fs::write(dir.join("notes"), b"x").expect("written");

            assert_eq!(free_name(&dir.join("notes")), Some(dir.join("notes (2)")));
        }
    }

    mod renaming {
        use super::*;

        #[test]
        fn a_file_ends_up_with_the_new_name() {
            let dir = scratch("rename-plain");
            let from = dir.join("before.txt");
            std::fs::write(&from, b"body").expect("written");

            let to = rename(&from, "after.txt").expect("renames");

            assert_eq!(to, dir.join("after.txt"));
            assert!(!from.exists());
            assert_eq!(std::fs::read(&to).expect("read"), b"body");
        }

        /// Renaming one file onto another is how somebody loses the other one,
        /// and no undo would bring it back.
        #[test]
        fn it_refuses_to_land_on_something_that_is_already_there() {
            let dir = scratch("rename-onto");
            let from = dir.join("one.txt");
            std::fs::write(&from, b"one").expect("written");
            std::fs::write(dir.join("two.txt"), b"two").expect("written");

            assert!(rename(&from, "two.txt").is_err());
            assert_eq!(
                std::fs::read(dir.join("two.txt")).expect("read"),
                b"two",
                "the other file was overwritten",
            );
        }

        #[test]
        fn a_name_that_is_not_usable_is_refused_before_anything_moves() {
            let dir = scratch("rename-bad");
            let from = dir.join("one.txt");
            std::fs::write(&from, b"one").expect("written");

            assert!(rename(&from, "a/b.txt").is_err());
            assert!(from.exists(), "the file moved anyway");
        }

        #[test]
        fn renaming_something_to_its_own_name_says_so() {
            let dir = scratch("rename-same");
            let from = dir.join("one.txt");
            std::fs::write(&from, b"one").expect("written");

            assert!(rename(&from, "one.txt").is_err());
            assert!(from.exists());
        }
    }

    mod compressing {
        use super::*;

        fn names_in(zip: &Path) -> Vec<String> {
            let file = std::fs::File::open(zip).expect("open");
            let mut archive = zip::ZipArchive::new(file).expect("read");

            (0..archive.len())
                .map(|at| archive.by_index(at).expect("entry").name().to_string())
                .collect()
        }

        #[test]
        fn a_file_becomes_a_zip_beside_it() {
            let dir = scratch("zip-file");
            let path = dir.join("report.txt");
            std::fs::write(&path, b"the body").expect("written");

            let made = compress(&path).expect("compresses");

            assert_eq!(made, dir.join("report.zip"));
            assert_eq!(names_in(&made), vec!["report.txt"]);
        }

        #[test]
        fn a_folder_keeps_its_shape_inside_the_archive() {
            let dir = scratch("zip-folder");
            let root = dir.join("project");
            std::fs::create_dir_all(root.join("deep")).expect("dirs");
            std::fs::write(root.join("top.txt"), b"top").expect("written");
            std::fs::write(root.join("deep").join("under.txt"), b"under").expect("written");

            let made = compress(&root).expect("compresses");

            let mut found = names_in(&made);
            found.sort();
            // Forward slashes whatever the platform: a backslash is a legal
            // character in a name on the other side of the archive.
            assert_eq!(found, vec!["deep/under.txt", "top.txt"]);
        }

        #[test]
        fn a_second_archive_does_not_overwrite_the_first() {
            let dir = scratch("zip-twice");
            let path = dir.join("report.txt");
            std::fs::write(&path, b"body").expect("written");

            let first = compress(&path).expect("compresses");
            let second = compress(&path).expect("compresses again");

            assert_ne!(first, second);
            assert_eq!(second, dir.join("report (2).zip"));
            assert!(first.exists(), "the first archive was replaced");
        }

        #[test]
        fn what_went_in_comes_back_out_unchanged() {
            use std::io::Read;

            let dir = scratch("zip-roundtrip");
            let path = dir.join("report.txt");
            let body: Vec<u8> = (0..50_000u32).map(|n| (n % 251) as u8).collect();
            std::fs::write(&path, &body).expect("written");

            let made = compress(&path).expect("compresses");

            let file = std::fs::File::open(&made).expect("open");
            let mut archive = zip::ZipArchive::new(file).expect("read");
            let mut entry = archive.by_name("report.txt").expect("entry");
            let mut back = Vec::new();
            entry.read_to_end(&mut back).expect("read out");

            assert_eq!(back, body);
        }

        #[test]
        fn something_that_is_not_there_says_so_and_leaves_nothing_behind() {
            let dir = scratch("zip-absent");

            assert!(compress(&dir.join("nothing.txt")).is_err());
            // The half-made archive is cleaned up rather than left as a file
            // nothing can read.
            assert!(!dir.join("nothing.zip").exists());
        }
    }
}

#[cfg(test)]
mod checksums {
    use super::*;

    const SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn each_length_is_the_checksum_it_belongs_to() {
        assert_eq!(looks_like_checksum(&"a".repeat(32)), Some(Checksum::Md5));
        assert_eq!(looks_like_checksum(&"a".repeat(40)), Some(Checksum::Sha1));
        assert_eq!(looks_like_checksum(SHA256), Some(Checksum::Sha256));
        assert_eq!(looks_like_checksum(&"f".repeat(128)), Some(Checksum::Sha512));
    }

    /// A hash copied off a page brings whitespace with it, and some tools
    /// print it in spaced groups.
    #[test]
    fn whitespace_around_it_and_inside_it_is_ignored() {
        assert_eq!(looks_like_checksum(&format!("  {SHA256}\n")), Some(Checksum::Sha256));
        assert_eq!(
            looks_like_checksum("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad"),
            Some(Checksum::Sha256),
        );
    }

    #[test]
    fn ordinary_text_is_not_a_checksum() {
        assert_eq!(looks_like_checksum("hello"), None);
        assert_eq!(looks_like_checksum(""), None);
        assert_eq!(looks_like_checksum("   "), None);
        // Right length, but not hex.
        assert_eq!(looks_like_checksum(&"z".repeat(64)), None);
        // Hex, but no checksum is this long.
        assert_eq!(looks_like_checksum(&"a".repeat(50)), None);
    }

    /// A path can be all hex characters and is still not a checksum, because
    /// the slashes are not.
    #[test]
    fn something_with_punctuation_in_it_is_not_a_checksum() {
        assert_eq!(looks_like_checksum("abc/def"), None);
        assert_eq!(looks_like_checksum(&format!("sha256:{SHA256}")), None);
    }

    /// Case is not part of the value, and a comparison that says no because
    /// one side is upper case is worse than no comparison.
    #[test]
    fn case_and_spacing_do_not_make_two_checksums_different() {
        assert!(same_checksum(SHA256, &SHA256.to_uppercase()));
        assert!(same_checksum(SHA256, &format!("  {SHA256}  ")));
        assert!(same_checksum(
            SHA256,
            "BA7816BF 8F01CFEA 414140DE 5DAE2223 B00361A3 96177A9C B410FF61 F20015AD",
        ));
    }

    #[test]
    fn two_different_checksums_are_different() {
        assert!(!same_checksum(SHA256, &"a".repeat(64)));
    }

    /// Nothing matches nothing, or an empty clipboard would verify anything.
    #[test]
    fn nothing_never_matches() {
        assert!(!same_checksum("", ""));
        assert!(!same_checksum("   ", ""));
    }
}
