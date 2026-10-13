import { describe, expect, it } from "vitest";

import { frameRegion } from "@/editor/annotations/overlay";
import { contentBox, frameMap } from "@/editor/annotations/view";
import type { Frame, FrameHeader } from "@/editor/frame";
import { toImage, toView } from "@/markup/view";

const header = (view = { x: 0, y: 0, width: 1, height: 1 }): FrameHeader => ({
  width: 1000,
  height: 600,
  playing: false,
  seq: 1,
  tMs: 0,
  edits: 0,
  complete: true,
  content: { x: 100, y: 50, width: 800, height: 500 },
  view,
});

describe("frameMap", () => {
  const screen = { width: 1600, height: 1000 };

  it("places the recording's points in the frame box", () => {
    const map = frameMap(header(), screen, { width: 500, height: 300 });
    expect(toView(map, { x: 0, y: 0 })).toEqual({ x: 50, y: 25 });
    expect(toView(map, { x: 1600, y: 1000 })).toEqual({ x: 450, y: 275 });
    expect(map.zoom).toBe(0.25);
    expect(toImage(map, { x: 250, y: 150 })).toEqual({ x: 800, y: 500 });
    expect(contentBox(header(), { width: 500, height: 300 })).toEqual({
      x: 50,
      y: 25,
      width: 400,
      height: 250,
    });
  });

  it("follows the zoom", () => {
    const zoomed = header({ x: 0.25, y: 0.25, width: 0.5, height: 0.5 });
    const map = frameMap(zoomed, screen, { width: 1000, height: 600 });
    expect(map.zoom).toBe(1);
    expect(toView(map, { x: 400, y: 250 })).toEqual({ x: 100, y: 50 });
    expect(toView(map, { x: 1200, y: 750 })).toEqual({ x: 900, y: 550 });
  });
});

describe("frameRegion", () => {
  it("converts NV12 like the preview renderer", () => {
    const width = 4;
    const height = 2;
    const frame: Frame = {
      header: { ...header(), width, height },
      y: new Uint8Array([0, 255, 128, 128, 0, 255, 128, 128]),
      uv: new Uint8Array([128, 128, 128, 255]),
    };
    const { data } = frameRegion(frame, { x: 1, y: 0, width: 3, height: 1 });
    expect([...data.slice(0, 4)]).toEqual([255, 255, 255, 255]);
    // Luma 128 with red chroma: 128 + 1.5748 × 127 clips, 128 − 0.4681 × 127 ≈ 69.
    expect([...data.slice(4, 8)]).toEqual([255, 69, 128, 255]);
  });
});
