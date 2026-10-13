import { describe, expect, it } from "vitest";

import {
  annotation,
  box,
  segment,
  settings,
} from "@/editor/fixtures.test-util";
import {
  createEditorStore,
  redoIn,
  undoIn,
  visibleSegments,
} from "@/editor/store";

function loaded() {
  const store = createEditorStore();
  store
    .getState()
    .load(settings(), 10_000, [segment(1000, 3000), segment(5000, 7000)]);
  return store;
}

const history = (store: ReturnType<typeof createEditorStore>) =>
  store.temporal.getState();

describe("editor store", () => {
  it("starts with an empty history after loading", () => {
    const store = loaded();
    expect(store.getState().settings?.zoom.level).toBe(2);
    expect(history(store).pastStates).toHaveLength(0);
  });

  it("undoes and redoes section updates", () => {
    const store = loaded();
    store.getState().update("background", { padding: 0.2 });
    store.getState().update("cursor", { size: 3 });
    expect(history(store).pastStates).toHaveLength(2);

    history(store).undo();
    expect(store.getState().settings?.cursor.size).toBe(1.5);
    expect(store.getState().settings?.background.padding).toBe(0.2);
    history(store).undo();
    expect(store.getState().settings?.background.padding).toBe(0.08);

    history(store).redo();
    expect(store.getState().settings?.background.padding).toBe(0.2);
    expect(store.getState().settings?.cursor.size).toBe(1.5);
    expect(history(store).futureStates).toHaveLength(1);
  });

  it("ignores updates that change nothing", () => {
    const store = loaded();
    const before = store.getState().settings;
    store.getState().update("cursor", { size: 1.5 });
    expect(store.getState().settings).toBe(before);
    expect(history(store).pastStates).toHaveLength(0);
  });

  it("keeps other fields of a section", () => {
    const store = loaded();
    store.getState().update("background", { padding: 0.3 });
    expect(store.getState().settings?.background.cornerRadius).toBe(0.015);
  });

  it("records a whole gesture as one undo step", () => {
    const store = loaded();
    store.getState().beginGesture();
    for (const padding of [0.1, 0.12, 0.14, 0.16]) {
      store.getState().update("background", { padding });
      expect(store.getState().lastChange).toBe("live");
    }
    store.getState().endGesture();
    expect(store.getState().lastChange).toBe("commit");
    expect(history(store).pastStates).toHaveLength(1);

    history(store).undo();
    expect(store.getState().settings?.background.padding).toBe(0.08);
    history(store).redo();
    expect(store.getState().settings?.background.padding).toBe(0.16);
  });

  it("does not record a gesture that changed nothing", () => {
    const store = loaded();
    store.getState().beginGesture();
    store.getState().endGesture();
    expect(history(store).pastStates).toHaveLength(0);
  });

  it("a new change clears redo", () => {
    const store = loaded();
    store.getState().update("cursor", { size: 2 });
    history(store).undo();
    expect(history(store).futureStates).toHaveLength(1);
    store.getState().update("cursor", { size: 4 });
    expect(history(store).futureStates).toHaveLength(0);
  });

  it("editing a generated segment turns auto zoom off and keeps the others", () => {
    const store = loaded();
    store.getState().updateSegment(1, { level: 3 });
    const zoom = store.getState().settings!.zoom;
    expect(zoom.auto).toBe(false);
    expect(zoom.segments).toEqual([
      segment(1000, 3000),
      segment(5000, 7000, 3),
    ]);
    expect(visibleSegments(store.getState())).toBe(zoom.segments);

    history(store).undo();
    expect(store.getState().settings!.zoom.auto).toBe(true);
    expect(visibleSegments(store.getState())).toHaveLength(2);
  });

  it("adds and deletes segments with undo", () => {
    const store = loaded();
    expect(store.getState().addSegment(2000)).toBe(false);
    expect(store.getState().addSegment(8500)).toBe(true);
    expect(store.getState().selected).toBe(2);
    expect(visibleSegments(store.getState())).toHaveLength(3);

    store.getState().deleteSegment(0);
    expect(visibleSegments(store.getState()).map((s) => s.startMs)).toEqual([
      5000, 7500,
    ]);
    expect(store.getState().selected).toBeNull();

    history(store).undo();
    expect(visibleSegments(store.getState())).toHaveLength(3);
    history(store).undo();
    expect(store.getState().settings!.zoom.auto).toBe(true);
  });

  it("sets the trim", () => {
    const store = loaded();
    store.getState().setTrim({ startMs: 500, endMs: 9000 });
    expect(store.getState().settings?.trim).toEqual({
      startMs: 500,
      endMs: 9000,
    });
  });

  it("adds an annotation at the playhead, selects it and undoes it", () => {
    const store = loaded();
    const id = store.getState().addAnnotation(box(), 2000)!;
    const state = store.getState();
    expect(state.selectedAnnotation).toBe(id);
    expect(state.settings!.annotations.items).toEqual([
      { id, shape: box(), track: 0, startMs: 2000, endMs: 5000 },
    ]);
    undoIn(store);
    expect(store.getState().settings!.annotations.items).toEqual([]);
    expect(store.getState().selectedAnnotation).toBeNull();
    redoIn(store);
    expect(store.getState().settings!.annotations.items).toHaveLength(1);
  });

  it("puts an annotation on a new track when the active one is busy", () => {
    const store = loaded();
    store.getState().update("annotations", {
      items: [annotation("a", 0, 10_000)],
    });
    const id = store.getState().addAnnotation(box(), 1000)!;
    expect(store.getState().settings!.annotations.tracks).toBe(2);
    expect(store.getState().activeTrack).toBe(1);
    expect(
      store.getState().settings!.annotations.items.find((a) => a.id === id)
        ?.track,
    ).toBe(1);
  });

  it("restyles the selected annotation and remembers the style", () => {
    const store = loaded();
    store.getState().update("annotations", {
      items: [annotation("a", 0, 1000)],
    });
    store.getState().selectAnnotation("a");
    store.getState().setStyle({ color: "#34c759", width: 6 });
    expect(store.getState().settings!.annotations.items[0].shape).toEqual({
      ...box(),
      color: "#34c759",
      width: 6,
    });
    expect(store.getState().style.width).toBe(6);
  });

  it("makes a drag on the preview one undo step and shows the result", () => {
    const store = loaded();
    store.getState().update("annotations", {
      items: [annotation("a", 0, 1000)],
    });
    store.getState().beginGesture();
    store.getState().setHidden("a");
    store.getState().setAnnotationShape("a", box(50, 60));
    expect(store.getState().hidden).toBeNull();
    store.getState().endGesture();
    expect(history(store).pastStates).toHaveLength(2);
    history(store).undo();
    expect(store.getState().settings!.annotations.items[0].shape).toEqual(
      box(),
    );
  });

  it("selects either a zoom segment or an annotation", () => {
    const store = loaded();
    store.getState().update("annotations", {
      items: [annotation("a", 0, 1000)],
    });
    store.getState().select(1);
    store.getState().selectAnnotation("a");
    expect(store.getState().selected).toBeNull();
    store.getState().select(0);
    expect(store.getState().selectedAnnotation).toBeNull();
  });

  it("deletes annotations and tracks", () => {
    const store = loaded();
    store.getState().update("annotations", {
      tracks: 3,
      items: [annotation("a", 0, 1000, 0), annotation("b", 0, 1000, 2)],
    });
    store.getState().selectAnnotation("b");
    store.getState().deleteTrack(2);
    expect(store.getState().settings!.annotations).toEqual({
      tracks: 2,
      items: [annotation("a", 0, 1000, 0)],
    });
    expect(store.getState().selectedAnnotation).toBeNull();
    store.getState().deleteAnnotation("a");
    expect(store.getState().settings!.annotations.items).toEqual([]);
    store.getState().addTrack();
    expect(store.getState().settings!.annotations.tracks).toBe(3);
  });

  it("records no undo step for a gesture that ends where it began", () => {
    const store = loaded();
    const before = store.getState().settings;
    store.getState().beginGesture();
    const id = store.getState().addAnnotation(box(), 1000)!;
    store.getState().deleteAnnotation(id);
    store.getState().endGesture();
    expect(history(store).pastStates).toHaveLength(0);
    expect(store.getState().settings).toBe(before);
  });
});
