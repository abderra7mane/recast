import type { Settings } from "@/editor/settings";

/** Positions and sizes are in the screenshot's pixels; widths and font sizes in points. */
export type Point = { x: number; y: number };
export type Rect = { x: number; y: number; width: number; height: number };
export type Size = { width: number; height: number };

export type LineShape = {
  id: string;
  kind: "arrow" | "line";
  from: Point;
  to: Point;
  color: string;
  width: number;
};

export type BoxShape = {
  id: string;
  kind: "rect" | "ellipse";
  rect: Rect;
  color: string;
  width: number;
  fill: boolean;
};

export type HighlightShape = {
  id: string;
  kind: "highlight";
  rect: Rect;
  color: string;
};

/** Hides the screenshot under it; marks stay on top. */
export type ObscureShape = {
  id: string;
  kind: "blur" | "pixelate";
  rect: Rect;
};

export type TextShape = {
  id: string;
  kind: "text";
  /** Top-left corner of the first line. */
  at: Point;
  text: string;
  color: string;
  fontSize: number;
};

export type Shape =
  LineShape | BoxShape | HighlightShape | ObscureShape | TextShape;
export type ShapeKind = Shape["kind"];
/** A shape before it gets its id. */
export type NewShape = Shape extends infer S
  ? S extends Shape
    ? Omit<S, "id">
    : never
  : never;
export type Tool = "select" | "crop" | ShapeKind;

export type Background = Settings["background"];

export type Doc = {
  shapes: Shape[];
  /** The part of the screenshot kept, or the whole of it. */
  crop: Rect | null;
  beautify: boolean;
  background: Background;
};

export type Style = {
  color: string;
  width: number;
  fill: boolean;
  fontSize: number;
  highlight: string;
};

export const DEFAULT_STYLE: Style = {
  color: "#ff3b30",
  width: 3,
  fill: false,
  fontSize: 20,
  highlight: "#ffd60a",
};

export const isLine = (shape: Shape): shape is LineShape =>
  shape.kind === "arrow" || shape.kind === "line";

export const isObscure = (shape: Shape): shape is ObscureShape =>
  shape.kind === "blur" || shape.kind === "pixelate";

export const hasRect = (
  shape: Shape,
): shape is BoxShape | HighlightShape | ObscureShape => "rect" in shape;

export type StyleOptions = {
  color: boolean;
  width: boolean;
  fill: boolean;
  fontSize: boolean;
};

const NONE: StyleOptions = {
  color: false,
  width: false,
  fill: false,
  fontSize: false,
};

/** The style options that apply to shapes of `kind`. */
export function styleOptions(kind: Tool): StyleOptions {
  switch (kind) {
    case "arrow":
    case "line":
      return { ...NONE, color: true, width: true };
    case "rect":
    case "ellipse":
      return { ...NONE, color: true, width: true, fill: true };
    case "highlight":
      return { ...NONE, color: true };
    case "text":
      return { ...NONE, color: true, fontSize: true };
    default:
      return NONE;
  }
}

/** `shape` with the parts of `style` that apply to it. */
export function restyle(shape: Shape, style: Partial<Style>): Shape {
  switch (shape.kind) {
    case "arrow":
    case "line":
      return {
        ...shape,
        color: style.color ?? shape.color,
        width: style.width ?? shape.width,
      };
    case "rect":
    case "ellipse":
      return {
        ...shape,
        color: style.color ?? shape.color,
        width: style.width ?? shape.width,
        fill: style.fill ?? shape.fill,
      };
    case "highlight":
      return { ...shape, color: style.highlight ?? shape.color };
    case "text":
      return {
        ...shape,
        color: style.color ?? shape.color,
        fontSize: style.fontSize ?? shape.fontSize,
      };
    default:
      return shape;
  }
}

/** The style a selected shape shows in the panel. */
export function styleOf(shape: Shape, fallback: Style): Style {
  switch (shape.kind) {
    case "arrow":
    case "line":
      return { ...fallback, color: shape.color, width: shape.width };
    case "rect":
    case "ellipse":
      return {
        ...fallback,
        color: shape.color,
        width: shape.width,
        fill: shape.fill,
      };
    case "highlight":
      return { ...fallback, highlight: shape.color };
    case "text":
      return { ...fallback, color: shape.color, fontSize: shape.fontSize };
    default:
      return fallback;
  }
}
