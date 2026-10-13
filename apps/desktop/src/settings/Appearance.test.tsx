// @vitest-environment jsdom
import { useState } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AppearanceSettings, CustomTheme } from "@/bindings";
import { BUILTIN_THEMES } from "@/theme/builtins";
import {
  DEFAULT_APPEARANCE,
  resolveAppearance,
  themeFile,
  type Appearance,
} from "@/theme/theme";

const commands = vi.hoisted(() => ({
  setAppearance: vi.fn(),
  importTheme: vi.fn(),
  exportTheme: vi.fn(),
}));
const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@/bindings", () => ({ commands }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const { AppearanceSettings } = await import("@/settings/Appearance");
const { applyTheme } = await import("@/theme/apply");

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });
const cssValue = (name: string) =>
  document.documentElement.style.getPropertyValue(name);
const dracula = BUILTIN_THEMES.find((t) => t.id === "dracula")!;

/** Renders the section with its state kept like the Settings window keeps it. */
function renderAppearance(initial: Partial<Appearance> = {}) {
  const state = { current: { ...DEFAULT_APPEARANCE, ...initial } };
  applyTheme(resolveAppearance(state.current));
  function Harness() {
    const [appearance, setAppearance] = useState(state.current);
    state.current = appearance;
    return (
      <AppearanceSettings appearance={appearance} onChange={setAppearance} />
    );
  }
  const view = render(<Harness />);
  return Object.assign(state, { view });
}

const saved = () =>
  commands.setAppearance.mock.lastCall?.[0] as
    Required<AppearanceSettings> | undefined;

const mine = (patch: Partial<CustomTheme> = {}): CustomTheme => ({
  id: "custom-mine",
  name: "Mine",
  base: "dark",
  basedOn: "dracula",
  colors: { ...dracula.colors },
  ...patch,
});

beforeEach(() => {
  vi.clearAllMocks();
  commands.setAppearance.mockImplementation(() => ok({}));
  commands.exportTheme.mockImplementation(() => ok(null));
});

afterEach(() => {
  vi.useRealTimers();
  applyTheme(null);
});

describe("Appearance", () => {
  it("lists System and every built-in theme with Recast Dark chosen", () => {
    renderAppearance();
    const group = screen.getByRole("group", { name: "Themes" });
    const cards = within(group).getAllByRole("button");
    expect(cards.map((c) => c.getAttribute("aria-label"))).toEqual([
      "System",
      ...BUILTIN_THEMES.map((t) => t.name),
    ]);
    expect(
      screen
        .getByRole("button", { name: "Recast Dark" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(screen.getByText(/Built-in themes can’t be changed/)).toBeTruthy();
  });

  it("applies a picked theme at once and saves it", async () => {
    const user = userEvent.setup();
    const state = renderAppearance();
    await user.click(screen.getByRole("button", { name: "Nord" }));
    expect(cssValue("--t-background")).toBe("#2e3440");
    expect(state.current.theme).toBe("nord");
    await vi.waitFor(() => expect(commands.setAppearance).toHaveBeenCalled());
    expect(saved()?.theme).toBe("nord");
    expect(saved()?.applied?.dark.vars["--t-background"]).toBe("#2e3440");
    expect(saved()?.applied?.followSystem).toBe(false);
  });

  it("goes back to the last saved theme when Rust refuses a change", async () => {
    const user = userEvent.setup();
    commands.setAppearance.mockImplementationOnce(() => ok({}));
    const state = renderAppearance();
    await user.click(screen.getByRole("button", { name: "Nord" }));
    commands.setAppearance.mockImplementationOnce(() =>
      Promise.resolve({ status: "error", error: "cannot save" }),
    );
    await user.click(screen.getByRole("button", { name: "Dracula" }));
    await vi.waitFor(() => expect(state.current.theme).toBe("nord"));
    expect(cssValue("--t-background")).toBe("#2e3440");
    expect(screen.getByRole("alert").textContent).toBe("cannot save");
  });

  it("lets System use any light and dark theme", async () => {
    const user = userEvent.setup();
    const state = renderAppearance();
    await user.click(screen.getByRole("button", { name: "System" }));
    expect(state.current.applied?.followSystem).toBe(true);
    expect(
      screen.getByRole("combobox", { name: "Light appearance" }).textContent,
    ).toBe("Recast Light");
    expect(
      screen.getByRole("combobox", { name: "Dark appearance" }).textContent,
    ).toBe("Recast Dark");
    expect(screen.queryByRole("button", { name: /Duplicate/ })).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: /Duplicate/ }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("duplicates a built-in theme into an editable one", async () => {
    const user = userEvent.setup();
    const state = renderAppearance({ theme: "dracula" });
    await user.click(screen.getByRole("button", { name: /Duplicate/ }));
    const copy = state.current.customThemes[0];
    expect(copy).toMatchObject({
      name: "Dracula Copy",
      base: "dark",
      basedOn: "dracula",
    });
    expect(state.current.theme).toBe(copy.id);
    expect(
      within(screen.getByRole("group", { name: "Your themes" }))
        .getByRole("button", { name: "Dracula Copy" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(screen.getByRole("region", { name: "Theme editor" })).toBeTruthy();
    expect(screen.getByTestId("theme-preview")).toBeTruthy();
  });

  it("shows and sends color edits at once, skipping invalid ones", () => {
    const state = renderAppearance({
      theme: "custom-mine",
      customThemes: [mine()],
    });
    const hex = screen.getByRole("textbox", { name: "Accent hex" });
    fireEvent.change(hex, { target: { value: "#12345" } });
    expect(hex.getAttribute("aria-invalid")).toBe("true");
    expect(cssValue("--t-accent")).toBe("#bd93f9");

    fireEvent.change(hex, { target: { value: "#123456" } });
    expect(cssValue("--t-accent")).toBe("#123456");
    fireEvent.change(screen.getByLabelText("Background color"), {
      target: { value: "#000000" },
    });
    expect(cssValue("--t-background")).toBe("#000000");
    expect(commands.setAppearance).toHaveBeenCalledTimes(2);
    expect(saved()?.customThemes[0].colors).toMatchObject({
      accent: "#123456",
      background: "#000000",
    });
    expect(state.current.customThemes[0].colors.accent).toBe("#123456");
  });

  it("keeps the alpha when a color is picked, and resets colors", () => {
    renderAppearance({
      theme: "custom-mine",
      customThemes: [
        mine({ colors: { ...dracula.colors, lane: "#11223344" } }),
      ],
    });
    fireEvent.change(screen.getByLabelText("Empty track color"), {
      target: { value: "#aabbcc" },
    });
    expect(cssValue("--t-lane")).toBe("#aabbcc44");

    fireEvent.click(screen.getByRole("button", { name: "Reset Empty track" }));
    expect(cssValue("--t-lane")).toBe(dracula.colors.lane);
    fireEvent.change(screen.getByRole("textbox", { name: "Accent hex" }), {
      target: { value: "#000" },
    });
    expect(screen.getByRole("status").textContent ?? "").toMatch(
      /Hard to read: Accent/,
    );
    fireEvent.click(screen.getByRole("button", { name: /Reset All/ }));
    expect(cssValue("--t-accent")).toBe(dracula.colors.accent);
    expect(screen.queryByText(/Hard to read/)).toBeNull();
  });

  it("renames a theme to a free name and switches its base", () => {
    const state = renderAppearance({
      theme: "custom-mine",
      customThemes: [mine(), mine({ id: "custom-other", name: "Night" })],
    });
    const name = screen.getByRole("textbox", { name: "Name" });
    fireEvent.change(name, { target: { value: "  " } });
    expect(state.current.customThemes[0].name).toBe("Mine");
    fireEvent.change(name, { target: { value: "Night Shift" } });
    expect(state.current.customThemes[0].name).toBe("Night Shift");
    fireEvent.change(name, { target: { value: "night" } });
    fireEvent.blur(name);
    expect(state.current.customThemes[0].name).toBe("night 2");
    fireEvent.change(name, { target: { value: "Nord" } });
    fireEvent.blur(name);
    expect(state.current.customThemes[0].name).toBe("Nord 2");
    expect((name as HTMLInputElement).value).toBe("Nord 2");

    fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    expect(state.current.customThemes[0].base).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("deletes a custom theme after a second click and goes back to its original", async () => {
    const user = userEvent.setup();
    const state = renderAppearance({
      theme: "custom-mine",
      systemDark: "custom-mine",
      customThemes: [mine()],
    });
    await user.click(screen.getByRole("button", { name: /^Delete$/ }));
    expect(state.current.customThemes).toHaveLength(1);
    await user.click(
      screen.getByRole("button", { name: /Click Again to Delete/ }),
    );
    expect(state.current.customThemes).toEqual([]);
    expect(state.current.theme).toBe("dracula");
    expect(state.current.systemDark).toBe("recast-dark");
    expect(screen.getByText("Deleted “Mine”.")).toBeTruthy();
  });

  it("imports a theme file and reports what it filled in", async () => {
    const user = userEvent.setup();
    dialog.open.mockResolvedValue("/Users/ada/Themes/rose.json");
    commands.importTheme.mockReturnValue(
      ok(
        JSON.stringify({
          format: "recast-theme",
          version: 1,
          name: "Recast Dark",
          base: "dark",
          colors: { background: "#1a1020", accent: "#ff77aa", shiny: "#fff" },
        }),
      ),
    );
    const state = renderAppearance();
    await user.click(screen.getByRole("button", { name: /Import/ }));
    expect(commands.importTheme).toHaveBeenCalledWith(
      "/Users/ada/Themes/rose.json",
    );
    const imported = state.current.customThemes[0];
    expect(imported.name).toBe("Recast Dark 2");
    expect(imported.colors.accent).toBe("#ff77aa");
    expect(state.current.theme).toBe(imported.id);
    expect(cssValue("--t-background")).toBe("#1a1020");
    expect(
      screen.getByText(
        /Imported “Recast Dark 2”\. Ignored unknown colors: shiny\. 44 missing colors were filled in/,
      ),
    ).toBeTruthy();
  });

  it("refuses a malformed theme file with the reason", async () => {
    const user = userEvent.setup();
    dialog.open.mockResolvedValue("/tmp/bad.json");
    commands.importTheme.mockReturnValue(
      ok(
        JSON.stringify({
          format: "recast-theme",
          version: 1,
          name: "Bad",
          colors: { accent: "pink" },
        }),
      ),
    );
    const state = renderAppearance();
    await user.click(screen.getByRole("button", { name: /Import/ }));
    expect(screen.getByRole("alert").textContent).toBe(
      `Couldn't import bad.json. "accent" must be a hex color like #1e1e2e or #1e1e2e80, not "pink".`,
    );
    expect(state.current.customThemes).toEqual([]);
    expect(commands.setAppearance).not.toHaveBeenCalled();
  });

  it("exports the chosen theme", async () => {
    const user = userEvent.setup();
    dialog.save.mockResolvedValue("/Users/ada/Desktop/Dracula.json");
    renderAppearance({ theme: "dracula" });
    await user.click(screen.getByRole("button", { name: /Export/ }));
    expect(dialog.save).toHaveBeenCalledWith(
      expect.objectContaining({ defaultPath: "Dracula.json" }),
    );
    expect(commands.exportTheme).toHaveBeenCalledWith(
      "/Users/ada/Desktop/Dracula.json",
      themeFile(dracula),
    );
    expect(screen.getByRole("status").textContent).toBe(
      "Exported “Dracula” to Dracula.json.",
    );
  });
});
