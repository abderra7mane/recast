import { invoke } from "@tauri-apps/api/core";

import { commands, type MarkupAction } from "@/bindings";
import { cropArea, isFull } from "@/markup/crop";
import type { Background, Doc, Size } from "@/markup/model";
import {
  createRenderer,
  renderOutput,
  type Ctx,
  type MakeCanvas,
  type Renderer,
} from "@/markup/render";

export const makeCanvas: MakeCanvas = (width, height) => {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext("2d") as Ctx;
  return { canvas, ctx };
};

/** Loads this window's screenshot and a renderer for it. */
export async function loadRenderer(
  image: Size,
  scale: number,
): Promise<Renderer> {
  const bytes = await invoke<ArrayBuffer>("markup_image");
  const pixels = new ImageData(
    new Uint8ClampedArray(bytes),
    image.width,
    image.height,
  );
  const base = await createImageBitmap(pixels);
  return createRenderer(base, pixels, scale, makeCanvas);
}

/** Padding Beautify puts around a `width × height` image, in whole pixels. */
export const paddingPixels = (size: Size, padding: number) =>
  Math.round(
    Math.min(Math.max(padding, 0), 0.5) * Math.min(size.width, size.height),
  );

/** Corner radius Beautify gives a `width × height` image at its own size, in pixels. */
export const cornerPixels = (size: Size, radius: number) => {
  const short = Math.min(size.width, size.height);
  return Math.min(radius * short, short / 2);
};

export const isEdited = (doc: Doc, image: Size) =>
  doc.shapes.length > 0 || (doc.crop !== null && !isFull(doc.crop, image));

let nextToken = 1;

/**
 * Sends the marked-up, cropped screenshot to the backend and carries out `action` with
 * it, beautified when Beautify is on. An unedited screenshot is sent as it was taken.
 * The image goes with a token the finish names, so it can't be mixed up with another.
 */
export async function finish(
  renderer: Renderer,
  doc: Doc,
  action: MarkupAction,
) {
  let token: number | null = null;
  if (isEdited(doc, renderer.image)) {
    token = nextToken++;
    const area = cropArea(doc.crop, renderer.image);
    const data = renderOutput(renderer, doc, area, makeCanvas);
    await invoke("markup_stage", new Uint8Array(data.buffer), {
      headers: {
        "x-recast-width": String(area.width),
        "x-recast-height": String(area.height),
        "x-recast-token": String(token),
      },
    });
  }
  const background: Background | null = doc.beautify ? doc.background : null;
  return commands.markupFinish(action, token, background);
}

/** `run`, ignoring calls made while an earlier one hasn't finished. */
export function oneAtATime<A extends unknown[]>(
  run: (...args: A) => Promise<void>,
) {
  let running = false;
  return async (...args: A) => {
    if (running) return;
    running = true;
    try {
      await run(...args);
    } finally {
      running = false;
    }
  };
}
