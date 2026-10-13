import type { Rect } from "@/markup/model";

/** Header of a preview frame; must match `editor::protocol` in Rust. */
export const HEADER_LEN = 72;
const MAGIC = 0x32464352; // "RCF2" read as a little-endian u32
const PLAYING = 1;
const COMPLETE = 2;

export type FrameHeader = {
  width: number;
  height: number;
  playing: boolean;
  seq: number;
  tMs: number;
  /** The `seq` of the newest edits the frame shows. */
  edits: number;
  /** Whether every annotation shown had its image. */
  complete: boolean;
  /** Where the recording is in the frame, in frame pixels. */
  content: Rect;
  /** The part of the recording shown, 0..1; zoom shows less than all of it. */
  view: Rect;
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
  const flags = view.getUint32(12, true);
  const rect = (at: number): Rect => ({
    x: view.getFloat32(at, true),
    y: view.getFloat32(at + 4, true),
    width: view.getFloat32(at + 8, true),
    height: view.getFloat32(at + 12, true),
  });
  return {
    header: {
      width,
      height,
      playing: (flags & PLAYING) !== 0,
      seq: Number(view.getBigUint64(16, true)),
      tMs: view.getFloat64(24, true),
      edits: view.getUint32(32, true),
      complete: (flags & COMPLETE) !== 0,
      content: rect(40),
      view: rect(56),
    },
    y: new Uint8Array(buffer, HEADER_LEN, lumaLen),
    uv: new Uint8Array(buffer, HEADER_LEN + lumaLen, lumaLen / 2),
  };
}
