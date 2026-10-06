import { useEffect, useRef, useState } from "react";
import { Pause, Play, Redo2, Repeat, Undo2 } from "lucide-react";
import { cn } from "cn";
import { useStore } from "zustand";

import { commands, type EditorInit } from "@/bindings";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ExportDialog } from "@/editor/ExportDialog";
import { Inspector, type InspectorTab } from "@/editor/inspector/Inspector";
import {
  playbackStore,
  seek,
  setLooping,
  togglePlay,
  usePlayback,
} from "@/editor/playback";
import { Preview } from "@/editor/Preview";
import {
  complete,
  FRAME_MS,
  formatTime,
  type Settings,
} from "@/editor/settings";
import { editorStore, redo, undo, useEditor } from "@/editor/store";
import { createSequence, createSettingsSync } from "@/editor/sync";
import { Timeline } from "@/editor/Timeline";
import { isTyping } from "@/editor/keys";
import { Tip } from "@/editor/Tip";

/** Sends edits to the backend; returns a function that waits until all are applied. */
function useSettingsSync() {
  const flush = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    let inflight: Promise<unknown> = Promise.resolve();
    const sequence = createSequence();
    const sync = createSettingsSync<Settings>((settings) => {
      const seq = sequence.next();
      inflight = commands.editorSetSettings(settings, seq).then((result) => {
        if (result.status === "ok" && result.data && sequence.accept(seq)) {
          editorStore.getState().setAutoSegments(complete(result.data));
        }
      });
    }, 100);
    const unsubscribe = editorStore.subscribe((state, prev) => {
      if (prev.gesture && !state.gesture) sync.flush();
      if (!state.settings || !prev.settings || state.settings === prev.settings)
        return;
      if (state.lastChange === "live") sync.schedule(state.settings);
      else sync.commit(state.settings);
    });
    flush.current = async () => {
      sync.flush();
      await inflight;
    };
    return () => {
      sync.flush();
      unsubscribe();
    };
  }, []);
  return flush;
}

function useShortcuts(durationMs: number) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target)) return;
      const onSlider =
        e.target instanceof HTMLElement &&
        e.target.getAttribute("role") === "slider";
      const step = (direction: number) => {
        const { timeMs } = playbackStore.getState();
        seek(
          Math.min(
            Math.max(timeMs + direction * (e.shiftKey ? 1000 : FRAME_MS), 0),
            durationMs,
          ),
        );
      };
      if (e.metaKey && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      } else if (e.key === " " && !e.metaKey) {
        e.preventDefault();
        togglePlay();
      } else if (e.key === "ArrowLeft" && !onSlider) {
        e.preventDefault();
        step(-1);
      } else if (e.key === "ArrowRight" && !onSlider) {
        e.preventDefault();
        step(1);
      } else if (e.key === "Delete" || e.key === "Backspace") {
        const { selected, deleteSegment } = editorStore.getState();
        if (selected !== null) {
          e.preventDefault();
          deleteSegment(selected);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [durationMs]);
}

function PlayerBar({ durationMs }: { durationMs: number }) {
  const playing = usePlayback((s) => s.playing);
  const looping = usePlayback((s) => s.looping);
  const timeMs = usePlayback((s) => s.timeMs);
  const stats = usePlayback(
    (s) =>
      `${s.renderer ?? "…"} · ${s.fps} fps${s.seekMs === null ? "" : ` · seek ${Math.round(s.seekMs)} ms`}`,
  );
  return (
    <div className="flex h-12 items-center gap-3 border-t px-4">
      <span className="text-muted-foreground w-28 font-mono text-xs tabular-nums">
        {formatTime(timeMs)} / {formatTime(durationMs)}
      </span>
      <div className="flex flex-1 items-center justify-center gap-1">
        <Tip label={playing ? "Pause (Space)" : "Play (Space)"}>
          <Button
            size="icon"
            variant="secondary"
            className="size-9 rounded-full"
            aria-label={playing ? "Pause" : "Play"}
            onMouseDown={(e) => e.preventDefault()}
            onClick={togglePlay}
          >
            {playing ? <Pause /> : <Play />}
          </Button>
        </Tip>
        <Tip label={looping ? "Stop looping" : "Play the clip in a loop"}>
          <Button
            size="icon"
            variant="ghost"
            className={cn("size-8", looping && "text-sky-400")}
            aria-label="Loop"
            aria-pressed={looping}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setLooping(!looping)}
          >
            <Repeat />
          </Button>
        </Tip>
      </div>
      <span className="text-muted-foreground w-28 text-right font-mono text-[10px]">
        {import.meta.env.DEV ? stats : ""}
      </span>
    </div>
  );
}

function HistoryButtons() {
  const canUndo = useStore(
    editorStore.temporal,
    (h) => h.pastStates.length > 0,
  );
  const canRedo = useStore(
    editorStore.temporal,
    (h) => h.futureStates.length > 0,
  );
  return (
    <div className="flex items-center">
      <Tip label="Undo (⌘Z)">
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          aria-label="Undo"
          disabled={!canUndo}
          onClick={undo}
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
          onClick={redo}
        >
          <Redo2 />
        </Button>
      </Tip>
    </div>
  );
}

export function Editor() {
  const [init, setInit] = useState<EditorInit | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<InspectorTab>("background");
  const flushSettings = useSettingsSync();
  const durationMs = useEditor((s) => s.durationMs);
  const selected = useEditor((s) => s.selected);
  useShortcuts(durationMs);

  useEffect(() => {
    void commands.editorOpen().then((result) => {
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      const data = result.data;
      editorStore
        .getState()
        .load(
          complete(data.settings),
          data.durationMs ?? 0,
          complete(data.autoSegments),
        );
      playbackStore.setState({ timeMs: complete(data.settings).trim.startMs });
      setInit(data);
    });
  }, []);

  useEffect(() => {
    if (selected !== null) setTab("zoom");
  }, [selected]);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <p className="text-destructive text-sm">
          Can’t open the project: {error}
        </p>
      </div>
    );
  }
  if (!init) {
    return (
      <div className="text-muted-foreground flex h-full items-center justify-center text-sm">
        Opening…
      </div>
    );
  }

  return (
    <TooltipProvider delayDuration={400}>
      <div className="bg-background flex h-full flex-col">
        <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
          <h1 className="truncate text-sm font-semibold">
            {init.project.name}
          </h1>
          <HistoryButtons />
          <div className="ml-auto">
            <ExportDialog
              bundlePath={init.bundlePath}
              suggestedPath={init.exportPath}
              beforeExport={() => flushSettings.current()}
            />
          </div>
        </header>
        <div className="flex min-h-0 flex-1">
          <main className="flex min-w-0 flex-1 flex-col">
            <div className="min-h-0 flex-1 bg-black/40 p-6">
              <Preview url={init.previewUrl} />
            </div>
            <PlayerBar durationMs={durationMs} />
          </main>
          <aside className="w-80 shrink-0 border-l">
            <Inspector project={init.project} tab={tab} onTabChange={setTab} />
          </aside>
        </div>
        <div className="h-52 shrink-0 border-t">
          <Timeline clicks={init.clicks} />
        </div>
      </div>
    </TooltipProvider>
  );
}
