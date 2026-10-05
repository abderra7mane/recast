import { describe, expect, it } from "vitest";

import {
  formatShortcut,
  fromKeyPress,
  type KeyPress,
} from "@/settings/shortcuts";

const press = (code: string, held: Partial<KeyPress> = {}): KeyPress => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...held,
});

describe("fromKeyPress", () => {
  it("builds the stored form in a fixed modifier order", () => {
    expect(
      fromKeyPress(
        press("KeyR", { metaKey: true, shiftKey: true, altKey: true }),
      ),
    ).toEqual({ kind: "shortcut", shortcut: "Alt+Shift+Cmd+KeyR" });
    expect(fromKeyPress(press("F5", { metaKey: true, ctrlKey: true }))).toEqual(
      { kind: "shortcut", shortcut: "Ctrl+Cmd+F5" },
    );
    expect(fromKeyPress(press("F13"))).toEqual({
      kind: "shortcut",
      shortcut: "F13",
    });
  });

  it("waits while only modifiers are held", () => {
    expect(
      fromKeyPress(press("MetaLeft", { metaKey: true, altKey: true })),
    ).toEqual({ kind: "pending", symbols: "⌥⌘" });
  });

  it("uses Esc to cancel and Delete to clear", () => {
    expect(fromKeyPress(press("Escape"))).toEqual({ kind: "cancel" });
    expect(fromKeyPress(press("Backspace"))).toEqual({ kind: "clear" });
    expect(fromKeyPress(press("Delete"))).toEqual({ kind: "clear" });
    expect(fromKeyPress(press("Backspace", { metaKey: true }))).toEqual({
      kind: "shortcut",
      shortcut: "Cmd+Backspace",
    });
  });

  it("rejects keys macOS can't register", () => {
    expect(fromKeyPress(press("Escape", { metaKey: true }))).toEqual({
      kind: "unsupported",
      code: "Escape",
    });
    expect(fromKeyPress(press("MediaPlayPause"))).toEqual({
      kind: "unsupported",
      code: "MediaPlayPause",
    });
  });
});

describe("formatShortcut", () => {
  it("shows shortcuts the way macOS menus do", () => {
    expect(formatShortcut("Alt+Shift+Cmd+KeyR")).toBe("⌥⇧⌘R");
    expect(formatShortcut("Cmd+Ctrl+ArrowLeft")).toBe("⌃⌘←");
    expect(formatShortcut("Option+Command+Digit5")).toBe("⌥⌘5");
    expect(formatShortcut("F13")).toBe("F13");
    expect(formatShortcut(null)).toBe("");
  });

  it("round-trips what a key press records", () => {
    const recorded = fromKeyPress(
      press("Slash", { ctrlKey: true, shiftKey: true }),
    );
    expect(recorded.kind).toBe("shortcut");
    if (recorded.kind === "shortcut") {
      expect(formatShortcut(recorded.shortcut)).toBe("⌃⇧/");
    }
  });
});
