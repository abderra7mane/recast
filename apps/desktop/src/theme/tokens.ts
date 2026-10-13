import type { ThemeBase } from "@/bindings";

export type { ThemeBase };

export const TOKEN_GROUPS = [
  "Window",
  "Controls",
  "Status",
  "Timeline",
  "Canvas",
] as const;

export type TokenGroup = (typeof TOKEN_GROUPS)[number];

/** Every color a theme sets. Marks, backgrounds and exports never use them. */
export const TOKENS = [
  { key: "background", group: "Window", label: "Background" },
  { key: "foreground", group: "Window", label: "Text" },
  { key: "surface", group: "Window", label: "Panels and menus" },
  { key: "muted", group: "Window", label: "Subtle fill" },
  { key: "mutedForeground", group: "Window", label: "Secondary text" },
  { key: "hover", group: "Window", label: "Hover" },
  { key: "border", group: "Window", label: "Borders" },
  { key: "input", group: "Window", label: "Field borders" },
  { key: "stage", group: "Window", label: "Behind the preview" },
  { key: "overlay", group: "Window", label: "Behind dialogs" },
  { key: "scrollbar", group: "Window", label: "Scrollbars" },
  { key: "selection", group: "Window", label: "Selected text" },
  { key: "primary", group: "Controls", label: "Primary button" },
  { key: "primaryForeground", group: "Controls", label: "Primary button text" },
  { key: "accent", group: "Controls", label: "Accent" },
  { key: "accentForeground", group: "Controls", label: "On accent" },
  { key: "ring", group: "Controls", label: "Focus ring" },
  { key: "knob", group: "Controls", label: "Slider knob" },
  { key: "danger", group: "Status", label: "Danger" },
  { key: "dangerForeground", group: "Status", label: "Danger button text" },
  { key: "warning", group: "Status", label: "Warning" },
  { key: "success", group: "Status", label: "Success" },
  { key: "playhead", group: "Timeline", label: "Playhead" },
  { key: "lane", group: "Timeline", label: "Empty track" },
  { key: "recording", group: "Timeline", label: "Recording" },
  { key: "recordingBorder", group: "Timeline", label: "Recording border" },
  { key: "clip", group: "Timeline", label: "Clip" },
  { key: "clipBorder", group: "Timeline", label: "Clip border" },
  { key: "clipForeground", group: "Timeline", label: "Clip text" },
  { key: "trimHandle", group: "Timeline", label: "Trim handles" },
  { key: "click", group: "Timeline", label: "Left clicks" },
  { key: "clickAlt", group: "Timeline", label: "Other clicks" },
  { key: "zoom", group: "Timeline", label: "Zoom" },
  { key: "zoomBorder", group: "Timeline", label: "Zoom border" },
  { key: "zoomForeground", group: "Timeline", label: "Zoom text" },
  { key: "zoomAuto", group: "Timeline", label: "Auto zoom" },
  { key: "zoomAutoBorder", group: "Timeline", label: "Auto zoom border" },
  { key: "zoomAutoForeground", group: "Timeline", label: "Auto zoom text" },
  { key: "annotation", group: "Timeline", label: "Annotation" },
  { key: "annotationBorder", group: "Timeline", label: "Annotation border" },
  { key: "obscure", group: "Timeline", label: "Blur and pixelate" },
  { key: "obscureBorder", group: "Timeline", label: "Blur border" },
  { key: "segmentForeground", group: "Timeline", label: "Annotation text" },
  { key: "segmentSelected", group: "Timeline", label: "Selected item" },
  { key: "cropShade", group: "Canvas", label: "Outside the crop" },
  { key: "cropGuide", group: "Canvas", label: "Crop guides" },
] as const satisfies readonly {
  key: string;
  group: TokenGroup;
  label: string;
}[];

export type TokenKey = (typeof TOKENS)[number]["key"];
export type ThemeColors = Record<TokenKey, string>;

export const TOKEN_KEYS: TokenKey[] = TOKENS.map((t) => t.key);

const isToken = new Set<string>(TOKEN_KEYS);
export const isTokenKey = (key: string): key is TokenKey => isToken.has(key);

/** The CSS custom property of `key`: `zoomAuto` is `--t-zoom-auto`. */
export const cssVar = (key: TokenKey) =>
  `--t-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
