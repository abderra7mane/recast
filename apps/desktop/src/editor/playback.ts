import { useStore } from "zustand";
import { createStore } from "zustand/vanilla";

import { commands } from "@/bindings";
import type { FrameHeader } from "@/editor/frame";
import type { RendererKind } from "@/editor/renderer";

export type PlaybackState = {
  playing: boolean;
  looping: boolean;
  /** Playhead position on the recording's timeline. */
  timeMs: number;
  renderer: RendererKind | null;
  connected: boolean;
  /** Frames drawn in the last second. */
  fps: number;
  /** From the last seek to drawing its frame. */
  seekMs: number | null;
};

export const playbackStore = createStore<PlaybackState>()(() => ({
  playing: false,
  looping: false,
  timeMs: 0,
  renderer: null,
  connected: false,
  fps: 0,
  seekMs: null,
}));

export function usePlayback<T>(selector: (state: PlaybackState) => T): T {
  return useStore(playbackStore, selector);
}

let seekSeq = 0;
let pendingSeek: { tMs: number; at: number } | null = null;

export function seek(tMs: number) {
  seekSeq += 1;
  pendingSeek = { tMs, at: performance.now() };
  playbackStore.setState({ timeMs: tMs });
  void commands.editorSeek(tMs, seekSeq);
}

export function play() {
  playbackStore.setState({ playing: true });
  void commands.editorPlay();
}

export function pause() {
  playbackStore.setState({ playing: false });
  void commands.editorPause();
}

export const togglePlay = () =>
  playbackStore.getState().playing ? pause() : play();

export function setLooping(looping: boolean) {
  playbackStore.setState({ looping });
  void commands.editorSetLoop(looping);
}

/** Updates the playhead from a drawn frame. */
export function frameDrawn(header: FrameHeader) {
  const state = playbackStore.getState();
  if (pendingSeek && Math.abs(header.tMs - pendingSeek.tMs) < 1e-6) {
    playbackStore.setState({ seekMs: performance.now() - pendingSeek.at });
    pendingSeek = null;
  }
  if (header.playing) {
    playbackStore.setState({ playing: true, timeMs: header.tMs });
  } else if (state.playing) {
    playbackStore.setState({ playing: false, timeMs: header.tMs });
  }
}
