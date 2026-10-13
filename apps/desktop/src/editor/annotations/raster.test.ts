import { createCanvas } from "@napi-rs/canvas";
import { describe, expect, it, vi } from "vitest";

import {
  anchor,
  createRasterUploader,
  MAX_RASTER_SIDE,
  rasterize,
  rasterMessage,
} from "@/editor/annotations/raster";
import type { AnnotationShape } from "@/editor/settings";
import { measureText, type Ctx, type MakeCanvas } from "@/markup/render";

const makeCanvas: MakeCanvas = (width, height) => {
  const canvas = createCanvas(width, height);
  return {
    canvas: canvas as unknown as CanvasImageSource,
    ctx: canvas.getContext("2d") as unknown as Ctx,
  };
};
const measuring = makeCanvas(1, 1).ctx;
const measure = (shape: Parameters<typeof measureText>[1]) =>
  measureText(measuring, shape, 1);

const filled: AnnotationShape = {
  kind: "rect",
  rect: { x: 100, y: 40, width: 20, height: 10 },
  color: "#ff0000",
  width: 2,
  fill: true,
};

describe("rasterize", () => {
  it("draws the shape around its anchor at the given density", () => {
    const { placement, rgba } = rasterize(filled, 4, makeCanvas, measure);
    expect(anchor(filled)).toEqual({ x: 100, y: 40 });
    // The rectangle plus its stroke and a margin on each side.
    const margin = 2 + 2 * 0.75;
    expect(placement.x).toBeCloseTo(-margin);
    expect(placement.y).toBeCloseTo(-margin);
    expect(placement.width).toBe(Math.ceil((20 + 2 * margin) * 4));
    expect(placement.widthPts).toBeCloseTo(placement.width / 4);
    const at = (xPt: number, yPt: number) => {
      const x = Math.floor((xPt - placement.x) * 4);
      const y = Math.floor((yPt - placement.y) * 4);
      return [
        ...rgba.slice(
          (y * placement.width + x) * 4,
          (y * placement.width + x) * 4 + 4,
        ),
      ];
    };
    expect(at(10, 5)).toEqual([255, 0, 0, 255]);
    expect(at(-3, -3)[3]).toBe(0);
  });

  it("keeps large images under the size limit", () => {
    const line: AnnotationShape = {
      kind: "line",
      from: { x: 0, y: 0 },
      to: { x: 3000, y: 10 },
      color: "#000000",
      width: 2,
    };
    const { placement } = rasterize(line, 4, makeCanvas, measure);
    expect(placement.width).toBeLessThanOrEqual(MAX_RASTER_SIDE);
    expect(placement.widthPts).toBeGreaterThan(3000);
  });

  it("sizes text by its lines", () => {
    const text: AnnotationShape = {
      kind: "text",
      at: { x: 5, y: 5 },
      text: "One\nTwo",
      color: "#ffffff",
      fontSize: 20,
    };
    const { placement } = rasterize(text, 2, makeCanvas, measure);
    expect(placement.heightPts).toBeGreaterThan(2 * 20 * 1.25);
    expect(placement.x).toBeLessThan(0);
  });
});

describe("rasterMessage", () => {
  it("puts the placement before the pixels", () => {
    const raster = rasterize(filled, 1, makeCanvas, measure);
    const body = rasterMessage(raster);
    const length = new DataView(body.buffer).getUint32(0, true);
    const placement = JSON.parse(
      new TextDecoder().decode(body.subarray(4, 4 + length)),
    );
    expect(placement).toEqual(raster.placement);
    expect(body.length - 4 - length).toBe(raster.rgba.length);
  });
});

describe("createRasterUploader", () => {
  const thick: AnnotationShape = { ...filled, width: 9 } as AnnotationShape;

  it("sends one image at a time, the newest look, and skips blur", async () => {
    const releases: (() => void)[] = [];
    const send = vi.fn(
      () => new Promise<void>((resolve) => releases.push(resolve)),
    );
    const shapes: Record<string, AnnotationShape> = {
      a: filled,
      b: filled,
      c: { kind: "blur", rect: { x: 0, y: 0, width: 1, height: 1 }, block: 12 },
    };
    let clock = 0;
    const uploader = createRasterUploader({
      send,
      shapeOf: (id) => shapes[id],
      pxPerPt: () => 1,
      makeCanvas,
      measure,
      now: () => clock,
    });
    uploader.upload(["a", "b", "c", "missing"]);
    expect(send).toHaveBeenCalledTimes(1);
    // While the first image is on its way, "a" changes twice.
    shapes.a = { ...filled, width: 5 } as AnnotationShape;
    uploader.upload(["a"]);
    shapes.a = thick;
    uploader.upload(["a"]);
    const flushed = uploader.flush();
    releases.shift()!();
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(2));
    releases.shift()!();
    await flushed;
    const body = send.mock.calls[1] as unknown as [Uint8Array];
    const length = new DataView(body[0].buffer).getUint32(0, true);
    expect(
      JSON.parse(new TextDecoder().decode(body[0].subarray(4, 4 + length)))
        .shape,
    ).toEqual(thick);

    uploader.upload(["a"]);
    await uploader.flush();
    expect(send).toHaveBeenCalledTimes(2);
    clock = 5000;
    uploader.upload(["a"]);
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(3));
    releases.shift()!();
    await uploader.flush();
  });
});
