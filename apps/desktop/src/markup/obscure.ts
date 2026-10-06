import type { Rect } from "@/markup/model";

/** Straight RGBA pixels, tightly packed. */
export type Pixels = {
  width: number;
  height: number;
  data: Uint8ClampedArray | Uint8Array;
};

/** Size of the blocks blur and pixelate average, in points. */
export const BLOCK_POINTS = 12;

export const blockSize = (scale: number) =>
  Math.max(4, Math.round(BLOCK_POINTS * scale));

/** `rect` on whole pixels and inside `size`; empty when it lies outside. */
export function pixelRect(
  rect: Rect,
  size: { width: number; height: number },
): Rect {
  const left = Math.max(0, Math.floor(rect.x));
  const top = Math.max(0, Math.floor(rect.y));
  const right = Math.min(size.width, Math.ceil(rect.x + rect.width));
  const bottom = Math.min(size.height, Math.ceil(rect.y + rect.height));
  return {
    x: left,
    y: top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  };
}

/** The cell each of `count` pixels falls in. */
function cellIndex(count: number, block: number) {
  const index = new Int32Array(count);
  for (let i = 0; i < count; i++) index[i] = Math.floor(i / block);
  return index;
}

/**
 * Averages of `block × block` cells of `area`, aligned to its corner: premultiplied
 * red, green and blue, then alpha, each in 0–255.
 */
function cells(source: Pixels, area: Rect, block: number) {
  const columns = Math.ceil(area.width / block);
  const rows = Math.ceil(area.height / block);
  const sums = new Float64Array(columns * rows * 4);
  const counts = new Float64Array(columns * rows);
  const column = cellIndex(area.width, block);
  const data = source.data;
  for (let y = 0; y < area.height; y++) {
    const row = Math.floor(y / block) * columns;
    let i = ((area.y + y) * source.width + area.x) * 4;
    for (let x = 0; x < area.width; x++, i += 4) {
      const cell = row + column[x];
      const a = data[i + 3] / 255;
      const s = cell * 4;
      sums[s] += data[i] * a;
      sums[s + 1] += data[i + 1] * a;
      sums[s + 2] += data[i + 2] * a;
      sums[s + 3] += data[i + 3];
      counts[cell] += 1;
    }
  }
  for (let cell = 0; cell < counts.length; cell++) {
    for (let c = 0; c < 4; c++) sums[cell * 4 + c] /= counts[cell];
  }
  return { columns, rows, values: sums };
}

/** Each cell averaged with its neighbours, weighted 1-2-1 on both axes. */
function smooth(values: Float64Array, columns: number, rows: number) {
  const pass = (input: Float64Array, dc: number, dr: number) => {
    const output = new Float64Array(input.length);
    for (let row = 0; row < rows; row++) {
      for (let column = 0; column < columns; column++) {
        const at = (step: number) => {
          const c = Math.min(Math.max(column + step * dc, 0), columns - 1);
          const r = Math.min(Math.max(row + step * dr, 0), rows - 1);
          return (r * columns + c) * 4;
        };
        const prev = at(-1);
        const here = at(0);
        const next = at(1);
        for (let ch = 0; ch < 4; ch++) {
          output[here + ch] =
            (input[prev + ch] + 2 * input[here + ch] + input[next + ch]) / 4;
        }
      }
    }
    return output;
  };
  return pass(pass(values, 1, 0), 0, 1);
}

/** The two cells around pixel `position` and the weight of the second. */
function along(position: number, count: number, block: number) {
  const g = Math.min(Math.max((position + 0.5) / block - 0.5, 0), count - 1);
  const i0 = Math.floor(g);
  return { i0, i1: Math.min(i0 + 1, count - 1), t: g - i0 };
}

/** Writes premultiplied `r, g, b, a` as a straight pixel at `o`. */
function writePixel(
  out: Uint8ClampedArray,
  o: number,
  r: number,
  g: number,
  b: number,
  a: number,
) {
  const k = a > 0 ? 255 / a : 0;
  out[o] = r * k;
  out[o + 1] = g * k;
  out[o + 2] = b * k;
  out[o + 3] = a;
}

/**
 * The pixels of `rect` in `source` blurred or pixelated, as straight RGBA of the
 * rect's whole-pixel size. Only `block`-sized cell averages of the source reach the
 * output, so the detail under it is gone, not just softened.
 */
export function obscure(
  source: Pixels,
  rect: Rect,
  kind: "blur" | "pixelate",
  block: number,
): Uint8ClampedArray {
  const area = pixelRect(rect, source);
  const { width, height } = area;
  const out = new Uint8ClampedArray(width * height * 4);
  if (width === 0 || height === 0) return out;
  const grid = cells(source, area, block);
  const { columns, rows } = grid;
  const stride = width * 4;

  if (kind === "pixelate") {
    const column = cellIndex(width, block);
    const v = grid.values;
    for (let y = 0; y < height; y++) {
      if (y % block !== 0) {
        out.copyWithin(y * stride, (y - 1) * stride, y * stride);
        continue;
      }
      const row = (y / block) * columns;
      for (let x = 0; x < width; x++) {
        const c = (row + column[x]) * 4;
        writePixel(out, y * stride + x * 4, v[c], v[c + 1], v[c + 2], v[c + 3]);
      }
    }
    return out;
  }

  const values = smooth(smooth(grid.values, columns, rows), columns, rows);
  // Interpolated across each row of cells first, then down between two rows.
  const across = new Float64Array(rows * stride);
  for (let x = 0; x < width; x++) {
    const h = along(x, columns, block);
    for (let row = 0; row < rows; row++) {
      const a = (row * columns + h.i0) * 4;
      const b = (row * columns + h.i1) * 4;
      const o = row * stride + x * 4;
      for (let ch = 0; ch < 4; ch++) {
        across[o + ch] =
          values[a + ch] + (values[b + ch] - values[a + ch]) * h.t;
      }
    }
  }
  for (let y = 0; y < height; y++) {
    const v = along(y, rows, block);
    const top = v.i0 * stride;
    const bottom = v.i1 * stride;
    const t = v.t;
    for (let i = 0; i < stride; i += 4) {
      const p = top + i;
      const q = bottom + i;
      writePixel(
        out,
        y * stride + i,
        across[p] + (across[q] - across[p]) * t,
        across[p + 1] + (across[q + 1] - across[p + 1]) * t,
        across[p + 2] + (across[q + 2] - across[p + 2]) * t,
        across[p + 3] + (across[q + 3] - across[p + 3]) * t,
      );
    }
  }
  return out;
}
