import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Upload } from "lucide-react";

import { commands, events } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Progress } from "@/components/ui/progress";
import { pause } from "@/editor/playback";

type ExportState =
  | { kind: "idle" }
  | { kind: "starting"; output: string }
  | { kind: "running"; output: string; frame: number; total: number }
  | { kind: "cancelling"; output: string }
  | { kind: "done"; output: string }
  | { kind: "failed"; message: string }
  | { kind: "cancelled" };

/**
 * Export button and its modal. The editor is blocked while an export runs, so
 * the file always matches the edits shown when it started.
 */
export function ExportDialog({
  bundlePath,
  suggestedPath,
  beforeExport,
}: {
  bundlePath: string;
  suggestedPath: string;
  beforeExport: () => Promise<void>;
}) {
  const [state, setState] = useState<ExportState>({ kind: "idle" });

  useEffect(() => {
    const progress = events.exportProgress.listen(({ payload }) => {
      if (payload.bundlePath !== bundlePath) return;
      setState((s) =>
        s.kind === "starting" || s.kind === "running"
          ? {
              kind: "running",
              output: s.output,
              frame: payload.frame,
              total: payload.totalFrames,
            }
          : s,
      );
    });
    const finished = events.exportFinished.listen(({ payload }) => {
      if (payload.bundlePath !== bundlePath) return;
      if (payload.cancelled) setState({ kind: "cancelled" });
      else if (payload.error)
        setState({ kind: "failed", message: payload.error });
      else setState({ kind: "done", output: payload.outputPath });
    });
    return () => {
      void progress.then((off) => off());
      void finished.then((off) => off());
    };
  }, [bundlePath]);

  const start = async () => {
    const output = await save({
      defaultPath: suggestedPath,
      filters: [{ name: "MPEG-4 video", extensions: ["mp4"] }],
    });
    if (!output) return;
    pause();
    setState({ kind: "starting", output });
    await beforeExport();
    const result = await commands.exportStart(output);
    if (result.status === "error")
      setState({ kind: "failed", message: result.error });
  };

  const cancel = () => {
    if (state.kind === "running") {
      setState({ kind: "cancelling", output: state.output });
      void commands.exportCancel();
    }
  };

  const busy =
    state.kind === "starting" ||
    state.kind === "running" ||
    state.kind === "cancelling";
  const percent =
    state.kind === "running" && state.total > 0
      ? (state.frame / state.total) * 100
      : 0;

  return (
    <>
      <Button size="sm" onClick={() => void start()} disabled={busy}>
        <Upload /> Export
      </Button>
      <Dialog
        open={state.kind !== "idle"}
        onOpenChange={(open) => !open && !busy && setState({ kind: "idle" })}
      >
        <DialogContent
          showCloseButton={!busy}
          onEscapeKeyDown={(e) => busy && e.preventDefault()}
          onPointerDownOutside={(e) => busy && e.preventDefault()}
          onInteractOutside={(e) => busy && e.preventDefault()}
        >
          <DialogHeader>
            <DialogTitle>
              {busy && "Exporting…"}
              {state.kind === "done" && "Export finished"}
              {state.kind === "failed" && "Export failed"}
              {state.kind === "cancelled" && "Export cancelled"}
            </DialogTitle>
            <DialogDescription className="break-all">
              {"output" in state && state.output}
              {state.kind === "failed" && state.message}
              {state.kind === "cancelled" && "The partial file was removed."}
            </DialogDescription>
          </DialogHeader>
          {busy && (
            <div className="space-y-2">
              <Progress value={percent} />
              <p className="text-muted-foreground text-xs tabular-nums">
                {state.kind === "running"
                  ? `Frame ${state.frame} of ${state.total} · ${Math.floor(percent)}%`
                  : state.kind === "cancelling"
                    ? "Cancelling…"
                    : "Preparing…"}
              </p>
            </div>
          )}
          <DialogFooter>
            {busy && (
              <Button
                variant="outline"
                onClick={cancel}
                disabled={state.kind !== "running"}
              >
                Cancel
              </Button>
            )}
            {state.kind === "done" && (
              <Button
                variant="outline"
                onClick={() => void commands.revealInFinder(state.output)}
              >
                <FolderOpen /> Reveal in Finder
              </Button>
            )}
            {!busy && (
              <Button onClick={() => setState({ kind: "idle" })}>Close</Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
