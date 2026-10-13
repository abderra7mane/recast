import { describe, expect, it } from "vitest";

import {
  annotationStyle,
  asShape,
  DEFAULT_ANNOTATION_STYLE,
  deleteTrack,
  fromShape,
  gapAt,
  MAX_TRACKS,
  moveAnnotation,
  moveToTrack,
  placeAnnotation,
  resizeAnnotation,
  restyleShape,
  stackOrder,
  visibleAt,
} from "@/editor/annotations/model";
import { annotation, box } from "@/editor/fixtures.test-util";
import type { AnnotationShape } from "@/editor/settings";

const blur: AnnotationShape = {
  kind: "blur",
  rect: { x: 0, y: 0, width: 10, height: 10 },
  block: 12,
};

describe("annotation shapes", () => {
  it("round-trip through markup shapes, keeping the cell size", () => {
    const a = annotation("a", 0, 1000, 0, blur);
    const shape = asShape(a);
    expect(shape.id).toBe("a");
    expect(fromShape(shape, 20)).toEqual(blur);
    expect(fromShape({ kind: "pixelate", rect: blur.rect }, 20)).toEqual({
      kind: "pixelate",
      rect: blur.rect,
      block: 20,
    });
    expect(fromShape(asShape(annotation("b", 0, 1, 0)), 12)).toEqual(box());
  });

  it("restyles only what applies", () => {
    expect(restyleShape(box(), { color: "#000000", fontSize: 40 })).toEqual({
      ...box(),
      color: "#000000",
    });
    expect(restyleShape(blur, { color: "#000000", block: 30 })).toEqual({
      ...blur,
      block: 30,
    });
    const style = annotationStyle(blur, DEFAULT_ANNOTATION_STYLE);
    expect(style.block).toBe(12);
    expect(annotationStyle(box(), { ...style, block: 7 }).width).toBe(3);
  });

  it("are visible from start to end, both included", () => {
    const a = annotation("a", 100, 200);
    expect([99, 100, 200, 201].map((t) => visibleAt(a, t))).toEqual([
      false,
      true,
      true,
      false,
    ]);
  });

  it("stack later tracks on top, blur included", () => {
    const items = [
      annotation("top", 0, 1, 2),
      annotation("hide", 0, 1, 2, blur),
      annotation("bottom", 0, 1, 0),
      annotation("hide-low", 0, 1, 0, blur),
    ];
    expect(stackOrder(items).map((a) => a.id)).toEqual([
      "bottom",
      "hide-low",
      "top",
      "hide",
    ]);
  });
});

describe("placing a new annotation", () => {
  const busy = {
    tracks: 2,
    items: [annotation("a", 0, 5000, 0), annotation("b", 6000, 7000, 1)],
  };

  it("starts at the playhead on the preferred track when it has room", () => {
    expect(placeAnnotation(busy, 2000, 10_000, 1)).toEqual({
      track: 1,
      startMs: 2000,
      endMs: 5000,
    });
  });

  it("shortens to the gap and keeps the playhead inside", () => {
    expect(placeAnnotation(busy, 5500, 10_000, 1)).toEqual({
      track: 1,
      startMs: 3000,
      endMs: 6000,
    });
    expect(placeAnnotation(busy, 9900, 10_000, 1)).toEqual({
      track: 1,
      startMs: 7000,
      endMs: 10_000,
    });
  });

  it("uses another track, then a new one", () => {
    expect(placeAnnotation(busy, 1000, 10_000, 0)?.track).toBe(1);
    const one = { tracks: 1, items: [annotation("a", 0, 5000, 0)] };
    expect(placeAnnotation(one, 1000, 10_000, 0)).toEqual({
      track: 1,
      startMs: 1000,
      endMs: 4000,
    });
    expect(
      placeAnnotation({ tracks: MAX_TRACKS, items: [] }, 0, 10_000, 0),
    ).not.toBeNull();
    const full = {
      tracks: MAX_TRACKS,
      items: Array.from({ length: MAX_TRACKS }, (_, i) =>
        annotation(`a${i}`, 0, 10_000, i),
      ),
    };
    expect(placeAnnotation(full, 100, 10_000, 0)).toBeNull();
  });

  it("finds the gap around a time", () => {
    expect(gapAt(busy.items, 1, 6500, 10_000)).toBeNull();
    expect(gapAt(busy.items, 1, 8000, 10_000)).toEqual({
      startMs: 7000,
      endMs: 10_000,
    });
    expect(gapAt(busy.items, 1, 6500, 10_000, "b")).toEqual({
      startMs: 0,
      endMs: 10_000,
    });
  });
});

describe("moving annotations on the timeline", () => {
  const items = [
    annotation("a", 1000, 2000, 0),
    annotation("b", 3000, 4000, 0),
    annotation("c", 1000, 3500, 1),
  ];

  it("stops at neighbors on its own track", () => {
    const moved = moveAnnotation(items, "a", 5000, 0, 10_000);
    expect(moved.find((a) => a.id === "a")).toMatchObject({
      startMs: 2000,
      endMs: 3000,
      track: 0,
    });
  });

  it("goes to another track when it fits in the gap there", () => {
    const moved = moveAnnotation(items, "b", 500, 1, 10_000);
    expect(moved.find((a) => a.id === "b")).toMatchObject({
      track: 1,
      startMs: 3500,
      endMs: 4500,
    });
    const blocked = moveAnnotation(items, "a", 0, 1, 10_000);
    expect(blocked.find((a) => a.id === "a")).toMatchObject({
      track: 0,
      startMs: 1000,
    });
  });

  it("resizes within its neighbors and a minimum length", () => {
    const longer = resizeAnnotation(items, "a", "end", 9000, 10_000);
    expect(longer.find((a) => a.id === "a")?.endMs).toBe(3000);
    const shorter = resizeAnnotation(items, "a", "end", 0, 10_000);
    expect(shorter.find((a) => a.id === "a")?.endMs).toBe(1300);
  });

  it("moves to a layer only when nothing is in the way", () => {
    expect(moveToTrack(items, "b", 1)).toBeNull();
    expect(moveToTrack(items, "b", 2)?.find((a) => a.id === "b")?.track).toBe(
      2,
    );
  });

  it("deletes a track and moves the later ones up", () => {
    expect(deleteTrack({ tracks: 3, items }, 0)).toEqual({
      tracks: 2,
      items: [annotation("c", 1000, 3500, 0)],
    });
    expect(deleteTrack({ tracks: 1, items: [items[0]] }, 0)).toEqual({
      tracks: 1,
      items: [],
    });
  });
});
