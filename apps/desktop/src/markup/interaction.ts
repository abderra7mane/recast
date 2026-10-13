import { bounds, handles, rectFrom, type Handle } from "@/markup/geometry";
import type { TextMeasure } from "@/markup/geometry";
import type { NewShape, Point, Rect, Shape, Style, Tool } from "@/markup/model";
import type { Ctx } from "@/markup/render";

/** Handle squares and how close the pointer must be to grab them, in CSS pixels. */
export const HANDLE_SIZE = 8;
export const GRAB = 7;
/** Shapes drawn smaller than this, in CSS pixels, are dropped. */
export const MIN_DRAWN = 3;
const ACCENT = "#0a84ff";

export const RESIZE_CURSORS: Record<Handle, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  n: "ns-resize",
  s: "ns-resize",
  e: "ew-resize",
  w: "ew-resize",
  start: "move",
  end: "move",
};

export const TOOL_CURSORS: Partial<Record<Tool, string>> = {
  select: "default",
  text: "text",
};

/** A shape of `tool` starting at `at`, before it is dragged out. */
export function newShape(tool: Tool, at: Point, style: Style): NewShape | null {
  const rect = { ...at, width: 0, height: 0 };
  switch (tool) {
    case "arrow":
    case "line":
      return {
        kind: tool,
        from: at,
        to: at,
        color: style.color,
        width: style.width,
      };
    case "rect":
    case "ellipse":
      return {
        kind: tool,
        rect,
        color: style.color,
        width: style.width,
        fill: style.fill,
      };
    case "highlight":
      return { kind: tool, rect, color: style.highlight };
    case "blur":
    case "pixelate":
      return { kind: tool, rect };
    default:
      return null;
  }
}

/** `shape` dragged out from `from` to `to`. */
export function drawnTo<S extends Shape | NewShape>(
  shape: S,
  from: Point,
  to: Point,
): S {
  if (shape.kind === "arrow" || shape.kind === "line") return { ...shape, to };
  if ("rect" in shape) return { ...shape, rect: rectFrom(from, to) };
  return shape;
}

export function tooSmall(shape: Shape | NewShape, minimum: number) {
  if (shape.kind === "arrow" || shape.kind === "line") {
    return (
      Math.hypot(shape.to.x - shape.from.x, shape.to.y - shape.from.y) < minimum
    );
  }
  if ("rect" in shape) {
    return shape.rect.width < minimum || shape.rect.height < minimum;
  }
  return false;
}

/** A handle square at `at`, in the context's current units. */
export function drawHandle(ctx: Ctx, at: Point) {
  const half = HANDLE_SIZE / 2;
  ctx.fillStyle = "#ffffff";
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = 1.5;
  ctx.fillRect(at.x - half, at.y - half, HANDLE_SIZE, HANDLE_SIZE);
  ctx.strokeRect(at.x - half, at.y - half, HANDLE_SIZE, HANDLE_SIZE);
}

/**
 * The selection of `shape`: a dashed box around all but lines, and its handles. `toView`
 * maps shape coordinates to the context's units (CSS pixels).
 */
export function drawSelection(
  ctx: Ctx,
  shape: Shape,
  measure: TextMeasure,
  toView: (p: Point) => Point,
) {
  if (shape.kind !== "arrow" && shape.kind !== "line") {
    const box: Rect = bounds(shape, measure);
    const a = toView(box);
    const b = toView({ x: box.x + box.width, y: box.y + box.height });
    ctx.setLineDash([4, 3]);
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = 1;
    ctx.strokeRect(a.x, a.y, b.x - a.x, b.y - a.y);
    ctx.setLineDash([]);
  }
  handles(shape, measure).forEach(({ at }) => drawHandle(ctx, toView(at)));
}
