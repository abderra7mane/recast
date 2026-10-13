import type {
  Annotation,
  Annotations,
  AnnotationShape,
} from "@/editor/settings";
import {
  MIN_SEGMENT_MS,
  moveSegment,
  resizeSegment,
  type Span,
} from "@/editor/timeline-math";
import {
  DEFAULT_STYLE,
  restyle,
  styleOf,
  type NewShape,
  type Shape,
  type Style,
  type Tool,
} from "@/markup/model";

/** How long a new annotation shows, from the playhead. */
export const DEFAULT_DURATION_MS = 3000;
export const MIN_ANNOTATION_MS = MIN_SEGMENT_MS;
/** Must match `recast_project::MAX_TRACKS`. */
export const MAX_TRACKS = 32;
/** Cell size of blur and pixelate, in points; must match `recast_project::DEFAULT_BLOCK`. */
export const DEFAULT_BLOCK = 12;
export const MIN_BLOCK = 4;
export const MAX_BLOCK = 48;

export type AnnotationTool = Exclude<Tool, "crop">;
export type AnnotationStyle = Style & { block: number };

export const DEFAULT_ANNOTATION_STYLE: AnnotationStyle = {
  ...DEFAULT_STYLE,
  block: DEFAULT_BLOCK,
};

type Obscure = Extract<AnnotationShape, { kind: "blur" | "pixelate" }>;

export const isObscure = (shape: { kind: string }): shape is Obscure =>
  shape.kind === "blur" || shape.kind === "pixelate";

/** The annotation as a screenshot markup shape, in points; blur keeps its block. */
export const asShape = (annotation: Annotation): Shape =>
  ({ ...annotation.shape, id: annotation.id }) as Shape;

/** A markup shape as an annotation shape; blur and pixelate get `block` unless they have one. */
export function fromShape(
  shape: Shape | NewShape,
  block: number,
): AnnotationShape {
  const copy = { ...shape } as NewShape & { id?: string; block?: number };
  delete copy.id;
  if (isObscure(copy))
    return { ...copy, block: copy.block ?? block } as Obscure;
  return copy as AnnotationShape;
}

/** `shape` with the parts of `style` that apply to it, cell size included. */
export function restyleShape(
  shape: AnnotationShape,
  style: Partial<AnnotationStyle>,
): AnnotationShape {
  if (isObscure(shape)) {
    return style.block === undefined ? shape : { ...shape, block: style.block };
  }
  return fromShape(
    restyle({ ...shape, id: "" } as Shape, style),
    DEFAULT_BLOCK,
  );
}

/** The style a selected annotation shows in the sidebar. */
export function annotationStyle(
  shape: AnnotationShape,
  fallback: AnnotationStyle,
): AnnotationStyle {
  if (isObscure(shape)) return { ...fallback, block: shape.block };
  return {
    ...styleOf({ ...shape, id: "" } as Shape, fallback),
    block: fallback.block,
  };
}

export const visibleAt = (span: Span, tMs: number) =>
  span.startMs <= tMs && tMs <= span.endMs;

const byStart = (a: Annotation, b: Annotation) => a.startMs - b.startMs;

/**
 * Annotations in drawing order, as the compositor draws them: later tracks over earlier
 * ones. A blurred or pixelated area also hides the annotations under it.
 */
export const stackOrder = (items: Annotation[]): Annotation[] =>
  [...items].sort((a, b) => a.track - b.track || byStart(a, b));

export const onTrack = (items: Annotation[], track: number) =>
  items.filter((a) => a.track === track).sort(byStart);

const clamp = (value: number, min: number, max: number) =>
  Math.min(Math.max(value, min), max);

/** The free time around `tMs` on `track`, or `null` when an annotation shows then. */
export function gapAt(
  items: Annotation[],
  track: number,
  tMs: number,
  durationMs: number,
  except?: string,
): { startMs: number; endMs: number } | null {
  const others = onTrack(items, track).filter((a) => a.id !== except);
  if (others.some((a) => visibleAt(a, tMs))) return null;
  return {
    startMs: Math.max(
      0,
      ...others.filter((a) => a.endMs < tMs).map((a) => a.endMs),
    ),
    endMs: Math.min(
      durationMs,
      ...others.filter((a) => a.startMs > tMs).map((a) => a.startMs),
    ),
  };
}

/**
 * Where a new annotation drawn at `tMs` goes: from `tMs` for up to
 * [`DEFAULT_DURATION_MS`] on `preferred` or the first other track with room, or on a
 * new track after the others. `null` when every track is full and no more can be added.
 */
export function placeAnnotation(
  annotations: Annotations,
  tMs: number,
  durationMs: number,
  preferred: number,
): { track: number; startMs: number; endMs: number } | null {
  const tracks = Array.from({ length: annotations.tracks }, (_, i) => i);
  const order = tracks.includes(preferred)
    ? [preferred, ...tracks.filter((t) => t !== preferred)]
    : tracks;
  const fit = (startMs: number, endMs: number) => {
    const length = Math.min(DEFAULT_DURATION_MS, endMs - startMs);
    const start = clamp(tMs, startMs, endMs - length);
    return { startMs: start, endMs: start + length };
  };
  for (const track of order) {
    const gap = gapAt(annotations.items, track, tMs, durationMs);
    if (gap && gap.endMs - gap.startMs >= MIN_ANNOTATION_MS) {
      return { track, ...fit(gap.startMs, gap.endMs) };
    }
  }
  if (annotations.tracks >= MAX_TRACKS) return null;
  return { track: annotations.tracks, ...fit(0, durationMs) };
}

const replaceItem = (items: Annotation[], annotation: Annotation) =>
  items.map((a) => (a.id === annotation.id ? annotation : a));

/**
 * Moves annotation `id` of `items` (as they were when the drag began) by `deltaMs`.
 * Over another track it goes there when it fits in the gap under it; on its own track it
 * stops at its neighbors.
 */
export function moveAnnotation(
  items: Annotation[],
  id: string,
  deltaMs: number,
  track: number,
  durationMs: number,
): Annotation[] {
  const annotation = items.find((a) => a.id === id);
  if (!annotation) return items;
  const length = annotation.endMs - annotation.startMs;
  if (track !== annotation.track && track >= 0) {
    const startMs = annotation.startMs + deltaMs;
    const center = clamp(startMs + length / 2, 0, durationMs);
    const gap = gapAt(items, track, center, durationMs, id);
    if (gap && gap.endMs - gap.startMs >= length) {
      const start = clamp(startMs, gap.startMs, gap.endMs - length);
      return replaceItem(items, {
        ...annotation,
        track,
        startMs: start,
        endMs: start + length,
      });
    }
  }
  const own = onTrack(items, annotation.track);
  const i = own.findIndex((a) => a.id === id);
  return replaceItem(items, moveSegment(own, i, deltaMs, durationMs)[i]);
}

/** Moves one edge of annotation `id` to `tMs`, keeping a minimum length and its neighbors. */
export function resizeAnnotation(
  items: Annotation[],
  id: string,
  edge: "start" | "end",
  tMs: number,
  durationMs: number,
): Annotation[] {
  const annotation = items.find((a) => a.id === id);
  if (!annotation) return items;
  const own = onTrack(items, annotation.track);
  const i = own.findIndex((a) => a.id === id);
  return replaceItem(items, resizeSegment(own, i, edge, tMs, durationMs)[i]);
}

/** Annotation `id` on `track` at the same times, or `null` when another one is in the way. */
export function moveToTrack(
  items: Annotation[],
  id: string,
  track: number,
): Annotation[] | null {
  const annotation = items.find((a) => a.id === id);
  if (!annotation) return null;
  const blocked = items.some(
    (a) =>
      a.id !== id &&
      a.track === track &&
      a.startMs < annotation.endMs &&
      a.endMs > annotation.startMs,
  );
  return blocked ? null : replaceItem(items, { ...annotation, track });
}

/** Removes `track` and its annotations; the tracks after it move up. One track always stays. */
export function deleteTrack(
  annotations: Annotations,
  track: number,
): Annotations {
  return {
    tracks: Math.max(1, annotations.tracks - 1),
    items: annotations.items
      .filter((a) => a.track !== track)
      .map((a) => (a.track > track ? { ...a, track: a.track - 1 } : a)),
  };
}

let nextId = 1;

/** An id no other annotation of the project has. */
export const newAnnotationId = () =>
  `a${Date.now().toString(36)}${(nextId++).toString(36)}`;
