//! Just enough of the Windows shortcut format to find what a `.lnk` points at.
//!
//! Resolving a shortcut normally means `IShellLink`, which lives behind the
//! `windows` crate's COM feature; enabling that feature on top of the shell
//! and GDI ones aborts rustc with an out-of-memory on this machine. The format
//! is documented (MS-SHLLINK) and the part needed here is small, so it is read
//! directly instead.
//!
//! Only the target path is extracted. Arguments, working directory and the
//! rest are irrelevant: launching still goes through the shortcut itself, and
//! the target is wanted purely to pull an un-badged icon out of the real
//! executable.

/// Offset of `LinkFlags` in the fixed-size header.
const LINK_FLAGS_OFFSET: usize = 0x14;
/// The header is a fixed 0x4C bytes; everything else follows it.
const HEADER_SIZE: usize = 0x4C;

const HAS_LINK_TARGET_ID_LIST: u32 = 1 << 0;
const HAS_LINK_INFO: u32 = 1 << 1;

/// `LinkInfo` carries the Unicode path fields only when its header is at
/// least this large.
const LINK_INFO_HEADER_WITH_UNICODE: u32 = 0x24;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Reads a NUL-terminated single-byte string.
fn ansi_at(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    Some(String::from_utf8_lossy(&rest[..end]).into_owned())
}

/// Reads a NUL-terminated UTF-16 string.
fn wide_at(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let units: Vec<u16> = rest
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();

    (!units.is_empty()).then(|| String::from_utf16_lossy(&units))
}

/// The file a shortcut points at, if it records one.
///
/// Returns `None` for shortcuts with no local target: those pointing at a
/// packaged app, a virtual shell folder, or a network location that is
/// described only by its ID list.
pub fn target_of(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    parse(&bytes)
}

/// Split out from the file read so it can be tested on bytes directly.
pub fn parse(bytes: &[u8]) -> Option<String> {
    if bytes.len() < HEADER_SIZE || u32_at(bytes, 0)? != HEADER_SIZE as u32 {
        return None;
    }

    let flags = u32_at(bytes, LINK_FLAGS_OFFSET)?;
    let mut cursor = HEADER_SIZE;

    // The target ID list is a shell-internal structure. Its contents are not
    // needed, only its length, so it can be stepped over.
    if flags & HAS_LINK_TARGET_ID_LIST != 0 {
        let size = u16_at(bytes, cursor)? as usize;
        cursor = cursor.checked_add(2)?.checked_add(size)?;
    }

    if flags & HAS_LINK_INFO == 0 {
        return None;
    }

    let info = cursor;
    let info_header_size = u32_at(bytes, info + 0x04)?;

    // Unicode fields when present, since the ANSI ones lose anything outside
    // the system code page.
    if info_header_size >= LINK_INFO_HEADER_WITH_UNICODE {
        if let Some(offset) = u32_at(bytes, info + 0x1C) {
            if offset != 0 {
                if let Some(found) = wide_at(bytes, info + offset as usize) {
                    return Some(found);
                }
            }
        }
    }

    let offset = u32_at(bytes, info + 0x10)?;
    if offset == 0 {
        return None;
    }

    let found = ansi_at(bytes, info + offset as usize)?;
    (!found.is_empty()).then_some(found)
}
