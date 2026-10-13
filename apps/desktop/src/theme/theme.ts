import type {
  AppearanceSettings,
  AppliedTheme,
  CustomTheme,
  ThemeStyle,
} from "@/bindings";
import { BUILTIN_THEMES, defaultThemeFor, type Theme } from "@/theme/builtins";
import { contrastOn, isDark, normalizeHex, withAlpha } from "@/theme/color";
import {
  cssVar,
  isTokenKey,
  TOKEN_KEYS,
  TOKENS,
  type ThemeBase,
  type ThemeColors,
  type TokenKey,
} from "@/theme/tokens";

export type { Theme };

export const SYSTEM = "system";
export const THEME_FORMAT = "recast-theme";
export const THEME_VERSION = 1;
export const MAX_NAME_LENGTH = 60;

export type Appearance = Required<AppearanceSettings>;

export const DEFAULT_APPEARANCE: Appearance = {
  theme: "recast-dark",
  systemLight: "recast-light",
  systemDark: "recast-dark",
  customThemes: [],
  applied: null,
};

/** Where a missing color comes from before the base's default theme is used. */
const FALLBACK: Partial<Record<TokenKey, TokenKey>> = {
  surface: "background",
  muted: "surface",
  hover: "muted",
  input: "border",
  ring: "accent",
  selection: "accent",
  recordingBorder: "recording",
  clipBorder: "clip",
  zoomBorder: "zoom",
  zoomAutoBorder: "zoomAuto",
  zoomAutoForeground: "zoomForeground",
  annotationBorder: "annotation",
  obscureBorder: "obscure",
  segmentSelected: "foreground",
  playhead: "danger",
};

/**
 * Every token of `colors` that is a valid color, the rest filled from related tokens or
 * the default theme of `base`.
 */
export function completeColors(
  colors: Record<string, string>,
  base: ThemeBase,
): { colors: ThemeColors; missing: TokenKey[] } {
  const given = new Map<TokenKey, string>();
  for (const [key, value] of Object.entries(colors)) {
    const color = normalizeHex(value);
    if (isTokenKey(key) && color) given.set(key, color);
  }
  const defaults = defaultThemeFor(base).colors;
  const derived = (key: TokenKey) => {
    let from = FALLBACK[key];
    for (let hops = 0; from && hops < TOKEN_KEYS.length; hops++) {
      const value = given.get(from);
      if (value) return key === "selection" ? withAlpha(value, 0.35) : value;
      from = FALLBACK[from];
    }
    return undefined;
  };
  const filled = Object.fromEntries(
    TOKEN_KEYS.map((key) => [
      key,
      given.get(key) ?? derived(key) ?? defaults[key],
    ]),
  ) as ThemeColors;
  return {
    colors: filled,
    missing: TOKEN_KEYS.filter((key) => !given.has(key)),
  };
}

const customCache = new WeakMap<CustomTheme, Theme>();

export function customTheme(custom: CustomTheme): Theme {
  let theme = customCache.get(custom);
  if (!theme) {
    theme = {
      id: custom.id,
      name: custom.name,
      base: custom.base,
      colors: completeColors(custom.colors, custom.base).colors,
      builtin: false,
      basedOn: custom.basedOn ?? null,
    };
    customCache.set(custom, theme);
  }
  return theme;
}

export function findTheme(id: string, customs: CustomTheme[]) {
  const custom = customs.find((t) => t.id === id);
  return custom ? customTheme(custom) : BUILTIN_THEMES.find((t) => t.id === id);
}

/** The theme `id` names, or the default theme of `base` when it is gone. */
export const themeOr = (id: string, customs: CustomTheme[], base: ThemeBase) =>
  findTheme(id, customs) ?? defaultThemeFor(base);

export function styleOf(theme: Pick<Theme, "base" | "colors">): ThemeStyle {
  return {
    base: theme.base,
    vars: Object.fromEntries(
      TOKEN_KEYS.map((key) => [cssVar(key), theme.colors[key]]),
    ),
  };
}

/** What windows apply for `appearance`. */
export function resolveAppearance(appearance: Appearance): AppliedTheme {
  const customs = appearance.customThemes;
  if (appearance.theme === SYSTEM) {
    return {
      followSystem: true,
      light: styleOf(themeOr(appearance.systemLight, customs, "light")),
      dark: styleOf(themeOr(appearance.systemDark, customs, "dark")),
    };
  }
  const style = styleOf(themeOr(appearance.theme, customs, "dark"));
  return { followSystem: false, light: style, dark: style };
}

const sameStyle = (a: ThemeStyle, b: ThemeStyle) => {
  const names = Object.keys(a.vars);
  return (
    a.base === b.base &&
    names.length === Object.keys(b.vars).length &&
    names.every((name) => a.vars[name] === b.vars[name])
  );
};

/** Whether two resolved themes look the same, whatever order their variables are in. */
export const sameApplied = (
  a: AppliedTheme | null | undefined,
  b: AppliedTheme | null | undefined,
) =>
  !a || !b
    ? !a === !b
    : a.followSystem === b.followSystem &&
      sameStyle(a.light, b.light) &&
      sameStyle(a.dark, b.dark);

export function newThemeId() {
  const random = crypto.getRandomValues(new Uint32Array(2));
  return `custom-${[...random].map((n) => n.toString(36)).join("")}`;
}

/**
 * `name` cut to at most `max` characters (code points, as Rust counts them) without
 * splitting a character or an emoji sequence.
 */
export function clampName(name: string, max = MAX_NAME_LENGTH) {
  let clamped = "";
  let length = 0;
  for (const { segment } of new Intl.Segmenter().segment(name)) {
    const size = [...segment].length;
    if (length + size > max) break;
    clamped += segment;
    length += size;
  }
  return clamped;
}

/** `name`, numbered when another theme already has it. */
export function uniqueName(name: string, taken: string[]) {
  const trimmed = clampName(name.trim()) || "Theme";
  const names = new Set(taken.map((n) => n.toLowerCase()));
  if (!names.has(trimmed.toLowerCase())) return trimmed;
  for (let n = 2; ; n++) {
    const numbered = `${clampName(trimmed, MAX_NAME_LENGTH - 4)} ${n}`;
    if (!names.has(numbered.toLowerCase())) return numbered;
  }
}

export const allThemeNames = (customs: CustomTheme[]) => [
  ...BUILTIN_THEMES.map((t) => t.name),
  ...customs.map((t) => t.name),
];

/** An editable copy of `theme`. */
export function duplicate(theme: Theme, customs: CustomTheme[]): CustomTheme {
  return {
    id: newThemeId(),
    name: uniqueName(`${theme.name} Copy`, allThemeNames(customs)),
    base: theme.base,
    basedOn: theme.builtin ? theme.id : theme.basedOn,
    colors: { ...theme.colors },
  };
}

/** The colors a custom theme's reset goes back to. */
export function originalColors(custom: CustomTheme): ThemeColors {
  const source = BUILTIN_THEMES.find((t) => t.id === custom.basedOn);
  return source && source.base === custom.base
    ? source.colors
    : defaultThemeFor(custom.base).colors;
}

export type ThemeFile = {
  format: typeof THEME_FORMAT;
  version: number;
  name: string;
  base: ThemeBase;
  colors: ThemeColors;
};

export function themeFile(theme: Pick<Theme, "name" | "base" | "colors">) {
  const file: ThemeFile = {
    format: THEME_FORMAT,
    version: THEME_VERSION,
    name: theme.name,
    base: theme.base,
    colors: Object.fromEntries(
      TOKEN_KEYS.map((key) => [key, theme.colors[key]]),
    ) as ThemeColors,
  };
  return `${JSON.stringify(file, null, 2)}\n`;
}

export type ParsedTheme =
  | {
      ok: true;
      name: string;
      base: ThemeBase;
      colors: ThemeColors;
      /** What was filled in or left out, for the user to know. */
      warnings: string[];
    }
  | { ok: false; error: string };

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const list = (items: string[]) =>
  items.length > 4
    ? `${items.slice(0, 4).join(", ")} and ${items.length - 4} more`
    : items.join(", ");

/** Checks a theme file; colors it leaves out come from related or default colors. */
export function parseThemeFile(text: string): ParsedTheme {
  let json: unknown;
  try {
    json = JSON.parse(text);
  } catch (e) {
    return {
      ok: false,
      error: `The file isn't valid JSON (${(e as Error).message}).`,
    };
  }
  const fail = (error: string): ParsedTheme => ({ ok: false, error });
  if (!isObject(json)) return fail("A theme file holds one JSON object.");
  if (json.format !== THEME_FORMAT) {
    return fail(
      `This isn't a Recast theme: "format" must be "${THEME_FORMAT}".`,
    );
  }
  const version = json.version;
  if (
    typeof version !== "number" ||
    !Number.isInteger(version) ||
    version < 1
  ) {
    return fail(`"version" must be a whole number from 1.`);
  }
  if (version > THEME_VERSION) {
    return fail(
      `This theme needs a newer Recast (theme version ${version}; this one reads ${THEME_VERSION}).`,
    );
  }
  if (typeof json.name !== "string" || !json.name.trim()) {
    return fail(`The theme needs a "name".`);
  }
  if (!isObject(json.colors)) {
    return fail(`"colors" must be an object of token names and hex colors.`);
  }
  const colors = json.colors;
  for (const [key, value] of Object.entries(colors)) {
    if (!isTokenKey(key)) continue;
    if (typeof value !== "string" || !normalizeHex(value)) {
      return fail(
        `"${key}" must be a hex color like #1e1e2e or #1e1e2e80, not ${JSON.stringify(value)}.`,
      );
    }
  }
  let base: ThemeBase;
  if (json.base === "light" || json.base === "dark") {
    base = json.base;
  } else if (json.base === undefined) {
    const background =
      typeof colors.background === "string" ? colors.background : null;
    base = background && !isDark(background) ? "light" : "dark";
  } else {
    return fail(`"base" must be "light" or "dark".`);
  }
  const { colors: complete, missing } = completeColors(
    colors as Record<string, string>,
    base,
  );
  const warnings: string[] = [];
  const unknown = Object.keys(colors).filter((key) => !isTokenKey(key));
  if (unknown.length) {
    warnings.push(`Ignored unknown colors: ${list(unknown)}.`);
  }
  if (missing.length) {
    warnings.push(
      `${missing.length} missing ${missing.length === 1 ? "color was" : "colors were"} filled in: ${list(missing)}.`,
    );
  }
  if (json.base === undefined) {
    warnings.push(`No "base" given; it looks ${base}.`);
  }
  return {
    ok: true,
    name: clampName(json.name.trim()),
    base,
    colors: complete,
    warnings,
  };
}

type Check = {
  label: string;
  text: TokenKey;
  /** Layers under the text, the first at the bottom. */
  on: TokenKey[][];
  min: number;
};

const CHECKS: Check[] = [
  {
    label: "Text",
    text: "foreground",
    on: [
      ["background"],
      ["background", "surface"],
      ["background", "muted"],
      ["surface", "hover"],
    ],
    min: 4.5,
  },
  {
    label: "Secondary text",
    text: "mutedForeground",
    on: [["background"], ["background", "surface"], ["background", "muted"]],
    min: 4.5,
  },
  {
    label: "Primary button text",
    text: "primaryForeground",
    on: [["background", "primary"]],
    min: 4.5,
  },
  {
    label: "Danger",
    text: "danger",
    on: [["background"], ["background", "surface"]],
    min: 4.5,
  },
  {
    label: "Warning",
    text: "warning",
    on: [["background"], ["background", "surface"]],
    min: 4.5,
  },
  {
    label: "Success",
    text: "success",
    on: [["background"], ["background", "surface"]],
    min: 4.5,
  },
  {
    label: "Accent",
    text: "accent",
    on: [["background"], ["background", "surface"]],
    min: 3,
  },
  {
    label: "On accent",
    text: "accentForeground",
    on: [["background", "accent"]],
    min: 3,
  },
  { label: "Playhead", text: "playhead", on: [["background"]], min: 3 },
  {
    label: "Clip text",
    text: "clipForeground",
    on: [["background", "recording", "clip"]],
    min: 4.5,
  },
  {
    label: "Zoom text",
    text: "zoomForeground",
    on: [["background", "lane", "zoom"]],
    min: 4.5,
  },
  {
    label: "Auto zoom text",
    text: "zoomAutoForeground",
    on: [["background", "lane", "zoomAuto"]],
    min: 4.5,
  },
  {
    label: "Annotation text",
    text: "segmentForeground",
    on: [
      ["background", "lane", "annotation"],
      ["background", "lane", "obscure"],
    ],
    min: 4.5,
  },
];

export type ContrastIssue = { label: string; ratio: number; min: number };

/** Pairs below WCAG AA: 4.5:1 for text, 3:1 for graphics. */
export function contrastIssues(
  colors: ThemeColors,
  base: ThemeBase,
): ContrastIssue[] {
  const issues: ContrastIssue[] = [];
  const add = (label: string, ratio: number, min: number) => {
    if (ratio < min) issues.push({ label, ratio, min });
  };
  for (const check of CHECKS) {
    const ratio = Math.min(
      ...check.on.map((layers) =>
        contrastOn(
          colors[check.text],
          layers.map((key) => colors[key]),
        ),
      ),
    );
    add(check.label, ratio, check.min);
  }
  // Dark themes show the danger button's fill at 60%.
  const fill = base === "dark" ? withAlpha(colors.danger, 0.6) : colors.danger;
  add(
    "Danger button text",
    contrastOn(colors.dangerForeground, [colors.background, fill]),
    4.5,
  );
  return issues;
}

export const TOKEN_LABELS = Object.fromEntries(
  TOKENS.map((t) => [t.key, t.label]),
) as Record<TokenKey, string>;
