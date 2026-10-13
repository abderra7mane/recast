/** sRGB channels from 0 to 255 and alpha from 0 to 1. */
export type Rgba = { r: number; g: number; b: number; a: number };

const HEX = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`; `null` for anything else. */
export function parseHex(value: string): Rgba | null {
  const match = HEX.exec(value.trim());
  if (!match) return null;
  let hex = match[1];
  if (hex.length <= 4) hex = [...hex].map((c) => c + c).join("");
  const channel = (i: number) => parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  return {
    r: channel(0),
    g: channel(1),
    b: channel(2),
    a: hex.length === 8 ? channel(3) / 255 : 1,
  };
}

const byte = (n: number) =>
  Math.round(Math.min(255, Math.max(0, n)))
    .toString(16)
    .padStart(2, "0");

/** `#rrggbb`, or `#rrggbbaa` when not opaque. */
export function toHex({ r, g, b, a }: Rgba) {
  const alpha = Math.round(a * 255);
  return `#${byte(r)}${byte(g)}${byte(b)}${alpha < 255 ? byte(alpha) : ""}`;
}

/** The canonical form of a color, or `null` when it isn't one. */
export function normalizeHex(value: string) {
  const color = parseHex(value);
  return color ? toHex(color) : null;
}

export function withAlpha(value: string, alpha: number) {
  const color = parseHex(value);
  return color ? toHex({ ...color, a: alpha }) : value;
}

/** `top` painted over `bottom`. */
export function over(top: Rgba, bottom: Rgba): Rgba {
  const a = top.a + bottom.a * (1 - top.a);
  if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
  const blend = (t: number, b: number) =>
    (t * top.a + b * bottom.a * (1 - top.a)) / a;
  return {
    r: blend(top.r, bottom.r),
    g: blend(top.g, bottom.g),
    b: blend(top.b, bottom.b),
    a,
  };
}

/** The color seen where `layers` are painted in order, the first at the bottom. */
export function flatten(layers: string[]): Rgba {
  return layers.reduce<Rgba>(
    (seen, layer) => over(parseHex(layer) ?? { r: 0, g: 0, b: 0, a: 0 }, seen),
    { r: 0, g: 0, b: 0, a: 1 },
  );
}

const linear = (c: number) => {
  const s = c / 255;
  return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
};

export function luminance({ r, g, b }: Rgba) {
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** The WCAG contrast ratio of two opaque colors, from 1 to 21. */
export function contrast(a: Rgba, b: Rgba) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** The contrast of `text` painted over `layers`. */
export function contrastOn(text: string, layers: string[]) {
  const ground = flatten(layers);
  return contrast(over(parseHex(text) ?? ground, ground), ground);
}

/** `from` moved `t` of the way to `to`, keeping `from`'s alpha. */
export function mix(from: string, to: string, t: number) {
  const a = parseHex(from);
  const b = parseHex(to);
  if (!a || !b) return from;
  const at = (x: number, y: number) => x + (y - x) * t;
  return toHex({ r: at(a.r, b.r), g: at(a.g, b.g), b: at(a.b, b.b), a: a.a });
}

export const isDark = (value: string) =>
  luminance(parseHex(value) ?? { r: 0, g: 0, b: 0, a: 1 }) < 0.18;
