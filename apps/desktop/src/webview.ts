import { isTyping } from "@/editor/keys";

export type KeyPress = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

/** Whether `e` reloads the page or opens the Web Inspector. */
export function isPageShortcut(e: KeyPress) {
  if (e.code === "F5") return true;
  if (!e.metaKey || e.ctrlKey) return false;
  if (e.code === "KeyR") return !e.altKey;
  return e.altKey && !e.shiftKey && ["KeyI", "KeyC", "KeyJ"].includes(e.code);
}

/** Hides the webview's Reload / Inspect Element menu and their shortcuts. */
export function lockPage() {
  window.addEventListener("contextmenu", (e) => {
    if (!isTyping(e.target)) e.preventDefault();
  });
  window.addEventListener(
    "keydown",
    (e) => {
      if (isPageShortcut(e)) e.preventDefault();
    },
    { capture: true },
  );
}
