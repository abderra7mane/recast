import { describe, expect, it } from "vitest";

import { autoSize, enlargement } from "@/editor/export-size";

describe("export size", () => {
  it("keeps the recording's pixels at Auto", () => {
    expect(autoSize({ width: 1368, height: 954 }, 0.08)).toEqual([1520, 1106]);
    expect(autoSize({ width: 301, height: 200 }, 0.1)).toEqual([342, 240]);
    expect(enlargement({ width: 1368, height: 954 }, 0.08, "auto")).toBe(1);
  });

  it("tells how much a preset enlarges", () => {
    const video = { width: 1368, height: 954 };
    expect(enlargement(video, 0.08, "4k")).toBeCloseTo(2160 / 1106, 5);
    expect(enlargement(video, 0, "1080p")).toBeCloseTo(1080 / 954, 5);
    const retina = { width: 3456, height: 2234 };
    expect(enlargement(retina, 0.08, "4k")).toBeLessThan(1);
    const portrait = { width: 800, height: 1200 };
    expect(enlargement(portrait, 0, "1440p")).toBeCloseTo(1.8, 5);
  });
});
