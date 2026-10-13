import type {
  Annotation,
  AnnotationShape,
  Segment,
  Settings,
} from "@/editor/settings";

export const segment = (
  startMs: number,
  endMs: number,
  level = 2,
): Segment => ({
  startMs,
  endMs,
  level,
  focus: { kind: "followCursor" },
});

export function settings(): Settings {
  return {
    version: 1,
    background: {
      fill: { kind: "gradient", from: "#4f46e5", to: "#db2777", angleDeg: 135 },
      padding: 0.08,
      cornerRadius: 0.015,
      shadow: { opacity: 0.45, blur: 0.04, offsetY: 0.012 },
    },
    cursor: { size: 1.5, smoothing: 0.5, hideWhenIdle: false },
    zoom: { auto: true, level: 2, segments: [] },
    clicks: { ripple: true, color: "#ffffffd9", size: 26, squish: true },
    sounds: {
      enabled: true,
      pack: "mouseClick",
      volume: 0.6,
      separateLeftRight: false,
    },
    audio: { micVolume: 1, systemVolume: 1 },
    trim: { startMs: 0, endMs: 10_000 },
    export: { codec: "h264", resolution: "1080p", fps: "60", quality: 0.7 },
    annotations: { tracks: 1, items: [] },
  };
}

export const box = (
  x = 10,
  y = 20,
  width = 100,
  height = 50,
): AnnotationShape => ({
  kind: "rect",
  rect: { x, y, width, height },
  color: "#ff3b30",
  width: 3,
  fill: false,
});

export const annotation = (
  id: string,
  startMs: number,
  endMs: number,
  track = 0,
  shape: AnnotationShape = box(),
): Annotation => ({ id, startMs, endMs, track, shape });
