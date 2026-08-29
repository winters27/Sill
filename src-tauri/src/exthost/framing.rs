//! Frame codec for the extension host channel.
//!
//! Each frame is a 4-byte big-endian length followed by that many bytes of
//! UTF-8 JSON. The length excludes its own header. This is the Rust half of
//! `host/src/proto/framing.ts` and the two must not drift.

use tokio_util::codec::LengthDelimitedCodec;

/// Ceiling for a single frame.
///
/// The default 8 MB is enough for ordinary traffic, but a first render of a
/// very large list is one frame, and truncating it would look like a silent
/// protocol fault rather than a size limit. 64 MB is far above anything a
/// sane extension produces while still bounding a runaway.
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Builds the codec used for both directions of the host channel.
pub fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .length_field_length(4)
        .big_endian()
        // The header is not counted in the advertised length, which is what
        // the TypeScript writer does, so no adjustment is applied.
        .length_adjustment(0)
        .max_frame_length(MAX_FRAME_BYTES)
        .new_codec()
}
