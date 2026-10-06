import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Check,
  Copy,
  FolderOpen,
  Maximize2,
  Redo2,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { useStore } from "zustand";

import { commands, type MarkupAction, type MarkupInit } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { TooltipProvider } from "@/components/ui/tooltip";
import { GestureContext } from "@/editor/inspector/gesture";
import { isTyping } from "@/editor/keys";
import { complete } from "@/editor/settings";
import { Tip } from "@/editor/Tip";
import { MarkupCanvas } from "@/markup/MarkupCanvas";
import { MarkupPanel } from "@/markup/MarkupPanel";
import type { Tool } from "@/markup/model";
import type { Renderer } from "@/markup/render";
import { finish, loadRenderer, oneAtATime } from "@/markup/session";
import { fileShortcut, type FileAction } from "@/markup/shortcuts";
import { markupStore, redo, undo, useMarkup } from "@/markup/store";
import { toolInfo, TOOLS } from "@/markup/tools";
import { stepZoom, zoomPercent } from "@/markup/view";

const GESTURE = {
  begin: () => markupStore.getState().beginGesture(),
  end: () => markupStore.getState().endGesture(),
};

type Status = { text: string; path?: string; error?: boolean };

function zoomBy(direction: 1 | -1) {
  const { shownZoom, scale, setZoom } = markupStore.getState();
  setZoom(stepZoom(shownZoom, direction, scale));
}

const zoomToFit = () => markupStore.getState().setZoom("fit");
const actualSize = () =>
  markupStore.getState().setZoom(1 / markupStore.getState().scale);

function Toolbar() {
  const tool = useMarkup((s) => s.tool);
  const scale = useMarkup((s) => s.scale);
  const shownZoom = useMarkup((s) => s.shownZoom);
  const canUndo = useStore(
    markupStore.temporal,
    (h) => h.pastStates.length > 0,
  );
  const canRedo = useStore(
    markupStore.temporal,
    (h) => h.futureStates.length > 0,
  );
  return (
    <header className="flex h-12 shrink-0 items-center gap-3 border-b px-3">
      <ToggleGroup
        type="single"
        size="sm"
        aria-label="Tools"
        value={tool}
        onValueChange={(value) =>
          value && markupStore.getState().setTool(value as Tool)
        }
      >
        {TOOLS.map(({ tool, label, key, icon: Icon }) => (
          <Tip key={tool} label={`${label} (${key.toUpperCase()})`}>
            <ToggleGroupItem value={tool} aria-label={label} className="px-2.5">
              <Icon />
            </ToggleGroupItem>
          </Tip>
        ))}
      </ToggleGroup>
      <div className="flex items-center">
        <Tip label="Undo (⌘Z)">
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label="Undo"
            disabled={!canUndo}
            onClick={() => undo(markupStore)}
          >
            <Undo2 />
          </Button>
        </Tip>
        <Tip label="Redo (⇧⌘Z)">
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label="Redo"
            disabled={!canRedo}
            onClick={() => redo(markupStore)}
          >
            <Redo2 />
          </Button>
        </Tip>
      </div>
      <div className="ml-auto flex items-center gap-1">
        <Tip label="Zoom out (⌘−)">
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label="Zoom out"
            onClick={() => zoomBy(-1)}
          >
            <ZoomOut />
          </Button>
        </Tip>
        <Tip label="Actual size (⌘1)">
          <button
            type="button"
            className="text-muted-foreground hover:text-foreground w-12 text-center font-mono text-xs tabular-nums"
            onClick={actualSize}
          >
            {zoomPercent(shownZoom, scale)}%
          </button>
        </Tip>
        <Tip label="Zoom in (⌘+)">
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label="Zoom in"
            onClick={() => zoomBy(1)}
          >
            <ZoomIn />
          </Button>
        </Tip>
        <Tip label="Zoom to fit (⌘0)">
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            aria-label="Zoom to fit"
            onClick={zoomToFit}
          >
            <Maximize2 />
          </Button>
        </Tip>
      </div>
    </header>
  );
}

function useShortcuts(actions: Record<FileAction, () => void>) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target)) return;
      const store = markupStore.getState();
      const key = e.key.toLowerCase();
      const onSlider =
        e.target instanceof HTMLElement &&
        e.target.getAttribute("role") === "slider";
      const run = (action: () => void) => {
        e.preventDefault();
        action();
      };

      const file = fileShortcut(e);
      if (file) {
        run(actions[file]);
        return;
      }
      if (e.metaKey) {
        if (key === "z") run(() => (e.shiftKey ? redo : undo)(markupStore));
        else if (key === "d" && store.selected)
          run(() => store.duplicate(store.selected!));
        else if (key === "=" || key === "+") run(() => zoomBy(1));
        else if (key === "-") run(() => zoomBy(-1));
        else if (key === "0") run(zoomToFit);
        else if (key === "1") run(actualSize);
        return;
      }
      if (e.ctrlKey || e.altKey) return;
      if ((key === "backspace" || key === "delete") && store.selected) {
        run(() => store.remove(store.selected!));
      } else if (key === "escape") {
        run(() =>
          store.tool === "crop" ? store.setTool("select") : store.select(null),
        );
      } else if (key === "enter" && store.tool === "crop") {
        run(() => store.setTool("select"));
      } else if (key.startsWith("arrow") && store.selected && !onSlider) {
        const step = (e.shiftKey ? 10 : 1) * store.scale;
        const dx =
          key === "arrowleft" ? -step : key === "arrowright" ? step : 0;
        const dy = key === "arrowup" ? -step : key === "arrowdown" ? step : 0;
        run(() => store.nudge(store.selected!, dx, dy));
      } else {
        const picked = TOOLS.find((info) => info.key === key);
        if (picked && !e.shiftKey) run(() => store.setTool(picked.tool));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [actions]);
}

function Editor({ init, renderer }: { init: MarkupInit; renderer: Renderer }) {
  const tool = useMarkup((s) => s.tool);
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);

  const [actions] = useState(() => {
    const finishWith = async (
      action: MarkupAction,
      done: (path: string | null) => Status,
    ) => {
      setBusy(true);
      setStatus(null);
      try {
        const result = await finish(
          renderer,
          markupStore.getState().doc,
          action,
        );
        setStatus(
          result.status === "ok"
            ? done(result.data)
            : { text: result.error, error: true },
        );
      } catch (e) {
        setStatus({ text: String(e), error: true });
      } finally {
        setBusy(false);
      }
    };
    const run = oneAtATime(async (file: FileAction) => {
      if (file === "copy") {
        await finishWith({ kind: "copy" }, () => ({
          text: "Copied to the clipboard.",
        }));
      } else if (file === "done") {
        await finishWith({ kind: "done" }, () => ({ text: "Saved." }));
      } else {
        const path = await save({
          defaultPath: `${init.name}.png`,
          filters: [{ name: "PNG image", extensions: ["png"] }],
        });
        if (!path) return;
        await finishWith({ kind: "saveAs", path }, (saved) => ({
          text: `Saved to ${saved ?? path}`,
          path: saved ?? path,
        }));
      }
    });
    return {
      copy: () => void run("copy"),
      saveAs: () => void run("saveAs"),
      done: () => void run("done"),
    };
  });
  useShortcuts(actions);

  return (
    <div className="bg-background text-foreground flex h-screen flex-col select-none">
      <Toolbar />
      <div className="flex min-h-0 flex-1">
        <main className="flex min-w-0 flex-1 flex-col">
          <MarkupCanvas renderer={renderer} />
          <footer className="flex h-12 shrink-0 items-center gap-2 border-t px-3">
            <p
              className={
                status?.error
                  ? "text-destructive min-w-0 flex-1 truncate text-xs"
                  : "text-muted-foreground min-w-0 flex-1 truncate text-xs"
              }
              title={status?.text}
            >
              {status?.text ?? toolInfo(tool).hint}
            </p>
            {status?.path && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => commands.revealInFinder(status.path!)}
              >
                <FolderOpen /> Show in Finder
              </Button>
            )}
            <Tip label="Copy the edited screenshot (⌘C)">
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={actions.copy}
              >
                <Copy /> Copy
              </Button>
            </Tip>
            <Tip label="Save a copy as a new PNG (⇧⌘S)">
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={actions.saveAs}
              >
                Save As…
              </Button>
            </Tip>
            <Tip label="Save over the screenshot and close (⌘Return)">
              <Button size="sm" disabled={busy} onClick={actions.done}>
                <Check /> Done
              </Button>
            </Tip>
          </footer>
        </main>
        <aside className="w-72 shrink-0 overflow-y-auto border-l">
          <GestureContext.Provider value={GESTURE}>
            <MarkupPanel />
          </GestureContext.Provider>
        </aside>
      </div>
    </div>
  );
}

export function Markup() {
  const [loaded, setLoaded] = useState<{
    init: MarkupInit;
    renderer: Renderer;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const result = await commands.markupOpen();
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      const init = result.data;
      const scale = Math.max(init.scaleFactor ?? 1, 1);
      const image = { width: init.width, height: init.height };
      const renderer = await loadRenderer(image, scale);
      if (cancelled) return;
      markupStore.getState().load(image, scale, complete(init.background));
      setLoaded({ init, renderer });
    })().catch((e) => setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <div className="flex h-screen items-center justify-center p-8">
        <p className="text-destructive text-sm">
          Can’t open the screenshot: {error}
        </p>
      </div>
    );
  }
  if (!loaded) {
    return (
      <div className="text-muted-foreground flex h-screen items-center justify-center text-sm">
        Opening…
      </div>
    );
  }
  return (
    <TooltipProvider delayDuration={400}>
      <Editor init={loaded.init} renderer={loaded.renderer} />
    </TooltipProvider>
  );
}
