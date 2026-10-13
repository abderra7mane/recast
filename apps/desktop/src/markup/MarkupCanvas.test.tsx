// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { handles } from "@/markup/geometry";
import { MarkupCanvas } from "@/markup/MarkupCanvas";
import type { Background, Shape } from "@/markup/model";
import type { Renderer } from "@/markup/render";
import { markupStore } from "@/markup/store";

vi.mock("@/bindings", () => ({ commands: {} }));

let frames: FrameRequestCallback[] = [];

/** Runs the animation frames requested so far; returns how many there were. */
function runFrames() {
  const pending = frames.filter(Boolean);
  frames = [];
  pending.forEach((callback) => callback(0));
  return pending.length;
}

function fakeRenderer() {
  return {
    scale: 1,
    image: { width: 800, height: 500 },
    measure: () => ({ width: 10, height: 10 }),
    render: vi.fn<Renderer["render"]>(),
    drawDragged: vi.fn<Renderer["drawDragged"]>(),
  } satisfies Renderer;
}

beforeEach(() => {
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.push(callback);
    return frames.length;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    frames[id - 1] = () => {};
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      callback: () => void;
      constructor(callback: () => void) {
        this.callback = callback;
      }
      observe() {
        this.callback();
      }
      disconnect() {}
    },
  );
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(500);
  const ctx = new Proxy({}, { get: () => () => {}, set: () => true });
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    ctx as GPUCanvasContext,
  );
  markupStore.getState().load({ width: 800, height: 500 }, 1, {} as Background);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

// Fit zoom centers the 800 × 500 picture with offsets (51.2, 32).
const ZOOM = 0.872;
const view = (x: number, y: number) => ({
  clientX: 51.2 + x * ZOOM,
  clientY: 32 + y * ZOOM,
});

function addRect() {
  const id = markupStore.getState().add({
    kind: "rect",
    rect: { x: 100, y: 100, width: 200, height: 100 },
    color: "#ff0000",
    width: 3,
    fill: true,
  });
  markupStore.temporal.getState().clear();
  return id;
}

function setup() {
  const renderer = fakeRenderer();
  render(<MarkupCanvas renderer={renderer} />);
  const surface = screen.getByLabelText("Screenshot").parentElement!;
  runFrames();
  renderer.render.mockClear();
  let changes = 0;
  markupStore.subscribe(() => changes++);
  return { renderer, surface, changes: () => changes };
}

describe("MarkupCanvas", () => {
  it("drags out a shape without touching the store or the picture", () => {
    markupStore.getState().setTool("rect");
    const { renderer, surface, changes } = setup();

    fireEvent.pointerDown(surface, { button: 0, clientX: 100, clientY: 100 });
    const before = changes();
    for (let i = 1; i <= 3; i++) {
      fireEvent.pointerMove(surface, { clientX: 100 + i * 50, clientY: 200 });
      fireEvent.pointerMove(surface, { clientX: 110 + i * 50, clientY: 210 });
      expect(runFrames()).toBe(1);
    }
    expect(changes()).toBe(before);
    expect(renderer.render).not.toHaveBeenCalled();
    const drawn = renderer.drawDragged.mock.lastCall![2] as Shape & {
      rect: { width: number };
    };
    expect(drawn.kind).toBe("rect");
    expect(drawn.rect.width).toBeCloseTo(160 / ZOOM, 0);

    act(() => {
      fireEvent.pointerUp(surface, { clientX: 260, clientY: 210 });
    });
    const { doc, selected } = markupStore.getState();
    expect(doc.shapes).toHaveLength(1);
    expect(doc.shapes[0]).toMatchObject({ kind: "rect", rect: drawn.rect });
    expect(selected).toBe(doc.shapes[0].id);
    expect(markupStore.temporal.getState().pastStates).toHaveLength(1);
    expect(renderer.render).toHaveBeenCalledTimes(1);
  });

  it("moves a shape with one picture render at each end", () => {
    const id = addRect();
    const { renderer, surface, changes } = setup();
    const inside = view(200, 150);

    fireEvent.pointerDown(surface, { button: 0, ...inside });
    expect(renderer.render).not.toHaveBeenCalled();
    const before = changes();
    for (let i = 1; i <= 3; i++) {
      fireEvent.pointerMove(surface, {
        clientX: inside.clientX + i * 20,
        clientY: inside.clientY,
      });
      expect(runFrames()).toBe(1);
    }
    expect(changes()).toBe(before);
    expect(renderer.render).toHaveBeenCalledTimes(1);
    expect(renderer.render.mock.lastCall![1].shapes).toEqual([]);

    act(() => {
      fireEvent.pointerUp(surface, { clientX: inside.clientX + 60 });
    });
    const shape = markupStore.getState().doc.shapes[0] as Shape & {
      rect: { x: number };
    };
    expect(shape.id).toBe(id);
    expect(shape.rect.x).toBeCloseTo(100 + 60 / ZOOM, 0);
    expect(markupStore.temporal.getState().pastStates).toHaveLength(1);
    expect(renderer.render).toHaveBeenCalledTimes(2);
    expect(renderer.render.mock.lastCall![1].shapes).toEqual([shape]);
  });

  it("lifts the marks above a moved shape out of the picture", () => {
    addRect();
    const store = markupStore.getState();
    store.add({
      kind: "blur",
      rect: { x: 400, y: 300, width: 100, height: 100 },
    });
    store.add({
      kind: "line",
      from: { x: 0, y: 0 },
      to: { x: 50, y: 50 },
      color: "#000000",
      width: 2,
    });
    const [rect, blur, line] = markupStore.getState().doc.shapes;
    store.select(rect.id);
    const { renderer, surface } = setup();
    const inside = view(200, 150);

    fireEvent.pointerDown(surface, { button: 0, ...inside });
    fireEvent.pointerMove(surface, {
      clientX: inside.clientX + 20,
      clientY: inside.clientY,
    });
    runFrames();
    expect(renderer.render.mock.lastCall![1].shapes).toEqual([blur]);
    expect(renderer.drawDragged.mock.lastCall![1].shapes).toEqual([
      rect,
      blur,
      line,
    ]);
  });

  it("leaves the picture alone when a shape is clicked", () => {
    addRect();
    const { renderer, surface } = setup();
    act(() => {
      fireEvent.pointerDown(surface, { button: 0, ...view(200, 150) });
      fireEvent.pointerUp(surface, view(200, 150));
    });
    expect(renderer.render).not.toHaveBeenCalled();
    expect(renderer.drawDragged).not.toHaveBeenCalled();
    expect(markupStore.temporal.getState().pastStates).toHaveLength(0);
  });

  it("resizes a shape from a handle", () => {
    const id = addRect();
    const { renderer, surface, changes } = setup();
    const corner = view(300, 200);

    fireEvent.pointerDown(surface, { button: 0, ...corner });
    const before = changes();
    fireEvent.pointerMove(surface, {
      clientX: corner.clientX + 40,
      clientY: corner.clientY + 20,
    });
    expect(runFrames()).toBe(1);
    expect(changes()).toBe(before);
    expect(renderer.drawDragged.mock.lastCall![2]).toMatchObject({ id });

    act(() => {
      fireEvent.pointerUp(surface, {
        clientX: corner.clientX + 40,
        clientY: corner.clientY + 20,
      });
    });
    const shape = markupStore.getState().doc.shapes[0];
    expect(shape).toMatchObject({ id, rect: { x: 100, y: 100 } });
    const { rect } = shape as Shape & {
      rect: { width: number; height: number };
    };
    expect(rect.width).toBeCloseTo(200 + 40 / ZOOM, 0);
    expect(rect.height).toBeCloseTo(100 + 20 / ZOOM, 0);
    expect(markupStore.temporal.getState().pastStates).toHaveLength(1);
  });

  it("drops a shape drawn too small", () => {
    markupStore.getState().setTool("rect");
    const { surface, changes } = setup();
    act(() => {
      fireEvent.pointerDown(surface, { button: 0, clientX: 100, clientY: 100 });
      fireEvent.pointerMove(surface, { clientX: 101, clientY: 101 });
      fireEvent.pointerUp(surface, { clientX: 101, clientY: 101 });
    });
    expect(markupStore.getState().doc.shapes).toHaveLength(0);
    expect(markupStore.temporal.getState().pastStates).toHaveLength(0);
    expect(changes()).toBe(0);
  });

  it("ends a drag the browser cancels", () => {
    markupStore.getState().setTool("rect");
    const { surface } = setup();
    fireEvent.pointerDown(surface, { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(surface, { clientX: 200, clientY: 200 });
    runFrames();
    act(() => {
      fireEvent.pointerCancel(surface, { clientX: 200, clientY: 200 });
    });
    expect(markupStore.getState().doc.shapes).toHaveLength(1);
    fireEvent.pointerMove(surface, { clientX: 300, clientY: 300 });
    expect(runFrames()).toBe(0);
    expect(markupStore.getState().doc.shapes).toHaveLength(1);
  });

  it("finishes typing before dragging the text", () => {
    const store = markupStore.getState();
    store.setTool("text");
    act(() => store.startText({ x: 100, y: 100 }));
    act(() => store.setText("Hi"));
    const { surface } = setup();
    const [text] = markupStore.getState().doc.shapes;
    const handle = handles(text, () => ({ width: 10, height: 10 }))[0].at;

    act(() => {
      fireEvent.pointerDown(surface, {
        button: 0,
        ...view(handle.x, handle.y),
      });
    });
    expect(markupStore.getState().editing).toBeNull();
    expect(screen.queryByLabelText("Text")).toBeNull();
  });
});
