import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import App from "@/App";
import { Beautify } from "@/beautify/Beautify";
import { Editor } from "@/editor/Editor";

import "@/styles/styles.css";

function windowLabel() {
  try {
    return getCurrentWebviewWindow().label;
  } catch {
    return "";
  }
}

function view() {
  const label = windowLabel();
  if (label.startsWith("editor-")) return <Editor />;
  if (label.startsWith("beautify-")) return <Beautify />;
  return <App />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>{view()}</StrictMode>,
);
