import {
  commands,
  events,
  type AppliedTheme,
  type ThemeStyle,
} from "@/bindings";
import { RECAST_DARK } from "@/theme/builtins";
import {
  DEFAULT_APPEARANCE,
  resolveAppearance,
  sameApplied,
} from "@/theme/theme";
import { cssVar, type TokenKey } from "@/theme/tokens";

declare global {
  interface Window {
    /** Set by the script Rust runs before the page loads. */
    __RECAST_THEME__?: AppliedTheme;
  }
}

let current: AppliedTheme | null = null;
let shown: Record<string, string> = {};
const listeners = new Set<() => void>();
const colorCache = new Map<TokenKey, string>();

const darkQuery = () =>
  typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

/** The style `theme` calls for now. */
export function styleFor(theme: AppliedTheme, systemDark: boolean) {
  return theme.followSystem && !systemDark ? theme.light : theme.dark;
}

/** Sets the variables that changed, so a live edit restyles only what uses them. */
function applyStyle(style: ThemeStyle | null) {
  const root = document.documentElement;
  const vars = style?.vars ?? {};
  for (const name of Object.keys(shown)) {
    if (!(name in vars)) root.style.removeProperty(name);
  }
  for (const [name, value] of Object.entries(vars)) {
    if (shown[name] !== value) root.style.setProperty(name, value);
  }
  shown = { ...vars };
  const base = style?.base ?? "dark";
  root.classList.toggle("dark", base === "dark");
  root.style.colorScheme = base;
  colorCache.clear();
  listeners.forEach((listener) => listener());
}

/** Shows `theme` in this window; `null` goes back to the stylesheet's Recast Dark. */
export function applyTheme(theme: AppliedTheme | null) {
  current = theme;
  applyStyle(theme ? styleFor(theme, darkQuery()?.matches ?? true) : null);
}

export const currentTheme = () => current;

/** Calls `listener` after every theme change; returns the unsubscribe. */
export function onThemeChange(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** A theme color for canvas drawing, which can't use CSS variables. */
export function themeColor(key: TokenKey) {
  if (typeof document === "undefined") return RECAST_DARK[key];
  let color = colorCache.get(key);
  if (color === undefined) {
    color =
      getComputedStyle(document.documentElement)
        .getPropertyValue(cssVar(key))
        .trim() || RECAST_DARK[key];
    colorCache.set(key, color);
  }
  return color;
}

/**
 * Keeps window `label` on the chosen theme: the one Rust applied before the first paint,
 * then every change from another window or from macOS. Brings the saved theme up to
 * date when built-in themes changed since it was resolved.
 */
export function startTheme(label: string) {
  applyTheme(window.__RECAST_THEME__ ?? null);
  darkQuery()?.addEventListener("change", () => {
    if (current?.followSystem) applyTheme(current);
  });
  void events.appearanceChanged
    .listen(({ payload }) => {
      if (payload.source !== label && !sameApplied(payload.applied, current)) {
        applyTheme(payload.applied);
      }
    })
    .catch(() => {});
  void commands
    .getSettings()
    .then((result) => {
      const saved = result.status === "ok" ? result.data.appearance : null;
      if (!saved) return;
      const applied = resolveAppearance({ ...DEFAULT_APPEARANCE, ...saved });
      if (!sameApplied(applied, current)) applyTheme(applied);
      // Only if the choice is still the one read here, so it never undoes a newer one.
      if (!sameApplied(applied, saved.applied)) {
        void commands.refreshAppliedTheme(saved, applied);
      }
    })
    .catch(() => {});
}
