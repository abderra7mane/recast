import type { FrameHeader } from "@/editor/frame";
import type { Rect, Size } from "@/markup/model";
import type { ViewMap } from "@/markup/view";

/**
 * Maps the recording's points to a `box` (CSS pixels) showing the frame of `header`,
 * following its zoom. `screen` is the recorded area's size in points.
 */
export function frameMap(
  header: FrameHeader,
  screen: Size,
  box: Size,
): ViewMap {
  const k = box.width / header.width;
  const { content, view } = header;
  return {
    zoom: (content.width / (screen.width * view.width)) * k,
    offsetX: (content.x - (view.x / view.width) * content.width) * k,
    offsetY: (content.y - (view.y / view.height) * content.height) * k,
    originX: 0,
    originY: 0,
  };
}

/** The recording's area in `box`; annotations are clipped to it. */
export function contentBox(header: FrameHeader, box: Size): Rect {
  const k = box.width / header.width;
  const { x, y, width, height } = header.content;
  return { x: x * k, y: y * k, width: width * k, height: height * k };
}
