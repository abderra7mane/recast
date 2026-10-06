import type { Point, Size } from "@/markup/model";

/** Space kept around the picture in the view, in CSS pixels. */
export const MARGIN = 32;

/** Zoom levels in percent of the screenshot's size in points. */
const LEVELS = [
  0.05,
  0.1,
  0.25,
  1 / 3,
  0.5,
  2 / 3,
  0.75,
  1,
  1.25,
  1.5,
  2,
  3,
  4,
  6,
  8,
];

/**
 * The zoom (CSS pixels per screenshot pixel) that fits `doc` in `viewport`, never
 * above 100%: one screenshot point per CSS pixel.
 */
export function fitZoom(doc: Size, viewport: Size, scale: number): number {
  const width = Math.max(viewport.width - 2 * MARGIN, 1);
  const height = Math.max(viewport.height - 2 * MARGIN, 1);
  return Math.min(width / doc.width, height / doc.height, 1 / scale);
}

/** The next zoom level in `direction` (+1 in, -1 out) from `zoom`. */
export function stepZoom(zoom: number, direction: 1 | -1, scale: number) {
  const percent = zoom * scale;
  const next =
    direction > 0
      ? LEVELS.find((level) => level > percent * 1.001)
      : [...LEVELS].reverse().find((level) => level < percent / 1.001);
  return (next ?? (direction > 0 ? LEVELS.at(-1)! : LEVELS[0])) / scale;
}

export const zoomPercent = (zoom: number, scale: number) =>
  Math.round(zoom * scale * 100);

/** Where the picture sits in the scrolled view. */
export type ViewLayout = {
  /** Size of the scrolled content, in CSS pixels. */
  contentWidth: number;
  contentHeight: number;
  /** The picture's top-left corner in the viewport, in CSS pixels. */
  offsetX: number;
  offsetY: number;
};

/** Centers a picture smaller than the viewport, or scrolls a larger one. */
export function layoutView(
  doc: Size,
  zoom: number,
  viewport: Size,
  scroll: Point,
): ViewLayout {
  const axis = (picture: number, view: number, scrolled: number) => {
    const content = picture + 2 * MARGIN;
    return content <= view
      ? { content: view, offset: (view - picture) / 2 }
      : { content, offset: MARGIN - scrolled };
  };
  const x = axis(doc.width * zoom, viewport.width, scroll.x);
  const y = axis(doc.height * zoom, viewport.height, scroll.y);
  return {
    contentWidth: x.content,
    contentHeight: y.content,
    offsetX: x.offset,
    offsetY: y.offset,
  };
}

/** Maps between the viewport (CSS pixels) and the screenshot (its pixels). */
export type ViewMap = {
  zoom: number;
  offsetX: number;
  offsetY: number;
  /** The screenshot point drawn at the picture's top-left corner. */
  originX: number;
  originY: number;
};

export const toImage = (map: ViewMap, p: Point): Point => ({
  x: (p.x - map.offsetX) / map.zoom + map.originX,
  y: (p.y - map.offsetY) / map.zoom + map.originY,
});

export const toView = (map: ViewMap, p: Point): Point => ({
  x: (p.x - map.originX) * map.zoom + map.offsetX,
  y: (p.y - map.originY) * map.zoom + map.offsetY,
});

/** The canvas transform from screenshot pixels to device pixels. */
export function deviceTransform(
  map: ViewMap,
  pixelRatio: number,
): [number, number, number, number, number, number] {
  const k = map.zoom * pixelRatio;
  return [
    k,
    0,
    0,
    k,
    (map.offsetX - map.originX * map.zoom) * pixelRatio,
    (map.offsetY - map.originY * map.zoom) * pixelRatio,
  ];
}

/**
 * The scroll position that keeps the screenshot point under `anchor` (viewport CSS
 * pixels) in place when zooming to `nextZoom`.
 */
export function scrollForZoom(
  map: ViewMap,
  anchor: Point,
  nextZoom: number,
): Point {
  const ratio = nextZoom / map.zoom;
  return {
    x: MARGIN + (anchor.x - map.offsetX) * ratio - anchor.x,
    y: MARGIN + (anchor.y - map.offsetY) * ratio - anchor.y,
  };
}
