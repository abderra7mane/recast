import type { TextMeasure } from "@/markup/geometry";
import type {
  Doc,
  LineShape,
  ObscureShape,
  Point,
  Rect,
  Shape,
  Size,
  TextShape,
} from "@/markup/model";
import { isObscure } from "@/markup/model";
import { blockSize, obscure, pixelRect, type Pixels } from "@/markup/obscure";

export type Ctx = CanvasRenderingContext2D;
export type Surface = { canvas: CanvasImageSource; ctx: Ctx };
export type MakeCanvas = (width: number, height: number) => Surface;

export const FONT_FAMILY =
  '-apple-system, "Helvetica Neue", Helvetica, Arial, sans-serif';
export const FONT_WEIGHT = 600;
export const LINE_HEIGHT = 1.25;
export const HIGHLIGHT_OPACITY = 0.4;

export const fontFor = (pixels: number) =>
  `${FONT_WEIGHT} ${pixels}px ${FONT_FAMILY}`;

export const textLines = (text: string) => text.split("\n");

/** Arrow parts in screenshot pixels for a stroke `width` points wide. */
export function arrowParts(
  from: Point,
  to: Point,
  width: number,
  scale: number,
): { shaftEnd: Point; tip: Point; left: Point; right: Point } | null {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const length = Math.hypot(dx, dy);
  if (length < 1e-6) return null;
  const ux = dx / length;
  const uy = dy / length;
  const head = Math.min(length, (3 * width + 6) * scale);
  const half = head * 0.5;
  const base = { x: to.x - ux * head, y: to.y - uy * head };
  const overlap = Math.min(head * 0.3, length - head);
  return {
    shaftEnd: { x: base.x + ux * overlap, y: base.y + uy * overlap },
    tip: to,
    left: { x: base.x - uy * half, y: base.y + ux * half },
    right: { x: base.x + uy * half, y: base.y - ux * half },
  };
}

function strokeLine(ctx: Ctx, shape: LineShape, scale: number) {
  const width = shape.width * scale;
  ctx.strokeStyle = shape.color;
  ctx.fillStyle = shape.color;
  ctx.lineWidth = width;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const arrow =
    shape.kind === "arrow"
      ? arrowParts(shape.from, shape.to, shape.width, scale)
      : null;
  ctx.beginPath();
  ctx.moveTo(shape.from.x, shape.from.y);
  const end = arrow?.shaftEnd ?? shape.to;
  ctx.lineTo(end.x, end.y);
  ctx.stroke();
  if (arrow) {
    ctx.beginPath();
    ctx.moveTo(arrow.tip.x, arrow.tip.y);
    ctx.lineTo(arrow.left.x, arrow.left.y);
    ctx.lineTo(arrow.right.x, arrow.right.y);
    ctx.closePath();
    ctx.fill();
    ctx.lineWidth = Math.max(1, width / 3);
    ctx.stroke();
  }
}

function drawText(ctx: Ctx, shape: TextShape, scale: number) {
  const size = shape.fontSize * scale;
  const lineHeight = size * LINE_HEIGHT;
  ctx.font = fontFor(size);
  ctx.fillStyle = shape.color;
  ctx.textAlign = "left";
  ctx.textBaseline = "alphabetic";
  textLines(shape.text).forEach((line, i) => {
    const metrics = ctx.measureText(line);
    const ascent = metrics.fontBoundingBoxAscent ?? size * 0.8;
    const descent = metrics.fontBoundingBoxDescent ?? size * 0.2;
    const baseline =
      shape.at.y +
      i * lineHeight +
      (lineHeight - ascent - descent) / 2 +
      ascent;
    ctx.fillText(line, shape.at.x, baseline);
  });
}

export function drawShape(ctx: Ctx, shape: Shape, scale: number) {
  ctx.save();
  switch (shape.kind) {
    case "arrow":
    case "line":
      strokeLine(ctx, shape, scale);
      break;
    case "rect": {
      const { x, y, width, height } = shape.rect;
      ctx.lineJoin = "miter";
      ctx.lineWidth = shape.width * scale;
      ctx.strokeStyle = shape.color;
      if (shape.fill) {
        ctx.fillStyle = shape.color;
        ctx.fillRect(x, y, width, height);
      }
      ctx.strokeRect(x, y, width, height);
      break;
    }
    case "ellipse": {
      const { x, y, width, height } = shape.rect;
      ctx.beginPath();
      ctx.ellipse(
        x + width / 2,
        y + height / 2,
        width / 2,
        height / 2,
        0,
        0,
        Math.PI * 2,
      );
      ctx.lineWidth = shape.width * scale;
      ctx.strokeStyle = shape.color;
      if (shape.fill) {
        ctx.fillStyle = shape.color;
        ctx.fill();
      }
      ctx.stroke();
      break;
    }
    case "highlight": {
      const { x, y, width, height } = shape.rect;
      ctx.globalAlpha = HIGHLIGHT_OPACITY;
      ctx.fillStyle = shape.color;
      ctx.fillRect(x, y, width, height);
      break;
    }
    case "text":
      drawText(ctx, shape, scale);
      break;
  }
  ctx.restore();
}

/** A text shape's size in screenshot pixels, measured with `ctx`. */
export function measureText(ctx: Ctx, shape: TextShape, scale: number): Size {
  const size = shape.fontSize * scale;
  ctx.font = fontFor(size);
  const lines = textLines(shape.text);
  const width = Math.max(
    size * 0.5,
    ...lines.map((line) => ctx.measureText(line).width),
  );
  return { width, height: lines.length * size * LINE_HEIGHT };
}

export type Renderer = {
  scale: number;
  image: Size;
  measure: TextMeasure;
  /**
   * Draws `area` of the marked-up screenshot into `ctx`, one screenshot pixel per
   * canvas pixel, leaving out the shape `hidden`.
   */
  render: (
    ctx: Ctx,
    doc: Pick<Doc, "shapes">,
    area: Rect,
    hidden?: string | null,
  ) => void;
};

/**
 * Renders markup over `base`, whose straight RGBA `pixels` feed blur and pixelate.
 * Obscured areas are kept between renders while their rectangles stay the same.
 */
export function createRenderer(
  base: CanvasImageSource,
  pixels: Pixels,
  scale: number,
  makeCanvas: MakeCanvas,
): Renderer {
  const patches = new Map<string, Surface>();
  const measuring = makeCanvas(1, 1).ctx;
  const block = blockSize(scale);

  const patch = (shape: ObscureShape, area: Rect) => {
    const key = `${shape.kind} ${area.x} ${area.y} ${area.width} ${area.height}`;
    let surface = patches.get(key);
    if (!surface) {
      surface = makeCanvas(area.width, area.height);
      const image = surface.ctx.createImageData(area.width, area.height);
      image.data.set(obscure(pixels, area, shape.kind, block));
      surface.ctx.putImageData(image, 0, 0);
      patches.set(key, surface);
    }
    return { key, surface };
  };

  return {
    scale,
    image: { width: pixels.width, height: pixels.height },
    measure: (shape) => measureText(measuring, shape, scale),
    render(ctx, doc, area, hidden = null) {
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, area.width, area.height);
      ctx.translate(-area.x, -area.y);
      ctx.drawImage(base, 0, 0);
      const used = new Set<string>();
      for (const shape of doc.shapes) {
        if (!isObscure(shape)) continue;
        const rect = pixelRect(shape.rect, pixels);
        if (rect.width === 0 || rect.height === 0) continue;
        const { key, surface } = patch(shape, rect);
        used.add(key);
        ctx.clearRect(rect.x, rect.y, rect.width, rect.height);
        ctx.drawImage(surface.canvas, rect.x, rect.y);
      }
      for (const key of [...patches.keys()]) {
        if (!used.has(key)) patches.delete(key);
      }
      for (const shape of doc.shapes) {
        if (!isObscure(shape) && shape.id !== hidden) {
          drawShape(ctx, shape, scale);
        }
      }
      ctx.restore();
    },
  };
}

/** The marked-up `area` as straight RGBA, as it is exported. */
export function renderOutput(
  renderer: Renderer,
  doc: Pick<Doc, "shapes">,
  area: Rect,
  makeCanvas: MakeCanvas,
): Uint8ClampedArray {
  const { ctx } = makeCanvas(area.width, area.height);
  renderer.render(ctx, doc, area);
  return ctx.getImageData(0, 0, area.width, area.height).data;
}
