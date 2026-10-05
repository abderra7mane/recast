/** Key codes Recast accepts in a shortcut, and how macOS shows them. */
const KEY_SYMBOLS: Record<string, string> = {
  Space: "Space",
  Enter: "↩",
  Tab: "⇥",
  Backspace: "⌫",
  Delete: "⌦",
  Home: "↖",
  End: "↘",
  PageUp: "⇞",
  PageDown: "⇟",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};
for (const letter of "ABCDEFGHIJKLMNOPQRSTUVWXYZ") {
  KEY_SYMBOLS[`Key${letter}`] = letter;
}
for (let digit = 0; digit <= 9; digit++) {
  KEY_SYMBOLS[`Digit${digit}`] = `${digit}`;
}
for (let n = 1; n <= 20; n++) {
  KEY_SYMBOLS[`F${n}`] = `F${n}`;
}

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "CapsLock",
  "Fn",
  "FnLock",
]);

const MODIFIERS = [
  { name: "Ctrl", symbol: "⌃", flag: "ctrlKey" },
  { name: "Alt", symbol: "⌥", flag: "altKey" },
  { name: "Shift", symbol: "⇧", flag: "shiftKey" },
  { name: "Cmd", symbol: "⌘", flag: "metaKey" },
] as const;

export type KeyPress = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

export type Recorded =
  | { kind: "shortcut"; shortcut: string }
  /** Only modifiers are held so far; `symbols` shows them. */
  | { kind: "pending"; symbols: string }
  | { kind: "clear" }
  | { kind: "cancel" }
  | { kind: "unsupported"; code: string };

const heldSymbols = (press: KeyPress) =>
  MODIFIERS.filter((m) => press[m.flag])
    .map((m) => m.symbol)
    .join("");

/** What a key press means while recording a shortcut. */
export function fromKeyPress(press: KeyPress): Recorded {
  const anyModifier = MODIFIERS.some((m) => press[m.flag]);
  if (MODIFIER_CODES.has(press.code)) {
    return { kind: "pending", symbols: heldSymbols(press) };
  }
  if (press.code === "Escape" && !anyModifier) return { kind: "cancel" };
  if ((press.code === "Backspace" || press.code === "Delete") && !anyModifier) {
    return { kind: "clear" };
  }
  if (!(press.code in KEY_SYMBOLS)) {
    return { kind: "unsupported", code: press.code };
  }
  const names = MODIFIERS.filter((m) => press[m.flag]).map((m) => m.name);
  return { kind: "shortcut", shortcut: [...names, press.code].join("+") };
}

/** `Alt+Shift+Cmd+KeyR` → `⌥⇧⌘R`. */
export function formatShortcut(shortcut: string | null | undefined): string {
  if (!shortcut) return "";
  const parts = shortcut.split("+");
  const key = parts.pop() ?? "";
  const held = new Set(parts.map((p) => p.toLowerCase()));
  const aliases: Record<string, string[]> = {
    Ctrl: ["ctrl", "control"],
    Alt: ["alt", "option"],
    Shift: ["shift"],
    Cmd: ["cmd", "command", "super"],
  };
  const symbols = MODIFIERS.filter((m) =>
    aliases[m.name].some((alias) => held.has(alias)),
  )
    .map((m) => m.symbol)
    .join("");
  return symbols + (KEY_SYMBOLS[key] ?? key);
}
