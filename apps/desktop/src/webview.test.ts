import { describe, expect, it } from "vitest";

import { isPageShortcut, type KeyPress } from "@/webview";

const press = (code: string, held: Partial<KeyPress> = {}): KeyPress => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...held,
});

describe("isPageShortcut", () => {
  it("matches reload shortcuts", () => {
    expect(isPageShortcut(press("F5"))).toBe(true);
    expect(isPageShortcut(press("KeyR", { metaKey: true }))).toBe(true);
    expect(
      isPageShortcut(press("KeyR", { metaKey: true, shiftKey: true })),
    ).toBe(true);
  });

  it("matches Web Inspector shortcuts", () => {
    for (const code of ["KeyI", "KeyC", "KeyJ"]) {
      expect(isPageShortcut(press(code, { metaKey: true, altKey: true }))).toBe(
        true,
      );
    }
  });

  it("leaves editor shortcuts alone", () => {
    expect(isPageShortcut(press("KeyR"))).toBe(false);
    expect(isPageShortcut(press("KeyC", { metaKey: true }))).toBe(false);
    expect(isPageShortcut(press("KeyI", { metaKey: true }))).toBe(false);
    expect(isPageShortcut(press("KeyR", { ctrlKey: true }))).toBe(false);
  });
});
