/** Header of a preview frame; must match `editor::protocol` in Rust. */
export const HEADER_LEN = 32;
const MAGIC = 0x31464352; // "RCF1" read as a little-endian u32

export type FrameHeader = {
  width: number;
  height: number;
  playing: boolean;
  seq: number;
  tMs: number;
};

export type Frame = {
  header: FrameHeader;
  y: Uint8Array<ArrayBuffer>;
  uv: Uint8Array<ArrayBuffer>;
};

export function parseFrame(buffer: ArrayBuffer): Frame | null {
  if (buffer.byteLength < HEADER_LEN) return null;
  const view = new DataView(buffer);
  if (view.getUint32(0, true) !== MAGIC) return null;
  const width = view.getUint32(4, true);
  const height = view.getUint32(8, true);
  const lumaLen = width * height;
  if (buffer.byteLength !== HEADER_LEN + lumaLen + lumaLen / 2) return null;
  return {
    header: {
      width,
      height,
      playing: (view.getUint32(12, true) & 1) === 1,
      seq: Number(view.getBigUint64(16, true)),
      tMs: view.getFloat64(24, true),
    },
    y: new Uint8Array(buffer, HEADER_LEN, lumaLen),
    uv: new Uint8Array(buffer, HEADER_LEN + lumaLen, lumaLen / 2),
  };
}
