import {
  constrainAxis,
  contains,
  handleAt,
  rectHandles,
  type Handle,
} from "@/markup/geometry";
import type { Point, Rect, Size } from "@/markup/model";

/** The smallest crop, in screenshot pixels. */
export const MIN_CROP = 8;

export const fullRect = (image: Size): Rect => ({
  x: 0,
  y: 0,
  width: image.width,
  height: image.height,
});

/** The part of `image` that is kept. */
export const cropArea = (crop: Rect | null, image: Size): Rect =>
  crop ?? fullRect(image);

export const isFull = (rect: Rect, image: Size) =>
  rect.x === 0 &&
  rect.y === 0 &&
  rect.width === image.width &&
  rect.height === image.height;

const clamp = (v: number, low: number, high: number) =>
  Math.min(Math.max(v, low), high);

/** `rect` on whole pixels, inside `image` and at least [`MIN_CROP`] wide and high. */
export function snapCrop(rect: Rect, image: Size): Rect {
  const minW = Math.min(MIN_CROP, image.width);
  const minH = Math.min(MIN_CROP, image.height);
  const left = clamp(Math.round(rect.x), 0, image.width - minW);
  const top = clamp(Math.round(rect.y), 0, image.height - minH);
  const right = clamp(
    Math.round(rect.x + rect.width),
    left + minW,
    image.width,
  );
  const bottom = clamp(
    Math.round(rect.y + rect.height),
    top + minH,
    image.height,
  );
  return { x: left, y: top, width: right - left, height: bottom - top };
}

/** `start` moved by `delta`, kept inside `image`. `shift` keeps the move on one axis. */
export function moveCrop(
  start: Rect,
  delta: Point,
  image: Size,
  shift: boolean,
): Rect {
  const d = shift ? constrainAxis(delta) : delta;
  return {
    ...start,
    x: clamp(Math.round(start.x + d.x), 0, image.width - start.width),
    y: clamp(Math.round(start.y + d.y), 0, image.height - start.height),
  };
}

/**
 * `start` with `handle` dragged to `p`. The opposite side stays, the crop never flips
 * or leaves the image, and `keepAspect` keeps the shape of `start` on corner drags.
 */
export function resizeCrop(
  start: Rect,
  handle: Handle,
  p: Point,
  image: Size,
  keepAspect: boolean,
): Rect {
  let left = start.x;
  let top = start.y;
  let right = start.x + start.width;
  let bottom = start.y + start.height;
  const x = clamp(p.x, 0, image.width);
  const y = clamp(p.y, 0, image.height);
  if (handle.includes("w")) left = Math.min(x, right - MIN_CROP);
  if (handle.includes("e")) right = Math.max(x, left + MIN_CROP);
  if (handle.startsWith("n")) top = Math.min(y, bottom - MIN_CROP);
  if (handle.startsWith("s")) bottom = Math.max(y, top + MIN_CROP);

  const corner = handle.length === 2;
  if (keepAspect && corner && start.height > 0) {
    const aspect = start.width / start.height;
    const anchorX = handle.includes("w") ? right : left;
    const anchorY = handle.startsWith("n") ? bottom : top;
    const roomX = handle.includes("w") ? anchorX : image.width - anchorX;
    const roomY = handle.startsWith("n") ? anchorY : image.height - anchorY;
    let width = right - left;
    let height = bottom - top;
    if (width / height > aspect) height = width / aspect;
    else width = height * aspect;
    const fit = Math.min(1, roomX / width, roomY / height);
    width *= fit;
    height *= fit;
    left = handle.includes("w") ? anchorX - width : anchorX;
    right = left + width;
    top = handle.startsWith("n") ? anchorY - height : anchorY;
    bottom = top + height;
  }
  return snapCrop(
    { x: left, y: top, width: right - left, height: bottom - top },
    image,
  );
}

/** A crop dragged out from `start` to `p`, or `null` while it is too small. */
export function drawCrop(start: Point, p: Point, image: Size): Rect | null {
  const x0 = clamp(Math.min(start.x, p.x), 0, image.width);
  const y0 = clamp(Math.min(start.y, p.y), 0, image.height);
  const x1 = clamp(Math.max(start.x, p.x), 0, image.width);
  const y1 = clamp(Math.max(start.y, p.y), 0, image.height);
  if (x1 - x0 < MIN_CROP || y1 - y0 < MIN_CROP) return null;
  return snapCrop({ x: x0, y: y0, width: x1 - x0, height: y1 - y0 }, image);
}

/**
 * What a press at `p` does with the crop tool: resize from a handle, move the crop, or
 * drag out a new one. Inside an uncropped image there is nothing to move, so a press
 * there starts a new crop.
 */
export function cropPress(
  crop: Rect | null,
  image: Size,
  p: Point,
  tolerance: number,
):
  | { kind: "resize"; start: Rect; handle: Handle }
  | { kind: "move"; start: Rect }
  | { kind: "new" } {
  const start = cropArea(crop, image);
  const handle = handleAt(rectHandles(start), p, tolerance);
  if (handle) return { kind: "resize", start, handle };
  if (contains(start, p) && !isFull(start, image))
    return { kind: "move", start };
  return { kind: "new" };
}
