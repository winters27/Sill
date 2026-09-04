//! The documents applications remember, out of the jump lists Windows keeps.
//!
//! A jump list is the menu that appears when you right-click a taskbar button:
//! the documents that program has opened recently. Windows keeps one file per
//! application under `%APPDATA%\Microsoft\Windows\Recent\AutomaticDestinations`
//! and every program on the machine feeds it without being asked, because the
//! shell records it on the program's behalf whenever a file is opened through
//! a common dialog or an association.
//!
//! So it is the only list on Windows that knows what somebody actually worked
//! on, across every program, without any of those programs agreeing to
//! anything. Nothing else surfaces it. The Start menu shows a handful, and only
//! for pinned applications.
//!
//! ## Why this file is long
//!
//! Because the format is two formats. Each `*.automaticDestinations-ms` file is
//! an **OLE compound document**: the same container a `.doc` is, with a FAT, a
//! second FAT for small streams, and a directory. Inside it, one stream called
//! `DestList` holds the order and the paths, and the rest are shell links, one
//! per entry.
//!
//! Only `DestList` is read. It already carries the target path, when the entry
//! was last opened and whether it is pinned, which is everything a row needs;
//! parsing two hundred shell links per query to learn the same thing would be
//! paying twice.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. [`asked`] is a comparison against three words and [`matched`] takes
//! the reading as an argument, so a keystroke that is not one of them never
//! opens a file. That is the same shape `media` uses and it is tested the same
//! way, by counting the readings that ordinary queries take.

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};

/// One thing an application remembers having opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    /// Where the file is.
    pub path: String,
    /// The name of the jump list file this came out of, without its
    /// extension.
    ///
    /// **Not the application's name, and there is not one to be had.** Windows
    /// names each file after a hash of the program's identity and keeps no
    /// table anywhere on the machine mapping that back, so a row cannot
    /// honestly say which program remembered the document.
    ///
    /// Carried because it is what a row can be traced by: the probe prints it,
    /// and it is how "why is this document here" gets an answer. Sixteen
    /// characters per row against a bound of three hundred rows.
    pub source: String,
    /// When it was last opened, as a Windows FILETIME.
    ///
    /// Kept raw. It is only ever compared against another one to put the newest
    /// first, and converting to a calendar date would be inventing a
    /// requirement nothing has.
    pub at: u64,
    /// Whether the person pinned it to that application's list.
    pub pinned: bool,
    /// Whether what is there is a folder.
    ///
    /// Not in the file: a jump list records a path and nothing about what is
    /// at the end of it. Filled in when the path is checked for existing at
    /// all, which is one call answering both questions rather than two.
    pub folder: bool,
}

/*
 * The words that ask.
 *
 * The first word of the query, exactly, with whatever follows used to narrow
 * the list. Not a prefix match: "rec" is on the way to "recycle bin" and
 * "record", and a launcher that started reading two hundred files while
 * somebody was still typing would be doing the one thing rule 23 forbids.
 *
 * Three words rather than nine, because unlike the media row these do not name
 * an action somebody might type at any moment. They name this list.
 */
const ASKED_BY: &[&str] = &["recent", "recents", "jumplist"];

/// The filter after the word that asked, or nothing if this is not asking.
///
/// `Some("")` is somebody who typed just the word and wants the whole list;
/// `None` is every other query on earth. Distinguishing them is why this
/// answers an `Option<&str>` rather than a `bool`.
pub fn asked(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    let (first, rest) = match trimmed.find(char::is_whitespace) {
        Some(at) => (&trimmed[..at], trimmed[at..].trim_start()),
        None => (trimmed, ""),
    };

    if first.is_empty() {
        return None;
    }

    ASKED_BY
        .iter()
        .any(|one| first.eq_ignore_ascii_case(one))
        .then_some(rest)
}

/// How many rows one query may produce.
///
/// A jump list row is spliced into an ordinary search, so it competes with
/// applications and files for the space on screen. Twelve is more than fits
/// without scrolling and less than a wall.
pub const MOST_ROWS: usize = 12;

/// How many documents are kept in hand between keystrokes.
///
/// The cache exists because "recent" is followed by more typing: the words
/// after it narrow the list, and re-reading every jump list on the machine per
/// letter would be a hundred file opens per keystroke. The bound exists
/// because a cache with no bound is a leak with a good reason.
///
/// Three hundred of them is about forty kilobytes of paths, and it is the
/// three hundred most recently opened documents on the machine, which is
/// further back than anybody narrowing a list is looking.
pub const MOST_KEPT: usize = 300;

/// The documents matching a query, if the query asked for any.
///
/// `read` is the reading of the disk, and it is **not called** unless [`asked`]
/// says so. Taking it as an argument rather than calling it inside is what
/// lets a test prove that without any jump lists existing: the test passes a
/// reader that counts and asserts the count stayed at nought.
///
/// `on_disk` is the same idea for the second cost. A path out of a jump list is
/// frequently gone, deleted or on a drive that is not plugged in, and a row
/// that cannot be opened is worse than no row; but asking the filesystem about
/// three hundred paths per keystroke is not the way to find that out either.
/// So it is asked only about the handful that survived the filter. It answers
/// `None` for a path that is not there and `Some(folder)` for one that is,
/// because a folder and a file are drawn and acted on differently and one call
/// already knows which it is.
pub fn matched(
    query: &str,
    read: impl FnOnce() -> Vec<Recent>,
    on_disk: impl Fn(&str) -> Option<bool>,
) -> Vec<Recent> {
    let Some(filter) = asked(query) else {
        return Vec::new();
    };

    let found = read();
    let wanted: Vec<String> = filter
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect();

    found
        .into_iter()
        .filter(|one| {
            // Every word, anywhere in the path. The same "all of them, in any
            // order" rule the rest of the launcher matches by, so "recent tax
            // pdf" finds a PDF in a folder called tax.
            let lowered = one.path.to_lowercase();
            wanted.iter().all(|word| lowered.contains(word))
        })
        // Only now, and only for the ones that got this far.
        .filter_map(|one| on_disk(&one.path).map(|folder| Recent { folder, ..one }))
        .take(MOST_ROWS)
        .collect()
}

/// What to call the row.
pub fn title_for(one: &Recent) -> String {
    let path = std::path::Path::new(&one.path);

    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        // A drive root has no file name. Rare in a jump list and not worth a
        // row that says nothing.
        .unwrap_or_else(|| one.path.clone())
}

/// What goes underneath it: where the file is, and whether it was pinned.
///
/// The folder rather than the whole path, because the file name is already the
/// title and repeating it underneath wastes the only line there is.
pub fn subtitle_for(one: &Recent) -> String {
    let folder = std::path::Path::new(&one.path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();

    // A folder says so. Its title is a bare name like "Scans" or "assets",
    // which reads as a file, and the difference decides whether Enter opens
    // Explorer or a document.
    let what = match (one.pinned, one.folder) {
        (true, true) => "Pinned folder",
        (true, false) => "Pinned",
        (false, true) => "Folder",
        (false, false) => "",
    };

    match (what.is_empty(), folder.is_empty()) {
        (true, true) => "Recent".to_string(),
        (true, false) => folder,
        (false, true) => what.to_string(),
        (false, false) => format!("{what} · {folder}"),
    }
}

// ---------------------------------------------------------------------------
// The DestList stream
// ---------------------------------------------------------------------------

/// Where the entries start, after the stream's own header.
const DESTLIST_HEADER: usize = 32;

/// Longest path a jump list entry is allowed to claim.
///
/// The path length is read out of the file and used to step to the next entry,
/// so a wrong one does not produce a wrong row, it produces nonsense for the
/// whole rest of the stream. Windows itself cannot store a jump list path
/// longer than this, so a larger number means the entry layout was misread and
/// the right thing to do is stop.
const LONGEST_PATH: usize = 2048;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let slice = bytes.get(at..at + 8)?;
    let mut wide = [0u8; 8];
    wide.copy_from_slice(slice);
    Some(u64::from_le_bytes(wide))
}

/// Where the three fields worth reading sit inside one entry.
///
/// Written out per version rather than derived from one another. Version 6
/// moved the tail four bytes and version 1 puts the path much earlier, so any
/// arithmetic relating them would be a coincidence that held for three of the
/// four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// When the document was last opened, as a FILETIME.
    time_at: usize,
    /// Where it sits in the pinned list, or -1 for not pinned.
    pin_at: usize,
    /// Where the path's length in characters sits.
    length_at: usize,
    /// Bytes written after the path, before the next entry begins.
    trailer: usize,
}

/*
 * Every layout that has been seen, and which version writes it.
 *
 * Version 1 is Windows 7. Version 3 and 4 are Windows 8 and 10, and put
 * sixteen more bytes in front of the path; 4 also writes four bytes after it.
 *
 * **Version 6 is what this machine actually has**, on Windows 11 build 26200,
 * and it was not in any description of this format: four more bytes again in
 * front of the path, and the trailer version 4 introduced. Every one of the
 * two hundred and seven jump lists here is version 6, and read as version 4
 * they produce nothing at all.
 *
 * Which is why the version number is a hint here rather than an instruction.
 * See `destlist_entries`.
 */
const LAYOUTS: &[(u32, Layout)] = &[
    (
        1,
        Layout {
            time_at: 0x60,
            pin_at: 0x68,
            length_at: 0x6C,
            trailer: 0,
        },
    ),
    (
        3,
        Layout {
            time_at: 0x60,
            pin_at: 0x68,
            length_at: 0x7C,
            trailer: 0,
        },
    ),
    (
        4,
        Layout {
            time_at: 0x60,
            pin_at: 0x68,
            length_at: 0x7C,
            trailer: 4,
        },
    ),
    (
        6,
        Layout {
            time_at: 0x64,
            pin_at: 0x6C,
            length_at: 0x80,
            trailer: 4,
        },
    ),
];

/// The entries in a `DestList` stream.
///
/// Pure, so the awkward part of this file is testable without a compound
/// document, a jump list or Windows.
///
/// ## Why the version number is not simply obeyed
///
/// Reading the layout wrong does not misread a field. The path's length is
/// what steps to the next entry, so a wrong offset loses the position of every
/// entry after the first and the whole stream turns to noise. That is not a
/// theoretical risk: this machine writes **version 6**, which no description
/// of the format mentions, and the two thousand documents in its jump lists
/// read as nothing until the layout for it was worked out from the bytes.
///
/// The saving grace is that the stream says how many entries it holds, in its
/// own header, four bytes in. So the layout is not guessed: the one the
/// version names is tried, the count that comes back is compared against the
/// count the file declares, and a layout that does not agree with the file
/// about how many things are in it is rejected in favour of one that does.
///
/// A stream cut off part way through, which the two largest jump lists here
/// are, walks one fewer entry than it declares. So the closest is taken rather
/// than only an exact match, and a layout that finds less than half of what
/// was declared is treated as no answer at all.
pub fn destlist_entries(bytes: &[u8]) -> Vec<Recent> {
    let version = u32_at(bytes, 0).unwrap_or_default();
    let declared = u32_at(bytes, 4).unwrap_or_default() as usize;

    // The one this file names first, then the rest. Ordinary files stop at the
    // first, which is the whole reason for the order.
    let named = LAYOUTS.iter().find(|(one, _)| *one == version);
    let candidates = named
        .into_iter()
        .chain(LAYOUTS.iter().filter(|(one, _)| *one != version));

    let mut best: Option<(usize, usize, Vec<Recent>)> = None;

    for (_, layout) in candidates {
        let (walked, named, found) = entries_by(bytes, *layout);

        /*
         * The count alone is not enough, and this cost a test to learn.
         *
         * Read with version 4's offsets, a version 6 entry finds a zero where
         * the path length should be, reads a path of no characters, and steps
         * to exactly the right place anyway, because a zero-length path is the
         * same size as the four bytes version 6 put there. So the wrong layout
         * agreed with the header about the number of entries and produced a
         * stream of documents with no names.
         *
         * A layout that reads the right number of nothings is not the right
         * layout.
         */
        if walked == declared && named > 0 {
            return found;
        }

        let missed = declared.abs_diff(walked);
        if best
            .as_ref()
            .is_none_or(|(worst, fewer, _)| missed < *worst || (missed == *worst && named > *fewer))
        {
            best = Some((missed, named, found));
        }
    }

    match best {
        // Half is the line between "a stream that was cut off" and "these
        // bytes are not entries". A jump list is truncated by a crash all the
        // time; it does not lose most of itself.
        Some((missed, named, found)) if missed * 2 <= declared && named > 0 => found,
        _ => Vec::new(),
    }
}

/// One pass over the entries with one layout.
///
/// Answers three things: how many entries it stepped over, how many of those
/// had a path with anything in it at all, and the rows worth drawing. The
/// first is compared against the file's own header; the second is what tells a
/// layout that read the right number of empty strings from one that read the
/// documents; the third is filtered, so a jump list full of addresses would on
/// its own look like a layout that found nothing.
fn entries_by(bytes: &[u8], layout: Layout) -> (usize, usize, Vec<Recent>) {
    let Layout {
        time_at,
        pin_at,
        length_at,
        trailer,
    } = layout;

    let mut found = Vec::new();
    let mut walked = 0usize;
    let mut named = 0usize;
    let mut at = DESTLIST_HEADER;

    while at + length_at + 2 <= bytes.len() {
        let Some(count) = u16_at(bytes, at + length_at) else {
            break;
        };

        let characters = count as usize;
        if characters > LONGEST_PATH {
            break;
        }

        let from = at + length_at + 2;
        let Some(text) = bytes.get(from..from + characters * 2) else {
            break;
        };

        let path = String::from_utf16_lossy(
            &text
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<u16>>(),
        );

        // -1 means "not pinned"; anything else is the position it was pinned
        // at. Both are read before `at` moves on.
        let pinned = u32_at(bytes, at + pin_at).is_some_and(|status| status != u32::MAX);
        let when = u64_at(bytes, at + time_at).unwrap_or_default();

        at = from + characters * 2 + trailer;
        walked += 1;
        if !path.trim().is_empty() {
            named += 1;
        }

        // A jump list holds things that are not files: a shell folder written
        // as `::{GUID}`, a library, an `ms-gamebar://` address, which is the
        // very first entry in the largest jump list on this machine. None of
        // them is a row anything here can open, and a row that cannot be acted
        // on is not worth drawing.
        if !looks_like_a_path(&path) {
            continue;
        }

        found.push(Recent {
            path,
            source: String::new(),
            at: when,
            pinned,
            folder: false,
        });
    }

    (walked, named, found)
}

/// Whether this is a path on a disk rather than something only the shell
/// understands.
fn looks_like_a_path(path: &str) -> bool {
    if path.starts_with(r"\\") {
        return true;
    }

    let bytes = path.as_bytes();
    bytes.len() > 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

// ---------------------------------------------------------------------------
// The compound document around it
// ---------------------------------------------------------------------------

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
/// The last sector of a chain.
const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// A sector belonging to no stream.
const FREE: u32 = 0xFFFF_FFFF;
/// One directory entry is this many bytes, in every version.
const DIRECTORY_ENTRY: usize = 128;
/// A directory entry describing a stream.
const KIND_STREAM: u8 = 2;
/// Nothing here reads a file larger than this, and a header claiming more is a
/// header to stop reading rather than to allocate for.
const LONGEST_STREAM: u64 = 32 * 1024 * 1024;

/// One entry in the compound document's directory.
struct Listed {
    name: String,
    kind: u8,
    start: u32,
    size: u64,
}

/// Enough of a compound document to pull one stream out of it.
///
/// Reads over anything seekable, which is what lets every test here work
/// against bytes in memory rather than a file somebody has to have.
pub struct Compound {
    sector: usize,
    mini: usize,
    cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    directory: Vec<Listed>,
}

impl Compound {
    /// Reads the header, both allocation tables and the directory.
    pub fn open<S: Read + Seek>(source: &mut S) -> Result<Self, String> {
        let mut head = [0u8; 512];
        source
            .rewind()
            .and_then(|()| source.read_exact(&mut head))
            .map_err(|err| format!("could not read the header: {err}"))?;

        if head[..8] != SIGNATURE {
            return Err("not a compound document".to_string());
        }

        let shift = u16_at(&head, 0x1E).unwrap_or_default();
        // 512 and 4096 are the only sizes the format defines, and a shift
        // outside this range means the header is not what it says it is.
        if !(9..=12).contains(&shift) {
            return Err(format!("a sector size of 2^{shift} is not a sector size"));
        }
        let sector = 1usize << shift;

        let mini_shift = u16_at(&head, 0x20).unwrap_or_default();
        if !(1..=shift).contains(&mini_shift) {
            return Err(format!(
                "a mini sector size of 2^{mini_shift} makes no sense"
            ));
        }
        let mini = 1usize << mini_shift;

        let fat_count = u32_at(&head, 0x2C).unwrap_or_default() as usize;
        let directory_start = u32_at(&head, 0x30).unwrap_or(END_OF_CHAIN);
        let cutoff = u32_at(&head, 0x38).unwrap_or(4096) as u64;
        let mini_fat_start = u32_at(&head, 0x3C).unwrap_or(END_OF_CHAIN);
        let difat_start = u32_at(&head, 0x44).unwrap_or(END_OF_CHAIN);
        let difat_count = u32_at(&head, 0x48).unwrap_or_default() as usize;

        /*
         * Which sectors hold the FAT.
         *
         * The first hundred and nine of them are named in the header itself,
         * which covers every file up to about seven megabytes. Beyond that the
         * list continues in sectors of its own, each ending with the number of
         * the next. Jump lists never get there, and the walk is here anyway
         * because "never" about somebody else's file format is a guess.
         */
        let mut fat_sectors: Vec<u32> = (0..109)
            .filter_map(|at| u32_at(&head, 0x4C + at * 4))
            .filter(|&one| one != FREE)
            .collect();

        let mut next = difat_start;
        for _ in 0..difat_count {
            if next == END_OF_CHAIN || next == FREE {
                break;
            }

            let block = read_sector(source, sector, next)?;
            let per = sector / 4 - 1;
            fat_sectors.extend(
                (0..per)
                    .filter_map(|at| u32_at(&block, at * 4))
                    .filter(|&one| one != FREE),
            );
            next = u32_at(&block, per * 4).unwrap_or(END_OF_CHAIN);
        }

        fat_sectors.truncate(fat_count);

        let mut fat = Vec::with_capacity(fat_sectors.len() * sector / 4);
        for one in fat_sectors {
            let block = read_sector(source, sector, one)?;
            fat.extend((0..sector / 4).filter_map(|at| u32_at(&block, at * 4)));
        }

        // Built before the mini FAT and the directory, because reading either
        // of those follows a chain through it.
        let mut document = Self {
            sector,
            mini,
            cutoff,
            fat,
            mini_fat: Vec::new(),
            directory: Vec::new(),
        };

        for one in document.chain(mini_fat_start) {
            let block = read_sector(source, sector, one)?;
            document
                .mini_fat
                .extend((0..sector / 4).filter_map(|at| u32_at(&block, at * 4)));
        }

        for one in document.chain(directory_start) {
            let block = read_sector(source, sector, one)?;

            for at in (0..sector).step_by(DIRECTORY_ENTRY) {
                let Some(entry) = block.get(at..at + DIRECTORY_ENTRY) else {
                    break;
                };

                let named = u16_at(entry, 0x40).unwrap_or_default() as usize;
                // The length counts the terminator, and a zero means an entry
                // that was never used.
                let letters = named.saturating_sub(2) / 2;
                let name = String::from_utf16_lossy(
                    &entry[..letters.min(31) * 2]
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect::<Vec<u16>>(),
                );

                document.directory.push(Listed {
                    name,
                    kind: entry[0x42],
                    start: u32_at(entry, 0x74).unwrap_or(END_OF_CHAIN),
                    size: u64_at(entry, 0x78).unwrap_or_default(),
                });
            }
        }

        Ok(document)
    }

    /// The sectors of one chain, in order.
    ///
    /// Bounded by the size of the table it walks, which is what stops a file
    /// whose FAT points at itself from spinning forever. A corrupt jump list
    /// is not a rare thing: they are written by every program on the machine
    /// and truncated by every crash.
    fn chain(&self, from: u32) -> Vec<u32> {
        let mut found = Vec::new();
        let mut at = from;

        while at != END_OF_CHAIN && at != FREE && found.len() <= self.fat.len() {
            found.push(at);
            match self.fat.get(at as usize) {
                Some(&next) => at = next,
                None => break,
            }
        }

        found
    }

    /// The bytes of a stream by name, or nothing if there is no such stream.
    pub fn stream<S: Read + Seek>(
        &self,
        source: &mut S,
        wanted: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let Some(entry) = self
            .directory
            .iter()
            .find(|one| one.kind == KIND_STREAM && one.name == wanted)
        else {
            return Ok(None);
        };

        if entry.size > LONGEST_STREAM {
            return Err(format!("{wanted} claims to be {} bytes", entry.size));
        }

        let size = entry.size as usize;
        let mut out = Vec::with_capacity(size);

        if entry.size >= self.cutoff {
            for one in self.chain(entry.start) {
                out.extend(read_sector(source, self.sector, one)?);
                if out.len() >= size {
                    break;
                }
            }

            out.truncate(size);
            return Ok(Some(out));
        }

        /*
         * A small stream does not live in the file's own sectors.
         *
         * It lives inside the mini stream, which is itself an ordinary stream
         * hanging off the root directory entry, chained through a second
         * allocation table. So reading one is two lookups: which mini sector,
         * then which real sector that mini sector is inside.
         *
         * The alternative is to read the whole mini stream first, and on the
         * jump list for File Explorer on this machine that is a megabyte read
         * to find a few kilobytes.
         */
        let Some(root) = self.directory.first() else {
            return Err("the document has no root entry".to_string());
        };
        let holding = self.chain(root.start);

        let mut at = entry.start;
        let mut walked = 0usize;

        while at != END_OF_CHAIN && at != FREE && out.len() < size && walked <= self.mini_fat.len()
        {
            walked += 1;

            let offset = at as usize * self.mini;
            let which = offset / self.sector;
            let inside = offset % self.sector;

            let Some(&real) = holding.get(which) else {
                break;
            };

            let block = read_sector(source, self.sector, real)?;
            let Some(piece) = block.get(inside..inside + self.mini) else {
                break;
            };
            out.extend(piece);

            match self.mini_fat.get(at as usize) {
                Some(&next) => at = next,
                None => break,
            }
        }

        out.truncate(size);
        Ok(Some(out))
    }
}

/// One sector, by its number.
///
/// Sector zero begins immediately after the header, and the header occupies a
/// whole sector whatever the sector size is, which is why the offset is one
/// more than the number.
///
/// ## The last sector is routinely not all there
///
/// **Two of the two hundred and seven jump lists on this machine end part way
/// through a sector**, and they are the two largest: the File Explorer one and
/// the one behind it. A read demanding a whole sector fails on those files,
/// and it fails on the sector holding the end of `DestList`, so the answer for
/// the two applications with the most history was nothing at all.
///
/// A short tail is filled with zeroes and the entry parse stops where the
/// bytes stop, which is what a half-written record should do. A sector
/// beginning past the end of the file is a different thing: that is a chain
/// pointing somewhere there has never been anything, which means the table was
/// misread, and reporting it is how the probe found this in the first place.
fn read_sector<S: Read + Seek>(
    source: &mut S,
    sector: usize,
    index: u32,
) -> Result<Vec<u8>, String> {
    let at = (index as u64 + 1) * sector as u64;
    let mut block = vec![0u8; sector];

    source
        .seek(SeekFrom::Start(at))
        .map_err(|err| format!("could not reach sector {index}: {err}"))?;

    let mut filled = 0usize;
    while filled < sector {
        match source.read(&mut block[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(err) => return Err(format!("could not read sector {index}: {err}")),
        }
    }

    if filled == 0 {
        return Err(format!("sector {index} is past the end of the file"));
    }

    Ok(block)
}

/// The documents one jump list file remembers.
///
/// Separate from the walk over the folder so that a test can point it at one
/// real file and say what came out.
pub fn documents_in<S: Read + Seek>(source: &mut S, named: &str) -> Result<Vec<Recent>, String> {
    let document = Compound::open(source)?;
    let Some(stream) = document.stream(source, "DestList")? else {
        return Ok(Vec::new());
    };

    Ok(destlist_entries(&stream)
        .into_iter()
        .map(|mut one| {
            one.source = named.to_string();
            one
        })
        .collect())
}

/// Where Windows keeps the automatic jump lists.
#[cfg(windows)]
pub fn folder() -> Option<std::path::PathBuf> {
    let roaming = std::env::var_os("APPDATA")?;
    let path =
        std::path::PathBuf::from(roaming).join(r"Microsoft\Windows\Recent\AutomaticDestinations");

    path.is_dir().then_some(path)
}

/// The most recently opened documents on this machine, newest first.
///
/// Read when somebody asks and never otherwise. Two hundred files on this
/// machine, and the read of each one is a header, an allocation table and one
/// stream rather than the whole file: the largest jump list here is 1.3 MB and
/// the part of it worth reading is a fiftieth of that.
///
/// A file that will not parse is skipped in silence. They are written by every
/// program on the machine and truncated by every crash, so one that makes no
/// sense is an ordinary Tuesday rather than something to tell somebody about.
#[cfg(windows)]
pub fn recent() -> Vec<Recent> {
    let Some(folder) = folder() else {
        return Vec::new();
    };

    let Ok(listing) = std::fs::read_dir(&folder) else {
        return Vec::new();
    };

    let mut found: Vec<Recent> = Vec::new();

    for entry in listing.flatten() {
        let path = entry.path();
        if path.extension().and_then(|one| one.to_str()) != Some("automaticDestinations-ms") {
            continue;
        }

        let named = path
            .file_stem()
            .map(|one| one.to_string_lossy().into_owned())
            .unwrap_or_default();

        /*
         * The whole file at once, rather than the sectors that are wanted.
         *
         * Reading only what is needed sounds like the frugal choice and is the
         * expensive one: a chain is followed one sector at a time, so the
         * jump list for File Explorer alone is a thousand seeks and a thousand
         * reads. Two hundred files of that **measured 28 seconds cold**.
         *
         * The largest file here is 1.3 MB and the whole folder is 6 MB, read
         * once, into a buffer that is dropped before the next file is opened.
         */
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };

        if let Ok(documents) = documents_in(&mut std::io::Cursor::new(bytes), &named) {
            found.extend(documents);
        }
    }

    newest_first(found)
}

#[cfg(not(windows))]
pub fn recent() -> Vec<Recent> {
    Vec::new()
}

/// Newest first, one row per document, bounded.
///
/// Two programs opening the same file both record it, and two rows for one
/// document that do exactly the same thing is a worse list. The newer of the
/// two survives, so the time on the row is the last time the document was
/// opened by anything.
pub fn newest_first(mut found: Vec<Recent>) -> Vec<Recent> {
    found.sort_by(|left, right| right.at.cmp(&left.at));

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    found.retain(|one| seen.insert(one.path.to_lowercase()));
    found.truncate(MOST_KEPT);
    found
}

/// How long a reading is reused for.
///
/// Longer than the second the switches use, because what is behind this is a
/// couple of hundred file reads rather than one call into Windows, and because
/// what it answers does not change while somebody is typing: a document opened
/// in the last five seconds is not what "recent tax" is looking for.
pub const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(5);

/// The documents, read at most once every [`FRESH_FOR`].
pub fn now(held: &crate::state::Fresh<Vec<Recent>>) -> Vec<Recent> {
    held.get(recent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Builds a `DestList` stream the way Windows writes one.
    ///
    /// A helper rather than a fixture file, because what is being tested is the
    /// stepping from one entry to the next and that is only interesting with
    /// several entries of different lengths.
    fn destlist(version: u32, entries: &[(&str, u64, bool)]) -> Vec<u8> {
        let layout = LAYOUTS
            .iter()
            .find(|(one, _)| *one == version)
            .map(|(_, layout)| *layout)
            .expect("a version the reader knows");

        let mut out = vec![0u8; DESTLIST_HEADER];
        out[..4].copy_from_slice(&version.to_le_bytes());
        out[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());

        for (path, when, pinned) in entries {
            let mut entry = vec![0u8; layout.length_at];
            entry[layout.time_at..layout.time_at + 8].copy_from_slice(&when.to_le_bytes());
            entry[layout.pin_at..layout.pin_at + 4]
                .copy_from_slice(&if *pinned { 0u32 } else { u32::MAX }.to_le_bytes());

            let wide: Vec<u16> = path.encode_utf16().collect();
            entry.extend((wide.len() as u16).to_le_bytes());
            for unit in &wide {
                entry.extend(unit.to_le_bytes());
            }
            entry.extend(std::iter::repeat_n(0u8, layout.trailer));

            out.extend(entry);
        }

        out
    }

    #[test]
    fn the_windows_ten_layout_reads() {
        let stream = destlist(
            3,
            &[
                (r"C:\work\notes.md", 100, false),
                (r"C:\work\a much longer name.txt", 200, true),
            ],
        );

        let found = destlist_entries(&stream);
        assert_eq!(found.len(), 2, "the second entry was not found");
        assert_eq!(found[0].path, r"C:\work\notes.md");
        assert_eq!(found[1].path, r"C:\work\a much longer name.txt");
        assert!(found[1].pinned);
        assert!(!found[0].pinned);
        assert_eq!(found[1].at, 200);
    }

    /// Version 4 writes four more bytes after every path.
    ///
    /// Read as version 3 the first entry is right and every later one is
    /// garbage, which is the failure mode this whole function is shaped
    /// around.
    #[test]
    fn the_windows_eleven_layout_reads() {
        let stream = destlist(
            4,
            &[
                (r"C:\one\first.pdf", 10, false),
                (r"C:\two\second.pdf", 20, false),
                (r"C:\three\third.pdf", 30, false),
            ],
        );

        let paths: Vec<String> = destlist_entries(&stream)
            .into_iter()
            .map(|one| one.path)
            .collect();

        assert_eq!(
            paths,
            [
                r"C:\one\first.pdf",
                r"C:\two\second.pdf",
                r"C:\three\third.pdf"
            ]
        );
    }

    /// Windows 7 ends an entry sixteen bytes earlier.
    #[test]
    fn the_windows_seven_layout_reads() {
        let stream = destlist(1, &[(r"D:\old\report.doc", 5, false)]);
        let found = destlist_entries(&stream);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, r"D:\old\report.doc");
    }

    /// What this machine actually writes, on Windows 11 build 26200.
    ///
    /// Four bytes further along than version 4 everywhere after the entry
    /// number. Read as version 4 a version 6 stream yields nothing, which is
    /// what every jump list on this machine did until the bytes were looked
    /// at.
    #[test]
    fn the_layout_this_machine_writes_reads() {
        let stream = destlist(
            6,
            &[
                (r"C:\work\one.md", 0x01DCFF32DF3EE9AA, false),
                (r"C:\work\two.md", 0x01DCFF32DF3EE9AB, true),
            ],
        );

        let found = destlist_entries(&stream);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].path, r"C:\work\two.md");
        assert!(found[1].pinned);
        assert_eq!(found[0].at, 0x01DCFF32DF3EE9AA);
    }

    /// The offsets version 6 uses, as numbers, because they were worked out
    /// from a hex dump and nothing else on the machine records them.
    #[test]
    fn version_six_is_version_four_shifted_by_four() {
        let six = LAYOUTS
            .iter()
            .find(|(one, _)| *one == 6)
            .map(|(_, layout)| *layout)
            .expect("version 6 is known");
        let four = LAYOUTS
            .iter()
            .find(|(one, _)| *one == 4)
            .map(|(_, layout)| *layout)
            .expect("version 4 is known");

        assert_eq!(six.time_at, 0x64);
        assert_eq!(six.pin_at, 0x6C);
        assert_eq!(six.length_at, 0x80);
        assert_eq!(six.time_at, four.time_at + 4);
        assert_eq!(six.pin_at, four.pin_at + 4);
        assert_eq!(six.length_at, four.length_at + 4);
    }

    /// A version number that lies is still read.
    ///
    /// This is the whole reason the count in the header is consulted. A build
    /// of Windows nobody has yet will write a version nobody has heard of, and
    /// the choice is between reading it and showing nothing.
    #[test]
    fn a_stream_whose_version_is_not_the_one_it_says_is_still_read() {
        let stream = destlist(6, &[(r"C:\one\first.pdf", 10, false)]);
        let mut lying = stream.clone();
        lying[..4].copy_from_slice(&4u32.to_le_bytes());

        let found = destlist_entries(&lying);
        assert_eq!(
            found.first().map(|one| one.path.as_str()),
            Some(r"C:\one\first.pdf"),
            "a version 6 stream labelled 4 was not recovered"
        );
    }

    /// And bytes that are not entries under any layout produce nothing rather
    /// than the least bad nonsense.
    #[test]
    fn bytes_that_are_not_entries_produce_nothing() {
        let mut noise = vec![0u8; 32 + 600];
        noise[..4].copy_from_slice(&6u32.to_le_bytes());
        noise[4..8].copy_from_slice(&40u32.to_le_bytes());
        for (at, byte) in noise[32..].iter_mut().enumerate() {
            *byte = (at % 7) as u8 + 1;
        }

        assert!(
            destlist_entries(&noise).is_empty(),
            "a stream claiming forty entries and holding none produced rows"
        );
    }

    /// A jump list holds things the shell understands and nothing else can
    /// open.
    #[test]
    fn a_row_that_is_not_a_file_is_not_offered() {
        let stream = destlist(
            4,
            &[
                (r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}", 1, false),
                ("https://example.com/page", 2, false),
                (r"C:\real\file.txt", 3, false),
                (r"\\server\share\file.txt", 4, false),
            ],
        );

        let paths: Vec<String> = destlist_entries(&stream)
            .into_iter()
            .map(|one| one.path)
            .collect();

        assert_eq!(paths, [r"C:\real\file.txt", r"\\server\share\file.txt"]);
    }

    /// A truncated stream is an ordinary thing to find.
    #[test]
    fn a_half_written_stream_yields_what_it_has_and_stops() {
        let stream = destlist(
            4,
            &[(r"C:\a\one.txt", 1, false), (r"C:\a\two.txt", 2, false)],
        );

        // The header alone, a cut inside the first entry's fixed part, and a
        // cut inside the second entry's path. Not the last four bytes: those
        // are version 4's trailer, and by then both paths have been read, so a
        // stream missing them really does hold both entries.
        for cut in [DESTLIST_HEADER, DESTLIST_HEADER + 50, stream.len() - 10] {
            let found = destlist_entries(&stream[..cut]);
            assert!(
                found.len() < 2,
                "a stream cut at {cut} still produced both entries"
            );
        }
    }

    #[test]
    fn nonsense_is_not_a_panic() {
        assert!(destlist_entries(&[]).is_empty());
        assert!(destlist_entries(&[0u8; 8]).is_empty());
        assert!(destlist_entries(&[0xFFu8; 256]).is_empty());
    }

    /// An entry claiming a path longer than Windows can store means the layout
    /// was misread, and every later entry would be nonsense.
    #[test]
    fn an_impossible_path_length_stops_the_read() {
        let mut stream = destlist(4, &[(r"C:\a\one.txt", 1, false)]);
        stream[DESTLIST_HEADER + 0x7C..DESTLIST_HEADER + 0x7C + 2]
            .copy_from_slice(&9000u16.to_le_bytes());

        assert!(destlist_entries(&stream).is_empty());
    }

    // -- the gate ----------------------------------------------------------

    /// The whole cost claim, as a test.
    #[test]
    fn a_query_that_is_not_asking_never_reads_a_file() {
        let taken = Cell::new(0);
        let read = || {
            taken.set(taken.get() + 1);
            vec![Recent {
                path: r"C:\a\one.txt".to_string(),
                source: "abc".to_string(),
                at: 1,
                pinned: false,
                folder: false,
            }]
        };

        for query in [
            "",
            "   ",
            "rec",
            "rece",
            "recycle bin",
            "record",
            "recipes",
            "chrome",
            "notepad",
            "2+2",
            "jump",
            "recentish",
        ] {
            assert!(
                matched(query, read, |_| Some(false)).is_empty(),
                "{query:?} produced rows when it is not asking for any"
            );
        }

        assert_eq!(
            taken.get(),
            0,
            "the disk was read {} time(s) for queries that asked nothing",
            taken.get()
        );
    }

    #[test]
    fn the_words_that_ask_read_once_each() {
        for word in ASKED_BY {
            let taken = Cell::new(0);
            let read = || {
                taken.set(taken.get() + 1);
                vec![Recent {
                    path: r"C:\a\one.txt".to_string(),
                    source: "abc".to_string(),
                    at: 1,
                    pinned: false,
                    folder: false,
                }]
            };

            assert_eq!(
                matched(word, read, |_| Some(false)).len(),
                1,
                "{word:?} found none"
            );
            assert_eq!(taken.get(), 1, "{word:?} read {} times", taken.get());
        }
    }

    #[test]
    fn the_words_after_it_narrow_the_list() {
        let all = || {
            vec![
                Recent {
                    path: r"C:\tax\2024 return.pdf".to_string(),
                    source: "a".to_string(),
                    at: 3,
                    pinned: false,
                    folder: false,
                },
                Recent {
                    path: r"C:\work\notes.md".to_string(),
                    source: "a".to_string(),
                    at: 2,
                    pinned: false,
                    folder: false,
                },
            ]
        };

        let found = matched("recent tax pdf", all, |_| Some(false));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, r"C:\tax\2024 return.pdf");

        assert_eq!(matched("recent", all, |_| Some(false)).len(), 2);
        assert!(matched("recent nothing", all, |_| Some(false)).is_empty());
    }

    /// A path out of a jump list is frequently gone, and the filesystem is
    /// asked only about the few that survived the filter.
    #[test]
    fn a_document_that_is_gone_is_not_offered_and_is_asked_about_once() {
        let asked_about = std::cell::RefCell::new(Vec::new());

        let all = || {
            (0..50)
                .map(|n| Recent {
                    path: format!(r"C:\a\{n}.txt"),
                    source: "a".to_string(),
                    at: n,
                    pinned: false,
                    folder: false,
                })
                .collect()
        };

        let found = matched("recent 1", all, |path| {
            asked_about.borrow_mut().push(path.to_string());
            path.ends_with("1.txt").then_some(false)
        });

        assert!(found.iter().all(|one| one.path.ends_with("1.txt")));
        assert!(
            asked_about.borrow().len() <= 14,
            "the filesystem was asked about {} paths for one keystroke",
            asked_about.borrow().len()
        );
    }

    #[test]
    fn no_more_rows_than_fit() {
        let many = || {
            (0..100)
                .map(|n| Recent {
                    path: format!(r"C:\a\{n}.txt"),
                    source: "a".to_string(),
                    at: n,
                    pinned: false,
                    folder: false,
                })
                .collect()
        };

        assert_eq!(matched("recent", many, |_| Some(false)).len(), MOST_ROWS);
    }

    #[test]
    fn asking_is_not_a_matter_of_typing_it_neatly() {
        assert_eq!(asked("  Recent  "), Some(""));
        assert_eq!(asked("RECENT tax"), Some("tax"));
        assert_eq!(asked("recent"), Some(""));
        assert_eq!(asked("recently"), None);
    }

    // -- the list ----------------------------------------------------------

    #[test]
    fn one_document_opened_by_two_programs_is_one_row() {
        let found = newest_first(vec![
            Recent {
                path: r"C:\a\one.txt".to_string(),
                source: "aaa".to_string(),
                at: 5,
                pinned: false,
                folder: false,
            },
            Recent {
                path: r"C:\A\ONE.TXT".to_string(),
                source: "bbb".to_string(),
                at: 9,
                pinned: false,
                folder: false,
            },
        ]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].at, 9, "the older reading survived");
    }

    #[test]
    fn what_is_kept_is_bounded() {
        let many: Vec<Recent> = (0..MOST_KEPT * 2)
            .map(|n| Recent {
                path: format!(r"C:\a\{n}.txt"),
                source: "a".to_string(),
                at: n as u64,
                pinned: false,
                folder: false,
            })
            .collect();

        assert_eq!(newest_first(many).len(), MOST_KEPT);
    }

    #[test]
    fn a_row_says_the_file_and_then_where_it_is() {
        let one = Recent {
            path: r"C:\work\notes.md".to_string(),
            source: "a".to_string(),
            at: 1,
            pinned: false,
            folder: false,
        };

        assert_eq!(title_for(&one), "notes.md");
        assert_eq!(subtitle_for(&one), r"C:\work");

        let pinned = Recent {
            pinned: true,
            ..one.clone()
        };
        assert_eq!(subtitle_for(&pinned), r"Pinned · C:\work");

        // A folder's title is a bare name like "Scans", which reads as a file
        // until the row says otherwise. This machine's jump lists hold several.
        let folder = Recent {
            folder: true,
            ..one
        };
        assert_eq!(subtitle_for(&folder), r"Folder · C:\work");
    }

    // -- the container -----------------------------------------------------

    /// Builds a compound document holding one named stream.
    ///
    /// Small enough to go in the mini stream, or large enough not to, which is
    /// the branch worth having: the two are read through different tables and
    /// only one of them is exercised by a jump list small enough to be
    /// convenient.
    fn compound(name: &str, body: &[u8]) -> Vec<u8> {
        const SECTOR: usize = 512;
        const MINI: usize = 64;
        const CUTOFF: usize = 4096;

        let mini = body.len() < CUTOFF;

        // Sector 0: FAT. Sector 1: directory. Sector 2: mini FAT.
        // Sector 3 onwards: the mini stream, or the stream itself.
        let mut fat = vec![FREE; SECTOR / 4];
        fat[0] = 0xFFFF_FFFD; // this sector holds the FAT
        fat[1] = END_OF_CHAIN; // the directory
        fat[2] = END_OF_CHAIN; // the mini FAT

        let sectors = body.len().div_ceil(SECTOR).max(1);
        for at in 0..sectors {
            fat[3 + at] = if at + 1 == sectors {
                END_OF_CHAIN
            } else {
                (4 + at) as u32
            };
        }

        let mut mini_fat = vec![FREE; SECTOR / 4];
        let mini_sectors = body.len().div_ceil(MINI).max(1);
        if mini {
            for at in 0..mini_sectors {
                mini_fat[at] = if at + 1 == mini_sectors {
                    END_OF_CHAIN
                } else {
                    (at + 1) as u32
                };
            }
        }

        let mut header = vec![0u8; SECTOR];
        header[..8].copy_from_slice(&SIGNATURE);
        header[0x1E..0x20].copy_from_slice(&9u16.to_le_bytes());
        header[0x20..0x22].copy_from_slice(&6u16.to_le_bytes());
        header[0x2C..0x30].copy_from_slice(&1u32.to_le_bytes());
        header[0x30..0x34].copy_from_slice(&1u32.to_le_bytes());
        header[0x38..0x3C].copy_from_slice(&(CUTOFF as u32).to_le_bytes());
        header[0x3C..0x40].copy_from_slice(&2u32.to_le_bytes());
        header[0x40..0x44].copy_from_slice(&1u32.to_le_bytes());
        header[0x44..0x48].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
        header[0x48..0x4C].copy_from_slice(&0u32.to_le_bytes());
        header[0x4C..0x50].copy_from_slice(&0u32.to_le_bytes());
        for at in 1..109 {
            header[0x4C + at * 4..0x50 + at * 4].copy_from_slice(&FREE.to_le_bytes());
        }

        let mut directory = vec![0u8; SECTOR];
        let mut write = |at: usize, name: &str, kind: u8, start: u32, size: u64| {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            for (n, unit) in wide.iter().enumerate() {
                directory[at + n * 2..at + n * 2 + 2].copy_from_slice(&unit.to_le_bytes());
            }
            directory[at + 0x40..at + 0x42]
                .copy_from_slice(&((wide.len() * 2) as u16).to_le_bytes());
            directory[at + 0x42] = kind;
            directory[at + 0x74..at + 0x78].copy_from_slice(&start.to_le_bytes());
            directory[at + 0x78..at + 0x80].copy_from_slice(&size.to_le_bytes());
        };

        // The root entry owns the mini stream.
        write(
            0,
            "Root Entry",
            5,
            if mini { 3 } else { END_OF_CHAIN },
            if mini { body.len() as u64 } else { 0 },
        );
        write(
            DIRECTORY_ENTRY,
            name,
            KIND_STREAM,
            if mini { 0 } else { 3 },
            body.len() as u64,
        );

        let mut out = header;
        for one in fat {
            out.extend(one.to_le_bytes());
        }
        out.extend(directory);
        for one in mini_fat {
            out.extend(one.to_le_bytes());
        }
        out.extend(body);
        out.resize(out.len().div_ceil(SECTOR) * SECTOR, 0);
        out
    }

    #[test]
    fn a_small_stream_is_read_through_the_mini_allocation_table() {
        let body: Vec<u8> = (0..1000u32).map(|n| (n % 251) as u8).collect();
        let file = compound("DestList", &body);
        let mut source = std::io::Cursor::new(file);

        let document = Compound::open(&mut source).expect("opens");
        let read = document
            .stream(&mut source, "DestList")
            .expect("reads")
            .expect("is there");

        assert_eq!(read, body, "the mini stream was not reassembled");
    }

    /// The other half of the same branch. A jump list for a program with a lot
    /// of history has a `DestList` well past the cutoff, and it is read
    /// through the ordinary table instead.
    #[test]
    fn a_large_stream_is_read_through_the_ordinary_table() {
        let body: Vec<u8> = (0..9000u32).map(|n| (n % 251) as u8).collect();
        let file = compound("DestList", &body);
        let mut source = std::io::Cursor::new(file);

        let document = Compound::open(&mut source).expect("opens");
        let read = document
            .stream(&mut source, "DestList")
            .expect("reads")
            .expect("is there");

        assert_eq!(read.len(), body.len());
        assert_eq!(read, body);
    }

    #[test]
    fn a_stream_that_is_not_there_is_not_an_error() {
        let file = compound("DestList", b"anything");
        let mut source = std::io::Cursor::new(file);
        let document = Compound::open(&mut source).expect("opens");

        assert!(document
            .stream(&mut source, "SomethingElse")
            .expect("reads")
            .is_none());
    }

    #[test]
    fn something_that_is_not_a_compound_document_is_refused() {
        let mut source = std::io::Cursor::new(vec![0u8; 4096]);
        assert!(Compound::open(&mut source).is_err());

        let mut short = std::io::Cursor::new(b"tiny".to_vec());
        assert!(Compound::open(&mut short).is_err());
    }

    /// A FAT pointing at itself must not be walked forever.
    #[test]
    fn a_chain_that_loops_does_not_hang() {
        let mut file = compound("DestList", &[7u8; 9000]);
        // Sector 3 is the first of the stream; point it back at itself.
        let fat = 512;
        file[fat + 3 * 4..fat + 4 * 4].copy_from_slice(&3u32.to_le_bytes());

        let mut source = std::io::Cursor::new(file);
        let document = Compound::open(&mut source).expect("opens");
        let read = document.stream(&mut source, "DestList").expect("reads");

        assert!(read.is_some_and(|bytes| bytes.len() == 9000));
    }

    #[test]
    fn a_whole_jump_list_reads_end_to_end() {
        let stream = destlist(
            4,
            &[
                (r"C:\work\one.md", 10, false),
                (r"C:\work\two.md", 20, true),
            ],
        );
        let file = compound("DestList", &stream);
        let mut source = std::io::Cursor::new(file);

        let found = documents_in(&mut source, "abc123").expect("reads");

        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|one| one.source == "abc123"));
    }
}
