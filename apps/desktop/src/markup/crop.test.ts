import { describe, expect, it } from "vitest";

import {
  cropArea,
  cropPress,
  drawCrop,
  isFull,
  MIN_CROP,
  moveCrop,
  resizeCrop,
  snapCrop,
} from "@/markup/crop";

const image = { width: 400, height: 300 };

describe("crop", () => {
  it("is the whole image until set", () => {
    expect(cropArea(null, image)).toEqual({
      x: 0,
      y: 0,
      width: 400,
      height: 300,
    });
    expect(isFull(cropArea(null, image), image)).toBe(true);
    expect(isFull({ x: 1, y: 0, width: 399, height: 300 }, image)).toBe(false);
  });

  it("snaps to whole pixels inside the image", () => {
    expect(
      snapCrop({ x: 10.4, y: 9.6, width: 50.2, height: 20 }, image),
    ).toEqual({ x: 10, y: 10, width: 51, height: 20 });
    expect(
      snapCrop({ x: -20, y: 250, width: 500, height: 100 }, image),
    ).toEqual({ x: 0, y: 250, width: 400, height: 50 });
    expect(snapCrop({ x: 398, y: 0, width: 1, height: 1 }, image)).toEqual({
      x: 392,
      y: 0,
      width: MIN_CROP,
      height: MIN_CROP,
    });
  });

  it("resizes from a handle without flipping or leaving the image", () => {
    const start = { x: 100, y: 100, width: 200, height: 100 };
    expect(resizeCrop(start, "se", { x: 350.6, y: 260 }, image, false)).toEqual(
      { x: 100, y: 100, width: 251, height: 160 },
    );
    expect(resizeCrop(start, "se", { x: 900, y: 900 }, image, false)).toEqual({
      x: 100,
      y: 100,
      width: 300,
      height: 200,
    });
    expect(resizeCrop(start, "nw", { x: 500, y: 500 }, image, false)).toEqual({
      x: 300 - MIN_CROP,
      y: 200 - MIN_CROP,
      width: MIN_CROP,
      height: MIN_CROP,
    });
    expect(resizeCrop(start, "e", { x: 150, y: 0 }, image, false)).toEqual({
      x: 100,
      y: 100,
      width: 50,
      height: 100,
    });
    expect(resizeCrop(start, "n", { x: 0, y: -50 }, image, false)).toEqual({
      x: 100,
      y: 0,
      width: 200,
      height: 200,
    });
  });

  it("keeps the aspect ratio with Shift, within the image", () => {
    const start = { x: 100, y: 100, width: 200, height: 100 };
    expect(resizeCrop(start, "se", { x: 400, y: 120 }, image, true)).toEqual({
      x: 100,
      y: 100,
      width: 300,
      height: 150,
    });
    expect(resizeCrop(start, "se", { x: 400, y: 300 }, image, true)).toEqual({
      x: 100,
      y: 100,
      width: 300,
      height: 150,
    });
    expect(resizeCrop(start, "nw", { x: 0, y: 90 }, image, true)).toEqual({
      x: 0,
      y: 50,
      width: 300,
      height: 150,
    });
  });

  it("moves inside the image, along one axis with Shift", () => {
    const start = { x: 100, y: 100, width: 200, height: 100 };
    expect(moveCrop(start, { x: 30.4, y: -20 }, image, false)).toEqual({
      ...start,
      x: 130,
      y: 80,
    });
    expect(moveCrop(start, { x: 500, y: 500 }, image, false)).toEqual({
      ...start,
      x: 200,
      y: 200,
    });
    expect(moveCrop(start, { x: 30, y: -20 }, image, true)).toEqual({
      ...start,
      x: 130,
    });
  });

  it("draws a new crop once it is big enough", () => {
    expect(drawCrop({ x: 10, y: 10 }, { x: 14, y: 100 }, image)).toBe(null);
    expect(drawCrop({ x: 300, y: 200 }, { x: 500, y: 100 }, image)).toEqual({
      x: 300,
      y: 100,
      width: 100,
      height: 100,
    });
  });

  it("starts a new crop from inside an uncropped image", () => {
    expect(cropPress(null, image, { x: 200, y: 150 }, 4)).toEqual({
      kind: "new",
    });
    expect(
      cropPress(
        { x: 0, y: 0, width: 400, height: 300 },
        image,
        { x: 200, y: 150 },
        4,
      ),
    ).toEqual({ kind: "new" });
    expect(cropPress(null, image, { x: 399, y: 299 }, 4)).toMatchObject({
      kind: "resize",
      handle: "se",
    });
  });

  it("moves or resizes an existing crop, and starts a new one outside it", () => {
    const crop = { x: 100, y: 100, width: 200, height: 100 };
    expect(cropPress(crop, image, { x: 150, y: 150 }, 4)).toEqual({
      kind: "move",
      start: crop,
    });
    expect(cropPress(crop, image, { x: 102, y: 98 }, 4)).toEqual({
      kind: "resize",
      start: crop,
      handle: "nw",
    });
    expect(cropPress(crop, image, { x: 20, y: 20 }, 4)).toEqual({
      kind: "new",
    });
  });
});
