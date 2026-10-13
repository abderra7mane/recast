import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import type { CustomTheme } from "@/bindings";
import { BUILTIN_THEMES, RECAST_DARK, RECAST_LIGHT } from "@/theme/builtins";
import { contrast, normalizeHex, parseHex } from "@/theme/color";
import {
  clampName,
  completeColors,
  contrastIssues,
  DEFAULT_APPEARANCE,
  duplicate,
  originalColors,
  parseThemeFile,
  resolveAppearance,
  SYSTEM,
  styleOf,
  themeFile,
  uniqueName,
} from "@/theme/theme";
import { cssVar, TOKEN_KEYS, TOKENS } from "@/theme/tokens";

const STYLESHEET = readFileSync(
  resolve(__dirname, "../styles/styles.css"),
  "utf8",
);

const file = (patch: Record<string, unknown>) =>
  JSON.stringify({
    format: "recast-theme",
    version: 1,
    name: "Mine",
    base: "dark",
    colors: { ...RECAST_DARK },
    ...patch,
  });

describe("built-in themes", () => {
  it("are unique and set every token to a valid color", () => {
    expect(new Set(BUILTIN_THEMES.map((t) => t.id)).size).toBe(
      BUILTIN_THEMES.length,
    );
    expect(BUILTIN_THEMES.length).toBeGreaterThanOrEqual(12);
    for (const theme of BUILTIN_THEMES) {
      expect(Object.keys(theme.colors).sort(), theme.id).toEqual(
        [...TOKEN_KEYS].sort(),
      );
      for (const key of TOKEN_KEYS) {
        expect(normalizeHex(theme.colors[key]), `${theme.id}.${key}`).toBe(
          theme.colors[key],
        );
      }
    }
  });

  it("meet WCAG AA contrast", () => {
    for (const theme of BUILTIN_THEMES) {
      expect(contrastIssues(theme.colors, theme.base), theme.id).toEqual([]);
    }
  });

  it("match their base", () => {
    for (const theme of BUILTIN_THEMES) {
      const background = parseHex(theme.colors.background)!;
      const black = { r: 0, g: 0, b: 0, a: 1 };
      const white = { r: 255, g: 255, b: 255, a: 1 };
      const darker = contrast(background, white) > contrast(background, black);
      expect(darker ? "dark" : "light", theme.id).toBe(theme.base);
    }
  });

  it("keep the stylesheet's defaults equal to Recast Dark", () => {
    for (const key of TOKEN_KEYS) {
      const match = new RegExp(`${cssVar(key)}:\\s*(#[0-9a-f]+);`).exec(
        STYLESHEET,
      );
      expect(match?.[1], key).toBe(RECAST_DARK[key]);
    }
  });

  it("include System's two defaults and keep the palettes' signature colors", () => {
    const byId = Object.fromEntries(BUILTIN_THEMES.map((t) => [t.id, t]));
    expect(byId["recast-light"].colors).toBe(RECAST_LIGHT);
    expect(byId.dracula.colors.background).toBe("#282a36");
    expect(byId.dracula.colors.accent).toBe("#bd93f9");
    expect(byId.nord.colors.background).toBe("#2e3440");
    expect(byId["solarized-light"].colors.background).toBe("#fdf6e3");
    expect(byId["catppuccin-mocha"].colors.background).toBe("#1e1e2e");
    expect(byId["github-light"].colors.accent).toBe("#0969da");
  });
});

describe("tokens", () => {
  it("have unique keys and CSS variables", () => {
    expect(new Set(TOKENS.map((t) => t.key)).size).toBe(TOKENS.length);
    expect(new Set(TOKEN_KEYS.map(cssVar)).size).toBe(TOKENS.length);
    expect(cssVar("zoomAutoForeground")).toBe("--t-zoom-auto-foreground");
  });
});

describe("resolveAppearance", () => {
  it("applies one theme, or a light and a dark one for System", () => {
    const fixed = resolveAppearance({ ...DEFAULT_APPEARANCE, theme: "nord" });
    expect(fixed.followSystem).toBe(false);
    expect(fixed.dark.vars["--t-background"]).toBe("#2e3440");
    expect(fixed.light).toEqual(fixed.dark);

    const system = resolveAppearance({
      ...DEFAULT_APPEARANCE,
      theme: SYSTEM,
      systemLight: "github-light",
      systemDark: "dracula",
    });
    expect(system.followSystem).toBe(true);
    expect(system.light.base).toBe("light");
    expect(system.light.vars["--t-background"]).toBe("#ffffff");
    expect(system.dark.vars["--t-background"]).toBe("#282a36");
  });

  it("falls back to Recast when a theme is gone", () => {
    const missing = resolveAppearance({
      ...DEFAULT_APPEARANCE,
      theme: SYSTEM,
      systemLight: "deleted",
      systemDark: "deleted",
    });
    expect(missing.light).toEqual(
      styleOf({ base: "light", colors: RECAST_LIGHT }),
    );
    expect(missing.dark).toEqual(
      styleOf({ base: "dark", colors: RECAST_DARK }),
    );
  });

  it("uses custom themes, filling colors they lack", () => {
    const custom: CustomTheme = {
      id: "custom-a",
      name: "Mine",
      base: "light",
      basedOn: null,
      colors: { background: "#FFEEDD", accent: "#123456" },
    };
    const applied = resolveAppearance({
      ...DEFAULT_APPEARANCE,
      theme: "custom-a",
      customThemes: [custom],
    });
    expect(applied.dark.base).toBe("light");
    expect(applied.dark.vars["--t-background"]).toBe("#ffeedd");
    expect(applied.dark.vars["--t-surface"]).toBe("#ffeedd");
    expect(applied.dark.vars["--t-ring"]).toBe("#123456");
    expect(applied.dark.vars["--t-foreground"]).toBe(RECAST_LIGHT.foreground);
    expect(Object.keys(applied.dark.vars)).toHaveLength(TOKEN_KEYS.length);
  });
});

describe("theme files", () => {
  it("round-trip every built-in theme", () => {
    for (const theme of BUILTIN_THEMES) {
      const parsed = parseThemeFile(themeFile(theme));
      expect(parsed).toEqual({
        ok: true,
        name: theme.name,
        base: theme.base,
        colors: theme.colors,
        warnings: [],
      });
    }
  });

  it("are versioned and list every token", () => {
    const json = JSON.parse(themeFile(BUILTIN_THEMES[2]));
    expect(json.format).toBe("recast-theme");
    expect(json.version).toBe(1);
    expect(Object.keys(json.colors)).toEqual(TOKEN_KEYS);
  });

  it("reject malformed files with a reason", () => {
    const error = (text: string) => {
      const parsed = parseThemeFile(text);
      return parsed.ok ? null : parsed.error;
    };
    expect(error("{nope")).toMatch(/isn't valid JSON/);
    expect(error("[]")).toMatch(/one JSON object/);
    expect(error(file({ format: "vscode-theme" }))).toMatch(
      /isn't a Recast theme/,
    );
    expect(error(file({ version: "1" }))).toMatch(/"version"/);
    expect(error(file({ version: 2 }))).toMatch(/newer Recast/);
    expect(error(file({ name: " " }))).toMatch(/"name"/);
    expect(error(file({ base: "dim" }))).toMatch(/"base"/);
    expect(error(file({ colors: ["#000000"] }))).toMatch(/"colors"/);
    expect(error(file({ colors: { accent: "blue" } }))).toMatch(
      /"accent" must be a hex color/,
    );
    expect(error(file({ colors: { accent: 255 } }))).toMatch(/"accent"/);
  });

  it("fill in missing colors and ignore unknown ones", () => {
    const parsed = parseThemeFile(
      file({
        name: "  Partial  ",
        base: undefined,
        colors: {
          background: "#fdf6e3",
          border: "#93A1A1",
          accent: "#268bd2",
          sidebar: "#000000",
        },
      }),
    );
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;
    expect(parsed.name).toBe("Partial");
    expect(parsed.base).toBe("light");
    expect(parsed.colors.input).toBe("#93a1a1");
    expect(parsed.colors.ring).toBe("#268bd2");
    expect(parsed.colors.selection).toBe("#268bd259");
    expect(parsed.colors.surface).toBe("#fdf6e3");
    expect(parsed.colors.foreground).toBe(RECAST_LIGHT.foreground);
    expect(parsed.warnings).toEqual([
      "Ignored unknown colors: sidebar.",
      expect.stringMatching(
        /^43 missing colors were filled in: foreground, surface/,
      ),
      `No "base" given; it looks light.`,
    ]);
  });

  it("accept short hex colors and keep alpha", () => {
    const { colors, missing } = completeColors(
      { background: "#000", lane: "#fff1", nope: "#fff" },
      "dark",
    );
    expect(colors.background).toBe("#000000");
    expect(colors.lane).toBe("#ffffff11");
    expect(missing).not.toContain("lane");
    expect(missing).toContain("foreground");
  });
});

describe("custom themes", () => {
  it("copy a theme under a free name and reset to the original", () => {
    const dracula = BUILTIN_THEMES.find((t) => t.id === "dracula")!;
    const first = duplicate(dracula, []);
    expect(first.name).toBe("Dracula Copy");
    expect(first.basedOn).toBe("dracula");
    expect(first.colors).toEqual(dracula.colors);
    expect(first.id).toMatch(/^custom-/);
    const second = duplicate(dracula, [first]);
    expect(second.name).toBe("Dracula Copy 2");
    expect(second.id).not.toBe(first.id);
    expect(originalColors(second)).toBe(dracula.colors);
    expect(originalColors({ ...second, base: "light" })).toBe(RECAST_LIGHT);
  });

  it("keep names whole characters when cut to length", () => {
    const moon = "a".repeat(59) + "🌙";
    expect(clampName(moon)).toBe(moon);
    expect(clampName("a".repeat(60) + "🌙")).toBe("a".repeat(60));
    expect(clampName("b".repeat(59) + "👩‍💻x")).toBe("b".repeat(59));
    const numbered = uniqueName("a".repeat(55) + "🌙🌙", [
      "a".repeat(55) + "🌙🌙",
    ]);
    expect(numbered).toBe("a".repeat(55) + "🌙 2");
    const parsed = parseThemeFile(file({ name: "a".repeat(60) + "🌙" }));
    expect(parsed.ok && parsed.name).toBe("a".repeat(60));
    const json = JSON.stringify(numbered);
    expect(json).not.toMatch(/\\ud[89a-f]/i);
  });

  it("get unique names", () => {
    expect(uniqueName("Nord", ["nord"])).toBe("Nord 2");
    expect(uniqueName("Nord", ["Nord", "Nord 2"])).toBe("Nord 3");
    expect(uniqueName("  ", [])).toBe("Theme");
    expect(uniqueName("x".repeat(80), []).length).toBe(60);
  });

  it("report poor contrast", () => {
    const issues = contrastIssues(
      { ...RECAST_DARK, foreground: "#222222", accent: "#101010" },
      "dark",
    );
    expect(issues.map((i) => i.label)).toEqual(["Text", "Accent"]);
    expect(issues[0].ratio).toBeLessThan(4.5);
  });
});
