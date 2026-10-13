import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Pause, Play, Redo2, Repeat, Undo2 } from "lucide-react";
import { cn } from "cn";
import { useStore } from "zustand";

import { commands, type EditorInit } from "@/bindings";
import { Button } from "@/components/ui/button";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  AnnotationLayer,
  type AnnotationLayerHandle,
} from "@/editor/annotations/AnnotationLayer";
import {
  createRasterUploader,
  type RasterUploader,
} from "@/editor/annotations/raster";
import { AnnotationToolbar } from "@/editor/annotations/Toolbar";
import { ANNOTATION_TOOLS } from "@/editor/annotations/tools";
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
import {
  annotationById,
  editorStore,
  redo,
  undo,
  useEditor,
} from "@/editor/store";
import { createSequence, createSettingsSync } from "@/editor/sync";
import { Timeline } from "@/editor/Timeline";
import { isTyping } from "@/editor/keys";
import { Tip } from "@/editor/Tip";
import { measureText } from "@/markup/render";
import { makeCanvas } from "@/markup/session";

type Edits = { settings: Settings; hidden: string | null };

/**
 * Sends edits, and which annotation the preview leaves out, to the backend, and the
 * annotation images it asks for; returns a function that waits until all are applied.
 */
function useSettingsSync(uploader: RasterUploader) {
  const flush = useRef<() => Promise<void>>(async () => {});
  useEffect(() => {
    let inflight: Promise<unknown> = Promise.resolve();
    const sequence = createSequence();
    const sync = createSettingsSync<Edits>(({ settings, hidden }) => {
      const seq = sequence.next();
      playbackStore.setState({ editsSent: seq });
      inflight = commands
        .editorSetSettings(settings, hidden, seq)
        .then((result) => {
          if (result.status === "ok" && result.data && sequence.accept(seq)) {
            const { autoSegments, missingRasters } = result.data;
            editorStore.getState().setAutoSegments(complete(autoSegments));
            uploader.upload(missingRasters);
          }
        });
    }, 100);
    const unsubscribe = editorStore.subscribe((state, prev) => {
      if (prev.gesture && !state.gesture) sync.flush();
      if (!state.settings || !prev.settings) return;
      const edits = { settings: state.settings, hidden: state.hidden };
      if (state.hidden !== prev.hidden) sync.commit(edits);
      else if (state.settings === prev.settings) return;
      else if (state.lastChange === "live") sync.schedule(edits);
      else sync.commit(edits);
    });
    flush.current = async () => {
      sync.flush();
      await inflight;
      await uploader.flush();
    };
    return () => {
      sync.flush();
      unsubscribe();
    };
  }, [uploader]);
  return flush;
}

/** Draws annotation images with the screenshot editor's code and sends them. */
function useRasterUploader(pxPerPt: () => number) {
  return useMemo(() => {
    const measuring = makeCanvas(1, 1).ctx;
    return createRasterUploader({
      send: (body) => invoke("editor_annotation_raster", body),
      shapeOf: (id) => annotationById(editorStore.getState(), id)?.shape,
      pxPerPt,
      makeCanvas,
      measure: (shape) => measureText(measuring, shape, 1),
    });
  }, [pxPerPt]);
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
        const {
          selected,
          deleteSegment,
          selectedAnnotation,
          deleteAnnotation,
        } = editorStore.getState();
        if (selectedAnnotation !== null) {
          e.preventDefault();
          deleteAnnotation(selectedAnnotation);
        } else if (selected !== null) {
          e.preventDefault();
          deleteSegment(selected);
        }
      } else if (e.key === "Escape") {
        const { tool, setTool, selectAnnotation } = editorStore.getState();
        if (tool !== "select") setTool("select");
        else selectAnnotation(null);
      } else if (!e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        const picked = ANNOTATION_TOOLS.find(
          (info) => info.key === e.key.toLowerCase(),
        );
        if (picked) {
          e.preventDefault();
          editorStore.getState().setTool(picked.tool);
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
            className={cn("size-8", looping && "text-brand")}
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
  const rasterScale = useRef(2);
  const [pxPerPt] = useState(() => () => rasterScale.current);
  const uploader = useRasterUploader(pxPerPt);
  const flushSettings = useSettingsSync(uploader);
  const layer = useRef<AnnotationLayerHandle>(null);
  const durationMs = useEditor((s) => s.durationMs);
  const selected = useEditor((s) => s.selected);
  const selectedAnnotation = useEditor((s) => s.selectedAnnotation);
  const drawing = useEditor((s) => s.tool !== "select");
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
      const { width, bounds } = data.project.recording;
      // Twice the recording's own pixels, so annotations stay sharp when zoomed in.
      rasterScale.current = (2 * width) / Math.max(bounds.width ?? 0, 1);
      uploader.upload(data.missingRasters);
      setInit(data);
    });
  }, [uploader]);

  useEffect(() => {
    if (selected !== null) setTab("zoom");
  }, [selected]);

  useEffect(() => {
    if (selectedAnnotation !== null || drawing) setTab("annotations");
  }, [selectedAnnotation, drawing]);

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
          <div className="mx-auto">
            <AnnotationToolbar />
          </div>
          <div>
            <ExportDialog
              bundlePath={init.bundlePath}
              suggestedPath={init.exportPath}
              beforeExport={() => flushSettings.current()}
            />
          </div>
        </header>
        <div className="flex min-h-0 flex-1">
          <main className="flex min-w-0 flex-1 flex-col">
            <div className="bg-stage min-h-0 flex-1 p-6">
              <Preview
                url={init.previewUrl}
                onFrame={(frame) => layer.current?.frameShown(frame)}
              >
                <AnnotationLayer
                  ref={layer}
                  screen={{
                    width: init.project.recording.bounds.width ?? 1,
                    height: init.project.recording.bounds.height ?? 1,
                  }}
                />
              </Preview>
            </div>
            <PlayerBar durationMs={durationMs} />
          </main>
          <aside className="w-80 shrink-0 border-l">
            <Inspector project={init.project} tab={tab} onTabChange={setTab} />
          </aside>
        </div>
        <div className="h-64 shrink-0 border-t">
          <Timeline clicks={init.clicks} />
        </div>
      </div>
    </TooltipProvider>
  );
}
