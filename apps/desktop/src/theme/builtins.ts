import { contrastOn, isDark, mix, withAlpha } from "@/theme/color";
import type { ThemeBase, ThemeColors } from "@/theme/tokens";

export type Theme = {
  id: string;
  name: string;
  base: ThemeBase;
  colors: ThemeColors;
  builtin: boolean;
  /** For a custom theme, the built-in theme it was copied from. */
  basedOn: string | null;
};

/** Recast's original look; the stylesheet's defaults match it. */
export const RECAST_DARK: ThemeColors = {
  background: "#0f0f11",
  foreground: "#fafafa",
  surface: "#18181b",
  muted: "#27272a",
  mutedForeground: "#9f9fa9",
  hover: "#27272a",
  border: "#ffffff1a",
  input: "#ffffff26",
  stage: "#00000066",
  overlay: "#00000080",
  scrollbar: "#ffffff4d",
  selection: "#0a84ff59",
  primary: "#e4e4e7",
  primaryForeground: "#18181b",
  accent: "#0a84ff",
  accentForeground: "#ffffff",
  ring: "#71717b",
  knob: "#ffffff",
  danger: "#ff6467",
  dangerForeground: "#ffffff",
  warning: "#fe9a00",
  success: "#00a63e",
  playhead: "#fb2c36",
  lane: "#ffffff08",
  recording: "#024a7066",
  recordingBorder: "#00a6f44d",
  clip: "#0084d180",
  clipBorder: "#00bcffb3",
  clipForeground: "#f0f9ffe6",
  trimHandle: "#74d4ff",
  click: "#ffd230",
  clickAlt: "#f4a8ff",
  zoom: "#8e51ff99",
  zoomBorder: "#c4b4ff99",
  zoomForeground: "#ffffff",
  zoomAuto: "#8e51ff4d",
  zoomAutoBorder: "#a684ff66",
  zoomAutoForeground: "#ede9fe",
  annotation: "#fe9a008c",
  annotationBorder: "#ffd23099",
  obscure: "#90a1b966",
  obscureBorder: "#cad5e280",
  segmentForeground: "#ffffff",
  segmentSelected: "#ffffff",
  cropShade: "#0000008c",
  cropGuide: "#ffffff59",
};

export const RECAST_LIGHT: ThemeColors = {
  background: "#ffffff",
  foreground: "#09090b",
  surface: "#ffffff",
  muted: "#f4f4f5",
  mutedForeground: "#66666f",
  hover: "#f4f4f5",
  border: "#e4e4e7",
  input: "#e4e4e7",
  stage: "#0000000f",
  overlay: "#00000059",
  scrollbar: "#0000004d",
  selection: "#0071e340",
  primary: "#18181b",
  primaryForeground: "#fafafa",
  accent: "#0071e3",
  accentForeground: "#ffffff",
  ring: "#a1a1aa",
  knob: "#ffffff",
  danger: "#e7000b",
  dangerForeground: "#ffffff",
  warning: "#bb4d00",
  success: "#008236",
  playhead: "#e7000b",
  lane: "#0000000a",
  recording: "#0084d11a",
  recordingBorder: "#0084d14d",
  clip: "#0084d140",
  clipBorder: "#0084d1b3",
  clipForeground: "#024a70",
  trimHandle: "#0084d1",
  click: "#e17100",
  clickAlt: "#c800de",
  zoom: "#8e51ff40",
  zoomBorder: "#8e51ffb3",
  zoomForeground: "#4d179a",
  zoomAuto: "#8e51ff1f",
  zoomAutoBorder: "#8e51ff66",
  zoomAutoForeground: "#4d179a",
  annotation: "#fe9a0059",
  annotationBorder: "#e17100b3",
  obscure: "#62748e40",
  obscureBorder: "#62748e99",
  segmentForeground: "#1d293d",
  segmentSelected: "#09090b",
  cropShade: "#0000008c",
  cropGuide: "#ffffff59",
};

/** A theme's own palette, mapped to tokens by `fromPalette`. */
type Palette = {
  base: ThemeBase;
  background: string;
  surface: string;
  muted: string;
  hover?: string;
  border: string;
  foreground: string;
  /** Comments or secondary text. */
  comment: string;
  accent: string;
  primary?: string;
  primaryForeground?: string;
  stage: string;
  red: string;
  orange: string;
  yellow: string;
  green: string;
  blue: string;
  purple: string;
  magenta: string;
};

const TEXT = 4.5;
const GRAPHIC = 3;

const passes = (color: string, grounds: string[][], min: number) =>
  grounds.every((layers) => contrastOn(color, layers) >= min);

/**
 * `color`, moved toward `toward` just enough to reach `min` contrast on every ground,
 * so palette colors keep their hue where they can.
 */
function legible(
  color: string,
  grounds: string[][],
  min: number,
  toward: string,
) {
  for (let step = 0; step <= 20; step++) {
    const moved = mix(color, toward, step / 20);
    if (passes(moved, grounds, min)) return moved;
  }
  const extreme = isDark(toward) ? "#000000" : "#ffffff";
  for (let step = 1; step <= 20; step++) {
    const moved = mix(toward, extreme, step / 20);
    if (passes(moved, grounds, min)) return moved;
  }
  return extreme;
}

/**
 * The first of `candidates` readable on `grounds`; else the one nearest black or white,
 * whichever reads better, moved toward it until it reads.
 */
function textOn(grounds: string[][], candidates: string[], min = TEXT) {
  const found = candidates.find((c) => passes(c, grounds, min));
  if (found) return found;
  const score = (c: string) =>
    Math.min(...grounds.map((layers) => contrastOn(c, layers)));
  const extreme = score("#000000") > score("#ffffff") ? "#000000" : "#ffffff";
  const nearest = candidates.reduce((a, b) =>
    contrastOn(b, [extreme]) < contrastOn(a, [extreme]) ? b : a,
  );
  return legible(nearest, grounds, min, extreme);
}

/** A button fill and its label, darkening or lightening the fill until they read. */
function button(fill: string, label: string, under: string) {
  const toward = isDark(label) ? "#ffffff" : "#000000";
  for (let step = 0; step <= 20; step++) {
    const moved = mix(fill, toward, step / 20);
    if (contrastOn(label, [under, moved]) >= TEXT) return [moved, label];
  }
  return [toward, label];
}

/**
 * Fills of `colors` at up to `alpha` over `under`, and a label color from `texts` they
 * read with: the first that keeps them near `alpha`, else the one keeping them most
 * opaque. Timeline items so keep the palette's hues.
 */
function tints(
  colors: string[],
  alpha: number,
  texts: string[],
  under: string[],
) {
  const faintest = (text: string, color: string) => {
    let a = alpha;
    while (
      a > 0.1 &&
      contrastOn(text, [...under, withAlpha(color, a)]) < TEXT
    ) {
      a -= 0.05;
    }
    return Math.max(a, 0.1);
  };
  const options = texts.map((text) => {
    const alphas = colors.map((color) => faintest(text, color));
    return { text, alphas, worst: Math.min(...alphas) };
  });
  const best =
    options.find((o) => o.worst >= alpha * 0.75) ??
    options.reduce((a, b) => (b.worst > a.worst ? b : a));
  return {
    fills: colors.map((color, i) => withAlpha(color, best.alphas[i])),
    text: best.text,
  };
}

function fromPalette(p: Palette): ThemeColors {
  const dark = p.base === "dark";
  const bg = p.background;
  const extreme = dark ? "#ffffff" : "#000000";
  const surfaces = [[bg], [bg, p.surface], [bg, p.muted]];
  const foreground = legible(p.foreground, surfaces, TEXT, extreme);
  const status = (color: string) =>
    legible(color, [[bg], [bg, p.surface]], TEXT, extreme);
  const accent = legible(p.accent, [[bg], [bg, p.surface]], GRAPHIC, extreme);
  const [primary, primaryForeground] = button(
    p.primary ?? accent,
    p.primaryForeground ?? bg,
    bg,
  );
  const danger = status(p.red);
  const dangerFill = dark ? withAlpha(danger, 0.6) : danger;
  const lane = withAlpha(foreground, dark ? 0.04 : 0.05);
  const recording = withAlpha(p.blue, dark ? 0.2 : 0.12);
  const texts = [foreground, bg, extreme];
  const clip = tints([p.blue], dark ? 0.45 : 0.3, texts, [bg, recording]);
  const zoom = tints([p.purple], dark ? 0.55 : 0.3, texts, [bg, lane]);
  const zoomAuto = tints([p.purple], dark ? 0.25 : 0.14, texts, [bg, lane]);
  const marks = tints([p.orange, p.comment], dark ? 0.5 : 0.35, texts, [
    bg,
    lane,
  ]);
  return {
    background: bg,
    foreground,
    surface: p.surface,
    muted: p.muted,
    mutedForeground: legible(p.comment, surfaces, TEXT, foreground),
    hover: p.hover ?? p.muted,
    border: p.border,
    input: p.border,
    stage: p.stage,
    overlay: dark ? "#00000080" : "#00000059",
    scrollbar: withAlpha(foreground, 0.3),
    selection: withAlpha(accent, 0.35),
    primary,
    primaryForeground,
    accent,
    accentForeground: textOn(
      [[bg, accent]],
      [dark ? bg : "#ffffff", foreground],
      GRAPHIC,
    ),
    ring: accent,
    knob: dark ? foreground : "#ffffff",
    danger,
    dangerForeground: textOn([[bg, dangerFill]], ["#ffffff", foreground, bg]),
    warning: status(dark ? p.yellow : p.orange),
    success: status(p.green),
    playhead: legible(p.red, [[bg]], GRAPHIC, extreme),
    lane,
    recording,
    recordingBorder: withAlpha(p.blue, 0.35),
    clip: clip.fills[0],
    clipBorder: withAlpha(p.blue, 0.75),
    clipForeground: textOn([[bg, recording, clip.fills[0]]], [clip.text]),
    trimHandle: p.blue,
    click: dark ? p.yellow : p.orange,
    clickAlt: p.magenta,
    zoom: zoom.fills[0],
    zoomBorder: withAlpha(p.purple, 0.8),
    zoomForeground: textOn([[bg, lane, zoom.fills[0]]], [zoom.text]),
    zoomAuto: zoomAuto.fills[0],
    zoomAutoBorder: withAlpha(p.purple, 0.45),
    zoomAutoForeground: textOn(
      [[bg, lane, zoomAuto.fills[0]]],
      [zoomAuto.text],
    ),
    annotation: marks.fills[0],
    annotationBorder: withAlpha(p.yellow, 0.6),
    obscure: marks.fills[1],
    obscureBorder: withAlpha(p.comment, 0.7),
    segmentForeground: textOn(
      marks.fills.map((fill) => [bg, lane, fill]),
      [marks.text],
    ),
    segmentSelected: foreground,
    cropShade: "#0000008c",
    cropGuide: "#ffffff59",
  };
}

const builtin = (
  id: string,
  name: string,
  base: ThemeBase,
  colors: ThemeColors,
): Theme => ({ id, name, base, colors, builtin: true, basedOn: null });

const palette = (id: string, name: string, p: Palette) =>
  builtin(id, name, p.base, fromPalette(p));

export const BUILTIN_THEMES: Theme[] = [
  builtin("recast-dark", "Recast Dark", "dark", RECAST_DARK),
  builtin("recast-light", "Recast Light", "light", RECAST_LIGHT),
  palette("dracula", "Dracula", {
    base: "dark",
    background: "#282a36",
    surface: "#21222c",
    muted: "#44475a",
    border: "#44475a",
    foreground: "#f8f8f2",
    comment: "#6272a4",
    accent: "#bd93f9",
    primaryForeground: "#282a36",
    stage: "#191a21",
    red: "#ff5555",
    orange: "#ffb86c",
    yellow: "#f1fa8c",
    green: "#50fa7b",
    blue: "#8be9fd",
    purple: "#bd93f9",
    magenta: "#ff79c6",
  }),
  palette("nord", "Nord", {
    base: "dark",
    background: "#2e3440",
    surface: "#3b4252",
    muted: "#434c5e",
    border: "#4c566a",
    foreground: "#eceff4",
    comment: "#616e88",
    accent: "#88c0d0",
    primaryForeground: "#2e3440",
    stage: "#242933",
    red: "#bf616a",
    orange: "#d08770",
    yellow: "#ebcb8b",
    green: "#a3be8c",
    blue: "#81a1c1",
    purple: "#b48ead",
    magenta: "#b48ead",
  }),
  palette("solarized-dark", "Solarized Dark", {
    base: "dark",
    background: "#002b36",
    surface: "#073642",
    muted: "#073642",
    border: "#586e7566",
    foreground: "#93a1a1",
    comment: "#586e75",
    accent: "#268bd2",
    primaryForeground: "#fdf6e3",
    stage: "#00212b",
    red: "#dc322f",
    orange: "#cb4b16",
    yellow: "#b58900",
    green: "#859900",
    blue: "#268bd2",
    purple: "#6c71c4",
    magenta: "#d33682",
  }),
  palette("solarized-light", "Solarized Light", {
    base: "light",
    background: "#fdf6e3",
    surface: "#eee8d5",
    muted: "#eee8d5",
    border: "#93a1a166",
    foreground: "#586e75",
    comment: "#93a1a1",
    accent: "#268bd2",
    primaryForeground: "#fdf6e3",
    stage: "#eee8d5",
    red: "#dc322f",
    orange: "#cb4b16",
    yellow: "#b58900",
    green: "#859900",
    blue: "#268bd2",
    purple: "#6c71c4",
    magenta: "#d33682",
  }),
  palette("catppuccin-latte", "Catppuccin Latte", {
    base: "light",
    background: "#eff1f5",
    surface: "#e6e9ef",
    muted: "#ccd0da",
    border: "#bcc0cc",
    foreground: "#4c4f69",
    comment: "#6c6f85",
    accent: "#8839ef",
    primaryForeground: "#eff1f5",
    stage: "#dce0e8",
    red: "#d20f39",
    orange: "#fe640b",
    yellow: "#df8e1d",
    green: "#40a02b",
    blue: "#1e66f5",
    purple: "#8839ef",
    magenta: "#ea76cb",
  }),
  palette("catppuccin-mocha", "Catppuccin Mocha", {
    base: "dark",
    background: "#1e1e2e",
    surface: "#181825",
    muted: "#313244",
    hover: "#45475a",
    border: "#45475a",
    foreground: "#cdd6f4",
    comment: "#a6adc8",
    accent: "#cba6f7",
    primaryForeground: "#1e1e2e",
    stage: "#11111b",
    red: "#f38ba8",
    orange: "#fab387",
    yellow: "#f9e2af",
    green: "#a6e3a1",
    blue: "#89b4fa",
    purple: "#cba6f7",
    magenta: "#f5c2e7",
  }),
  palette("gruvbox-dark", "Gruvbox Dark", {
    base: "dark",
    background: "#282828",
    surface: "#32302f",
    muted: "#3c3836",
    hover: "#504945",
    border: "#504945",
    foreground: "#ebdbb2",
    comment: "#a89984",
    accent: "#fabd2f",
    primaryForeground: "#282828",
    stage: "#1d2021",
    red: "#fb4934",
    orange: "#fe8019",
    yellow: "#fabd2f",
    green: "#b8bb26",
    blue: "#83a598",
    purple: "#d3869b",
    magenta: "#d3869b",
  }),
  palette("tokyo-night", "Tokyo Night", {
    base: "dark",
    background: "#1a1b26",
    surface: "#16161e",
    muted: "#292e42",
    border: "#414868",
    foreground: "#c0caf5",
    comment: "#565f89",
    accent: "#7aa2f7",
    primaryForeground: "#1a1b26",
    stage: "#16161e",
    red: "#f7768e",
    orange: "#ff9e64",
    yellow: "#e0af68",
    green: "#9ece6a",
    blue: "#7aa2f7",
    purple: "#bb9af7",
    magenta: "#bb9af7",
  }),
  palette("one-dark", "One Dark", {
    base: "dark",
    background: "#282c34",
    surface: "#21252b",
    muted: "#2c313c",
    hover: "#3e4451",
    border: "#3e4451",
    foreground: "#abb2bf",
    comment: "#5c6370",
    accent: "#61afef",
    primaryForeground: "#282c34",
    stage: "#21252b",
    red: "#e06c75",
    orange: "#d19a66",
    yellow: "#e5c07b",
    green: "#98c379",
    blue: "#61afef",
    purple: "#c678dd",
    magenta: "#c678dd",
  }),
  palette("github-light", "GitHub Light", {
    base: "light",
    background: "#ffffff",
    surface: "#f6f8fa",
    muted: "#eaeef2",
    border: "#d0d7de",
    foreground: "#1f2328",
    comment: "#656d76",
    accent: "#0969da",
    primary: "#1f883d",
    primaryForeground: "#ffffff",
    stage: "#f6f8fa",
    red: "#d1242f",
    orange: "#bc4c00",
    yellow: "#9a6700",
    green: "#1a7f37",
    blue: "#0969da",
    purple: "#8250df",
    magenta: "#bf3989",
  }),
  palette("github-dark", "GitHub Dark", {
    base: "dark",
    background: "#0d1117",
    surface: "#161b22",
    muted: "#21262d",
    hover: "#30363d",
    border: "#30363d",
    foreground: "#e6edf3",
    comment: "#7d8590",
    accent: "#2f81f7",
    primary: "#238636",
    primaryForeground: "#ffffff",
    stage: "#010409",
    red: "#f85149",
    orange: "#db6d28",
    yellow: "#d29922",
    green: "#3fb950",
    blue: "#2f81f7",
    purple: "#a371f7",
    magenta: "#db61a2",
  }),
  palette("monokai", "Monokai", {
    base: "dark",
    background: "#272822",
    surface: "#1e1f1c",
    muted: "#3e3d32",
    hover: "#49483e",
    border: "#49483e",
    foreground: "#f8f8f2",
    comment: "#75715e",
    accent: "#66d9ef",
    primary: "#a6e22e",
    primaryForeground: "#272822",
    stage: "#1e1f1c",
    red: "#f92672",
    orange: "#fd971f",
    yellow: "#e6db74",
    green: "#a6e22e",
    blue: "#66d9ef",
    purple: "#ae81ff",
    magenta: "#f92672",
  }),
];

export const defaultThemeFor = (base: ThemeBase) =>
  base === "dark" ? BUILTIN_THEMES[0] : BUILTIN_THEMES[1];
