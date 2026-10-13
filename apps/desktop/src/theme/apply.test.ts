// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppliedTheme } from "@/bindings";
import { BUILTIN_THEMES } from "@/theme/builtins";
import {
  DEFAULT_APPEARANCE,
  resolveAppearance,
  sameApplied,
  styleOf,
} from "@/theme/theme";

const commands = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setAppearance: vi.fn(),
  refreshAppliedTheme: vi.fn(),
}));
const listeners = vi.hoisted(
  () =>
    [] as ((e: {
      payload: { applied: AppliedTheme | null; source: string };
    }) => void)[],
);
const events = vi.hoisted(() => ({
  appearanceChanged: {
    listen: vi.fn((listener: (typeof listeners)[number]) => {
      listeners.push(listener);
      return Promise.resolve(() => {});
    }),
  },
}));
vi.mock("@/bindings", () => ({ commands, events }));

const { applyTheme, onThemeChange, startTheme, themeColor } =
  await import("@/theme/apply");

const byId = (id: string) => BUILTIN_THEMES.find((t) => t.id === id)!;
const fixed = (id: string): AppliedTheme => {
  const style = styleOf(byId(id));
  return { followSystem: false, light: style, dark: style };
};
const root = document.documentElement;
const cssValue = (name: string) => root.style.getPropertyValue(name);

let systemDark = true;
const media = new Set<() => void>();

beforeEach(() => {
  systemDark = true;
  media.clear();
  listeners.length = 0;
  window.matchMedia = vi.fn(() => ({
    get matches() {
      return systemDark;
    },
    addEventListener: (_: string, listener: () => void) => media.add(listener),
  })) as unknown as typeof window.matchMedia;
  vi.clearAllMocks();
});

afterEach(() => {
  applyTheme(null);
  delete window.__RECAST_THEME__;
});

describe("applyTheme", () => {
  it("sets every variable, the dark class and the color scheme", () => {
    applyTheme(fixed("solarized-light"));
    expect(cssValue("--t-background")).toBe("#fdf6e3");
    expect(cssValue("--t-zoom-auto-foreground")).not.toBe("");
    expect(root.classList.contains("dark")).toBe(false);
    expect(root.style.colorScheme).toBe("light");

    applyTheme(fixed("dracula"));
    expect(cssValue("--t-background")).toBe("#282a36");
    expect(root.classList.contains("dark")).toBe(true);
    expect(root.style.colorScheme).toBe("dark");
  });

  it("touches only the variables that changed", () => {
    applyTheme(fixed("nord"));
    const edited = fixed("nord");
    edited.dark.vars["--t-accent"] = "#ff0000";
    const setProperty = vi.spyOn(root.style, "setProperty");
    applyTheme(edited);
    expect(setProperty.mock.calls).toEqual([["--t-accent", "#ff0000"]]);
    setProperty.mockRestore();
  });

  it("goes back to the stylesheet's Recast Dark", () => {
    applyTheme(fixed("github-light"));
    applyTheme(null);
    expect(cssValue("--t-background")).toBe("");
    expect(root.classList.contains("dark")).toBe(true);
  });

  it("follows macOS for System", () => {
    const system = resolveAppearance({
      ...DEFAULT_APPEARANCE,
      theme: "system",
      systemLight: "catppuccin-latte",
      systemDark: "catppuccin-mocha",
    });
    systemDark = false;
    applyTheme(system);
    expect(cssValue("--t-background")).toBe("#eff1f5");
    systemDark = true;
    applyTheme(system);
    expect(cssValue("--t-background")).toBe("#1e1e2e");
  });

  it("tells canvases to redraw and gives them the new colors", () => {
    const changed = vi.fn();
    const stop = onThemeChange(changed);
    applyTheme(fixed("dracula"));
    expect(changed).toHaveBeenCalledTimes(1);
    expect(themeColor("accent")).toBe("#bd93f9");
    applyTheme(fixed("nord"));
    expect(themeColor("accent")).toBe("#88c0d0");
    stop();
    applyTheme(null);
    expect(changed).toHaveBeenCalledTimes(2);
  });
});

/** `applied` as Rust sends it back: variables in sorted order. */
const fromRust = (applied: AppliedTheme): AppliedTheme => {
  const sorted = (style: AppliedTheme["dark"]) => ({
    base: style.base,
    vars: Object.fromEntries(
      Object.entries(style.vars).sort(([a], [b]) => (a < b ? -1 : 1)),
    ),
  });
  return {
    ...applied,
    light: sorted(applied.light),
    dark: sorted(applied.dark),
  };
};

describe("startTheme", () => {
  const settings = (appearance: object) =>
    Promise.resolve({ status: "ok", data: { appearance } });

  it("keeps the theme applied before the page loaded and follows other windows", async () => {
    const saved = fromRust(fixed("monokai"));
    expect(Object.keys(saved.dark.vars)).not.toEqual(
      Object.keys(fixed("monokai").dark.vars),
    );
    window.__RECAST_THEME__ = saved;
    commands.getSettings.mockReturnValue(
      settings({ ...DEFAULT_APPEARANCE, theme: "monokai", applied: saved }),
    );
    const setProperty = vi.spyOn(root.style, "setProperty");
    startTheme("library");
    expect(cssValue("--t-background")).toBe("#272822");
    await vi.waitFor(() => expect(commands.getSettings).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve));
    expect(commands.refreshAppliedTheme).not.toHaveBeenCalled();
    expect(setProperty).toHaveBeenCalledTimes(
      Object.keys(saved.dark.vars).length,
    );
    setProperty.mockRestore();

    listeners[0]({
      payload: { applied: fixed("one-dark"), source: "settings" },
    });
    expect(cssValue("--t-background")).toBe("#282c34");
    listeners[0]({ payload: { applied: fixed("nord"), source: "library" } });
    expect(cssValue("--t-background")).toBe("#282c34");
  });

  it("switches with macOS while on System", () => {
    window.__RECAST_THEME__ = resolveAppearance({
      ...DEFAULT_APPEARANCE,
      theme: "system",
    });
    commands.getSettings.mockReturnValue(new Promise(() => {}));
    startTheme("library");
    expect(cssValue("--t-background")).toBe("#0f0f11");
    systemDark = false;
    media.forEach((listener) => listener());
    expect(cssValue("--t-background")).toBe("#ffffff");
  });

  it("refreshes an out-of-date theme only if the choice is unchanged", async () => {
    const stale = fixed("dracula");
    stale.dark.vars["--t-background"] = "#000000";
    const saved = { ...DEFAULT_APPEARANCE, theme: "dracula", applied: stale };
    commands.getSettings.mockReturnValue(settings(saved));
    startTheme("library");
    await vi.waitFor(() =>
      expect(commands.refreshAppliedTheme).toHaveBeenCalledWith(
        saved,
        fixed("dracula"),
      ),
    );
    expect(commands.setAppearance).not.toHaveBeenCalled();
    expect(cssValue("--t-background")).toBe("#282a36");
  });
});

describe("sameApplied", () => {
  it("ignores the order of variables", () => {
    expect(sameApplied(fixed("nord"), fromRust(fixed("nord")))).toBe(true);
    expect(sameApplied(fixed("nord"), fixed("dracula"))).toBe(false);
    expect(sameApplied(null, undefined)).toBe(true);
    expect(sameApplied(null, fixed("nord"))).toBe(false);
    const fewer = fixed("nord");
    delete fewer.dark.vars["--t-accent"];
    expect(sameApplied(fixed("nord"), fewer)).toBe(false);
  });
});
