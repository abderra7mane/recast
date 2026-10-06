import { describe, expect, it } from "vitest";

import { fileShortcut } from "@/markup/shortcuts";

const press = (
  key: string,
  extra: { shiftKey?: boolean; repeat?: boolean } = {},
) => ({
  key,
  metaKey: true,
  shiftKey: false,
  repeat: false,
  ...extra,
});

describe("file shortcuts", () => {
  it("maps ⌘C, ⇧⌘S and ⌘Return", () => {
    expect(fileShortcut(press("c"))).toBe("copy");
    expect(fileShortcut(press("S", { shiftKey: true }))).toBe("saveAs");
    expect(fileShortcut(press("Enter"))).toBe("done");
    expect(fileShortcut(press("s"))).toBe(null);
    expect(fileShortcut(press("C", { shiftKey: true }))).toBe(null);
    expect(fileShortcut({ ...press("c"), metaKey: false })).toBe(null);
  });

  it("ignores a held key", () => {
    expect(fileShortcut(press("c", { repeat: true }))).toBe(null);
    expect(fileShortcut(press("Enter", { repeat: true }))).toBe(null);
  });
});
