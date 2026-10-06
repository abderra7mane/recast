import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createCanvas, loadImage } from "@napi-rs/canvas";
import { describe, expect, it } from "vitest";

import { fullRect } from "@/markup/crop";
import { checkerboard, variance } from "@/markup/fixtures.test-util";
import type { NewShape, Rect, Shape } from "@/markup/model";
import type { Pixels } from "@/markup/obscure";
import {
  arrowParts,
  createRenderer,
  renderOutput,
  type Ctx,
  type MakeCanvas,
} from "@/markup/render";

const GOLDENS = join(import.meta.dirname, "goldens");
const SCALE = 2;
const W = 320;
const H = 200;

const makeCanvas: MakeCanvas = (width, height) => {
  const canvas = createCanvas(width, height);
  return {
    canvas: canvas as unknown as CanvasImageSource,
    ctx: canvas.getContext("2d") as unknown as Ctx,
  };
};

/** A light screenshot with a checkerboard, color bars and thin dark strokes. */
function synthetic(): Pixels {
  const data = new Uint8ClampedArray(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      let px = [236, 236, 240, 255];
      if (x < 120 && y < 80) {
        const v = (x + y) % 2 === 0 ? 30 : 230;
        px = [v, v, v, 255];
      } else if (y >= 170) {
        px = [(x * 255) / W, 120, 255 - (x * 255) / W, 255];
      } else if (x >= 200 && y % 8 < 2) {
        px = [40, 40, 48, 255];
      }
      data.set(px, (y * W + x) * 4);
    }
  }
  return { width: W, height: H, data };
}

function setup(pixels: Pixels = synthetic(), scale = SCALE) {
  const base = makeCanvas(pixels.width, pixels.height);
  const image = base.ctx.createImageData(pixels.width, pixels.height);
  image.data.set(pixels.data);
  base.ctx.putImageData(image, 0, 0);
  return {
    pixels,
    renderer: createRenderer(base.canvas, pixels, scale, makeCanvas),
  };
}

const withIds = (shapes: NewShape[]): Shape[] =>
  shapes.map((shape, i) => ({ ...shape, id: `s${i}` }) as Shape);

function render(
  shapes: NewShape[],
  area?: Rect,
  pixels?: Pixels,
  scale?: number,
) {
  const { renderer, pixels: source } = setup(pixels, scale);
  const region = area ?? fullRect(source);
  return renderOutput(
    renderer,
    { shapes: withIds(shapes) },
    region,
    makeCanvas,
  );
}

async function decode(path: string) {
  const image = await loadImage(readFileSync(path));
  const { ctx } = makeCanvas(image.width, image.height);
  ctx.drawImage(image as unknown as CanvasImageSource, 0, 0);
  return ctx.getImageData(0, 0, image.width, image.height).data;
}

function encode(width: number, height: number, data: Uint8ClampedArray) {
  const canvas = createCanvas(width, height);
  const ctx = canvas.getContext("2d");
  const image = ctx.createImageData(width, height);
  image.data.set(data);
  ctx.putImageData(image, 0, 0);
  return canvas.toBuffer("image/png");
}

/** Compares with `goldens/<name>.png`; `UPDATE_GOLDENS=1` rewrites it. */
async function golden(
  name: string,
  data: Uint8ClampedArray,
  width = W,
  height = H,
) {
  const path = join(GOLDENS, `${name}.png`);
  if (process.env.UPDATE_GOLDENS) {
    mkdirSync(GOLDENS, { recursive: true });
    writeFileSync(path, encode(width, height, data));
    return;
  }
  if (!existsSync(path)) {
    throw new Error(`${path} is missing; run \`make update-goldens\``);
  }
  const expected = await decode(path);
  expect(expected.length).toBe(data.length);
  let off = 0;
  let total = 0;
  for (let i = 0; i < data.length; i += 4) {
    let worst = 0;
    for (let c = 0; c < 3; c++) {
      const d = Math.abs(data[i + c] - expected[i + c]);
      total += d;
      worst = Math.max(worst, d);
    }
    if (worst > 24) off++;
  }
  const mean = total / ((data.length / 4) * 3);
  const allowed = (width * height) / 500;
  if (mean > 1 || off > allowed) {
    const failures = join(tmpdir(), "recast-golden-failures");
    mkdirSync(failures, { recursive: true });
    writeFileSync(join(failures, `${name}.png`), encode(width, height, data));
    throw new Error(
      `${name}: mean difference ${mean.toFixed(3)}, ${off} pixels off (allowed ${allowed}); actual image in ${failures}`,
    );
  }
}

const red = "#ff3b30";

const TOOLS: Record<string, NewShape[]> = {
  arrow: [
    {
      kind: "arrow",
      from: { x: 30, y: 160 },
      to: { x: 260, y: 40 },
      color: red,
      width: 3,
    },
  ],
  line: [
    {
      kind: "line",
      from: { x: 20, y: 120 },
      to: { x: 300, y: 120 },
      color: "#0a84ff",
      width: 2,
    },
  ],
  rect: [
    {
      kind: "rect",
      rect: { x: 40, y: 30, width: 120, height: 80 },
      color: red,
      width: 3,
      fill: false,
    },
    {
      kind: "rect",
      rect: { x: 200, y: 100, width: 80, height: 50 },
      color: "#30d158",
      width: 2,
      fill: true,
    },
  ],
  ellipse: [
    {
      kind: "ellipse",
      rect: { x: 60, y: 40, width: 200, height: 110 },
      color: red,
      width: 4,
      fill: false,
    },
  ],
  highlight: [
    {
      kind: "highlight",
      rect: { x: 180, y: 20, width: 120, height: 60 },
      color: "#ffd60a",
    },
  ],
  text: [
    {
      kind: "text",
      at: { x: 130, y: 90 },
      text: "Recast\nmarkup",
      color: "#000000",
      fontSize: 16,
    },
  ],
  blur: [{ kind: "blur", rect: { x: 20, y: 10, width: 200, height: 120 } }],
  pixelate: [
    { kind: "pixelate", rect: { x: 20, y: 10, width: 200, height: 120 } },
  ],
};

describe("export renderer", () => {
  for (const [tool, shapes] of Object.entries(TOOLS)) {
    it(`draws ${tool} at native scale like its golden`, async () => {
      await golden(tool, render(shapes));
    });
  }

  it("draws the crop like its golden", async () => {
    const crop = { x: 100, y: 40, width: 180, height: 140 };
    await golden("crop", render(TOOLS.arrow, crop), crop.width, crop.height);
  });

  it("scales stroke widths from points to pixels", () => {
    const at = (data: Uint8ClampedArray, x: number, y: number) =>
      data.slice((y * W + x) * 4, (y * W + x) * 4 + 3);
    const isRed = (px: Uint8ClampedArray) => px[0] > 200 && px[1] < 100;
    const rows = (scale: number, y: number) => {
      const line: NewShape = {
        kind: "line",
        from: { x: 10, y },
        to: { x: 190, y },
        color: red,
        width: 3,
      };
      const data = render([line], undefined, undefined, scale);
      return Array.from({ length: H }, (_, y) => y).filter((y) =>
        isRed(at(data, 100, y)),
      );
    };
    expect(rows(2, 140)).toEqual([137, 138, 139, 140, 141, 142]);
    expect(rows(1, 140.5)).toEqual([139, 140, 141]);
  });

  it("leaves the screenshot untouched outside the marks", () => {
    const source = synthetic();
    const data = render([
      {
        kind: "rect",
        rect: { x: 40, y: 30, width: 60, height: 40 },
        color: red,
        width: 1,
        fill: false,
      },
    ]);
    let changed = 0;
    for (let y = 0; y < H; y++) {
      for (let x = 0; x < W; x++) {
        const near = x >= 38 && x <= 102 && y >= 28 && y <= 72;
        const i = (y * W + x) * 4;
        if (!near && data[i] !== source.data[i]) changed++;
      }
    }
    expect(changed).toBe(0);
  });

  it("crops pixel for pixel", () => {
    const source = synthetic();
    const crop = { x: 37, y: 21, width: 151, height: 97 };
    const data = render([], crop);
    for (let y = 0; y < crop.height; y++) {
      const out = data.slice(y * crop.width * 4, (y + 1) * crop.width * 4);
      const start = ((crop.y + y) * W + crop.x) * 4;
      expect(out).toEqual(source.data.slice(start, start + crop.width * 4));
    }
  });

  for (const kind of ["blur", "pixelate"] as const) {
    it(`${kind} destroys the detail under it in the output`, () => {
      const source = checkerboard(W, H);
      const rect = { x: 40, y: 20, width: 200, height: 120 };
      const data = render([{ kind, rect }], undefined, source);
      const inside = new Uint8ClampedArray(rect.width * rect.height * 4);
      for (let y = 0; y < rect.height; y++) {
        const start = ((rect.y + y) * W + rect.x) * 4;
        inside.set(
          data.slice(start, start + rect.width * 4),
          y * rect.width * 4,
        );
      }
      expect(variance(source.data)).toBeGreaterThan(16_000);
      expect(variance(inside)).toBeLessThan(1);
      const outside = (y: number) => data.slice(y * W * 4, (y + 1) * W * 4);
      expect(outside(10)).toEqual(source.data.slice(10 * W * 4, 11 * W * 4));
    });
  }

  it("keeps marks above blur and pixelate", () => {
    const arrow = TOOLS.arrow[0];
    const blur: NewShape = {
      kind: "blur",
      rect: { x: 0, y: 0, width: W, height: H },
    };
    expect(render([arrow, blur])).toEqual(render([blur, arrow]));
  });

  it("previews exactly what it exports", () => {
    const { renderer } = setup();
    const doc = {
      shapes: withIds([...TOOLS.blur, ...TOOLS.text, ...TOOLS.arrow]),
    };
    const area = fullRect({ width: W, height: H });
    const preview = makeCanvas(W, H);
    renderer.render(preview.ctx, doc, area);
    renderer.render(preview.ctx, doc, area);
    const exported = renderOutput(renderer, doc, area, makeCanvas);
    expect(preview.ctx.getImageData(0, 0, W, H).data).toEqual(exported);
  });

  it("leaves out the text being typed", () => {
    const { renderer } = setup();
    const shapes = withIds(TOOLS.text);
    const area = fullRect({ width: W, height: H });
    const without = renderOutput(renderer, { shapes: [] }, area, makeCanvas);
    const { ctx } = makeCanvas(W, H);
    renderer.render(ctx, { shapes }, area, shapes[0].id);
    expect(ctx.getImageData(0, 0, W, H).data).toEqual(without);
  });
});

describe("text", () => {
  const white = (width: number, height: number): Pixels => ({
    width,
    height,
    data: new Uint8ClampedArray(width * height * 4).fill(255),
  });

  /** Gray edge pixels per fully dark pixel: lower is crisper. */
  function softness(data: Uint8ClampedArray) {
    let dark = 0;
    let gray = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i] < 40) dark++;
      else if (data[i] < 215) gray++;
    }
    return { dark, ratio: gray / dark };
  }

  it("is drawn at the screen's scale, crisper than enlarged text", () => {
    const text: NewShape = {
      kind: "text",
      at: { x: 4, y: 4 },
      text: "Hamburgefonstiv 0123",
      color: "#000000",
      fontSize: 13,
    };
    const native = render([text], undefined, white(400, 60), 2);

    const at1x = render(
      [{ ...text, at: { x: 2, y: 2 } }],
      undefined,
      white(200, 30),
      1,
    );
    const small = makeCanvas(200, 30);
    const image = small.ctx.createImageData(200, 30);
    image.data.set(at1x);
    small.ctx.putImageData(image, 0, 0);
    const enlarged = makeCanvas(400, 60);
    enlarged.ctx.imageSmoothingEnabled = true;
    enlarged.ctx.drawImage(small.canvas, 0, 0, 400, 60);
    const upscaled = enlarged.ctx.getImageData(0, 0, 400, 60).data;

    const crisp = softness(native);
    const soft = softness(upscaled);
    expect(crisp.dark).toBeGreaterThan(400);
    expect(crisp.ratio).toBeLessThan(soft.ratio * 0.6);
  });

  it("matches its golden at scale 2", async () => {
    const text: NewShape = {
      kind: "text",
      at: { x: 6, y: 6 },
      text: "Retina text 13 pt",
      color: "#1c1c1e",
      fontSize: 13,
    };
    await golden(
      "text-2x",
      render([text], undefined, white(320, 50), 2),
      320,
      50,
    );
  });
});

describe("arrow", () => {
  it("puts the head at the end and scales it with the stroke", () => {
    const parts = arrowParts({ x: 0, y: 0 }, { x: 100, y: 0 }, 3, 2)!;
    expect(parts.tip).toEqual({ x: 100, y: 0 });
    expect(parts.left.x).toBeCloseTo(70);
    expect(Math.abs(parts.left.y - parts.right.y)).toBeCloseTo(30);
    expect(parts.shaftEnd.x).toBeGreaterThan(70);
    expect(parts.shaftEnd.x).toBeLessThan(100);

    const short = arrowParts({ x: 0, y: 0 }, { x: 10, y: 0 }, 3, 2)!;
    expect(short.left.x).toBeCloseTo(0);
    expect(arrowParts({ x: 5, y: 5 }, { x: 5, y: 5 }, 3, 2)).toBe(null);
  });
});
