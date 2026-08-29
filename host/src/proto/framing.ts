/**
 * Wire framing for the stdio channel.
 *
 * Each frame is a 4-byte big-endian uint32 payload length followed by that many
 * bytes of UTF-8 JSON. The length does not include the 4-byte header.
 *
 * The Rust side reads these with tokio_util's LengthDelimitedCodec configured
 * for a 4-byte big-endian length field and zero length adjustment. Any change
 * here has to be mirrored there.
 */

const HEADER_BYTES = 4;

/** Guards against a desynchronised stream trying to allocate absurd buffers. */
const MAX_FRAME_BYTES = 64 * 1024 * 1024;

export function encodeFrame(payload: string): Buffer {
  const body = Buffer.from(payload, "utf8");
  const frame = Buffer.allocUnsafe(body.length + HEADER_BYTES);
  frame.writeUInt32BE(body.length, 0);
  body.copy(frame, HEADER_BYTES);
  return frame;
}

/**
 * Accumulates stdin chunks and yields whole frames. A single read can contain
 * a partial frame, several frames, or both, so nothing can assume alignment.
 */
export class FrameDecoder {
  private buffer: Buffer = Buffer.alloc(0);

  push(chunk: Buffer): string[] {
    this.buffer = this.buffer.length === 0 ? chunk : Buffer.concat([this.buffer, chunk]);

    const frames: string[] = [];

    while (this.buffer.length >= HEADER_BYTES) {
      const length = this.buffer.readUInt32BE(0);

      if (length > MAX_FRAME_BYTES) {
        throw new Error(
          `frame length ${length} exceeds maximum ${MAX_FRAME_BYTES}; stream is out of sync`,
        );
      }

      if (this.buffer.length - HEADER_BYTES < length) break;

      frames.push(this.buffer.subarray(HEADER_BYTES, HEADER_BYTES + length).toString("utf8"));
      this.buffer = this.buffer.subarray(HEADER_BYTES + length);
    }

    return frames;
  }
}
