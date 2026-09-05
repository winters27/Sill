//! Turning one picture into another kind of picture.
//!
//! A file arrives as a `.webp` and something will only take a `.png`, or a
//! screenshot is a `.png` and wants to be a smaller `.jpg`. Both are a right
//! click and a program somebody had to install, and both are one row here.
//!
//! ## Why Windows' own codecs rather than a crate
//!
//! The imaging component this uses is the same one Explorer's thumbnails and
//! the Photos application go through, so what Sill can read is exactly what
//! the rest of the machine can read, and it grows when somebody installs a
//! codec rather than when Sill is rebuilt. It also costs no dependency:
//! `Graphics_Imaging` was already switched on for reading text out of
//! pictures.
//!
//! **HEIC needs two Store packages, not one.** The HEIF Image Extensions
//! carry the container and the HEVC Video Extensions carry the pixels inside
//! it, because a `.heic` is HEVC frames in a HEIF box. With only the first,
//! decoding fails in a way that looks like a broken file, so the refusal
//! below names both rather than repeating what Windows said.
//!
//! **Writing is PNG and JPEG only.** Windows ships no WebP *encoder*, only a
//! decoder, so offering "convert to WebP" would be a row that always failed.
//! Reading WebP works, which is the direction people actually want.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. Every function here runs from an action somebody chose.

use std::path::{Path, PathBuf};

/// What a picture can be turned into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
}

impl Format {
    /// The extension the new file gets, without its dot.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
        }
    }

    /// What the action is called, and what the row says.
    pub fn title(self) -> &'static str {
        match self {
            Format::Png => "Convert to PNG",
            Format::Jpeg => "Convert to JPEG",
        }
    }
}

/// The extensions worth offering a conversion on.
///
/// What Windows reads out of the box, plus the two that need a free Store
/// package. Deliberately a list rather than "ask the decoder": the question
/// is asked once per row drawn in the action panel, and standing up an
/// imaging pipeline to find out whether a file is a picture would cost more
/// than the conversion.
const READABLE: &[&str] = &[
    "png", "jpg", "jpeg", "jfif", "bmp", "gif", "tif", "tiff", "webp", "heic", "heif", "dib",
    "ico", "jxr", "wdp",
];

/// Whether this file is one a conversion can be offered on.
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .is_some_and(|extension| READABLE.contains(&extension.as_str()))
}

/// Where the converted file wants to go, before a collision is considered.
///
/// The stem is kept and only the extension changes, so a folder sorted by
/// name keeps the new file next to the old one. Pure, so the naming can be
/// checked without a codec.
pub fn output_name(path: &Path, to: Format) -> PathBuf {
    path.with_extension(to.extension())
}

/// Whether converting this file to this format would only rename it.
///
/// `.jpeg` to JPEG is the same picture with a different spelling, and a row
/// that re-encodes it would quietly cost a generation of quality for nothing.
pub fn already(path: &Path, to: Format) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default();

    match to {
        Format::Png => extension == "png",
        Format::Jpeg => matches!(extension.as_str(), "jpg" | "jpeg" | "jfif"),
    }
}

/// Converts one picture, and answers with the file it wrote.
///
/// The new file sits beside the old one under a free name, and the old one is
/// untouched: a conversion that replaced its input would be a destructive
/// action wearing a helpful name, and the undo for this is deleting what it
/// made rather than putting back what it removed.
#[cfg(windows)]
pub fn convert(path: &Path, to: Format) -> Result<PathBuf, String> {
    use windows::Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, SoftwareBitmap,
    };

    if !is_image(path) {
        return Err(format!(
            "{} is not a picture Sill knows how to read",
            crate::files_ops::name_of(path)
        ));
    }

    let read = std::fs::read(path).map_err(|err| format!("could not read that picture: {err}"))?;

    let source = stream_of(&read).map_err(|err| format!("could not read that picture: {err}"))?;

    let decoder = BitmapDecoder::CreateAsync(&source)
        .and_then(|pending| pending.join())
        .map_err(|_| unreadable(path))?;

    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .and_then(|pending| pending.join())
        .map_err(|_| unreadable(path))?;

    // JPEG holds no transparency, so the alpha is dropped deliberately here
    // rather than left for the encoder to refuse. PNG keeps it.
    let alpha = match to {
        Format::Png => BitmapAlphaMode::Premultiplied,
        Format::Jpeg => BitmapAlphaMode::Ignore,
    };

    let ready = SoftwareBitmap::ConvertWithAlpha(&bitmap, BitmapPixelFormat::Bgra8, alpha)
        .map_err(|err| format!("could not prepare that picture: {}", err.message()))?;

    let written =
        encode(&ready, to).map_err(|why| format!("could not write that picture: {why}"))?;

    // A free name, so converting the same file twice keeps both rather than
    // one silently replacing the other.
    let landed = crate::files_ops::free_name(&output_name(path, to))
        .ok_or_else(|| "there is nowhere free to write that picture".to_string())?;

    std::fs::write(&landed, written)
        .map_err(|err| format!("could not write {}: {err}", crate::files_ops::name_of(&landed)))?;

    Ok(landed)
}

#[cfg(not(windows))]
pub fn convert(_path: &Path, _to: Format) -> Result<PathBuf, String> {
    Err("only on Windows".to_string())
}

/// The bytes of a picture, encoded.
#[cfg(windows)]
fn encode(
    bitmap: &windows::Graphics::Imaging::SoftwareBitmap,
    to: Format,
) -> Result<Vec<u8>, String> {
    use windows::Graphics::Imaging::BitmapEncoder;
    use windows::Storage::Streams::InMemoryRandomAccessStream;

    let said = |err: windows::core::Error| err.message();

    let id = match to {
        Format::Png => BitmapEncoder::PngEncoderId(),
        Format::Jpeg => BitmapEncoder::JpegEncoderId(),
    }
    .map_err(said)?;

    let out = InMemoryRandomAccessStream::new().map_err(said)?;
    let encoder = BitmapEncoder::CreateAsync(id, &out)
        .and_then(|pending| pending.join())
        .map_err(said)?;

    encoder.SetSoftwareBitmap(bitmap).map_err(said)?;
    encoder
        .FlushAsync()
        .and_then(|pending| pending.join())
        .map_err(said)?;

    bytes_of(&out).map_err(said)
}

/// A stream holding these bytes, positioned at the start.
#[cfg(windows)]
fn stream_of(
    bytes: &[u8],
) -> windows::core::Result<windows::Storage::Streams::InMemoryRandomAccessStream> {
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream.GetOutputStreamAt(0)?)?;

    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.FlushAsync()?.join()?;
    // Detached, or dropping the writer closes the stream underneath the
    // decoder that is about to read it.
    writer.DetachStream()?;

    stream.Seek(0)?;
    Ok(stream)
}

/// Everything a stream holds, from the start.
#[cfg(windows)]
fn bytes_of(
    stream: &windows::Storage::Streams::InMemoryRandomAccessStream,
) -> windows::core::Result<Vec<u8>> {
    use windows::Storage::Streams::DataReader;

    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;

    reader.LoadAsync(size)?.join()?;
    let mut out = vec![0u8; size as usize];
    reader.ReadBytes(&mut out)?;

    Ok(out)
}

/// What to say when Windows would not read a picture.
///
/// The two Store packages are named for HEIC because that is the one case
/// where the failure is a missing codec rather than a broken file, and
/// Windows' own message says neither.
#[cfg(windows)]
fn unreadable(path: &Path) -> String {
    let name = crate::files_ops::name_of(path);
    let heic = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("heic") || extension.eq_ignore_ascii_case("heif")
        });

    if heic {
        format!(
            "Windows could not read {name}. A HEIC file needs both the HEIF Image \
             Extensions and the HEVC Video Extensions, which are free in the Store."
        )
    } else {
        format!("Windows could not read {name} as a picture")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_name_keeps_the_stem_and_changes_the_extension() {
        let from = Path::new(r"C:\Users\me\Pictures\holiday.webp");

        assert_eq!(
            output_name(from, Format::Png),
            Path::new(r"C:\Users\me\Pictures\holiday.png")
        );
        assert_eq!(
            output_name(from, Format::Jpeg),
            Path::new(r"C:\Users\me\Pictures\holiday.jpg")
        );

        // A name with dots in it keeps all but the last.
        assert_eq!(
            output_name(Path::new(r"C:\a\shot.2026-09-05.png"), Format::Jpeg),
            Path::new(r"C:\a\shot.2026-09-05.jpg")
        );
    }

    #[test]
    fn a_picture_is_recognised_by_its_extension_whatever_its_case() {
        for yes in ["a.png", "a.JPG", "a.Jpeg", "a.webp", "a.HEIC", "a.tif"] {
            assert!(is_image(Path::new(yes)), "{yes} is a picture");
        }

        for no in ["a.txt", "a.pdf", "a.mp4", "a.svg", "a", "a.pngx"] {
            assert!(!is_image(Path::new(no)), "{no} is not a picture Sill reads");
        }
    }

    /// Re-encoding a JPEG as a JPEG costs a generation of quality and gains
    /// nothing, so the row is not offered.
    #[test]
    fn converting_a_picture_to_what_it_already_is_is_not_offered() {
        assert!(already(Path::new("a.png"), Format::Png));
        assert!(already(Path::new("a.PNG"), Format::Png));
        assert!(already(Path::new("a.jpg"), Format::Jpeg));
        assert!(already(Path::new("a.jpeg"), Format::Jpeg));
        assert!(already(Path::new("a.jfif"), Format::Jpeg));

        assert!(!already(Path::new("a.png"), Format::Jpeg));
        assert!(!already(Path::new("a.webp"), Format::Png));
        assert!(!already(Path::new("a"), Format::Png));
    }

    #[test]
    fn each_format_names_itself_once() {
        assert_eq!(Format::Png.extension(), "png");
        assert_eq!(Format::Jpeg.extension(), "jpg");
        assert_ne!(Format::Png.title(), Format::Jpeg.title());
    }
}
