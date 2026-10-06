import { describe, expect, it } from "vitest";

import {
  deviceTransform,
  fitZoom,
  layoutView,
  MARGIN,
  scrollForZoom,
  stepZoom,
  toImage,
  toView,
  zoomPercent,
  type ViewMap,
} from "@/markup/view";

describe("zoom", () => {
  it("fits large screenshots and shows small ones at their size in points", () => {
    const viewport = { width: 1000 + 2 * MARGIN, height: 600 + 2 * MARGIN };
    expect(fitZoom({ width: 6016, height: 3384 }, viewport, 2)).toBeCloseTo(
      1000 / 6016,
    );
    expect(fitZoom({ width: 400, height: 300 }, viewport, 2)).toBe(0.5);
    expect(fitZoom({ width: 400, height: 300 }, viewport, 1)).toBe(1);
  });

  it("steps through levels in points", () => {
    expect(stepZoom(0.5, 1, 2)).toBe(1.25 / 2);
    expect(stepZoom(0.5, -1, 2)).toBe(0.75 / 2);
    expect(stepZoom(0.3, 1, 2)).toBe(2 / 3 / 2);
    expect(stepZoom(4, 1, 2)).toBe(4);
    expect(zoomPercent(0.5, 2)).toBe(100);
  });
});

describe("view mapping", () => {
  it("centers a picture smaller than the viewport", () => {
    const layout = layoutView(
      { width: 400, height: 300 },
      0.5,
      { width: 800, height: 600 },
      { x: 50, y: 50 },
    );
    expect(layout).toEqual({
      contentWidth: 800,
      contentHeight: 600,
      offsetX: 300,
      offsetY: 225,
    });
  });

  it("scrolls a picture larger than the viewport", () => {
    const layout = layoutView(
      { width: 6016, height: 3384 },
      0.5,
      { width: 800, height: 600 },
      { x: 100, y: 40 },
    );
    expect(layout).toEqual({
      contentWidth: 3008 + 2 * MARGIN,
      contentHeight: 1692 + 2 * MARGIN,
      offsetX: MARGIN - 100,
      offsetY: MARGIN - 40,
    });
  });

  it("maps view points to screenshot pixels and back, cropped and zoomed", () => {
    // A Retina screenshot cropped at (1000, 500), shown at 25% of its pixels.
    const map: ViewMap = {
      zoom: 0.25,
      offsetX: 40,
      offsetY: 30,
      originX: 1000,
      originY: 500,
    };
    expect(toImage(map, { x: 40, y: 30 })).toEqual({ x: 1000, y: 500 });
    expect(toImage(map, { x: 140, y: 80 })).toEqual({ x: 1400, y: 700 });
    expect(toView(map, { x: 1400, y: 700 })).toEqual({ x: 140, y: 80 });
  });

  it("draws screenshot pixels at device pixels on Retina", () => {
    const map: ViewMap = {
      zoom: 0.5,
      offsetX: 10,
      offsetY: 20,
      originX: 100,
      originY: 0,
    };
    const [a, b, c, d, e, f] = deviceTransform(map, 2);
    const at = (p: { x: number; y: number }) => ({
      x: a * p.x + c * p.y + e,
      y: b * p.x + d * p.y + f,
    });
    const view = toView(map, { x: 300, y: 50 });
    expect(at({ x: 300, y: 50 })).toEqual({ x: view.x * 2, y: view.y * 2 });
    expect(a).toBe(1);
  });

  it("keeps the point under the pointer while zooming", () => {
    const viewport = { width: 800, height: 600 };
    const doc = { width: 6000, height: 4000 };
    const scroll = { x: 500, y: 300 };
    const before = layoutView(doc, 0.25, viewport, scroll);
    const map: ViewMap = { zoom: 0.25, ...before, originX: 0, originY: 0 };
    const anchor = { x: 200, y: 150 };
    const target = toImage(map, anchor);

    const nextScroll = scrollForZoom(map, anchor, 0.5);
    const after = layoutView(doc, 0.5, viewport, nextScroll);
    const next: ViewMap = { zoom: 0.5, ...after, originX: 0, originY: 0 };
    expect(toImage(next, anchor).x).toBeCloseTo(target.x);
    expect(toImage(next, anchor).y).toBeCloseTo(target.y);
  });
});
