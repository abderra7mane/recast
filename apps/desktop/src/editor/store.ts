import { temporal, type TemporalState } from "zundo";
import { useStore } from "zustand";
import { createStore, type StoreApi } from "zustand/vanilla";

import {
  annotationStyle,
  DEFAULT_ANNOTATION_STYLE,
  deleteTrack,
  MAX_TRACKS,
  newAnnotationId,
  placeAnnotation,
  restyleShape,
  type AnnotationStyle,
  type AnnotationTool,
} from "@/editor/annotations/model";
import type {
  Annotation,
  Annotations,
  AnnotationShape,
  Section,
  Segment,
  Settings,
} from "@/editor/settings";
import * as math from "@/editor/timeline-math";

/** `live` changes happen during a gesture (a drag); everything else is a commit. */
export type ChangeKind = "live" | "commit";

export type EditorState = {
  settings: Settings | null;
  durationMs: number;
  /** What auto zoom generates for the current settings; comes from the backend. */
  autoSegments: Segment[];
  selected: number | null;
  lastChange: ChangeKind;
  gesture: { before: Settings } | null;
  /** What a drag on the preview does: select and move, or draw an annotation. */
  tool: AnnotationTool;
  /** The style of new annotations, and of the selected one. */
  style: AnnotationStyle;
  selectedAnnotation: string | null;
  /** The track new annotations go to first. */
  activeTrack: number;
  /** An annotation the preview leaves out while the editor draws it itself. */
  hidden: string | null;
  /** The text annotation being typed into. */
  editingText: string | null;

  load: (
    settings: Settings,
    durationMs: number,
    autoSegments: Segment[],
  ) => void;
  setAutoSegments: (segments: Segment[]) => void;
  update: <K extends Section>(section: K, patch: Partial<Settings[K]>) => void;
  beginGesture: () => void;
  endGesture: () => void;
  select: (index: number | null) => void;
  /** Replaces the zoom segments, turning auto zoom off. */
  setSegments: (segments: Segment[], selected?: number | null) => void;
  updateSegment: (index: number, patch: Partial<Segment>) => void;
  addSegment: (tMs: number) => boolean;
  deleteSegment: (index: number) => void;
  setTrim: (trim: { startMs: number; endMs: number }) => void;
  setTool: (tool: AnnotationTool) => void;
  /** Changes the style of new annotations and of the selected one. */
  setStyle: (patch: Partial<AnnotationStyle>) => void;
  selectAnnotation: (id: string | null) => void;
  /**
   * Adds an annotation showing from about `tMs`, on the active track or the first with
   * room, and selects it. Returns its id, or `null` when no track has room.
   */
  addAnnotation: (shape: AnnotationShape, tMs: number) => string | null;
  updateAnnotation: (
    id: string,
    patch: Partial<Omit<Annotation, "id">>,
  ) => void;
  /** Sets the shape of annotation `id` and shows it in the preview again. */
  setAnnotationShape: (id: string, shape: AnnotationShape) => void;
  deleteAnnotation: (id: string) => void;
  setAnnotationItems: (items: Annotation[]) => void;
  addTrack: () => void;
  /** Removes a track with its annotations. */
  deleteTrack: (track: number) => void;
  setHidden: (id: string | null) => void;
  setEditingText: (id: string | null) => void;
};

type Tracked = { settings: Settings | null };
type WithHistory = { temporal: StoreApi<TemporalState<Tracked>> };

/** The segments shown on the timeline: generated ones while auto zoom is on. */
export const visibleSegments = (state: EditorState): Segment[] =>
  state.settings?.zoom.auto
    ? state.autoSegments
    : (state.settings?.zoom.segments ?? []);

export const annotationById = (state: EditorState, id: string | null) =>
  id === null
    ? undefined
    : state.settings?.annotations.items.find((a) => a.id === id);

export function createEditorStore() {
  return createStore<EditorState>()(
    temporal(
      (set, get, api) => {
        const history = () => (api as unknown as WithHistory).temporal;
        const dropMissingSelection = () => {
          const { selectedAnnotation } = get();
          if (
            selectedAnnotation &&
            !annotationById(get(), selectedAnnotation)
          ) {
            set({ selectedAnnotation: null });
          }
        };
        const change = (settings: Settings, extra: Partial<EditorState> = {}) =>
          set({
            settings,
            lastChange: get().gesture ? "live" : "commit",
            ...extra,
          });

        return {
          settings: null,
          durationMs: 0,
          autoSegments: [],
          selected: null,
          lastChange: "commit",
          gesture: null,
          tool: "select",
          style: DEFAULT_ANNOTATION_STYLE,
          selectedAnnotation: null,
          activeTrack: 0,
          hidden: null,
          editingText: null,

          load: (settings, durationMs, autoSegments) => {
            set({
              settings,
              durationMs,
              autoSegments,
              selected: null,
              selectedAnnotation: null,
              hidden: null,
              editingText: null,
              activeTrack: 0,
              lastChange: "commit",
            });
            history().getState().clear();
          },

          setAutoSegments: (autoSegments) => set({ autoSegments }),

          update: (section, patch) => {
            const settings = get().settings;
            if (!settings) return;
            const current = settings[section] as Record<string, unknown>;
            const unchanged = Object.entries(patch).every(
              ([k, v]) => current[k] === v,
            );
            if (unchanged) return;
            change({
              ...settings,
              [section]: { ...settings[section], ...patch },
            });
          },

          beginGesture: () => {
            const { settings, gesture } = get();
            if (!settings || gesture) return;
            history().getState().pause();
            set({ gesture: { before: settings } });
          },

          endGesture: () => {
            const { gesture, settings } = get();
            if (!gesture) return;
            const changed =
              settings !== gesture.before &&
              JSON.stringify(settings) !== JSON.stringify(gesture.before);
            set({
              gesture: null,
              lastChange: "commit",
              ...(changed ? {} : { settings: gesture.before }),
            });
            const past = history().getState();
            past.resume();
            if (changed) {
              history().setState({
                pastStates: [...past.pastStates, { settings: gesture.before }],
                futureStates: [],
              });
            }
          },

          select: (selected) =>
            set(
              selected === null
                ? { selected }
                : { selected, selectedAnnotation: null },
            ),

          setSegments: (segments, selected) => {
            const settings = get().settings;
            if (!settings) return;
            change(
              {
                ...settings,
                zoom: { ...settings.zoom, auto: false, segments },
              },
              selected === undefined ? {} : { selected },
            );
          },

          updateSegment: (index, patch) => {
            const segments = visibleSegments(get());
            if (!segments[index]) return;
            get().setSegments(
              segments.map((s, i) => (i === index ? { ...s, ...patch } : s)),
            );
          },

          addSegment: (tMs) => {
            const state = get();
            if (!state.settings) return false;
            const added = math.addSegment(
              visibleSegments(state),
              tMs,
              state.durationMs,
              state.settings.zoom.level,
            );
            if (!added) return false;
            state.setSegments(added.segments, added.index);
            return true;
          },

          deleteSegment: (index) => {
            const segments = visibleSegments(get());
            if (!segments[index]) return;
            get().setSegments(math.deleteSegment(segments, index), null);
          },

          setTrim: ({ startMs, endMs }) =>
            get().update("trim", { startMs, endMs }),

          setTool: (tool) => set({ tool }),

          setStyle: (patch) => {
            const { style, selectedAnnotation } = get();
            set({ style: { ...style, ...patch } });
            const annotation = annotationById(get(), selectedAnnotation);
            if (!annotation) return;
            const shape = restyleShape(annotation.shape, patch);
            const changed = (Object.keys(shape) as (keyof typeof shape)[]).some(
              (key) => shape[key] !== annotation.shape[key],
            );
            if (changed) get().updateAnnotation(annotation.id, { shape });
          },

          selectAnnotation: (id) => {
            const annotation = annotationById(get(), id);
            set(
              annotation
                ? {
                    selectedAnnotation: annotation.id,
                    selected: null,
                    activeTrack: annotation.track,
                    style: annotationStyle(annotation.shape, get().style),
                  }
                : { selectedAnnotation: null },
            );
          },

          addAnnotation: (shape, tMs) => {
            const { settings, durationMs, activeTrack } = get();
            if (!settings) return null;
            const annotations = settings.annotations;
            const place = placeAnnotation(
              annotations,
              tMs,
              durationMs,
              activeTrack,
            );
            if (!place) return null;
            const id = newAnnotationId();
            const added: Annotations = {
              tracks: Math.max(annotations.tracks, place.track + 1),
              items: [...annotations.items, { id, shape, ...place }],
            };
            change(
              { ...settings, annotations: added },
              {
                selectedAnnotation: id,
                selected: null,
                activeTrack: place.track,
              },
            );
            return id;
          },

          updateAnnotation: (id, patch) => {
            const settings = get().settings;
            if (!annotationById(get(), id) || !settings) return;
            const items = settings.annotations.items.map((a) =>
              a.id === id ? { ...a, ...patch } : a,
            );
            get().update("annotations", { items });
          },

          setAnnotationShape: (id, shape) => {
            const settings = get().settings;
            if (!settings) return;
            const items = settings.annotations.items.map((a) =>
              a.id === id ? { ...a, shape } : a,
            );
            change(
              { ...settings, annotations: { ...settings.annotations, items } },
              { hidden: null },
            );
          },

          deleteAnnotation: (id) => {
            const { settings, selectedAnnotation } = get();
            if (!settings || !annotationById(get(), id)) return;
            const items = settings.annotations.items.filter((a) => a.id !== id);
            change(
              { ...settings, annotations: { ...settings.annotations, items } },
              selectedAnnotation === id ? { selectedAnnotation: null } : {},
            );
          },

          setAnnotationItems: (items) => get().update("annotations", { items }),

          addTrack: () => {
            const tracks = get().settings?.annotations.tracks ?? MAX_TRACKS;
            if (tracks >= MAX_TRACKS) return;
            get().update("annotations", { tracks: tracks + 1 });
            set({ activeTrack: tracks });
          },

          deleteTrack: (track) => {
            const settings = get().settings;
            if (!settings) return;
            const annotations = deleteTrack(settings.annotations, track);
            change(
              { ...settings, annotations },
              {
                activeTrack: Math.min(
                  get().activeTrack,
                  annotations.tracks - 1,
                ),
              },
            );
            dropMissingSelection();
          },

          setHidden: (hidden) => {
            if (get().hidden !== hidden) set({ hidden });
          },

          setEditingText: (editingText) => set({ editingText }),
        };
      },
      {
        partialize: (state): Tracked => ({ settings: state.settings }),
        equality: (past, current) => past.settings === current.settings,
        limit: 200,
      },
    ),
  );
}

export type EditorStore = ReturnType<typeof createEditorStore>;

export const editorStore = createEditorStore();

export function useEditor<T>(selector: (state: EditorState) => T): T {
  return useStore(editorStore, selector);
}

/** Drops a selected annotation that an undo or redo removed. */
function keepSelection(store: EditorStore) {
  const state = store.getState();
  if (
    state.selectedAnnotation !== null &&
    !annotationById(state, state.selectedAnnotation)
  ) {
    store.setState({ selectedAnnotation: null });
  }
}

export function undoIn(store: EditorStore) {
  store.temporal.getState().undo();
  keepSelection(store);
}

export function redoIn(store: EditorStore) {
  store.temporal.getState().redo();
  keepSelection(store);
}

export const undo = () => undoIn(editorStore);
export const redo = () => redoIn(editorStore);
