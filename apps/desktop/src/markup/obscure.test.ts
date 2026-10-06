import { describe, expect, it } from "vitest";

import { checkerboard, variance } from "@/markup/fixtures.test-util";
import { blockSize, obscure, pixelRect } from "@/markup/obscure";

describe("obscure", () => {
  it("scales its blocks with the screenshot", () => {
    expect(blockSize(1)).toBe(12);
    expect(blockSize(2)).toBe(24);
    expect(blockSize(0.1)).toBe(4);
  });

  it("clips rectangles to whole pixels inside the image", () => {
    expect(
      pixelRect(
        { x: -5.5, y: 10.2, width: 20, height: 400 },
        {
          width: 100,
          height: 50,
        },
      ),
    ).toEqual({ x: 0, y: 10, width: 15, height: 40 });
    expect(
      pixelRect(
        { x: 200, y: 0, width: 20, height: 10 },
        {
          width: 100,
          height: 50,
        },
      ).width,
    ).toBe(0);
  });

  for (const kind of ["blur", "pixelate"] as const) {
    it(`${kind} flattens a one-pixel checkerboard`, () => {
      const source = checkerboard(300, 200);
      expect(variance(source.data)).toBeGreaterThan(16_000);
      const out = obscure(
        source,
        { x: 13, y: 7, width: 240, height: 144 },
        kind,
        24,
      );
      expect(out.length).toBe(240 * 144 * 4);
      expect(variance(out)).toBeLessThan(1);
    });

    it(`${kind} keeps nothing but block averages`, () => {
      const a = checkerboard(96, 48);
      const b = checkerboard(96, 48);
      // Swap two pixels inside one block: the block's average stays the same.
      b.data.set([255, 255, 255, 255], 0);
      b.data.set([0, 0, 0, 255], 4);
      const rect = { x: 0, y: 0, width: 96, height: 48 };
      expect(obscure(b, rect, kind, 24)).toEqual(obscure(a, rect, kind, 24));
    });
  }

  it("pixelates into flat blocks of the average color", () => {
    const data = new Uint8ClampedArray(4 * 2 * 4);
    // Left block red and blue, right block all green.
    data.set([255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 0, 255, 0, 255]);
    data.set(
      [255, 0, 0, 255, 0, 0, 255, 255, 0, 255, 0, 255, 0, 255, 0, 255],
      16,
    );
    const out = obscure(
      { width: 4, height: 2, data },
      { x: 0, y: 0, width: 4, height: 2 },
      "pixelate",
      2,
    );
    expect([...out.slice(0, 4)]).toEqual([128, 0, 128, 255]);
    expect([...out.slice(8, 12)]).toEqual([0, 255, 0, 255]);
  });

  it("weights colors by their opacity", () => {
    const data = new Uint8ClampedArray([255, 0, 0, 255, 0, 0, 255, 0]);
    const out = obscure(
      { width: 2, height: 1, data },
      { x: 0, y: 0, width: 2, height: 1 },
      "pixelate",
      2,
    );
    expect([...out.slice(0, 4)]).toEqual([255, 0, 0, 128]);
  });
});
