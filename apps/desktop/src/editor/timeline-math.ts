import type { Segment } from "@/editor/settings";

export const MIN_SEGMENT_MS = 300;
export const NEW_SEGMENT_MS = 2000;
export const MIN_TRIM_MS = 100;
export const MIN_PX_PER_MS = 0.005;
export const MAX_PX_PER_MS = 2;

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

export const timeToPx = (tMs: number, pxPerMs: number) => tMs * pxPerMs;

export const pxToTime = (px: number, pxPerMs: number, durationMs: number) =>
  clamp(px / pxPerMs, 0, durationMs);

/** The scale that shows the whole recording in `widthPx`. */
export function fitScale(widthPx: number, durationMs: number): number {
  if (durationMs <= 0 || widthPx <= 0) return MIN_PX_PER_MS;
  return clamp(widthPx / durationMs, MIN_PX_PER_MS, MAX_PX_PER_MS);
}

export const zoomScale = (pxPerMs: number, factor: number) =>
  clamp(pxPerMs * factor, MIN_PX_PER_MS, MAX_PX_PER_MS);

const STEPS_MS = [
  100, 250, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000,
  300_000, 600_000,
];

/** Ruler tick times, at least `minSpacingPx` apart. */
export function rulerTicks(
  pxPerMs: number,
  durationMs: number,
  minSpacingPx = 80,
): { stepMs: number; ticks: number[] } {
  const stepMs =
    STEPS_MS.find((step) => step * pxPerMs >= minSpacingPx) ??
    STEPS_MS[STEPS_MS.length - 1];
  const ticks: number[] = [];
  for (let t = 0; t <= durationMs; t += stepMs) ticks.push(t);
  return { stepMs, ticks };
}

/** Free time around segment `i`: from the previous segment's end to the next one's start. */
function room(segments: Segment[], i: number, durationMs: number) {
  return {
    min: i > 0 ? segments[i - 1].endMs : 0,
    max: i < segments.length - 1 ? segments[i + 1].startMs : durationMs,
  };
}

const replace = (segments: Segment[], i: number, segment: Segment) =>
  segments.map((s, j) => (j === i ? segment : s));

/** Moves segment `i` by `deltaMs`, keeping its length and stopping at its neighbors. */
export function moveSegment(
  segments: Segment[],
  i: number,
  deltaMs: number,
  durationMs: number,
): Segment[] {
  const segment = segments[i];
  if (!segment) return segments;
  const { min, max } = room(segments, i, durationMs);
  const length = segment.endMs - segment.startMs;
  const startMs = clamp(
    segment.startMs + deltaMs,
    min,
    Math.max(min, max - length),
  );
  return replace(segments, i, { ...segment, startMs, endMs: startMs + length });
}

/** Moves one edge of segment `i` to `tMs`, keeping a minimum length and its neighbors. */
export function resizeSegment(
  segments: Segment[],
  i: number,
  edge: "start" | "end",
  tMs: number,
  durationMs: number,
): Segment[] {
  const segment = segments[i];
  if (!segment) return segments;
  const { min, max } = room(segments, i, durationMs);
  if (edge === "start") {
    const startMs = clamp(
      tMs,
      min,
      Math.max(min, segment.endMs - MIN_SEGMENT_MS),
    );
    return replace(segments, i, { ...segment, startMs });
  }
  const endMs = clamp(
    tMs,
    Math.min(max, segment.startMs + MIN_SEGMENT_MS),
    max,
  );
  return replace(segments, i, { ...segment, endMs });
}

/**
 * Adds a segment around `tMs` in the free gap there. Returns `null` when `tMs`
 * is inside a segment or the gap is too short.
 */
export function addSegment(
  segments: Segment[],
  tMs: number,
  durationMs: number,
  level: number,
): { segments: Segment[]; index: number } | null {
  if (tMs < 0 || tMs > durationMs) return null;
  if (segments.some((s) => tMs >= s.startMs && tMs < s.endMs)) return null;
  const index = segments.findIndex((s) => s.startMs > tMs);
  const at = index === -1 ? segments.length : index;
  const gapStart = at > 0 ? segments[at - 1].endMs : 0;
  const gapEnd = at < segments.length ? segments[at].startMs : durationMs;
  if (gapEnd - gapStart < MIN_SEGMENT_MS) return null;
  const length = Math.min(NEW_SEGMENT_MS, gapEnd - gapStart);
  const startMs = clamp(tMs - length / 2, gapStart, gapEnd - length);
  const segment: Segment = {
    startMs,
    endMs: startMs + length,
    level,
    focus: { kind: "followCursor" },
  };
  return {
    segments: [...segments.slice(0, at), segment, ...segments.slice(at)],
    index: at,
  };
}

export const deleteSegment = (segments: Segment[], i: number) =>
  segments.filter((_, j) => j !== i);

/** Moves a trim edge to `tMs`, keeping the trimmed range at least `MIN_TRIM_MS` long. */
export function moveTrim(
  trim: { startMs: number; endMs: number },
  edge: "start" | "end",
  tMs: number,
  durationMs: number,
): { startMs: number; endMs: number } {
  if (edge === "start") {
    return {
      ...trim,
      startMs: clamp(tMs, 0, Math.max(0, trim.endMs - MIN_TRIM_MS)),
    };
  }
  return {
    ...trim,
    endMs: clamp(
      tMs,
      Math.min(durationMs, trim.startMs + MIN_TRIM_MS),
      durationMs,
    ),
  };
}
