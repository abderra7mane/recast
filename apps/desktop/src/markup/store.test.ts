import { describe, expect, it } from "vitest";

import type { Background, BoxShape, NewShape } from "@/markup/model";
import {
  createMarkupStore,
  redo,
  undo,
  type MarkupStore,
} from "@/markup/store";

const background = { padding: 0.1 } as Background;

function loaded() {
  const store = createMarkupStore();
  store.getState().load({ width: 400, height: 300 }, 2, background);
  return store;
}

const rect = (x = 10): NewShape => ({
  kind: "rect",
  rect: { x, y: 10, width: 50, height: 40 },
  color: "#ff0000",
  width: 3,
  fill: false,
});

const shapes = (store: MarkupStore) => store.getState().doc.shapes;
const past = (store: MarkupStore) => store.temporal.getState().pastStates;

describe("markup store", () => {
  it("starts empty with no history", () => {
    const store = loaded();
    expect(shapes(store)).toEqual([]);
    expect(store.getState().doc.background).toBe(background);
    expect(past(store)).toHaveLength(0);
  });

  it("undoes and redoes adding, removing and duplicating", () => {
    const store = loaded();
    const id = store.getState().add(rect());
    expect(store.getState().selected).toBe(id);
    store.getState().duplicate(id);
    expect(shapes(store)).toHaveLength(2);
    expect((shapes(store)[1] as BoxShape).rect.x).toBe(10 + 24);
    store.getState().remove(id);
    expect(shapes(store)).toHaveLength(1);

    undo(store);
    expect(shapes(store)).toHaveLength(2);
    undo(store);
    expect(shapes(store)).toHaveLength(1);
    undo(store);
    expect(shapes(store)).toHaveLength(0);
    expect(store.getState().selected).toBe(null);
    redo(store);
    redo(store);
    redo(store);
    expect(shapes(store).map((s) => s.id)).not.toContain(id);
  });

  it("makes a drag one step", () => {
    const store = loaded();
    const id = store.getState().add(rect());
    store.getState().beginGesture();
    for (let i = 1; i <= 10; i++) store.getState().nudge(id, 1, 0);
    store.getState().endGesture();
    expect((shapes(store)[0] as BoxShape).rect.x).toBe(20);
    expect(past(store)).toHaveLength(2);
    undo(store);
    expect((shapes(store)[0] as BoxShape).rect.x).toBe(10);
  });

  it("leaves no step for a gesture that changes nothing", () => {
    const store = loaded();
    store.getState().beginGesture();
    const id = store.getState().add(rect());
    store.getState().remove(id);
    store.getState().endGesture();
    expect(past(store)).toHaveLength(0);
  });

  it("undoes style changes of the selected shape", () => {
    const store = loaded();
    store.getState().add(rect());
    store.getState().setStyle({ color: "#00ff00", width: 8 });
    expect(shapes(store)[0]).toMatchObject({ color: "#00ff00", width: 8 });
    store.getState().setStyle({ fontSize: 40 });
    expect(past(store)).toHaveLength(2);
    undo(store);
    expect(shapes(store)[0]).toMatchObject({ color: "#ff0000", width: 3 });
    expect(store.getState().style.color).toBe("#00ff00");
  });

  it("only changes the style for new shapes when nothing is selected", () => {
    const store = loaded();
    store.getState().add(rect());
    store.getState().select(null);
    store.getState().setStyle({ color: "#0000ff" });
    expect(shapes(store)[0]).toMatchObject({ color: "#ff0000" });
    expect(store.getState().style.color).toBe("#0000ff");
  });

  it("shows a selected shape's style", () => {
    const store = loaded();
    const id = store
      .getState()
      .add({ ...rect(), color: "#123456" } as NewShape);
    store.getState().select(null);
    store.getState().select(id);
    expect(store.getState().style.color).toBe("#123456");
  });

  it("undoes crop and Beautify changes", () => {
    const store = loaded();
    const crop = { x: 10, y: 10, width: 100, height: 80 };
    store.getState().setCrop(crop);
    store.getState().setBeautify(true);
    store.getState().setBackground({ padding: 0.2 });
    expect(store.getState().doc).toMatchObject({ crop, beautify: true });
    undo(store);
    expect(store.getState().doc.background.padding).toBe(0.1);
    undo(store);
    expect(store.getState().doc.beautify).toBe(false);
    undo(store);
    expect(store.getState().doc.crop).toBe(null);
  });

  it("makes typing a new text one step and drops empty text", () => {
    const store = loaded();
    store.getState().startText({ x: 5, y: 5 });
    const id = store.getState().editing;
    expect(id).not.toBe(null);
    store.getState().setText("H");
    store.getState().setText("Hi");
    store.getState().commitText();
    expect(shapes(store)[0]).toMatchObject({ kind: "text", text: "Hi" });
    expect(past(store)).toHaveLength(1);

    store.getState().startText({ x: 50, y: 50 });
    store.getState().setText("   ");
    store.getState().commitText();
    expect(shapes(store)).toHaveLength(1);
    expect(past(store)).toHaveLength(1);

    store.getState().editText(id!);
    store.getState().setText("Hi there");
    store.getState().setTool("arrow");
    expect(store.getState().editing).toBe(null);
    expect(shapes(store)[0]).toMatchObject({ text: "Hi there" });
    undo(store);
    expect(shapes(store)[0]).toMatchObject({ text: "Hi" });
  });

  it("deselects when picking the crop tool", () => {
    const store = loaded();
    store.getState().add(rect());
    store.getState().setTool("crop");
    expect(store.getState().selected).toBe(null);
  });
});
