import type { CaptureTarget, DisplayInfo, WindowInfo } from "@/bindings";

export type SourceKind = "display" | "window" | "region";

export type RegionInput = {
  x: string;
  y: string;
  width: string;
  height: string;
};

export type SourceForm = {
  kind: SourceKind;
  displayId: number | null;
  windowId: number | null;
  region: RegionInput;
};

export type TargetResult =
  { ok: true; target: CaptureTarget } | { ok: false; error: string };

function parseNumber(value: string): number | null {
  if (value.trim() === "") return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

export function buildTarget(
  form: SourceForm,
  displays: DisplayInfo[],
): TargetResult {
  switch (form.kind) {
    case "display":
      return form.displayId === null
        ? { ok: false, error: "Pick a display." }
        : { ok: true, target: { kind: "display", displayId: form.displayId } };
    case "window":
      return form.windowId === null
        ? { ok: false, error: "Pick a window." }
        : { ok: true, target: { kind: "window", windowId: form.windowId } };
    case "region": {
      const display = displays.find((d) => d.id === form.displayId);
      if (!display) return { ok: false, error: "Pick a display." };
      const x = parseNumber(form.region.x);
      const y = parseNumber(form.region.y);
      const width = parseNumber(form.region.width);
      const height = parseNumber(form.region.height);
      if (x === null || y === null || width === null || height === null) {
        return {
          ok: false,
          error: "Enter numbers for x, y, width and height.",
        };
      }
      if (x < 0 || y < 0 || width < 16 || height < 16) {
        return {
          ok: false,
          error: "x and y must be ≥ 0; width and height at least 16.",
        };
      }
      const maxWidth = display.bounds.width ?? 0;
      const maxHeight = display.bounds.height ?? 0;
      if (x + width > maxWidth || y + height > maxHeight) {
        return {
          ok: false,
          error: `The region must fit inside the ${maxWidth}×${maxHeight} display.`,
        };
      }
      return {
        ok: true,
        target: {
          kind: "region",
          displayId: display.id,
          rect: { x, y, width, height },
        },
      };
    }
  }
}

export function formatElapsed(ms: number | null): string {
  const total = Math.max(0, Math.floor((ms ?? 0) / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function windowLabel(window: WindowInfo): string {
  if (!window.title) return window.appName;
  if (!window.appName) return window.title;
  return `${window.appName} — ${window.title}`;
}
