import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

import { Beautify } from "@/beautify/Beautify";
import { Editor } from "@/editor/Editor";
import { Library } from "@/library/Library";
import { forwardToLogFile } from "@/logging";
import { Onboarding } from "@/onboarding/Onboarding";
import { Settings } from "@/settings/Settings";

import "@/styles/styles.css";

function windowLabel() {
  try {
    return getCurrentWebviewWindow().label;
  } catch {
    return "";
  }
}

function view(label: string) {
  if (label.startsWith("editor-")) return <Editor />;
  if (label.startsWith("beautify-")) return <Beautify />;
  if (label === "settings") return <Settings />;
  if (label === "onboarding") return <Onboarding />;
  return <Library />;
}

const label = windowLabel();
forwardToLogFile(label || "library");

createRoot(document.getElementById("root")!).render(
  <StrictMode>{view(label)}</StrictMode>,
);
