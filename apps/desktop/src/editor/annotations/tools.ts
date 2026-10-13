import type { AnnotationTool } from "@/editor/annotations/model";
import { TOOLS, toolInfo, type ToolInfo } from "@/markup/tools";

/** The screenshot editor's tools, without crop. */
export const ANNOTATION_TOOLS = TOOLS.filter(
  (info): info is ToolInfo & { tool: AnnotationTool } => info.tool !== "crop",
);

const HIDE =
  "Drag over what to hide. It stays hidden as the recording moves and zooms, in the preview and the export.";

const HINTS: Partial<Record<AnnotationTool, string>> = {
  select:
    "Click an annotation on the video to select it, drag to move it, drag its handles to resize. ⌫ deletes.",
  text: "Click the video to place text, or click a text to edit it. Esc or ⌘Return finishes.",
  blur: HIDE,
  pixelate: HIDE,
};

/** What to do with `tool` on the video. */
export const annotationHint = (tool: AnnotationTool) =>
  HINTS[tool] ?? toolInfo(tool).hint;

export const annotationLabel = (kind: AnnotationTool) => toolInfo(kind).label;
