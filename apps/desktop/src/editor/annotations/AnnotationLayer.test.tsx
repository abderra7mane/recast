// @vitest-environment jsdom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  AnnotationLayer,
  type AnnotationLayerHandle,
} from "@/editor/annotations/AnnotationLayer";
import { annotation, settings } from "@/editor/fixtures.test-util";
import type { Frame } from "@/editor/frame";
import { editorStore, useEditor } from "@/editor/store";

vi.mock("@/bindings", () => ({ commands: {} }));

const frame: Frame = {
  header: {
    width: 800,
    height: 500,
    playing: false,
    seq: 1,
    tMs: 500,
    edits: 0,
    complete: true,
    content: { x: 0, y: 0, width: 800, height: 500 },
    view: { x: 0, y: 0, width: 1, height: 1 },
  },
  y: new Uint8Array(800 * 500),
  uv: new Uint8Array(400 * 500),
};

/** Re-renders on selection changes, as the editor page does. */
function Page({ layer }: { layer: React.Ref<AnnotationLayerHandle> }) {
  useEditor((s) => s.selectedAnnotation);
  return <AnnotationLayer ref={layer} screen={{ width: 800, height: 500 }} />;
}

let frames: FrameRequestCallback[] = [];

beforeEach(() => {
  frames = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.push(callback);
    return frames.length;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    frames[id - 1] = () => {};
  });
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.hasPointerCapture = () => false;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(500);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** Runs the animation frames requested so far; returns how many there were. */
function runFrames() {
  const pending = frames.filter(Boolean);
  frames = [];
  pending.forEach((callback) => callback(0));
  return pending.length;
}

describe("AnnotationLayer", () => {
  it("keeps drawing a new shape after the page re-renders mid-drag", () => {
    const s = settings();
    s.annotations = { tracks: 1, items: [annotation("a", 0, 1000)] };
    editorStore.getState().load(s, 10_000, []);
    editorStore.getState().selectAnnotation("a");
    editorStore.getState().setTool("rect");
    const layer = createRef<AnnotationLayerHandle>();
    render(<Page layer={layer} />);
    act(() => layer.current!.frameShown(frame));
    runFrames();

    const surface = screen.getByLabelText("Annotations").parentElement!;
    act(() => {
      fireEvent.pointerDown(surface, { button: 0, clientX: 400, clientY: 300 });
    });
    expect(editorStore.getState().selectedAnnotation).toBeNull();
    runFrames();
    for (let i = 1; i <= 3; i++) {
      fireEvent.pointerMove(surface, { clientX: 400 + i * 10, clientY: 320 });
      expect(runFrames()).toBe(1);
    }
  });

  it("redraws when the selection changes while paused", () => {
    const s = settings();
    s.annotations = { tracks: 1, items: [annotation("a", 0, 1000)] };
    editorStore.getState().load(s, 10_000, []);
    const layer = createRef<AnnotationLayerHandle>();
    render(<Page layer={layer} />);
    act(() => layer.current!.frameShown(frame));
    for (const id of ["a", null, "a"]) {
      act(() => editorStore.getState().selectAnnotation(id));
      expect(runFrames()).toBe(1);
    }
  });
});
