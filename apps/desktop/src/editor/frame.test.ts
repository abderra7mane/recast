import { describe, expect, it } from "vitest";

import { HEADER_LEN, parseFrame } from "@/editor/frame";

function message(
  width: number,
  height: number,
  playing: boolean,
  seq: bigint,
  tMs: number,
  complete = false,
) {
  const luma = width * height;
  const buffer = new ArrayBuffer(HEADER_LEN + luma + luma / 2);
  const view = new DataView(buffer);
  new Uint8Array(buffer, 0, 4).set([0x52, 0x43, 0x46, 0x32]);
  view.setUint32(4, width, true);
  view.setUint32(8, height, true);
  view.setUint32(12, (playing ? 1 : 0) | (complete ? 2 : 0), true);
  view.setBigUint64(16, seq, true);
  view.setFloat64(24, tMs, true);
  view.setUint32(32, 9, true);
  [1, 2, 3, 4, 0.25, 0.5, 0.5, 0.25].forEach((v, i) =>
    view.setFloat32(40 + i * 4, v, true),
  );
  new Uint8Array(buffer, HEADER_LEN, luma).fill(200);
  new Uint8Array(buffer, HEADER_LEN + luma).fill(90);
  return buffer;
}

describe("parseFrame", () => {
  it("reads the header and planes", () => {
    const frame = parseFrame(message(4, 2, true, 7n, 1234.5, true))!;
    expect(frame.header).toEqual({
      width: 4,
      height: 2,
      playing: true,
      seq: 7,
      tMs: 1234.5,
      edits: 9,
      complete: true,
      content: { x: 1, y: 2, width: 3, height: 4 },
      view: { x: 0.25, y: 0.5, width: 0.5, height: 0.25 },
    });
    expect(frame.y).toHaveLength(8);
    expect(frame.uv).toHaveLength(4);
    expect([...frame.y]).toEqual(Array(8).fill(200));
    expect([...frame.uv]).toEqual(Array(4).fill(90));
    const paused = parseFrame(message(2, 2, false, 1n, 0))!.header;
    expect(paused.playing).toBe(false);
    expect(paused.complete).toBe(false);
  });

  it("rejects bad messages", () => {
    expect(parseFrame(new ArrayBuffer(10))).toBeNull();
    const wrongMagic = message(2, 2, false, 1n, 0);
    new Uint8Array(wrongMagic)[0] = 0;
    expect(parseFrame(wrongMagic)).toBeNull();
    expect(parseFrame(message(2, 2, false, 1n, 0).slice(0, 76))).toBeNull();
  });
});
