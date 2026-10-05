import { describe, expect, it } from "vitest";

import type { Picked, ScreenshotTaken } from "@/bindings";
import { formatElapsed, pickedLabel, screenshotMessage } from "@/recording";

describe("formatElapsed", () => {
  it("formats minutes and seconds", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(61_900)).toBe("1:01");
    expect(formatElapsed(null)).toBe("0:00");
  });
});

describe("pickedLabel", () => {
  const bounds = { x: 0, y: 0, width: 10, height: 10 };
  it("names the kind of target", () => {
    const window: Picked = {
      target: { kind: "window", windowId: 4 },
      displayId: 1,
      bounds,
      label: "Safari  1280 × 800",
    };
    expect(pickedLabel(window)).toBe("Window: Safari  1280 × 800");
    expect(
      pickedLabel({
        ...window,
        target: { kind: "region", displayId: 1, rect: bounds },
        label: "10 × 10",
      }),
    ).toBe("Area: 10 × 10");
  });
});

describe("screenshotMessage", () => {
  const taken: ScreenshotTaken = {
    path: "/Users/me/Pictures/Recast/Recast 1.png",
    width: 3024,
    height: 1964,
    copied: true,
    warnings: [],
  };
  it("says where the screenshot went", () => {
    expect(screenshotMessage(taken)).toBe(
      "Captured 3024 × 1964, saved to /Users/me/Pictures/Recast/Recast 1.png and copied to the clipboard.",
    );
    expect(screenshotMessage({ ...taken, path: null })).toBe(
      "Captured 3024 × 1964, copied to the clipboard.",
    );
    expect(screenshotMessage({ ...taken, path: null, copied: false })).toBe(
      "Captured 3024 × 1964.",
    );
  });
});
