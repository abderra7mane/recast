import { createContext } from "react";

import { editorStore } from "@/editor/store";

export type Gesture = { begin: () => void; end: () => void };

/** What a drag on a field starts and ends; the editor makes each drag one undo step. */
export const GestureContext = createContext<Gesture>({
  begin: () => editorStore.getState().beginGesture(),
  end: () => editorStore.getState().endGesture(),
});
