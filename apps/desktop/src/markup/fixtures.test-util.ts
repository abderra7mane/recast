import type { Pixels } from "@/markup/obscure";

/** One-pixel black and white checks: the finest detail an image can have. */
export function checkerboard(width: number, height: number): Pixels {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const v = (x + y) % 2 === 0 ? 0 : 255;
      data.set([v, v, v, 255], (y * width + x) * 4);
    }
  }
  return { width, height, data };
}

/** Variance of the red channel. */
export function variance(data: ArrayLike<number>): number {
  let sum = 0;
  let sum2 = 0;
  const n = data.length / 4;
  for (let i = 0; i < data.length; i += 4) {
    sum += data[i];
    sum2 += data[i] * data[i];
  }
  const mean = sum / n;
  return sum2 / n - mean * mean;
}
