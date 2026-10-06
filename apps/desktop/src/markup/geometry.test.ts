import { describe, expect, it } from "vitest";

import {
  bounds,
  constrainAngle,
  constrainSquare,
  drawPoint,
  handleAt,
  handles,
  hitTest,
  hits,
  moveShape,
  resizeRect,
  resizeShape,
  type TextMeasure,
} from "@/markup/geometry";
import type { BoxShape, LineShape, Shape, TextShape } from "@/markup/model";

/** Text 10 px wide per character and 20 px high per line. */
const measure: TextMeasure = (shape) => ({
  width: Math.max(...shape.text.split("\n").map((l) => l.length)) * 10,
  height: shape.text.split("\n").length * 20,
});

const line = (patch: Partial<LineShape> = {}): LineShape => ({
  id: "l",
  kind: "line",
  from: { x: 0, y: 0 },
  to: { x: 100, y: 0 },
  color: "#f00",
  width: 2,
  ...patch,
});

const box = (patch: Partial<BoxShape> = {}): BoxShape => ({
  id: "b",
  kind: "rect",
  rect: { x: 10, y: 10, width: 100, height: 50 },
  color: "#f00",
  width: 2,
  fill: false,
  ...patch,
});

const text = (patch: Partial<TextShape> = {}): TextShape => ({
  id: "t",
  kind: "text",
  at: { x: 50, y: 50 },
  text: "Hello",
  color: "#000",
  fontSize: 20,
  ...patch,
});

describe("hit testing", () => {
  it("finds lines within half their stroke plus the tolerance", () => {
    const shape = line();
    // A 2 pt stroke at scale 2 is 4 px wide; 2 px each side, plus 3 px.
    expect(hits(shape, { x: 50, y: 5 }, 3, 2, measure)).toBe(true);
    expect(hits(shape, { x: 50, y: 5.1 }, 3, 2, measure)).toBe(false);
    expect(hits(shape, { x: 105.1, y: 0 }, 3, 2, measure)).toBe(false);
    expect(hits(shape, { x: 105, y: 0 }, 3, 2, measure)).toBe(true);
  });

  it("finds outlines on their stroke and filled shapes inside", () => {
    expect(hits(box(), { x: 60, y: 35 }, 2, 1, measure)).toBe(false);
    expect(hits(box(), { x: 11, y: 35 }, 2, 1, measure)).toBe(true);
    expect(hits(box(), { x: 60, y: 63 }, 2, 1, measure)).toBe(true);
    expect(hits(box({ fill: true }), { x: 60, y: 35 }, 2, 1, measure)).toBe(
      true,
    );

    const ellipse = box({ kind: "ellipse" });
    expect(hits(ellipse, { x: 60, y: 35 }, 2, 1, measure)).toBe(false);
    expect(hits(ellipse, { x: 60, y: 10 }, 2, 1, measure)).toBe(true);
    expect(hits(ellipse, { x: 11, y: 11 }, 2, 1, measure)).toBe(false);
  });

  it("finds areas and text anywhere inside", () => {
    const blur: Shape = { id: "x", kind: "blur", rect: box().rect };
    expect(hits(blur, { x: 60, y: 35 }, 0, 1, measure)).toBe(true);
    expect(hits(text(), { x: 99, y: 69 }, 0, 1, measure)).toBe(true);
    expect(hits(text(), { x: 101, y: 69 }, 0, 1, measure)).toBe(false);
  });

  it("returns the topmost shape", () => {
    const below = box({ id: "below", fill: true });
    const above = box({ id: "above", fill: true });
    expect(hitTest([below, above], { x: 50, y: 30 }, 2, 1, measure)?.id).toBe(
      "above",
    );
    expect(hitTest([below, above], { x: 500, y: 30 }, 2, 1, measure)).toBe(
      null,
    );
  });
});

describe("handles", () => {
  it("puts lines' handles on their ends and boxes' on corners and edges", () => {
    expect(handles(line(), measure).map((h) => h.handle)).toEqual([
      "start",
      "end",
    ]);
    const points = handles(box(), measure);
    expect(points).toHaveLength(8);
    expect(points.find((h) => h.handle === "se")?.at).toEqual({
      x: 110,
      y: 60,
    });
    expect(points.find((h) => h.handle === "n")?.at).toEqual({ x: 60, y: 10 });
    expect(handles(text(), measure).map((h) => h.handle)).toEqual([
      "nw",
      "ne",
      "se",
      "sw",
    ]);
  });

  it("finds the nearest handle within the tolerance", () => {
    const points = handles(box(), measure);
    expect(handleAt(points, { x: 113, y: 62 }, 4)).toBe("se");
    expect(handleAt(points, { x: 115, y: 62 }, 4)).toBe(null);
    expect(handleAt(points, { x: 60, y: 9 }, 4)).toBe("n");
  });
});

describe("resizing", () => {
  it("keeps the opposite corner and flips past it", () => {
    const rect = { x: 10, y: 10, width: 100, height: 50 };
    expect(resizeRect(rect, "se", { x: 50, y: 100 }, false)).toEqual({
      x: 10,
      y: 10,
      width: 40,
      height: 90,
    });
    expect(resizeRect(rect, "se", { x: 0, y: 0 }, false)).toEqual({
      x: 0,
      y: 0,
      width: 10,
      height: 10,
    });
    expect(resizeRect(rect, "n", { x: 999, y: 0 }, false)).toEqual({
      x: 10,
      y: 0,
      width: 100,
      height: 60,
    });
    expect(resizeRect(rect, "w", { x: 20, y: 999 }, false)).toEqual({
      x: 20,
      y: 10,
      width: 90,
      height: 50,
    });
  });

  it("makes squares and circles with Shift", () => {
    const rect = { x: 10, y: 10, width: 100, height: 50 };
    expect(resizeRect(rect, "se", { x: 50, y: 100 }, true)).toEqual({
      x: 10,
      y: 10,
      width: 90,
      height: 90,
    });
    expect(resizeRect(rect, "nw", { x: 100, y: 0 }, true)).toEqual({
      x: 50,
      y: 0,
      width: 60,
      height: 60,
    });
    expect(
      drawPoint("ellipse", { x: 0, y: 0 }, { x: 30, y: -10 }, true),
    ).toEqual({ x: 30, y: -30 });
  });

  it("turns lines and arrows to 45° steps with Shift", () => {
    expect(constrainAngle({ x: 0, y: 0 }, { x: 100, y: 10 })).toEqual({
      x: 100,
      y: 0,
    });
    const diagonal = constrainAngle({ x: 0, y: 0 }, { x: 100, y: 80 });
    expect(diagonal.x).toBeCloseTo(diagonal.y);
    expect(diagonal.x).toBeCloseTo(90);
    expect(constrainAngle({ x: 5, y: 5 }, { x: 7, y: -100 })).toEqual({
      x: 5,
      y: -100,
    });

    const moved = resizeShape(
      line({ kind: "arrow" }),
      "end",
      { x: 50, y: 48 },
      true,
      measure,
    ) as LineShape;
    expect(moved.from).toEqual({ x: 0, y: 0 });
    expect(moved.to.x).toBeCloseTo(49);
    expect(moved.to.y).toBeCloseTo(49);
    const start = resizeShape(line(), "start", { x: 97, y: 40 }, true, measure);
    expect((start as LineShape).from).toEqual({ x: 100, y: 40 });
  });

  it("scales text from the opposite corner", () => {
    const resized = resizeShape(
      text(),
      "se",
      { x: 999, y: 90 },
      false,
      measure,
    ) as TextShape;
    expect(resized.fontSize).toBe(40);
    expect(resized.at).toEqual({ x: 50, y: 50 });

    const fromTop = resizeShape(
      text(),
      "nw",
      { x: 0, y: 60 },
      false,
      measure,
    ) as TextShape;
    expect(fromTop.fontSize).toBe(10);
    expect(fromTop.at.x).toBeCloseTo(75);
    expect(fromTop.at.y).toBeCloseTo(60);
  });

  it("squares from the anchor on the longer side", () => {
    expect(constrainSquare({ x: 10, y: 10 }, { x: 0, y: 40 })).toEqual({
      x: -20,
      y: 40,
    });
  });
});

describe("moving", () => {
  it("moves every kind of shape", () => {
    expect(moveShape(line(), 5, -5)).toMatchObject({
      from: { x: 5, y: -5 },
      to: { x: 105, y: -5 },
    });
    expect(moveShape(box(), 5, -5)).toMatchObject({
      rect: { x: 15, y: 5, width: 100, height: 50 },
    });
    expect(moveShape(text(), 5, -5)).toMatchObject({ at: { x: 55, y: 45 } });
    expect(bounds(text({ text: "ab\nabcd" }), measure)).toEqual({
      x: 50,
      y: 50,
      width: 40,
      height: 40,
    });
  });
});
