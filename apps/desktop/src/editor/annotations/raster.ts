import type { AnnotationShape } from "@/editor/settings";
import { bounds, type TextMeasure } from "@/markup/geometry";
import type { Point, Rect, Shape } from "@/markup/model";
import { drawShape, type MakeCanvas } from "@/markup/render";

/** Longest side of an annotation image, in pixels. */
export const MAX_RASTER_SIDE = 4096;

/** Where an annotation image goes; must match `editor::RasterPlacement` in Rust. */
export type RasterPlacement = {
  shape: AnnotationShape;
  width: number;
  height: number;
  /** Top-left corner in points from the shape's anchor. */
  x: number;
  y: number;
  widthPts: number;
  heightPts: number;
};

export type Raster = { placement: RasterPlacement; rgba: Uint8ClampedArray };

/** The point the backend places a shape's image from. */
export function anchor(shape: AnnotationShape): Point {
  switch (shape.kind) {
    case "arrow":
    case "line":
      return shape.from;
    case "text":
      return shape.at;
    default:
      return { x: shape.rect.x, y: shape.rect.y };
  }
}

/** What drawing `shape` covers, in points: its bounds with its stroke, arrowhead and edges. */
export function drawnBounds(shape: Shape, measure: TextMeasure): Rect {
  const box = bounds(shape, measure);
  let margin = 2;
  switch (shape.kind) {
    case "arrow":
      margin += (3 * shape.width + 6) / 2 + shape.width / 3;
      break;
    case "line":
      margin += shape.width / 2;
      break;
    case "rect":
    case "ellipse":
      margin += shape.width * 0.75;
      break;
    case "text":
      margin += shape.fontSize * 0.25;
      break;
  }
  return {
    x: box.x - margin,
    y: box.y - margin,
    width: box.width + 2 * margin,
    height: box.height + 2 * margin,
  };
}

/**
 * `shape` (in points) drawn by the screenshot editor's code at `pxPerPt` pixels per
 * point, fewer for images that would be larger than [`MAX_RASTER_SIDE`].
 */
export function rasterize(
  shape: AnnotationShape,
  pxPerPt: number,
  makeCanvas: MakeCanvas,
  measure: TextMeasure,
): Raster {
  const mark = { ...shape, id: "" } as Shape;
  const area = drawnBounds(mark, measure);
  const scale = Math.min(
    pxPerPt,
    MAX_RASTER_SIDE / Math.max(area.width, area.height, 1e-6),
  );
  const width = Math.max(1, Math.ceil(area.width * scale));
  const height = Math.max(1, Math.ceil(area.height * scale));
  const { ctx } = makeCanvas(width, height);
  ctx.setTransform(scale, 0, 0, scale, -area.x * scale, -area.y * scale);
  drawShape(ctx, mark, 1);
  const from = anchor(shape);
  return {
    placement: {
      shape,
      width,
      height,
      x: area.x - from.x,
      y: area.y - from.y,
      widthPts: width / scale,
      heightPts: height / scale,
    },
    rgba: ctx.getImageData(0, 0, width, height).data,
  };
}

/** The upload body: a little-endian u32 length, the placement as JSON, then the pixels. */
export function rasterMessage({ placement, rgba }: Raster): Uint8Array {
  const json = new TextEncoder().encode(JSON.stringify(placement));
  const out = new Uint8Array(4 + json.length + rgba.length);
  new DataView(out.buffer).setUint32(0, json.length, true);
  out.set(json, 4);
  out.set(rgba, 4 + json.length);
  return out;
}

/** An image sent this recently is not sent again when the backend asks while it is on its way. */
const RESEND_MS = 2000;

/**
 * Draws and sends the images the backend asks for, one at a time. Annotations asked for
 * while an image is on its way wait and are drawn as they are by then, so a slider
 * dragged across many values sends a few images, not one per value. `flush` waits until
 * every request is sent.
 */
export function createRasterUploader(options: {
  send: (body: Uint8Array) => Promise<unknown>;
  shapeOf: (id: string) => AnnotationShape | undefined;
  pxPerPt: () => number;
  makeCanvas: MakeCanvas;
  measure: TextMeasure;
  now?: () => number;
}) {
  const now = options.now ?? (() => performance.now());
  const queued = new Set<string>();
  const sent = new Map<string, number>();
  let running: Promise<void> | null = null;

  const pump = async () => {
    while (queued.size > 0) {
      const ids = [...queued];
      queued.clear();
      for (const id of ids) {
        const shape = options.shapeOf(id);
        if (!shape || shape.kind === "blur" || shape.kind === "pixelate")
          continue;
        const key = JSON.stringify(shape);
        const at = sent.get(key);
        if (at !== undefined && now() - at < RESEND_MS) continue;
        sent.set(key, now());
        try {
          const raster = rasterize(
            shape,
            options.pxPerPt(),
            options.makeCanvas,
            options.measure,
          );
          await options.send(rasterMessage(raster));
        } catch (e) {
          sent.delete(key);
          console.warn("cannot send an annotation image", e);
        }
      }
    }
  };
  const start = () => {
    running = pump().finally(() => {
      running = null;
      if (queued.size > 0) start();
    });
  };

  return {
    upload(ids: string[]) {
      for (const id of ids) queued.add(id);
      if (!running && queued.size > 0) start();
    },
    async flush() {
      while (running) await running;
    },
  };
}

export type RasterUploader = ReturnType<typeof createRasterUploader>;
