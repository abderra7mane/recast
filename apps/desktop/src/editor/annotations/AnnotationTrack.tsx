import { memo, type PointerEvent } from "react";
import { cn } from "cn";
import { useShallow } from "zustand/react/shallow";

import { isObscure } from "@/editor/annotations/model";
import { annotationLabel } from "@/editor/annotations/tools";
import type { Annotation } from "@/editor/settings";
import { editorStore, useEditor } from "@/editor/store";
import { timeToPx } from "@/editor/timeline-math";

export type TrackPress =
  | { kind: "annotation-move"; id: string; startX: number }
  | { kind: "annotation-resize"; id: string; edge: "start" | "end" };

function annotationName(annotation: Annotation) {
  const shape = annotation.shape;
  if (shape.kind === "text" && shape.text.trim()) {
    return shape.text.trim().split("\n")[0];
  }
  return annotationLabel(shape.kind);
}

/**
 * One annotation layer of the timeline. It renders only when its own annotations change,
 * so dragging one doesn't redraw the others.
 */
export const AnnotationTrack = memo(function AnnotationTrack({
  track,
  scale,
  durationMs,
  gutter,
  onPress,
}: {
  track: number;
  /** Pixels per millisecond. */
  scale: number;
  durationMs: number;
  gutter: number;
  onPress: (e: PointerEvent, press: TrackPress) => void;
}) {
  const items = useEditor(
    useShallow(
      (s) =>
        s.settings?.annotations.items.filter((a) => a.track === track) ?? [],
    ),
  );
  const selected = useEditor((s) => s.selectedAnnotation);
  const x = (t: number) => gutter + timeToPx(t, scale);
  return (
    <div
      role="group"
      aria-label={`Layer ${track + 1}`}
      className="relative h-9"
      onPointerDown={(e) => {
        if (e.target !== e.currentTarget) return;
        const store = editorStore.getState();
        store.selectAnnotation(null);
        store.select(null);
        editorStore.setState({ activeTrack: track });
      }}
    >
      <div
        className="pointer-events-none absolute inset-y-0 rounded-md bg-white/[0.03]"
        style={{ left: x(0), width: timeToPx(durationMs, scale) }}
      />
      <span className="text-muted-foreground/60 pointer-events-none sticky left-0 inline-block py-2.5 pl-5 text-[10px]">
        Layer {track + 1}
      </span>
      {items.map((a) => (
        <div
          key={a.id}
          data-annotation={a.id}
          title={annotationName(a)}
          className={cn(
            "absolute inset-y-0 flex cursor-grab items-center overflow-hidden rounded-md text-[11px] font-medium text-white active:cursor-grabbing",
            isObscure(a.shape)
              ? "bg-slate-400/40 ring-1 ring-slate-300/50"
              : "bg-amber-500/55 ring-1 ring-amber-300/60",
            selected === a.id && "ring-2 ring-white",
          )}
          style={{
            left: x(a.startMs),
            width: timeToPx(a.endMs - a.startMs, scale),
          }}
          onPointerDown={(e) => {
            editorStore.getState().selectAnnotation(a.id);
            onPress(e, {
              kind: "annotation-move",
              id: a.id,
              startX: e.clientX,
            });
          }}
        >
          <span className="pointer-events-none truncate px-2">
            {annotationName(a)}
          </span>
          {(["start", "end"] as const).map((edge) => (
            <div
              key={edge}
              className={cn(
                "absolute inset-y-0 w-1.5 cursor-ew-resize hover:bg-white/60",
                edge === "start" ? "left-0" : "right-0",
              )}
              onPointerDown={(e) => {
                editorStore.getState().selectAnnotation(a.id);
                onPress(e, { kind: "annotation-resize", id: a.id, edge });
              }}
            />
          ))}
        </div>
      ))}
    </div>
  );
});
