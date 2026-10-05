import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import App from "@/App";
import { Editor } from "@/editor/Editor";

import "@/styles/styles.css";

function isEditorWindow() {
  try {
    return getCurrentWebviewWindow().label.startsWith("editor-");
  } catch {
    return false;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>{isEditorWindow() ? <Editor /> : <App />}</StrictMode>,
);
