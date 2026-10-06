import type { Resolution } from "@/bindings";

type Size = { width: number; height: number };

const SHORT_SIDE: Record<Exclude<Resolution, "auto">, number> = {
  "1080p": 1080,
  "1440p": 1440,
  "4k": 2160,
};

/** Above this, the Export tab warns that a preset enlarges the recording. */
export const ENLARGE_WARNING = 1.05;

/** `padding` (a share of the shorter side) in whole video pixels, as the renderer rounds it. */
function paddingPixels(video: Size, padding: number) {
  const share = Math.min(Math.max(padding, 0), 0.5);
  return Math.round(share * Math.min(video.width, video.height));
}

/** The export size at Auto: the recording at its own size plus padding, made even. */
export function autoSize(video: Size, padding: number): [number, number] {
  const pad = paddingPixels(video, padding);
  const even = (n: number) => n + (n % 2);
  return [even(video.width + 2 * pad), even(video.height + 2 * pad)];
}

/** How much `resolution` enlarges the recording when nothing is zoomed in. */
export function enlargement(
  video: Size,
  padding: number,
  resolution: Resolution,
): number {
  if (resolution === "auto") return 1;
  const short = Math.min(video.width, video.height);
  return SHORT_SIDE[resolution] / (short + 2 * paddingPixels(video, padding));
}
