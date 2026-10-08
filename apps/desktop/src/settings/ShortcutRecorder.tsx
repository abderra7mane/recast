import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { formatShortcut, fromKeyPress } from "@/settings/shortcuts";

type Props = {
  label: string;
  shortcut: string | null;
  /** A conflict or registration failure to show under the shortcut. */
  error?: string | null;
  /** Saves the shortcut (`null` turns it off); resolves to an error message, if any. */
  onChange: (shortcut: string | null) => Promise<string | null>;
  /** Called when recording starts and stops, so global shortcuts can pause meanwhile. */
  onRecording?: (recording: boolean) => void;
};

export function ShortcutRecorder({
  label,
  shortcut,
  error,
  onChange,
  onRecording,
}: Props) {
  const [recording, setRecording] = useState(false);
  const [pending, setPending] = useState("");
  const [problem, setProblem] = useState<string | null>(null);

  const setListening = (listening: boolean) => {
    setRecording(listening);
    setPending("");
    onRecording?.(listening);
  };

  const save = async (next: string | null) => {
    setListening(false);
    setProblem(await onChange(next));
  };

  useEffect(() => {
    if (!recording) return;
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const recorded = fromKeyPress(event);
      switch (recorded.kind) {
        case "pending":
          setPending(recorded.symbols);
          break;
        case "cancel":
          setListening(false);
          break;
        case "clear":
          void save(null);
          break;
        case "unsupported":
          setProblem(`${recorded.code} can't be used in a shortcut.`);
          break;
        case "shortcut":
          void save(recorded.shortcut);
          break;
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      const recorded = fromKeyPress(event);
      setPending(recorded.kind === "pending" ? recorded.symbols : "");
    };
    const stop = () => setListening(false);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    window.addEventListener("blur", stop);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
      window.removeEventListener("blur", stop);
    };
    // `save` and `setListening` only close over setters and props that don't
    // change while recording.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording]);

  const message = problem ?? error;
  const shown = recording
    ? pending || "Type shortcut…"
    : formatShortcut(shortcut) || "Record Shortcut";

  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between gap-3">
        <span className="text-sm">{label}</span>
        <div className="flex items-center gap-1">
          <Button
            variant={recording ? "default" : "outline"}
            size="sm"
            className="w-40 font-mono"
            aria-label={`${label} shortcut`}
            aria-pressed={recording}
            onClick={() => {
              setProblem(null);
              setListening(!recording);
            }}
          >
            {shown}
          </Button>
          {shortcut && !recording ? (
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={`Turn off ${label} shortcut`}
              onClick={() => void save(null)}
            >
              <X />
            </Button>
          ) : (
            <span className="size-8" />
          )}
        </div>
      </div>
      {recording && (
        <p className="text-muted-foreground text-xs">
          Press the new shortcut. Esc cancels, Delete turns it off.
        </p>
      )}
      {message && (
        <p role="alert" className="text-destructive text-xs">
          {message}
        </p>
      )}
    </div>
  );
}
