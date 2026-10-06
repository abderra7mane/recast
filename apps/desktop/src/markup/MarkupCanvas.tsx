import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { commands } from "@/bindings";
import {
  cropArea,
  cropPress,
  drawCrop,
  fullRect,
  moveCrop,
  resizeCrop,
} from "@/markup/crop";
import {
  bounds,
  constrainAxis,
  drawPoint,
  handleAt,
  handles,
  hits,
  hitTest,
  moveShape,
  rectFrom,
  rectHandles,
  resizeShape,
  type Handle,
} from "@/markup/geometry";
import { createLatestRunner } from "@/markup/latest";
import type {
  Background,
  NewShape,
  Point,
  Rect,
  Shape,
  Size,
  Style,
  TextShape,
  Tool,
} from "@/markup/model";
import {
  fontFor,
  LINE_HEIGHT,
  type Renderer,
  type Surface,
} from "@/markup/render";
import { cornerPixels, makeCanvas, paddingPixels } from "@/markup/session";
import { markupStore, useMarkup } from "@/markup/store";
import {
  deviceTransform,
  fitZoom,
  layoutView,
  scrollForZoom,
  toImage,
  toView,
  type ViewMap,
} from "@/markup/view";

/** Handle squares and how close the pointer must be to grab them, in CSS pixels. */
const HANDLE_SIZE = 8;
const GRAB = 7;
/** Shapes drawn smaller than this, in CSS pixels, are dropped. */
const MIN_DRAWN = 3;
const ACCENT = "#0a84ff";

type Drag =
  | { kind: "create"; id: string; tool: Tool; from: Point }
  | { kind: "move"; start: Shape; from: Point }
  | { kind: "resize"; start: Shape; handle: Handle }
  | { kind: "crop-move"; start: Rect; from: Point }
  | { kind: "crop-resize"; start: Rect; handle: Handle }
  | { kind: "crop-new"; from: Point };

function newShape(tool: Tool, at: Point, style: Style): NewShape | null {
  const rect = { ...at, width: 0, height: 0 };
  switch (tool) {
    case "arrow":
    case "line":
      return {
        kind: tool,
        from: at,
        to: at,
        color: style.color,
        width: style.width,
      };
    case "rect":
    case "ellipse":
      return {
        kind: tool,
        rect,
        color: style.color,
        width: style.width,
        fill: style.fill,
      };
    case "highlight":
      return { kind: tool, rect, color: style.highlight };
    case "blur":
    case "pixelate":
      return { kind: tool, rect };
    default:
      return null;
  }
}

function drawnTo(shape: Shape, from: Point, to: Point): Shape {
  if (shape.kind === "arrow" || shape.kind === "line") return { ...shape, to };
  if ("rect" in shape) return { ...shape, rect: rectFrom(from, to) };
  return shape;
}

function tooSmall(shape: Shape, minimum: number) {
  if (shape.kind === "arrow" || shape.kind === "line") {
    return (
      Math.hypot(shape.to.x - shape.from.x, shape.to.y - shape.from.y) < minimum
    );
  }
  if ("rect" in shape) {
    return shape.rect.width < minimum || shape.rect.height < minimum;
  }
  return false;
}

const RESIZE_CURSORS: Record<Handle, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  n: "ns-resize",
  s: "ns-resize",
  e: "ew-resize",
  w: "ew-resize",
  start: "move",
  end: "move",
};

const TOOL_CURSORS: Partial<Record<Tool, string>> = {
  select: "default",
  text: "text",
};

/** The Beautify frame around the picture, rendered by the backend without it. */
function useFrame(
  enabled: boolean,
  background: Background,
  content: Size,
  max: Size,
): HTMLImageElement | null {
  const [frame, setFrame] = useState<HTMLImageElement | null>(null);
  const runner = useMemo(
    () =>
      createLatestRunner(
        async (request: {
          background: Background;
          content: Size;
          max: Size;
        }) => {
          const result = await commands.markupFrame(
            request.background,
            request.content.width,
            request.content.height,
            request.max.width,
            request.max.height,
          );
          if (result.status !== "ok") return;
          const image = new Image();
          image.src = result.data.url;
          await image.decode();
          setFrame(image);
        },
      ),
    [],
  );
  useEffect(() => {
    if (!enabled) return;
    runner.push({
      background,
      content: { width: content.width, height: content.height },
      max: { width: max.width, height: max.height },
    });
  }, [
    enabled,
    background,
    content.width,
    content.height,
    max.width,
    max.height,
    runner,
  ]);
  return enabled ? frame : null;
}

/** Device pixels the frame needs, in steps so zooming doesn't ask for every size. */
function frameSize(doc: Size, zoom: number, pixelRatio: number): Size {
  const step = (native: number) =>
    Math.min(native, Math.ceil((native * zoom * pixelRatio) / 512) * 512);
  return { width: step(doc.width), height: step(doc.height) };
}

function TextEditor({
  shape,
  map,
  renderer,
}: {
  shape: TextShape;
  map: ViewMap;
  renderer: Renderer;
}) {
  const field = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    const element = field.current;
    if (!element) return;
    element.focus();
    element.setSelectionRange(element.value.length, element.value.length);
  }, []);
  const at = toView(map, shape.at);
  const fontPixels = shape.fontSize * renderer.scale * map.zoom;
  const size = renderer.measure(shape);
  const commit = () => {
    if (markupStore.getState().editing === shape.id)
      markupStore.getState().commitText();
  };
  return (
    <textarea
      ref={field}
      aria-label="Text"
      value={shape.text}
      spellCheck={false}
      wrap="off"
      onChange={(e) => markupStore.getState().setText(e.target.value)}
      onBlur={commit}
      onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape" || (e.key === "Enter" && e.metaKey)) {
          e.preventDefault();
          commit();
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

export function MarkupCanvas({ renderer }: { renderer: Renderer }) {
  const doc = useMarkup((s) => s.doc);
  const tool = useMarkup((s) => s.tool);
  const selected = useMarkup((s) => s.selected);
  const editing = useMarkup((s) => s.editing);
  const zoomSetting = useMarkup((s) => s.zoom);
  const scale = renderer.scale;
  const image = renderer.image;

  const scroller = useRef<HTMLDivElement>(null);
  const surface = useRef<HTMLDivElement>(null);
  const view = useRef<HTMLCanvasElement>(null);
  const docCanvas = useRef<{ surface: Surface; drawn: unknown[] } | null>(null);
  const drag = useRef<Drag | null>(null);
  const lastMap = useRef<ViewMap | null>(null);
  const zoomAnchor = useRef<Point | null>(null);
  const [viewport, setViewport] = useState<Size>({ width: 0, height: 0 });
  const [scroll, setScroll] = useState<Point>({ x: 0, y: 0 });
  const [cursor, setCursor] = useState("default");

  const cropping = tool === "crop";
  const area = cropping ? fullRect(image) : cropArea(doc.crop, image);
  const beautified = doc.beautify && !cropping;
  const pad = beautified ? paddingPixels(area, doc.background.padding) : 0;
  const docSize = {
    width: area.width + 2 * pad,
    height: area.height + 2 * pad,
  };
  const zoom =
    zoomSetting === "fit"
      ? fitZoom(docSize, viewport.width > 0 ? viewport : docSize, scale)
      : zoomSetting;
  const layout = layoutView(docSize, zoom, viewport, scroll);
  const map: ViewMap = {
    zoom,
    offsetX: layout.offsetX,
    offsetY: layout.offsetY,
    originX: area.x - pad,
    originY: area.y - pad,
  };
  const pixelRatio = window.devicePixelRatio || 1;
  const frame = useFrame(
    beautified,
    doc.background,
    area,
    frameSize(docSize, zoom, pixelRatio),
  );

  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() =>
      setViewport({ width: element.clientWidth, height: element.clientHeight }),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    markupStore.setState({ shownZoom: zoom });
  }, [zoom]);

  useLayoutEffect(() => {
    const previous = lastMap.current;
    const element = scroller.current;
    if (!previous || !element || previous.zoom === zoom) return;
    const anchor = zoomAnchor.current ?? {
      x: viewport.width / 2,
      y: viewport.height / 2,
    };
    zoomAnchor.current = null;
    const next = scrollForZoom(previous, anchor, zoom);
    element.scrollTo(next.x, next.y);
    setScroll({ x: element.scrollLeft, y: element.scrollTop });
  }, [zoom, viewport.width, viewport.height]);

  useEffect(() => {
    lastMap.current = map;
  });

  useEffect(() => {
    const element = surface.current;
    if (!element) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const { shownZoom } = markupStore.getState();
      const bounds = element.getBoundingClientRect();
      zoomAnchor.current = {
        x: e.clientX - bounds.left,
        y: e.clientY - bounds.top,
      };
      const next = shownZoom * Math.exp(-e.deltaY * 0.01);
      markupStore
        .getState()
        .setZoom(Math.min(Math.max(next, 0.02 / scale), 8 / scale));
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [scale]);

  const shapes = doc.shapes;
  const selectedShape = shapes.find((s) => s.id === selected) ?? null;
  const editingShape = shapes.find((s) => s.id === editing);

  // Drawn as part of the render, not on an animation frame: WebKit holds back frames
  // for windows behind others.
  useLayoutEffect(() => {
    const canvas = view.current;
    if (!canvas || viewport.width === 0) return;
    const drawn = [shapes, area.x, area.y, area.width, area.height, editing];
    let target = docCanvas.current;
    if (!target || target.drawn.some((value, i) => value !== drawn[i])) {
      if (
        !target ||
        target.drawn[3] !== area.width ||
        target.drawn[4] !== area.height
      ) {
        target = { surface: makeCanvas(area.width, area.height), drawn };
      }
      renderer.render(target.surface.ctx, { shapes }, area, editing);
      target.drawn = drawn;
      docCanvas.current = target;
    }

    const width = Math.round(viewport.width * pixelRatio);
    const height = Math.round(viewport.height * pixelRatio);
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width, height);
    ctx.setTransform(...deviceTransform(map, pixelRatio));
    ctx.imageSmoothingEnabled = true;
    // WebKit filters large downscales properly only at "medium"; "high" aliases.
    ctx.imageSmoothingQuality = "medium";
    if (beautified) {
      if (frame) {
        ctx.drawImage(
          frame,
          map.originX,
          map.originY,
          docSize.width,
          docSize.height,
        );
      }
      ctx.save();
      ctx.beginPath();
      ctx.roundRect(
        area.x,
        area.y,
        area.width,
        area.height,
        cornerPixels(area, doc.background.cornerRadius),
      );
      ctx.clip();
      ctx.drawImage(target.surface.canvas, area.x, area.y);
      ctx.restore();
    } else {
      ctx.drawImage(target.surface.canvas, area.x, area.y);
    }

    ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    const square = (p: Point) => {
      const at = toView(map, p);
      const half = HANDLE_SIZE / 2;
      ctx.fillStyle = "#ffffff";
      ctx.strokeStyle = ACCENT;
      ctx.lineWidth = 1.5;
      ctx.fillRect(at.x - half, at.y - half, HANDLE_SIZE, HANDLE_SIZE);
      ctx.strokeRect(at.x - half, at.y - half, HANDLE_SIZE, HANDLE_SIZE);
    };
    const viewRect = (rect: Rect) => {
      const a = toView(map, rect);
      const b = toView(map, {
        x: rect.x + rect.width,
        y: rect.y + rect.height,
      });
      return { x: a.x, y: a.y, width: b.x - a.x, height: b.y - a.y };
    };

    if (cropping) {
      const crop = viewRect(doc.crop ?? fullRect(image));
      ctx.fillStyle = "rgba(0, 0, 0, 0.55)";
      ctx.beginPath();
      ctx.rect(0, 0, viewport.width, viewport.height);
      ctx.rect(crop.x, crop.y, crop.width, crop.height);
      ctx.fill("evenodd");
      ctx.strokeStyle = "rgba(255, 255, 255, 0.35)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (const third of [1 / 3, 2 / 3]) {
        ctx.moveTo(crop.x + crop.width * third, crop.y);
        ctx.lineTo(crop.x + crop.width * third, crop.y + crop.height);
        ctx.moveTo(crop.x, crop.y + crop.height * third);
        ctx.lineTo(crop.x + crop.width, crop.y + crop.height * third);
      }
      ctx.stroke();
      ctx.strokeStyle = "#ffffff";
      ctx.lineWidth = 1.5;
      ctx.strokeRect(crop.x, crop.y, crop.width, crop.height);
      rectHandles(doc.crop ?? fullRect(image)).forEach(({ at }) => square(at));
    } else if (selectedShape && selectedShape.id !== editing) {
      if (selectedShape.kind !== "arrow" && selectedShape.kind !== "line") {
        const box = viewRect(bounds(selectedShape, renderer.measure));
        ctx.setLineDash([4, 3]);
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1;
        ctx.strokeRect(box.x, box.y, box.width, box.height);
        ctx.setLineDash([]);
      }
      handles(selectedShape, renderer.measure).forEach(({ at }) => square(at));
    }
  });

  const local = (e: { clientX: number; clientY: number }) => {
    const rect = surface.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  const tolerance = GRAB / zoom;
  const state = () => markupStore.getState();

  const shapeHandle = (shape: Shape | null, p: Point) =>
    shape ? handleAt(handles(shape, renderer.measure), p, tolerance) : null;

  const hoverCursor = (p: Point): string => {
    if (cropping) {
      const press = cropPress(doc.crop, image, p, tolerance);
      if (press.kind === "resize") return RESIZE_CURSORS[press.handle];
      return press.kind === "move" ? "move" : "crosshair";
    }
    const handle = shapeHandle(selectedShape, p);
    if (handle) return RESIZE_CURSORS[handle];
    if (
      selectedShape &&
      hits(selectedShape, p, tolerance, scale, renderer.measure)
    ) {
      return "move";
    }
    if (tool === "select") {
      return hitTest(shapes, p, tolerance, scale, renderer.measure)
        ? "move"
        : "default";
    }
    return TOOL_CURSORS[tool] ?? "crosshair";
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const p = toImage(map, local(e));
    const store = state();

    if (cropping) {
      const press = cropPress(store.doc.crop, image, p, tolerance);
      store.beginGesture();
      drag.current =
        press.kind === "resize"
          ? { kind: "crop-resize", start: press.start, handle: press.handle }
          : press.kind === "move"
            ? { kind: "crop-move", start: press.start, from: p }
            : { kind: "crop-new", from: p };
    } else {
      const handle = shapeHandle(selectedShape, p);
      if (selectedShape && handle) {
        store.beginGesture();
        drag.current = { kind: "resize", start: selectedShape, handle };
      } else if (tool === "text") {
        const hit = hitTest(shapes, p, tolerance, scale, renderer.measure);
        if (hit?.kind === "text") {
          store.editText(hit.id);
        } else {
          const lineHeight = store.style.fontSize * scale * LINE_HEIGHT;
          store.startText({ x: p.x, y: p.y - lineHeight / 2 });
        }
        return;
      } else if (
        selectedShape &&
        hits(selectedShape, p, tolerance, scale, renderer.measure)
      ) {
        store.beginGesture();
        drag.current = { kind: "move", start: selectedShape, from: p };
      } else if (tool === "select") {
        const hit = hitTest(shapes, p, tolerance, scale, renderer.measure);
        store.select(hit?.id ?? null);
        if (!hit) return;
        store.beginGesture();
        drag.current = { kind: "move", start: hit, from: p };
      } else {
        const shape = newShape(tool, p, store.style);
        if (!shape) return;
        store.select(null);
        store.beginGesture();
        drag.current = { kind: "create", id: store.add(shape), tool, from: p };
      }
    }
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const p = toImage(map, local(e));
    const current = drag.current;
    if (!current) {
      setCursor(hoverCursor(p));
      return;
    }
    const store = state();
    switch (current.kind) {
      case "create": {
        const shape = store.doc.shapes.find((s) => s.id === current.id);
        if (!shape) return;
        const to = drawPoint(shape.kind, current.from, p, e.shiftKey);
        store.replace(drawnTo(shape, current.from, to));
        break;
      }
      case "move": {
        const delta = { x: p.x - current.from.x, y: p.y - current.from.y };
        const d = e.shiftKey ? constrainAxis(delta) : delta;
        store.replace(moveShape(current.start, d.x, d.y));
        break;
      }
      case "resize":
        store.replace(
          resizeShape(
            current.start,
            current.handle,
            p,
            e.shiftKey,
            renderer.measure,
          ),
        );
        break;
      case "crop-move":
        store.setCrop(
          moveCrop(
            current.start,
            { x: p.x - current.from.x, y: p.y - current.from.y },
            image,
            e.shiftKey,
          ),
        );
        break;
      case "crop-resize":
        store.setCrop(
          resizeCrop(current.start, current.handle, p, image, e.shiftKey),
        );
        break;
      case "crop-new": {
        const crop = drawCrop(current.from, p, image);
        if (crop) store.setCrop(crop);
        break;
      }
    }
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    drag.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
    const store = state();
    if (current.kind === "create") {
      const shape = store.doc.shapes.find((s) => s.id === current.id);
      if (shape && tooSmall(shape, MIN_DRAWN / zoom)) store.remove(shape.id);
    }
    store.endGesture();
    setCursor(hoverCursor(toImage(map, local(e))));
  };

  const onDoubleClick = (e: ReactMouseEvent<HTMLDivElement>) => {
    if (tool !== "select") return;
    const hit = hitTest(
      shapes,
      toImage(map, local(e)),
      tolerance,
      scale,
      renderer.measure,
    );
    if (hit?.kind === "text") state().editText(hit.id);
  };

  return (
    <div
      ref={scroller}
      className="relative min-h-0 flex-1 overflow-auto bg-black/40"
      onScroll={(e) =>
        setScroll({
          x: e.currentTarget.scrollLeft,
          y: e.currentTarget.scrollTop,
        })
      }
    >
      <div style={{ width: layout.contentWidth, height: layout.contentHeight }}>
        <div
          ref={surface}
          className="sticky top-0 left-0 touch-none overflow-hidden"
          style={{ width: viewport.width, height: viewport.height, cursor }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onDoubleClick={onDoubleClick}
        >
          <canvas
            ref={view}
            aria-label="Screenshot"
            className="absolute inset-0"
            style={{ width: viewport.width, height: viewport.height }}
          />
          {editingShape?.kind === "text" && (
            <TextEditor
              key={editingShape.id}
              shape={editingShape}
              map={map}
              renderer={renderer}
            />
          )}
        </div>
      </div>
    </div>
  );
}
