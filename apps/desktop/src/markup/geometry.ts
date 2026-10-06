import type { Point, Rect, Shape, Size, TextShape } from "@/markup/model";
import { hasRect, isLine } from "@/markup/model";

export type Handle =
  "start" | "end" | "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

/** A text shape's size in screenshot pixels. */
export type TextMeasure = (shape: TextShape) => Size;

const MIN_FONT_SIZE = 6;
const MAX_FONT_SIZE = 400;

export function rectFrom(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(b.x - a.x),
    height: Math.abs(b.y - a.y),
  };
}

export function contains(rect: Rect, p: Point, margin = 0): boolean {
  return (
    p.x >= rect.x - margin &&
    p.x <= rect.x + rect.width + margin &&
    p.y >= rect.y - margin &&
    p.y <= rect.y + rect.height + margin
  );
}

export function distanceToSegment(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length2 = dx * dx + dy * dy;
  const t =
    length2 === 0
      ? 0
      : Math.max(
          0,
          Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / length2),
        );
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** `to` turned to the nearest multiple of 45° around `from`, keeping its projection. */
export function constrainAngle(from: Point, to: Point): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const step = Math.PI / 4;
  const angle = Math.round(Math.atan2(dy, dx) / step) * step;
  const length = dx * Math.cos(angle) + dy * Math.sin(angle);
  return {
    x: from.x + round6(length * Math.cos(angle)),
    y: from.y + round6(length * Math.sin(angle)),
  };
}

const round6 = (v: number) => Math.round(v * 1e6) / 1e6;

/** `p` moved so that it spans a square with `anchor`, on the longer side. */
export function constrainSquare(anchor: Point, p: Point): Point {
  const dx = p.x - anchor.x;
  const dy = p.y - anchor.y;
  const side = Math.max(Math.abs(dx), Math.abs(dy));
  return {
    x: anchor.x + (dx < 0 ? -side : side),
    y: anchor.y + (dy < 0 ? -side : side),
  };
}

/** A move of `delta` kept on its main axis. */
export function constrainAxis(delta: Point): Point {
  return Math.abs(delta.x) >= Math.abs(delta.y)
    ? { x: delta.x, y: 0 }
    : { x: 0, y: delta.y };
}

export function bounds(shape: Shape, measure: TextMeasure): Rect {
  if (isLine(shape)) return rectFrom(shape.from, shape.to);
  if (shape.kind === "text") {
    return { ...shape.at, ...measure(shape) };
  }
  return shape.rect;
}

const RECT_HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];
const CORNERS: Handle[] = ["nw", "ne", "se", "sw"];

export function handlePoint(rect: Rect, handle: Handle): Point {
  const left = rect.x;
  const right = rect.x + rect.width;
  const top = rect.y;
  const bottom = rect.y + rect.height;
  const cx = rect.x + rect.width / 2;
  const cy = rect.y + rect.height / 2;
  switch (handle) {
    case "nw":
      return { x: left, y: top };
    case "n":
      return { x: cx, y: top };
    case "ne":
      return { x: right, y: top };
    case "e":
      return { x: right, y: cy };
    case "se":
      return { x: right, y: bottom };
    case "s":
      return { x: cx, y: bottom };
    case "sw":
      return { x: left, y: bottom };
    case "w":
      return { x: left, y: cy };
    default:
      return { x: cx, y: cy };
  }
}

export function rectHandles(rect: Rect): { handle: Handle; at: Point }[] {
  return RECT_HANDLES.map((handle) => ({
    handle,
    at: handlePoint(rect, handle),
  }));
}

/** The handles a selected shape shows. */
export function handles(
  shape: Shape,
  measure: TextMeasure,
): { handle: Handle; at: Point }[] {
  if (isLine(shape)) {
    return [
      { handle: "start", at: shape.from },
      { handle: "end", at: shape.to },
    ];
  }
  if (shape.kind === "text") {
    const box = bounds(shape, measure);
    return CORNERS.map((handle) => ({ handle, at: handlePoint(box, handle) }));
  }
  return rectHandles(shape.rect);
}

/** The handle within `tolerance` of `p`, nearest first. */
export function handleAt(
  points: { handle: Handle; at: Point }[],
  p: Point,
  tolerance: number,
): Handle | null {
  let best: Handle | null = null;
  let bestDistance = tolerance;
  for (const { handle, at } of points) {
    const distance = Math.max(Math.abs(p.x - at.x), Math.abs(p.y - at.y));
    if (distance <= bestDistance) {
      best = handle;
      bestDistance = distance;
    }
  }
  return best;
}

function ellipseDistance(rect: Rect, p: Point): number {
  const rx = rect.width / 2;
  const ry = rect.height / 2;
  if (rx < 1e-6 || ry < 1e-6) {
    return distanceToSegment(
      p,
      { x: rect.x, y: rect.y },
      { x: rect.x + rect.width, y: rect.y + rect.height },
    );
  }
  const dx = (p.x - rect.x - rx) / rx;
  const dy = (p.y - rect.y - ry) / ry;
  return Math.abs(Math.hypot(dx, dy) - 1) * Math.min(rx, ry);
}

function rectEdgeDistance(rect: Rect, p: Point): number {
  const right = rect.x + rect.width;
  const bottom = rect.y + rect.height;
  const inside = contains(rect, p);
  if (inside) {
    return Math.min(p.x - rect.x, right - p.x, p.y - rect.y, bottom - p.y);
  }
  const dx = Math.max(rect.x - p.x, 0, p.x - right);
  const dy = Math.max(rect.y - p.y, 0, p.y - bottom);
  return Math.hypot(dx, dy);
}

/**
 * Whether `p` is on `shape`. Outlines count only near their stroke, so shapes inside
 * them stay reachable; `tolerance` is in screenshot pixels, stroke widths in points
 * times `scale`.
 */
export function hits(
  shape: Shape,
  p: Point,
  tolerance: number,
  scale: number,
  measure: TextMeasure,
): boolean {
  switch (shape.kind) {
    case "arrow":
    case "line":
      return (
        distanceToSegment(p, shape.from, shape.to) <=
        (shape.width * scale) / 2 + tolerance
      );
    case "rect":
    case "ellipse": {
      const reach = (shape.width * scale) / 2 + tolerance;
      if (shape.fill) return contains(shape.rect, p, reach);
      return shape.kind === "rect"
        ? rectEdgeDistance(shape.rect, p) <= reach
        : ellipseDistance(shape.rect, p) <= reach;
    }
    case "text":
      return contains(bounds(shape, measure), p, tolerance);
    default:
      return contains(shape.rect, p, tolerance);
  }
}

/** The topmost shape at `p`. */
export function hitTest(
  shapes: Shape[],
  p: Point,
  tolerance: number,
  scale: number,
  measure: TextMeasure,
): Shape | null {
  for (let i = shapes.length - 1; i >= 0; i--) {
    if (hits(shapes[i], p, tolerance, scale, measure)) return shapes[i];
  }
  return null;
}

export function moveShape(shape: Shape, dx: number, dy: number): Shape {
  const by = (p: Point) => ({ x: p.x + dx, y: p.y + dy });
  if (isLine(shape))
    return { ...shape, from: by(shape.from), to: by(shape.to) };
  if (shape.kind === "text") return { ...shape, at: by(shape.at) };
  return { ...shape, rect: { ...shape.rect, ...by(shape.rect) } };
}

const OPPOSITE: Record<Handle, Handle> = {
  start: "end",
  end: "start",
  nw: "se",
  n: "s",
  ne: "sw",
  e: "w",
  se: "nw",
  s: "n",
  sw: "ne",
  w: "e",
};

/**
 * `rect` with `handle` dragged to `p`; the opposite side stays. Dragging past it flips
 * the rectangle. `square` makes corner drags span a square.
 */
export function resizeRect(
  rect: Rect,
  handle: Handle,
  p: Point,
  square: boolean,
): Rect {
  const anchor = handlePoint(rect, OPPOSITE[handle]);
  switch (handle) {
    case "n":
    case "s":
      return {
        x: rect.x,
        width: rect.width,
        y: Math.min(anchor.y, p.y),
        height: Math.abs(p.y - anchor.y),
      };
    case "e":
    case "w":
      return {
        y: rect.y,
        height: rect.height,
        x: Math.min(anchor.x, p.x),
        width: Math.abs(p.x - anchor.x),
      };
    default:
      return rectFrom(anchor, square ? constrainSquare(anchor, p) : p);
  }
}

/**
 * `shape` (as it was when the drag began) with `handle` dragged to `p`. `shift`
 * constrains lines to 45° and boxes to squares; text scales its font size.
 */
export function resizeShape(
  shape: Shape,
  handle: Handle,
  p: Point,
  shift: boolean,
  measure: TextMeasure,
): Shape {
  if (isLine(shape)) {
    if (handle === "start") {
      return { ...shape, from: shift ? constrainAngle(shape.to, p) : p };
    }
    return { ...shape, to: shift ? constrainAngle(shape.from, p) : p };
  }
  if (shape.kind === "text") {
    const box = bounds(shape, measure);
    const anchor = handlePoint(box, OPPOSITE[handle]);
    const factor = Math.max(
      Math.abs(p.y - anchor.y) / Math.max(box.height, 1e-6),
      MIN_FONT_SIZE / shape.fontSize,
    );
    const fontSize = Math.min(
      MAX_FONT_SIZE,
      Math.round(shape.fontSize * factor * 10) / 10,
    );
    const scaled = fontSize / shape.fontSize;
    const width = box.width * scaled;
    const height = box.height * scaled;
    const left = handle === "nw" || handle === "sw";
    const top = handle === "nw" || handle === "ne";
    return {
      ...shape,
      fontSize,
      at: {
        x: left ? anchor.x - width : anchor.x,
        y: top ? anchor.y - height : anchor.y,
      },
    };
  }
  if (hasRect(shape)) {
    return { ...shape, rect: resizeRect(shape.rect, handle, p, shift) };
  }
  return shape;
}

/** A new shape's end point while it is drawn from `start`. */
export function drawPoint(
  kind: Shape["kind"],
  start: Point,
  p: Point,
  shift: boolean,
): Point {
  if (!shift) return p;
  if (kind === "arrow" || kind === "line") return constrainAngle(start, p);
  return constrainSquare(start, p);
}
