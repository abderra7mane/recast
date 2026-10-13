import type { Frame } from "@/editor/frame";
import type { Rect } from "@/markup/model";
import { obscure, pixelRect, type Pixels } from "@/markup/obscure";
import type { Ctx, MakeCanvas, Surface } from "@/markup/render";
import type { ViewMap } from "@/markup/view";

/** `area` (whole frame pixels) of an NV12 frame as RGBA, converted like the preview renderer. */
export function frameRegion(frame: Frame, area: Rect): Pixels {
  const { width } = frame.header;
  const data = new Uint8ClampedArray(area.width * area.height * 4);
  let o = 0;
  for (let y = area.y; y < area.y + area.height; y++) {
    const luma = y * width;
    const chroma = (y >> 1) * width;
    for (let x = area.x; x < area.x + area.width; x++, o += 4) {
      const l = frame.y[luma + x];
      const c = chroma + (x & ~1);
      const cb = frame.uv[c] - 128;
      const cr = frame.uv[c + 1] - 128;
      data[o] = l + 1.5748 * cr;
      data[o + 1] = l - 0.1873 * cb - 0.4681 * cr;
      data[o + 2] = l + 1.8556 * cb;
      data[o + 3] = 255;
    }
  }
  return { width: area.width, height: area.height, data };
}

/**
 * Draws blur or pixelate over `rect` (points) into `ctx` (CSS pixels) while it is dragged,
 * from the frame on screen: the preview's stand-in until the compositor draws it.
 * `map` maps points to frame pixels and `scale` is CSS pixels per frame pixel.
 */
export function createObscurePreview(makeCanvas: MakeCanvas) {
  let cached: { key: string; surface: Surface; area: Rect } | null = null;
  return (
    ctx: Ctx,
    frame: Frame,
    kind: "blur" | "pixelate",
    rect: Rect,
    blockPts: number,
    map: ViewMap,
    scale: number,
  ) => {
    const { content } = frame.header;
    const left = Math.max(rect.x * map.zoom + map.offsetX, content.x);
    const top = Math.max(rect.y * map.zoom + map.offsetY, content.y);
    const right = Math.min(
      (rect.x + rect.width) * map.zoom + map.offsetX,
      content.x + content.width,
    );
    const bottom = Math.min(
      (rect.y + rect.height) * map.zoom + map.offsetY,
      content.y + content.height,
    );
    const area = pixelRect(
      { x: left, y: top, width: right - left, height: bottom - top },
      frame.header,
    );
    if (area.width <= 0 || area.height <= 0) return;
    const block = Math.max(2, Math.round(blockPts * map.zoom));
    const key = `${frame.header.seq} ${kind} ${block} ${area.x} ${area.y} ${area.width} ${area.height}`;
    if (cached?.key !== key) {
      const surface = makeCanvas(area.width, area.height);
      const image = surface.ctx.createImageData(area.width, area.height);
      const source = frameRegion(frame, area);
      image.data.set(obscure(source, { ...area, x: 0, y: 0 }, kind, block));
      surface.ctx.putImageData(image, 0, 0);
      cached = { key, surface, area };
    }
    ctx.drawImage(
      cached.surface.canvas,
      area.x * scale,
      area.y * scale,
      area.width * scale,
      area.height * scale,
    );
  };
}
