import { temporal, type TemporalState } from "zundo";
import { useStore } from "zustand";
import { createStore, type StoreApi } from "zustand/vanilla";

import type { Section, Segment, Settings } from "@/editor/settings";
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
};

type Tracked = { settings: Settings | null };
type WithHistory = { temporal: StoreApi<TemporalState<Tracked>> };

/** The segments shown on the timeline: generated ones while auto zoom is on. */
export const visibleSegments = (state: EditorState): Segment[] =>
  state.settings?.zoom.auto
    ? state.autoSegments
    : (state.settings?.zoom.segments ?? []);

export function createEditorStore() {
  return createStore<EditorState>()(
    temporal(
      (set, get, api) => {
        const history = () => (api as unknown as WithHistory).temporal;
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

          load: (settings, durationMs, autoSegments) => {
            set({
              settings,
              durationMs,
              autoSegments,
              selected: null,
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
            set({ gesture: null, lastChange: "commit" });
            const past = history().getState();
            past.resume();
            if (settings && settings !== gesture.before) {
              history().setState({
                pastStates: [...past.pastStates, { settings: gesture.before }],
                futureStates: [],
              });
            }
          },

          select: (selected) => set({ selected }),

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

export const undo = () => editorStore.temporal.getState().undo();
export const redo = () => editorStore.temporal.getState().redo();
