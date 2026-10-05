import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
} from "react";
import { Maximize2, ZoomIn, ZoomOut } from "lucide-react";
import { cn } from "cn";

import type { ClickMarker } from "@/bindings";
import { Button } from "@/components/ui/button";
import { usePlayback, seek, playbackStore } from "@/editor/playback";
import { formatTime, type Segment } from "@/editor/settings";
import { editorStore, useEditor, visibleSegments } from "@/editor/store";
import {
  fitScale,
  moveSegment,
  moveTrim,
  pxToTime,
  resizeSegment,
  rulerTicks,
  timeToPx,
  zoomScale,
} from "@/editor/timeline-math";

const GUTTER = 16;

type Drag =
  | { kind: "seek" }
  | { kind: "trim"; edge: "start" | "end" }
  | { kind: "move"; index: number; origin: Segment[]; startX: number }
  | { kind: "resize"; index: number; edge: "start" | "end"; origin: Segment[] };

function Playhead({ scale }: { scale: number }) {
  const timeMs = usePlayback((s) => s.timeMs);
  return (
    <div
      className="pointer-events-none absolute top-0 bottom-0 z-20 w-px bg-red-500"
      style={{ left: GUTTER + timeToPx(timeMs, scale) }}
    >
      <div className="absolute -top-0.5 -left-[5px] size-[11px] rounded-full bg-red-500" />
    </div>
  );
}

export function Timeline({ clicks }: { clicks: ClickMarker[] }) {
  const durationMs = useEditor((s) => s.durationMs);
  const trim = useEditor((s) => s.settings?.trim);
  const segments = useEditor(visibleSegments);
  const auto = useEditor((s) => s.settings?.zoom.auto ?? false);
  const selected = useEditor((s) => s.selected);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const [scale, setScale] = useState(0.05);
  const [fitted, setFitted] = useState(true);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element || !fitted) return;
    const fit = () =>
      setScale(fitScale(element.clientWidth - 2 * GUTTER, durationMs));
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [durationMs, fitted]);

  useEffect(
    () =>
      playbackStore.subscribe(({ timeMs, playing }) => {
        const element = scrollRef.current;
        if (!playing || !element) return;
        const x = GUTTER + timeToPx(timeMs, scale);
        if (
          x < element.scrollLeft ||
          x > element.scrollLeft + element.clientWidth - GUTTER
        ) {
          element.scrollLeft = x - GUTTER;
        }
      }),
    [scale],
  );

  const zoom = (factor: number, anchorX?: number) => {
    const element = scrollRef.current;
    if (!element) return;
    const anchor = anchorX ?? element.clientWidth / 2;
    const t = (element.scrollLeft + anchor - GUTTER) / scale;
    const next = zoomScale(scale, factor);
    setFitted(false);
    setScale(next);
    requestAnimationFrame(() => {
      element.scrollLeft = GUTTER + t * next - anchor;
    });
  };

  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const rect = element.getBoundingClientRect();
      zoom(Math.exp(-e.deltaY * 0.01), e.clientX - rect.left);
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  });

  const timeAt = (clientX: number) => {
    const rect = contentRef.current!.getBoundingClientRect();
    return pxToTime(clientX - rect.left - GUTTER, scale, durationMs);
  };

  const start = (e: PointerEvent, next: Drag) => {
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = next;
    if (next.kind !== "seek") editorStore.getState().beginGesture();
    if (next.kind === "seek") seek(timeAt(e.clientX));
  };

  const move = (e: PointerEvent) => {
    const current = drag.current;
    if (!current) return;
    const t = timeAt(e.clientX);
    const state = editorStore.getState();
    switch (current.kind) {
      case "seek":
        seek(t);
        break;
      case "trim":
        if (state.settings) {
          const { startMs, endMs } = state.settings.trim;
          state.setTrim(
            moveTrim({ startMs, endMs }, current.edge, t, durationMs),
          );
          seek(t);
        }
        break;
      case "move":
        state.setSegments(
          moveSegment(
            current.origin,
            current.index,
            (e.clientX - current.startX) / scale,
            durationMs,
          ),
        );
        break;
      case "resize":
        state.setSegments(
          resizeSegment(
            current.origin,
            current.index,
            current.edge,
            t,
            durationMs,
          ),
        );
        break;
    }
  };

  const finish = () => {
    if (drag.current && drag.current.kind !== "seek")
      editorStore.getState().endGesture();
    drag.current = null;
  };

  const handlers = {
    onPointerMove: move,
    onPointerUp: finish,
    onLostPointerCapture: finish,
  };
  const width = timeToPx(durationMs, scale) + 2 * GUTTER;
  const { stepMs, ticks } = rulerTicks(scale, durationMs);
  const trimStart = trim?.startMs ?? 0;
  const trimEnd = trim?.endMs ?? durationMs;
  const x = (t: number) => GUTTER + timeToPx(t, scale);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-1 px-3 py-1.5">
        <span className="text-muted-foreground text-xs">
          {auto ? "Auto zoom" : "Manual zoom"} · {segments.length} segment
          {segments.length === 1 ? "" : "s"}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <Button
            size="icon"
            variant="ghost"
            className="size-7"
            aria-label="Zoom timeline out"
            onClick={() => zoom(1 / 1.5)}
          >
            <ZoomOut />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="size-7"
            aria-label="Zoom timeline in"
            onClick={() => zoom(1.5)}
          >
            <ZoomIn />
          </Button>
          <Button
            size="icon"
            variant="ghost"
            className="size-7"
            aria-label="Fit timeline"
            onClick={() => setFitted(true)}
          >
            <Maximize2 />
          </Button>
        </div>
      </div>
      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden select-none"
      >
        <div
          ref={contentRef}
          className="relative h-full"
          style={{ width }}
          {...handlers}
        >
          <Playhead scale={scale} />

          <div
            className="relative h-6 cursor-pointer overflow-hidden border-b"
            onPointerDown={(e) => start(e, { kind: "seek" })}
          >
            {ticks.map((t) => (
              <div
                key={t}
                className="absolute top-0 h-full"
                style={{ left: x(t) }}
              >
                <div className="bg-border h-2 w-px" />
                <span className="text-muted-foreground absolute top-1.5 left-1 text-[10px] tabular-nums">
                  {formatTime(t, stepMs < 1000)}
                </span>
              </div>
            ))}
          </div>

          <div
            className="relative mt-2 h-12"
            onPointerDown={(e) => start(e, { kind: "seek" })}
          >
            <div
              className="absolute inset-y-0 rounded-md bg-sky-900/40 ring-1 ring-sky-500/30"
              style={{ left: x(0), width: timeToPx(durationMs, scale) }}
            />
            <div
              className="absolute inset-y-0 rounded-md bg-sky-600/50 ring-1 ring-sky-400/70"
              style={{
                left: x(trimStart),
                width: timeToPx(trimEnd - trimStart, scale),
              }}
            >
              <span className="absolute top-1 left-3 text-[11px] font-medium text-sky-50/90">
                Clip · {formatTime(trimEnd - trimStart)}
              </span>
            </div>
            {clicks.map((c, i) => (
              <div
                key={i}
                title={`${c.button} click at ${formatTime(c.tMs ?? 0)}`}
                className={cn(
                  "pointer-events-none absolute bottom-1 h-3 w-0.5 rounded-full",
                  c.button === "left" ? "bg-amber-300" : "bg-fuchsia-300",
                )}
                style={{ left: x(c.tMs ?? 0) }}
              />
            ))}
            {(["start", "end"] as const).map((edge) => (
              <div
                key={edge}
                role="slider"
                aria-label={`Trim ${edge}`}
                aria-valuenow={edge === "start" ? trimStart : trimEnd}
                className="absolute inset-y-0 z-10 w-2.5 cursor-ew-resize rounded-sm bg-sky-300 hover:bg-white"
                style={{
                  left:
                    x(edge === "start" ? trimStart : trimEnd) -
                    (edge === "start" ? 0 : 10),
                }}
                onPointerDown={(e) => start(e, { kind: "trim", edge })}
              />
            ))}
          </div>

          <div
            className="relative mt-2 h-9"
            onPointerDown={(e) => {
              if (e.target === e.currentTarget)
                editorStore.getState().select(null);
            }}
            onDoubleClick={(e) => {
              if (e.target === e.currentTarget)
                editorStore.getState().addSegment(timeAt(e.clientX));
            }}
          >
            <div
              className="pointer-events-none absolute inset-y-0 rounded-md bg-white/[0.03]"
              style={{ left: x(0), width: timeToPx(durationMs, scale) }}
            />
            {segments.map((s, index) => (
              <div
                key={index}
                className={cn(
                  "absolute inset-y-0 flex cursor-grab items-center justify-center overflow-hidden rounded-md text-[11px] font-medium active:cursor-grabbing",
                  auto
                    ? "bg-violet-500/30 text-violet-100 ring-1 ring-violet-400/40"
                    : "bg-violet-500/60 text-white ring-1 ring-violet-300/60",
                  selected === index && "ring-2 ring-white",
                )}
                style={{
                  left: x(s.startMs),
                  width: timeToPx(s.endMs - s.startMs, scale),
                }}
                onPointerDown={(e) => {
                  editorStore.getState().select(index);
                  start(e, {
                    kind: "move",
                    index,
                    origin: segments,
                    startX: e.clientX,
                  });
                }}
              >
                <span className="pointer-events-none truncate px-2">
                  {s.level.toFixed(1)}×
                </span>
                {(["start", "end"] as const).map((edge) => (
                  <div
                    key={edge}
                    className={cn(
                      "absolute inset-y-0 w-1.5 cursor-ew-resize hover:bg-white/60",
                      edge === "start" ? "left-0" : "right-0",
                    )}
                    onPointerDown={(e) => {
                      editorStore.getState().select(index);
                      start(e, {
                        kind: "resize",
                        index,
                        edge,
                        origin: segments,
                      });
                    }}
                  />
                ))}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
