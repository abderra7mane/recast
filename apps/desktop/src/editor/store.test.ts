import { describe, expect, it } from "vitest";

import { segment, settings } from "@/editor/fixtures.test-util";
import { createEditorStore, visibleSegments } from "@/editor/store";

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
});
