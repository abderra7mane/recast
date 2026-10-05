import type { Picked, ScreenshotTaken } from "@/bindings";

export function formatElapsed(ms: number | null): string {
  const total = Math.max(0, Math.floor((ms ?? 0) / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

/** "Window: Safari  1280 × 800" */
export function pickedLabel(picked: Picked): string {
  const kind = { display: "Display", window: "Window", region: "Area" }[
    picked.target.kind
  ];
  return `${kind}: ${picked.label}`;
}

/** What happened to a screenshot, for the main window. */
export function screenshotMessage(taken: ScreenshotTaken): string {
  const size = `${taken.width} × ${taken.height}`;
  const done = [
    taken.path ? `saved to ${taken.path}` : null,
    taken.copied ? "copied to the clipboard" : null,
  ].filter(Boolean);
  if (done.length === 0) return `Captured ${size}.`;
  return `Captured ${size}, ${done.join(" and ")}.`;
}
