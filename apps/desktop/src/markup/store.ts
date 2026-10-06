import { temporal, type TemporalState } from "zundo";
import { useStore } from "zustand";
import { createStore, type StoreApi } from "zustand/vanilla";

import { moveShape } from "@/markup/geometry";
import {
  DEFAULT_STYLE,
  restyle,
  styleOf,
  type Background,
  type Doc,
  type NewShape,
  type Point,
  type Rect,
  type Shape,
  type Size,
  type Style,
  type Tool,
} from "@/markup/model";

/** How far a duplicate lands from its original, in points. */
const DUPLICATE_OFFSET = 12;

export type Zoom = number | "fit";

export type MarkupState = {
  doc: Doc;
  image: Size;
  scale: number;
  tool: Tool;
  style: Style;
  selected: string | null;
  /** The text shape being typed into. */
  editing: string | null;
  zoom: Zoom;
  /** The zoom the view shows, with "fit" worked out. */
  shownZoom: number;
  gesture: { before: Doc } | null;

  load: (image: Size, scale: number, background: Background) => void;
  setTool: (tool: Tool) => void;
  select: (id: string | null) => void;
  /** Adds `shape` on top and selects it. */
  add: (shape: NewShape) => string;
  replace: (shape: Shape) => void;
  remove: (id: string) => void;
  duplicate: (id: string) => void;
  nudge: (id: string, dx: number, dy: number) => void;
  setCrop: (crop: Rect | null) => void;
  setStyle: (patch: Partial<Style>) => void;
  setBeautify: (beautify: boolean) => void;
  setBackground: (patch: Partial<Background>) => void;
  beginGesture: () => void;
  endGesture: () => void;
  /** Starts typing into a new text shape at `at`. */
  startText: (at: Point) => void;
  editText: (id: string) => void;
  setText: (text: string) => void;
  /** Ends typing; a shape left empty is removed. */
  commitText: () => void;
  setZoom: (zoom: Zoom) => void;
};

type Tracked = { doc: Doc };
type WithHistory = { temporal: StoreApi<TemporalState<Tracked>> };

const sameDoc = (a: Doc, b: Doc) =>
  a === b ||
  (a.crop === b.crop &&
    a.beautify === b.beautify &&
    a.background === b.background &&
    a.shapes.length === b.shapes.length &&
    a.shapes.every((shape, i) => shape === b.shapes[i]));

let nextId = 1;
const newId = () => `s${nextId++}`;

export function createMarkupStore() {
  return createStore<MarkupState>()(
    temporal(
      (set, get, api) => {
        const history = () => (api as unknown as WithHistory).temporal;
        const setDoc = (doc: Doc, extra: Partial<MarkupState> = {}) =>
          set({ doc, ...extra });
        const updateShapes = (map: (shapes: Shape[]) => Shape[]) => {
          const { doc } = get();
          setDoc({ ...doc, shapes: map(doc.shapes) });
        };
        const finishTyping = () => {
          if (get().editing) get().commitText();
        };

        return {
          doc: {
            shapes: [],
            crop: null,
            beautify: false,
            background: {} as Background,
          },
          image: { width: 1, height: 1 },
          scale: 1,
          tool: "select",
          style: DEFAULT_STYLE,
          selected: null,
          editing: null,
          zoom: "fit",
          shownZoom: 1,
          gesture: null,

          load: (image, scale, background) => {
            set({
              doc: { shapes: [], crop: null, beautify: false, background },
              image,
              scale,
              tool: "select",
              selected: null,
              editing: null,
              zoom: "fit",
              gesture: null,
            });
            history().getState().clear();
          },

          setTool: (tool) => {
            finishTyping();
            set({ tool, ...(tool === "crop" ? { selected: null } : {}) });
          },

          select: (selected) => {
            if (get().selected === selected) return;
            finishTyping();
            const shape = get().doc.shapes.find((s) => s.id === selected);
            set({
              selected: shape ? selected : null,
              ...(shape ? { style: styleOf(shape, get().style) } : {}),
            });
          },

          add: (shape) => {
            const id = newId();
            updateShapes((shapes) => [...shapes, { ...shape, id } as Shape]);
            set({ selected: id });
            return id;
          },

          replace: (shape) =>
            updateShapes((shapes) =>
              shapes.map((s) => (s.id === shape.id ? shape : s)),
            ),

          remove: (id) => {
            if (!get().doc.shapes.some((s) => s.id === id)) return;
            updateShapes((shapes) => shapes.filter((s) => s.id !== id));
            const { selected, editing } = get();
            set({
              selected: selected === id ? null : selected,
              editing: editing === id ? null : editing,
            });
          },

          duplicate: (id) => {
            const shape = get().doc.shapes.find((s) => s.id === id);
            if (!shape) return;
            const offset = DUPLICATE_OFFSET * get().scale;
            get().add(moveShape(shape, offset, offset));
          },

          nudge: (id, dx, dy) => {
            const shape = get().doc.shapes.find((s) => s.id === id);
            if (shape) get().replace(moveShape(shape, dx, dy));
          },

          setCrop: (crop) => {
            const { doc } = get();
            if (doc.crop === crop) return;
            setDoc({ ...doc, crop });
          },

          setStyle: (patch) => {
            const { style, selected, doc } = get();
            set({ style: { ...style, ...patch } });
            const shape = doc.shapes.find((s) => s.id === selected);
            if (!shape) return;
            const restyled = restyle(shape, patch);
            const changed = (Object.keys(restyled) as (keyof Shape)[]).some(
              (key) => restyled[key] !== shape[key],
            );
            if (changed) get().replace(restyled);
          },

          setBeautify: (beautify) => {
            const { doc } = get();
            if (doc.beautify !== beautify) setDoc({ ...doc, beautify });
          },

          setBackground: (patch) => {
            const { doc } = get();
            setDoc({ ...doc, background: { ...doc.background, ...patch } });
          },

          beginGesture: () => {
            if (get().gesture) return;
            history().getState().pause();
            set({ gesture: { before: get().doc } });
          },

          endGesture: () => {
            const { gesture, doc } = get();
            if (!gesture) return;
            const changed = !sameDoc(doc, gesture.before);
            set({ gesture: null, ...(changed ? {} : { doc: gesture.before }) });
            const past = history().getState();
            past.resume();
            if (changed) {
              history().setState({
                pastStates: [...past.pastStates, { doc: gesture.before }],
                futureStates: [],
              });
            }
          },

          startText: (at) => {
            finishTyping();
            const { style } = get();
            get().beginGesture();
            const id = get().add({
              kind: "text",
              at,
              text: "",
              color: style.color,
              fontSize: style.fontSize,
            });
            set({ editing: id });
          },

          editText: (id) => {
            if (get().editing === id) return;
            finishTyping();
            const shape = get().doc.shapes.find((s) => s.id === id);
            if (shape?.kind !== "text") return;
            get().select(id);
            get().beginGesture();
            set({ editing: id });
          },

          setText: (text) => {
            const { editing, doc } = get();
            const shape = doc.shapes.find((s) => s.id === editing);
            if (shape?.kind === "text") get().replace({ ...shape, text });
          },

          commitText: () => {
            const { editing, doc } = get();
            set({ editing: null });
            const shape = doc.shapes.find((s) => s.id === editing);
            if (shape?.kind === "text" && shape.text.trim() === "") {
              get().remove(shape.id);
            }
            get().endGesture();
          },

          setZoom: (zoom) => set({ zoom }),
        };
      },
      {
        partialize: (state): Tracked => ({ doc: state.doc }),
        equality: (past, current) => past.doc === current.doc,
        limit: 300,
      },
    ),
  );
}

export type MarkupStore = ReturnType<typeof createMarkupStore>;

/** Undoes the last change; a selection whose shape is gone is dropped. */
export function undo(store: MarkupStore) {
  if (store.getState().editing) store.getState().commitText();
  store.temporal.getState().undo();
  dropMissingSelection(store);
}

export function redo(store: MarkupStore) {
  if (store.getState().editing) store.getState().commitText();
  store.temporal.getState().redo();
  dropMissingSelection(store);
}

function dropMissingSelection(store: MarkupStore) {
  const { selected, doc } = store.getState();
  if (selected && !doc.shapes.some((s) => s.id === selected)) {
    store.setState({ selected: null });
  }
}

export const markupStore = createMarkupStore();

export function useMarkup<T>(selector: (state: MarkupState) => T): T {
  return useStore(markupStore, selector);
}
