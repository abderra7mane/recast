import type { EditSettings, ZoomSegment } from "@/bindings";

/**
 * The generated types mark fields with serde defaults as optional and Rust `f64`
 * as `number | null`. Rust always sends complete, sanitized settings, so the
 * editor works with this complete form.
 */
export type Complete<T> = T extends (infer U)[]
  ? Complete<U>[]
  : T extends object
    ? { [K in keyof T]-?: Complete<NonNullable<T[K]>> }
    : NonNullable<T>;

export type Settings = Complete<EditSettings>;
export type Segment = Complete<ZoomSegment>;
export type Section = Exclude<keyof Settings, "version">;

export const FRAME_MS = 1000 / 60;

export const complete = <T>(value: T) => value as Complete<T>;

export function formatTime(ms: number, withFraction = true): string {
  const total = Math.max(0, ms) / 1000;
  const minutes = Math.floor(total / 60);
  const seconds = total - minutes * 60;
  const whole = Math.floor(seconds).toString().padStart(2, "0");
  if (!withFraction) return `${minutes}:${whole}`;
  const tenths = Math.floor((seconds % 1) * 10);
  return `${minutes}:${whole}.${tenths}`;
}

export const percent = (v: number) => `${Math.round(v * 100)}%`;
export const times = (v: number) => `${v.toFixed(2)}×`;
