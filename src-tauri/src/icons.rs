//! Application icons, pulled out of the shell.
//!
//! Windows hands out icons as `HICON` handles, which are GDI objects rather
//! than image files, so getting a PNG out of one means asking GDI for the raw
//! device-independent bits and encoding them here.
//!
//! Results are cached, including failures: a shortcut that yields no icon
//! yields none every time, and re-asking the shell on every keystroke would be
//! a lot of COM work for a known answer.

use std::collections::HashMap;
use std::sync::Mutex;

/// How many icons are kept.
///
/// Was unbounded, which is a slow leak rather than a cache: every row ever
/// scrolled past left a base64 PNG behind for the life of the process, and a
/// machine with fourteen hundred indexed entries can eventually hold all of
/// them. A few hundred covers everything a person actually launches, several
/// times over, and re-extracting one that fell out is a millisecond.
const CAPACITY: usize = 512;

/// One cached answer, and when it was last wanted.
///
/// The stamp is what makes eviction pick something cold. Evicting by
/// insertion order instead would throw out the icon of the app you open every
/// day the moment you scrolled past five hundred you do not.
struct Entry {
    icon: Option<String>,
    used: u64,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<String, Entry>,
    /// Monotonic, so "least recently used" is just the smallest.
    clock: u64,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// A `data:` URI for the icon of a file, or `None` if it has none.
///
/// The lock is held across the extraction rather than only around the map.
/// Checking and inserting under separate locks lets two callers extract the
/// same icon at once, and concurrent GDI work can fail transiently, which the
/// cache would then make permanent. Serialising also avoids doing the same
/// work twice. Extraction is a millisecond of shell and GDI calls, and rows
/// ask for icons lazily, so there is nothing to gain from overlapping it.
pub fn icon_data_uri(path: &str) -> Option<String> {
    let mut guard = CACHE.lock().expect("icon cache poisoned");
    let cache = guard.get_or_insert_with(Cache::default);

    cache.clock += 1;
    let now = cache.clock;

    if let Some(entry) = cache.entries.get_mut(path) {
        entry.used = now;
        return entry.icon.clone();
    }

    let icon = extract(path);
    evict_if_full(cache);
    cache.entries.insert(
        path.to_string(),
        Entry {
            icon: icon.clone(),
            used: now,
        },
    );

    icon
}

/// Drops the coldest entry when there is no room for another.
///
/// A linear scan, which is the right trade at this size: it runs only when the
/// cache is full *and* something new is asked for, and a few hundred
/// comparisons is far less work than the GDI call that follows it.
fn evict_if_full(cache: &mut Cache) {
    if cache.entries.len() < CAPACITY {
        return;
    }

    let coldest = cache
        .entries
        .iter()
        .min_by_key(|(_, entry)| entry.used)
        .map(|(path, _)| path.clone());

    if let Some(path) = coldest {
        cache.entries.remove(&path);
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    fn stash(cache: &mut Cache, path: &str, at: u64) {
        evict_if_full(cache);
        cache.entries.insert(
            path.to_string(),
            Entry {
                icon: Some(path.to_string()),
                used: at,
            },
        );
    }

    #[test]
    fn the_cache_stops_growing_at_its_capacity() {
        // The whole point: an unbounded cache is a leak with a nice name.
        let mut cache = Cache::default();
        for i in 0..(CAPACITY * 2) {
            stash(&mut cache, &format!("app-{i}.exe"), i as u64);
        }
        assert_eq!(cache.entries.len(), CAPACITY);
    }

    #[test]
    fn a_recently_used_entry_outlives_an_old_one() {
        let mut cache = Cache::default();
        for i in 0..CAPACITY {
            stash(&mut cache, &format!("cold-{i}.exe"), i as u64);
        }

        // Touched now, so it is the newest thing in a full cache.
        let hot = "cold-0.exe";
        cache
            .entries
            .get_mut(hot)
            .expect("seeded")
            .used = u64::MAX;

        stash(&mut cache, "new.exe", 1);

        assert!(cache.entries.contains_key(hot), "the hot entry was evicted");
        assert!(!cache.entries.contains_key("cold-1.exe"), "the coldest survived");
    }
}

#[cfg(not(windows))]
fn extract(path: &str) -> Option<String> {
    image_file(path)
}

/// Makes sure COM is available on this thread.
///
/// `SHGetFileInfoW` is a shell call and needs an initialised apartment. Tauri's
/// main thread has one, but any other thread (a Tokio worker, a test thread)
/// does not, and the call then quietly returns no icon. Initialising is
/// idempotent: a thread already in an apartment gets `S_FALSE` or
/// `RPC_E_CHANGED_MODE`, both of which mean COM is usable and are ignored.
///
/// Deliberately never uninitialised. The thread may make more shell calls, and
/// tearing the apartment down under them is worse than leaving it up.
///
/// Declared by hand rather than through the `windows` crate's
/// `Win32_System_Com` feature: that feature is enormous, and enabling it on
/// top of the shell and GDI ones pushed rustc into an out-of-memory abort on
/// this machine. One extern declaration costs nothing to compile.
#[cfg(windows)]
fn ensure_com() {
    use std::cell::Cell;

    const COINIT_APARTMENTTHREADED: u32 = 0x2;

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut core::ffi::c_void, flags: u32) -> i32;
    }

    thread_local! {
        static DONE: Cell<bool> = const { Cell::new(false) };
    }

    DONE.with(|done| {
        if done.get() {
            return;
        }
        // SAFETY: a null reserved pointer is what the API expects, and the
        // HRESULT is deliberately ignored: every failure mode here means COM
        // is already usable on this thread.
        unsafe {
            CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        }
        done.set(true);
    });
}

/// Image files are returned as-is rather than going through GDI.
///
/// A packaged app's icon is a PNG in its install directory, not an icon
/// resource inside a PE file, so there is nothing to ask the shell for. The
/// bytes already are the picture.
fn image_file(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let mime = if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        return None;
    };

    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }

    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

#[cfg(windows)]
fn extract(path: &str) -> Option<String> {
    // Cheapest first: if the path is already a picture, no shell call is
    // needed at all.
    if let Some(found) = image_file(path) {
        return Some(found);
    }

    use windows::Win32::UI::WindowsAndMessaging::DestroyIcon;

    ensure_com();

    /*
     * Order matters, cheapest reliable source first.
     *
     * 1. A shortcut's own target. Most `.lnk` files report no icon location
     *    at all, because their icon simply comes from what they point at.
     * 2. The file's own icon resources, which covers a plain `.exe` or `.dll`.
     * 3. An explicit icon location, for shortcuts that override it.
     * 4. The shell's composited icon, but ONLY for things that are not
     *    shortcuts. That is the one source that never fails, and the shortcut
     *    badge it burns in is only ever added for a shortcut, so it is safe
     *    everywhere else.
     */
    let is_shortcut = {
        let lower = path.to_ascii_lowercase();
        lower.ends_with(".lnk") || lower.ends_with(".url")
    };

    let hicon = resolved_target(path)
        .and_then(|target| icon_of_file(&target))
        .or_else(|| (!is_shortcut).then(|| icon_of_file(path)).flatten())
        .or_else(|| icon_without_overlay(path))
        .or_else(|| {
            /*
             * Last resort, and it costs something.
             *
             * The shell's composited icon carries the shortcut badge, which is
             * why it is not reached first: when every icon came from here the
             * arrow appeared on all of them and looked wrong.
             *
             * It is still better than no icon. Reaching this point means the
             * target could not be read from LinkInfo and no icon location was
             * declared, which is true of roughly one row in thirty rather than
             * all of them.
             *
             * The real fix is decoding the shell ID list, where these
             * shortcuts record their target; `lnk::parse` reads only LinkInfo.
             */
            if is_shortcut && std::env::var_os("SILL_ICON_DEBUG").is_some() {
                eprintln!("[icons] falling back to the badged icon for {path}");
            }
            composited_icon(path)
        })?;

    let result = icon_to_png(hicon);

    // SAFETY: the handle came from SHDefExtractIconW or SHGetFileInfoW and
    // is not used again.
    unsafe {
        let _ = DestroyIcon(hicon);
    }

    if result.is_none() && std::env::var_os("SILL_ICON_DEBUG").is_some() {
        eprintln!("[icons] got an HICON but could not turn it into a PNG");
    }

    let png = result?;
    Some(format!(
        "data:image/png;base64,{}",
        base64_encode(&png)
    ))
}

/// The icon as the shell composites it, badges and all.
///
/// Only a fallback: for a `.lnk` this returns the icon with the shortcut arrow
/// burned into the bottom-left corner, which is noise in a launcher where
/// every single entry is a shortcut.
/// The executable a shortcut points at, if it is one.
#[cfg(windows)]
fn resolved_target(path: &str) -> Option<String> {
    if !path.to_ascii_lowercase().ends_with(".lnk") {
        return None;
    }

    let target = crate::lnk::target_of(std::path::Path::new(path))?;

    if std::env::var_os("SILL_ICON_DEBUG").is_some() {
        eprintln!("[icons] {path}
          -> target {target:?}");
    }

    std::path::Path::new(&target).is_file().then_some(target)
}

/// The pixel size every icon is extracted at.
///
/// Rows draw at 22 CSS pixels. `ExtractIconExW`'s "large" icon is whatever
/// `SM_CXICON` says, which is 32 at 100% scale, and 32 into 22 is a 0.69
/// downscale off an already small source: the result is soft at exactly the
/// size the launcher shows it. 64 is a size Windows icons genuinely contain,
/// it gives the resampler three times the pixels to work from, and it is
/// still large enough for a 200% display, where 22 CSS pixels is 44 real
/// ones.
#[cfg(windows)]
const ICON_PIXELS: u32 = 64;

/// The first icon resource in a file, with no shell involvement at all.
#[cfg(windows)]
fn icon_of_file(path: &str) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    icon_at_size(path, 0)
}

/// One icon out of `path` at [`ICON_PIXELS`], by resource index.
///
/// `SHDefExtractIconW` rather than `ExtractIconExW` for one reason: it takes
/// the size as an argument. `ExtractIconExW` only ever hands back the system
/// large and small sizes, so there is no way to ask it for more pixels.
#[cfg(windows)]
fn icon_at_size(path: &str, index: i32) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::SHDefExtractIconW;
    use windows::Win32::UI::WindowsAndMessaging::HICON;

    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut large = HICON::default();

    // SAFETY: `wide` is NUL-terminated and outlives the call; one icon is
    // requested into an owned handle, and the small slot is declined.
    let result = unsafe {
        SHDefExtractIconW(
            PCWSTR(wide.as_ptr()),
            index,
            0,
            Some(&mut large),
            None,
            ICON_PIXELS,
        )
    };

    (result.is_ok() && !large.is_invalid()).then_some(large)
}

#[cfg(windows)]
fn composited_icon(path: &str) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};

    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();

    // SAFETY: `wide` is NUL-terminated and outlives the call, and `info` is a
    // correctly sized owned struct.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            Default::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };

    if ok == 0 || info.hIcon.is_invalid() {
        if std::env::var_os("SILL_ICON_DEBUG").is_some() {
            eprintln!("[icons] SHGetFileInfoW ok={ok}");
        }
        return None;
    }

    Some(info.hIcon)
}

/// The icon straight from the file that defines it, with no shell overlay.
///
/// `SHGFI_ICONLOCATION` asks where an item's icon actually lives rather than
/// for the finished bitmap, so it reports an executable and an index. Pulling
/// the icon from there skips the shell's compositing step, and with it the
/// shortcut arrow.
#[cfg(windows)]
fn icon_without_overlay(path: &str) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICONLOCATION};

    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();

    // SAFETY: as above; this variant fills szDisplayName and iIcon rather
    // than creating an icon handle.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            Default::default(),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICONLOCATION,
        )
    };

    if ok == 0 {
        return None;
    }

    // szDisplayName holds the icon's source path, NUL-terminated.
    let end = info
        .szDisplayName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(info.szDisplayName.len());
    if end == 0 {
        // Plenty of items have no separate icon source; the caller falls back.
        return None;
    }

    // The shell reports these unexpanded, e.g. "%windir%\system32\imageres.dll".
    // The extractor does no expansion of its own, so without this the call
    // fails and the caller falls back to the composited icon, which is exactly
    // the badged version being avoided.
    let located = expand_env(&String::from_utf16_lossy(&info.szDisplayName[..end]));

    if std::env::var_os("SILL_ICON_DEBUG").is_some() {
        eprintln!("[icons] {path}
          -> {located:?} index {}", info.iIcon);
    }

    icon_at_size(&located, info.iIcon)
}

/// Replaces `%NAME%` spans with their environment values.
///
/// Written by hand rather than calling `ExpandEnvironmentStringsW`, which
/// would mean another `windows` crate feature, and feature accumulation in
/// that crate has already aborted this build once.
#[cfg(windows)]
fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];

        let Some(end) = after.find('%') else {
            // An unpaired % is literal, not the start of a variable.
            out.push_str(&rest[start..]);
            return out;
        };

        let name = &after[..end];
        match std::env::var(name) {
            Ok(value) => out.push_str(&value),
            // An unknown variable is left as written, which at least keeps
            // the failure legible instead of silently deleting the segment.
            Err(_) => {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
        }

        rest = &after[end + 1..];
    }

    out.push_str(rest);
    out
}

#[cfg(windows)]
fn icon_to_png(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut icon_info = ICONINFO::default();

    // SAFETY: `icon_info` is owned and correctly sized; the handle is valid.
    unsafe { GetIconInfo(hicon, &mut icon_info) }.ok()?;

    // GetIconInfo hands over two bitmaps that the caller now owns.
    let colour = icon_info.hbmColor;
    let mask = icon_info.hbmMask;

    let cleanup = || {
        // SAFETY: both handles came from GetIconInfo and are used nowhere else.
        unsafe {
            if !colour.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(colour.0));
            }
            if !mask.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(mask.0));
            }
        }
    };

    if colour.is_invalid() {
        cleanup();
        return None;
    }

    let mut bitmap = BITMAP::default();
    // SAFETY: `bitmap` is owned and the size matches the type being requested.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(colour.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut core::ffi::c_void),
        )
    };

    if read == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        if std::env::var_os("SILL_ICON_DEBUG").is_some() {
            eprintln!("[icons] GetObjectW read={read} w={} h={}", bitmap.bmWidth, bitmap.bmHeight);
        }
        cleanup();
        return None;
    }

    let width = bitmap.bmWidth;
    let height = bitmap.bmHeight;

    let mut header = BITMAPINFO::default();
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        // Negative height asks GDI for top-down rows, which is the order PNG
        // wants. Bottom-up would need the buffer reversed afterwards.
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    // SAFETY: the screen DC is released below, and the buffer is sized from
    // the same width and height handed to GDI in the header.
    let copied = unsafe {
        let dc = GetDC(None);
        let copied = GetDIBits(
            dc,
            colour,
            0,
            height as u32,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        copied
    };

    cleanup();

    if copied == 0 {
        if std::env::var_os("SILL_ICON_DEBUG").is_some() {
            eprintln!("[icons] GetDIBits copied no scanlines");
        }
        return None;
    }

    // GDI gives BGRA; PNG wants RGBA.
    let mut opaque = true;
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
        if chunk[3] != 0 {
            opaque = false;
        }
    }

    // Some older icons carry no alpha channel at all, leaving it entirely
    // zero, which would encode as a fully transparent image. Treating those as
    // opaque is closer to right than rendering nothing.
    if opaque {
        for chunk in pixels.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
    }

    encode_png(&pixels, width as u32, height as u32)
}

#[cfg(windows)]
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
