import {
  Circle,
  Crop,
  Droplets,
  Grid3x3,
  Highlighter,
  MousePointer2,
  MoveUpRight,
  Slash,
  Square,
  Type,
  type LucideIcon,
} from "lucide-react";

import type { Tool } from "@/markup/model";

export type ToolInfo = {
  tool: Tool;
  label: string;
  /** The key that picks the tool. */
  key: string;
  icon: LucideIcon;
  /** What to do with the tool, shown under the picture. */
  hint: string;
};

const HIDE =
  "Drag over what to hide. The saved image keeps no detail under it; marks stay on top.";

export const TOOLS: ToolInfo[] = [
  {
    tool: "select",
    label: "Select",
    key: "v",
    icon: MousePointer2,
    hint: "Click a mark to select it, drag to move it, drag its handles to resize. ⌫ deletes, ⌘D duplicates.",
  },
  {
    tool: "arrow",
    label: "Arrow",
    key: "a",
    icon: MoveUpRight,
    hint: "Drag to draw an arrow. Hold Shift for 45° angles.",
  },
  {
    tool: "line",
    label: "Line",
    key: "l",
    icon: Slash,
    hint: "Drag to draw a line. Hold Shift for 45° angles.",
  },
  {
    tool: "rect",
    label: "Rectangle",
    key: "r",
    icon: Square,
    hint: "Drag to draw a rectangle. Hold Shift for a square.",
  },
  {
    tool: "ellipse",
    label: "Ellipse",
    key: "o",
    icon: Circle,
    hint: "Drag to draw an ellipse. Hold Shift for a circle.",
  },
  {
    tool: "text",
    label: "Text",
    key: "t",
    icon: Type,
    hint: "Click to place text, or click a text to edit it. Esc or ⌘Return finishes.",
  },
  {
    tool: "highlight",
    label: "Highlight",
    key: "h",
    icon: Highlighter,
    hint: "Drag over what to highlight, like a marker.",
  },
  { tool: "blur", label: "Blur", key: "b", icon: Droplets, hint: HIDE },
  { tool: "pixelate", label: "Pixelate", key: "p", icon: Grid3x3, hint: HIDE },
  {
    tool: "crop",
    label: "Crop",
    key: "c",
    icon: Crop,
    hint: "Drag the handles or drag out a new area; Shift keeps the shape. Return applies, the original stays until Done.",
  },
];

export const toolInfo = (tool: Tool) =>
  TOOLS.find((info) => info.tool === tool) ?? TOOLS[0];
