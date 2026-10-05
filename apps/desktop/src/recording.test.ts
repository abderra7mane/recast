import { describe, expect, it } from "vitest";

import type { DisplayInfo, WindowInfo } from "@/bindings";
import {
  buildTarget,
  formatElapsed,
  windowLabel,
  type SourceForm,
} from "@/recording";

const displays: DisplayInfo[] = [
  {
    id: 7,
    name: "Main display",
    bounds: { x: 0, y: 0, width: 1512, height: 982 },
    scaleFactor: 2,
  },
];

const base: SourceForm = {
  kind: "display",
  displayId: 7,
  windowId: null,
  region: { x: "0", y: "0", width: "800", height: "600" },
};

describe("buildTarget", () => {
  it("builds a display target", () => {
    expect(buildTarget(base, displays)).toEqual({
      ok: true,
      target: { kind: "display", displayId: 7 },
    });
  });

  it("requires a window", () => {
    expect(buildTarget({ ...base, kind: "window" }, displays).ok).toBe(false);
    expect(
      buildTarget({ ...base, kind: "window", windowId: 42 }, displays),
    ).toEqual({ ok: true, target: { kind: "window", windowId: 42 } });
  });

  it("parses a region", () => {
    expect(
      buildTarget(
        {
          ...base,
          kind: "region",
          region: { x: "10", y: "20.5", width: "300", height: "200" },
        },
        displays,
      ),
    ).toEqual({
      ok: true,
      target: {
        kind: "region",
        displayId: 7,
        rect: { x: 10, y: 20.5, width: 300, height: 200 },
      },
    });
  });

  it("rejects bad regions", () => {
    const region = (x: string, width: string) =>
      buildTarget(
        { ...base, kind: "region", region: { ...base.region, x, width } },
        displays,
      );
    expect(region("abc", "100").ok).toBe(false);
    expect(region("", "100").ok).toBe(false);
    expect(region("0", "8").ok).toBe(false);
    expect(region("1000", "800").ok).toBe(false);
    expect(
      buildTarget({ ...base, kind: "region", displayId: 1 }, displays).ok,
    ).toBe(false);
  });
});

describe("formatElapsed", () => {
  it("formats minutes and seconds", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(61_900)).toBe("1:01");
    expect(formatElapsed(null)).toBe("0:00");
  });
});

describe("windowLabel", () => {
  const w: WindowInfo = {
    id: 1,
    title: "Inbox",
    appName: "Mail",
    bounds: { x: 0, y: 0, width: 10, height: 10 },
  };
  it("joins app and title", () => {
    expect(windowLabel(w)).toBe("Mail — Inbox");
    expect(windowLabel({ ...w, title: "" })).toBe("Mail");
  });
});
