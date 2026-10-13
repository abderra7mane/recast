import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
  type Ref,
} from "react";

import {
  asShape,
  fromShape,
  isObscure,
  stackOrder,
  visibleAt,
  type AnnotationTool,
} from "@/editor/annotations/model";
import { contentBox, frameMap } from "@/editor/annotations/view";
import { createObscurePreview } from "@/editor/annotations/overlay";
import type { Frame } from "@/editor/frame";
import { pause, playbackStore } from "@/editor/playback";
import type { Annotation } from "@/editor/settings";
import { annotationById, editorStore, useEditor } from "@/editor/store";
import {
  constrainAxis,
  drawPoint,
  handleAt,
  handles,
  hits,
  hitTest,
  moveShape,
  resizeShape,
  type Handle,
  type TextMeasure,
} from "@/markup/geometry";
import {
  drawnTo,
  drawSelection,
  GRAB,
  MIN_DRAWN,
  newShape,
  RESIZE_CURSORS,
  TOOL_CURSORS,
  tooSmall,
} from "@/markup/interaction";
import type { NewShape, Point, Shape, Size, TextShape } from "@/markup/model";
import { drawShape, fontFor, LINE_HEIGHT, measureText } from "@/markup/render";
import { makeCanvas } from "@/markup/session";
import { deviceTransform, toImage, toView, type ViewMap } from "@/markup/view";

type Drag =
  | {
      kind: "create";
      tool: AnnotationTool;
      from: Point;
      tMs: number;
      shape: NewShape;
    }
  | { kind: "move"; id: string; start: Shape; from: Point; shape: Shape }
  | { kind: "resize"; id: string; start: Shape; handle: Handle; shape: Shape };

/** An edited annotation the preview frames don't show yet, and the edits that will. */
type Pending = { id: string; shape: Shape; edits: number };

export type AnnotationLayerHandle = {
  /** Draws over `frame`, which the preview just showed. */
  frameShown: (frame: Frame) => void;
};

function visibleShapes(items: Annotation[], tMs: number): Shape[] {
  return stackOrder(items.filter((a) => visibleAt(a, tMs))).map(asShape);
}

function TextEditor({
  shape,
  map,
  measure,
  onCommit,
}: {
  shape: TextShape;
  map: ViewMap;
  measure: TextMeasure;
  onCommit: () => void;
}) {
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    element.setSelectionRange(element.value.length, element.value.length);
  }, []);
  const at = toView(map, shape.at);
  const fontPixels = shape.fontSize * map.zoom;
  const size = measure(shape);
  return (
    <textarea
      ref={field}
      aria-label="Text"
      value={shape.text}
      spellCheck={false}
      wrap="off"
      onChange={(e) => {
        const { updateAnnotation } = editorStore.getState();
        updateAnnotation(shape.id, {
          shape: fromShape({ ...shape, text: e.target.value }, 0),
        });
      }}
      onBlur={onCommit}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" || (e.key === "Enter" && e.metaKey)) {
          e.preventDefault();
          onCommit();
        }
      }}
      className="absolute resize-none overflow-hidden border-0 bg-transparent p-0 whitespace-pre outline-1 outline-sky-400 outline-dashed"
      style={{
        left: at.x,
        top: at.y,
        width: size.width * map.zoom + fontPixels,
        height: size.height * map.zoom,
        font: fontFor(fontPixels),
        lineHeight: LINE_HEIGHT,
        color: shape.color,
        caretColor: shape.color,
      }}
    />
  );
}

/**
 * Draws and edits annotations over the preview. Drags never wait on the backend: the
 * annotation being edited is left out of the preview frames and drawn here on each
 * animation frame, then handed back to the compositor when the drag ends.
 */
export function AnnotationLayer({
  screen,
  ref,
}: {
  /** The recorded area in points. */
  screen: Size;
  ref: Ref<AnnotationLayerHandle>;
}) {
  const surface = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const frame = useRef<Frame | null>(null);
  const drag = useRef<Drag | null>(null);
  const pending = useRef<Pending | null>(null);
  const scheduled = useRef(0);
  const editingId = useEditor((s) => s.editingText);
  const editing = useEditor((s) => annotationById(s, s.editingText));
  const [editMap, setEditMap] = useState<ViewMap | null>(null);

  const measure = useMemo<TextMeasure>(() => {
    const ctx = makeCanvas(1, 1).ctx;
    return (shape) => measureText(ctx, shape, 1);
  }, []);
  const obscurePreview = useMemo(() => createObscurePreview(makeCanvas), []);

  const box = (): Size => ({
    width: surface.current?.clientWidth ?? 0,
    height: surface.current?.clientHeight ?? 0,
  });

  const draw = () => {
    const element = canvas.current;
    const shown = frame.current;
    if (!element) return;
    const size = box();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.round(size.width * ratio);
    const height = Math.round(size.height * ratio);
    if (element.width !== width) element.width = width;
    if (element.height !== height) element.height = height;
    const ctx = element.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width, height);
    if (!shown || size.width === 0) return;

    const { header } = shown;
    const state = editorStore.getState();
    const map = frameMap(header, screen, size);
    const t = header.tMs;
    const current = drag.current;
    const waiting = pending.current;
    if (waiting && header.edits >= waiting.edits && header.complete) {
      pending.current = null;
    }

    const marks: Shape[] = [];
    if (current?.kind === "create") {
      marks.push({ ...current.shape, id: "" } as Shape);
    } else if (current) {
      const annotation = annotationById(state, current.id);
      if (annotation && visibleAt(annotation, t)) marks.push(current.shape);
    }
    const late = pending.current;
    if (late && late.id !== (current?.kind === "create" ? null : current?.id)) {
      const annotation = annotationById(state, late.id);
      if (annotation && visibleAt(annotation, t)) marks.push(late.shape);
    }

    const clip = contentBox(header, size);
    ctx.save();
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.beginPath();
    ctx.rect(clip.x, clip.y, clip.width, clip.height);
    ctx.clip();
    for (const mark of marks) {
      if (isObscure(mark)) {
        const frameScale = size.width / header.width;
        const block = (mark as { block?: number }).block ?? state.style.block;
        ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
        obscurePreview(
          ctx,
          shown,
          mark.kind,
          mark.rect,
          block,
          frameMap(header, screen, {
            width: header.width,
            height: header.height,
          }),
          frameScale,
        );
      } else {
        ctx.setTransform(...deviceTransform(map, ratio));
        drawShape(ctx, mark, 1);
      }
    }
    ctx.restore();

    const selectedId = state.selectedAnnotation;
    const selected =
      current && current.kind !== "create" && current.id === selectedId
        ? current.shape
        : (() => {
            const annotation = annotationById(state, selectedId);
            return annotation && visibleAt(annotation, t)
              ? asShape(annotation)
              : null;
          })();
    if (selected && selected.id !== state.editingText) {
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
      drawSelection(ctx, selected, measure, (p) => toView(map, p));
    }
  };

  const drawLatest = useRef(draw);
  useEffect(() => {
    drawLatest.current = draw;
  });

  const requestDraw = useCallback(() => {
    if (scheduled.current) return;
    scheduled.current = requestAnimationFrame(() => {
      scheduled.current = 0;
      drawLatest.current();
    });
  }, []);

  useImperativeHandle(ref, () => ({
    frameShown: (shown) => {
      frame.current = shown;
      cancelAnimationFrame(scheduled.current);
      scheduled.current = 0;
      draw();
    },
  }));

  useEffect(() => {
    const unsubscribe = editorStore.subscribe((state, prev) => {
      if (
        state.settings !== prev.settings ||
        state.selectedAnnotation !== prev.selectedAnnotation ||
        state.editingText !== prev.editingText
      ) {
        requestDraw();
      }
    });
    const element = surface.current;
    const observer = new ResizeObserver(requestDraw);
    if (element) observer.observe(element);
    return () => {
      unsubscribe();
      observer.disconnect();
      cancelAnimationFrame(scheduled.current);
      scheduled.current = 0;
    };
  }, [requestDraw]);

  const mapNow = () =>
    frame.current ? frameMap(frame.current.header, screen, box()) : null;

  const local = (e: { clientX: number; clientY: number }) => {
    const rect = surface.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  /** Waits for preview frames to show `id` before this layer stops drawing it. */
  const handBack = (id: string, shape: Shape) => {
    pending.current = { id, shape, edits: playbackStore.getState().editsSent };
    requestDraw();
  };

  const startEditing = (id: string) => {
    const store = editorStore.getState();
    pause();
    store.selectAnnotation(id);
    store.beginGesture();
    store.setHidden(id);
    store.setEditingText(id);
    setEditMap(mapNow());
  };

  const commitText = () => {
    const store = editorStore.getState();
    const id = store.editingText;
    if (!id) return;
    store.setEditingText(null);
    const annotation = annotationById(store, id);
    if (
      annotation?.shape.kind === "text" &&
      annotation.shape.text.trim() === ""
    ) {
      store.deleteAnnotation(id);
    }
    store.endGesture();
    store.setHidden(null);
    const kept = annotationById(editorStore.getState(), id);
    if (kept) handBack(id, asShape(kept));
  };

  const hoverCursor = (p: Point, map: ViewMap, tMs: number): string => {
    const state = editorStore.getState();
    const items = state.settings?.annotations.items ?? [];
    const tolerance = GRAB / map.zoom;
    const selected = annotationById(state, state.selectedAnnotation);
    if (selected && visibleAt(selected, tMs)) {
      const shape = asShape(selected);
      const handle = handleAt(handles(shape, measure), p, tolerance);
      if (handle) return RESIZE_CURSORS[handle];
      if (hits(shape, p, tolerance, 1, measure)) return "move";
    }
    if (state.tool === "select") {
      return hitTest(visibleShapes(items, tMs), p, tolerance, 1, measure)
        ? "move"
        : "default";
    }
    return TOOL_CURSORS[state.tool] ?? "crosshair";
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    const shown = frame.current;
    if (e.button !== 0 || !shown) return;
    e.preventDefault();
    const map = frameMap(shown.header, screen, box());
    const p = toImage(map, local(e));
    const tMs = shown.header.tMs;
    if (editorStore.getState().editingText) commitText();
    const store = editorStore.getState();
    const items = store.settings?.annotations.items ?? [];
    const tolerance = GRAB / map.zoom;
    const visible = visibleShapes(items, tMs);
    const selectedAnnotation = annotationById(store, store.selectedAnnotation);
    const selected =
      selectedAnnotation && visibleAt(selectedAnnotation, tMs)
        ? asShape(selectedAnnotation)
        : null;
    const handle = selected
      ? handleAt(handles(selected, measure), p, tolerance)
      : null;

    const grab = (next: Extract<Drag, { id: string }>) => {
      store.beginGesture();
      // A blurred area stays in the frames until it lands, so nothing shows unblurred.
      if (!isObscure(next.start)) store.setHidden(next.id);
      drag.current = next;
    };

    if (selected && handle) {
      grab({
        kind: "resize",
        id: selected.id,
        start: selected,
        handle,
        shape: selected,
      });
    } else if (store.tool === "text") {
      const hit = hitTest(visible, p, tolerance, 1, measure);
      if (hit?.kind === "text") {
        startEditing(hit.id);
      } else {
        const lineHeight = store.style.fontSize * LINE_HEIGHT;
        store.beginGesture();
        const id = store.addAnnotation(
          {
            kind: "text",
            at: { x: p.x, y: p.y - lineHeight / 2 },
            text: "",
            color: store.style.color,
            fontSize: store.style.fontSize,
          },
          tMs,
        );
        if (id) startEditing(id);
        else store.endGesture();
      }
      return;
    } else if (selected && hits(selected, p, tolerance, 1, measure)) {
      grab({
        kind: "move",
        id: selected.id,
        start: selected,
        from: p,
        shape: selected,
      });
    } else if (store.tool === "select") {
      const hit = hitTest(visible, p, tolerance, 1, measure);
      store.selectAnnotation(hit?.id ?? null);
      if (!hit) return;
      grab({ kind: "move", id: hit.id, start: hit, from: p, shape: hit });
    } else {
      const shape = newShape(store.tool, p, store.style);
      if (!shape) return;
      store.selectAnnotation(null);
      drag.current = { kind: "create", tool: store.tool, from: p, tMs, shape };
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    requestDraw();
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const map = mapNow();
    if (!map) return;
    const p = toImage(map, local(e));
    const current = drag.current;
    if (!current) {
      e.currentTarget.style.cursor = hoverCursor(
        p,
        map,
        frame.current!.header.tMs,
      );
      return;
    }
    switch (current.kind) {
      case "create": {
        const to = drawPoint(current.shape.kind, current.from, p, e.shiftKey);
        current.shape = drawnTo(current.shape, current.from, to);
        break;
      }
      case "move": {
        const delta = { x: p.x - current.from.x, y: p.y - current.from.y };
        const d = e.shiftKey ? constrainAxis(delta) : delta;
        current.shape = moveShape(current.start, d.x, d.y);
        break;
      }
      case "resize":
        current.shape = resizeShape(
          current.start,
          current.handle,
          p,
          e.shiftKey,
          measure,
        );
        break;
    }
    requestDraw();
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    drag.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    const store = editorStore.getState();
    const map = mapNow();
    if (current.kind === "create") {
      const shape = { ...current.shape, id: "" } as Shape;
      if (map && !tooSmall(shape, MIN_DRAWN / map.zoom)) {
        const id = store.addAnnotation(
          fromShape(shape, store.style.block),
          current.tMs,
        );
        if (id) handBack(id, { ...shape, id });
      }
      requestDraw();
      return;
    }
    const annotation = annotationById(store, current.id);
    const shape = fromShape(current.shape, store.style.block);
    if (
      annotation &&
      JSON.stringify(shape) !== JSON.stringify(annotation.shape)
    ) {
      store.setAnnotationShape(current.id, shape);
    } else {
      store.setHidden(null);
    }
    store.endGesture();
    handBack(current.id, current.shape);
  };

  const onDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    const shown = frame.current;
    if (!shown || editorStore.getState().tool !== "select") return;
    const map = frameMap(shown.header, screen, box());
    const items = editorStore.getState().settings?.annotations.items ?? [];
    const hit = hitTest(
      visibleShapes(items, shown.header.tMs),
      toImage(map, local(e)),
      GRAB / map.zoom,
      1,
      measure,
    );
    if (hit?.kind === "text") startEditing(hit.id);
  };

  const editingShape = editing ? asShape(editing) : null;

  return (
    <div
      ref={surface}
      className="absolute inset-0 touch-none"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={onDoubleClick}
    >
      <canvas
        ref={canvas}
        aria-label="Annotations"
        className="pointer-events-none absolute inset-0 h-full w-full"
      />
      {editingId && editingShape?.kind === "text" && editMap && (
        <TextEditor
          key={editingId}
          shape={editingShape}
          map={editMap}
          measure={measure}
          onCommit={commitText}
        />
      )}
    </div>
  );
}
