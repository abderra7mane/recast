import { describe, expect, it } from "vitest";

import { segment } from "@/editor/fixtures.test-util";
import {
  MIN_SEGMENT_MS,
  addSegment,
  deleteSegment,
  fitScale,
  moveSegment,
  moveTrim,
  pxToTime,
  resizeSegment,
  rulerTicks,
  timeToPx,
  zoomScale,
} from "@/editor/timeline-math";

const ranges = (segments: { startMs: number; endMs: number }[]) =>
  segments.map((s) => [s.startMs, s.endMs]);

describe("time and pixels", () => {
  it("maps both ways", () => {
    expect(timeToPx(2500, 0.1)).toBe(250);
    expect(pxToTime(250, 0.1, 10_000)).toBe(2500);
    expect(pxToTime(-20, 0.1, 10_000)).toBe(0);
    expect(pxToTime(5000, 0.1, 10_000)).toBe(10_000);
    for (const t of [0, 123, 9999]) {
      expect(pxToTime(timeToPx(t, 0.37), 0.37, 10_000)).toBeCloseTo(t, 9);
    }
  });

  it("fits and zooms within limits", () => {
    expect(fitScale(1000, 10_000)).toBe(0.1);
    expect(fitScale(1000, 0)).toBeGreaterThan(0);
    expect(zoomScale(0.1, 2)).toBe(0.2);
    expect(zoomScale(1.5, 2)).toBe(2);
    expect(zoomScale(0.006, 0.5)).toBe(0.005);
  });

  it("spaces ruler ticks", () => {
    const { stepMs, ticks } = rulerTicks(0.1, 10_000, 80);
    expect(stepMs).toBe(1000);
    expect(ticks).toHaveLength(11);
    expect(rulerTicks(0.01, 60_000, 80).stepMs).toBe(10_000);
  });
});

describe("segments", () => {
  const segments = [
    segment(1000, 2000),
    segment(4000, 5000),
    segment(8000, 9000),
  ];

  it("moves without overlapping neighbors or leaving the recording", () => {
    expect(ranges(moveSegment(segments, 1, 500, 10_000))[1]).toEqual([
      4500, 5500,
    ]);
    expect(ranges(moveSegment(segments, 1, -5000, 10_000))[1]).toEqual([
      2000, 3000,
    ]);
    expect(ranges(moveSegment(segments, 1, 5000, 10_000))[1]).toEqual([
      7000, 8000,
    ]);
    expect(ranges(moveSegment(segments, 0, -5000, 10_000))[0]).toEqual([
      0, 1000,
    ]);
    expect(ranges(moveSegment(segments, 2, 5000, 10_000))[2]).toEqual([
      9000, 10_000,
    ]);
    expect(moveSegment(segments, 1, 500, 10_000)[0]).toBe(segments[0]);
    expect(moveSegment(segments, 7, 500, 10_000)).toBe(segments);
  });

  it("resizes each edge with a minimum length", () => {
    expect(
      ranges(resizeSegment(segments, 1, "start", 3000, 10_000))[1],
    ).toEqual([3000, 5000]);
    expect(ranges(resizeSegment(segments, 1, "start", 0, 10_000))[1]).toEqual([
      2000, 5000,
    ]);
    expect(
      ranges(resizeSegment(segments, 1, "start", 4900, 10_000))[1],
    ).toEqual([5000 - MIN_SEGMENT_MS, 5000]);
    expect(ranges(resizeSegment(segments, 1, "end", 9500, 10_000))[1]).toEqual([
      4000, 8000,
    ]);
    expect(ranges(resizeSegment(segments, 1, "end", 4000, 10_000))[1]).toEqual([
      4000,
      4000 + MIN_SEGMENT_MS,
    ]);
    expect(
      ranges(resizeSegment(segments, 2, "end", 99_000, 10_000))[2],
    ).toEqual([8000, 10_000]);
  });

  it("adds a segment in free space, fitted to the gap", () => {
    const added = addSegment(segments, 3000, 10_000, 2.5)!;
    expect(added.index).toBe(1);
    expect(ranges(added.segments)).toEqual([
      [1000, 2000],
      [2000, 4000],
      [4000, 5000],
      [8000, 9000],
    ]);
    expect(added.segments[1]).toMatchObject({
      level: 2.5,
      focus: { kind: "followCursor" },
    });

    const atEnd = addSegment(segments, 9800, 10_000, 2)!;
    expect(atEnd.index).toBe(3);
    expect(ranges(atEnd.segments)[3]).toEqual([9000, 10_000]);

    const centered = addSegment([], 5000, 10_000, 2)!;
    expect(ranges(centered.segments)).toEqual([[4000, 6000]]);
    expect(ranges(addSegment([], 100, 10_000, 2)!.segments)).toEqual([
      [0, 2000],
    ]);
  });

  it("refuses to add inside a segment or in a tiny gap", () => {
    expect(addSegment(segments, 1500, 10_000, 2)).toBeNull();
    expect(
      addSegment([segment(0, 1000), segment(1100, 2000)], 1050, 10_000, 2),
    ).toBeNull();
    expect(addSegment(segments, 11_000, 10_000, 2)).toBeNull();
  });

  it("deletes a segment", () => {
    expect(ranges(deleteSegment(segments, 1))).toEqual([
      [1000, 2000],
      [8000, 9000],
    ]);
  });
});

describe("trim", () => {
  it("keeps the trimmed range inside the recording and not empty", () => {
    const trim = { startMs: 1000, endMs: 9000 };
    expect(moveTrim(trim, "start", 2000, 10_000)).toEqual({
      startMs: 2000,
      endMs: 9000,
    });
    expect(moveTrim(trim, "start", -50, 10_000)).toEqual({
      startMs: 0,
      endMs: 9000,
    });
    expect(moveTrim(trim, "start", 9500, 10_000)).toEqual({
      startMs: 8900,
      endMs: 9000,
    });
    expect(moveTrim(trim, "end", 20_000, 10_000)).toEqual({
      startMs: 1000,
      endMs: 10_000,
    });
    expect(moveTrim(trim, "end", 0, 10_000)).toEqual({
      startMs: 1000,
      endMs: 1100,
    });
  });
});
