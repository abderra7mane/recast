/** What a key press asks of the editor's Copy, Save As and Done buttons. */
export type FileAction = "copy" | "saveAs" | "done";

type Key = {
  key: string;
  metaKey: boolean;
  shiftKey: boolean;
  repeat: boolean;
};

/** The file action `e` stands for. A held key doesn't repeat it. */
export function fileShortcut(e: Key): FileAction | null {
  if (!e.metaKey || e.repeat) return null;
  const key = e.key.toLowerCase();
  if (key === "c" && !e.shiftKey) return "copy";
  if (key === "s" && e.shiftKey) return "saveAs";
  if (key === "enter") return "done";
  return null;
}
