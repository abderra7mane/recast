// @vitest-environment jsdom
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Project } from "@/bindings";
import { TooltipProvider } from "@/components/ui/tooltip";
import { segment, settings } from "@/editor/fixtures.test-util";
import type { Settings } from "@/editor/settings";

const commands = vi.hoisted(() => ({ clickSoundPreview: vi.fn() }));
vi.mock("@/bindings", () => ({ commands }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const { Inspector } = await import("@/editor/inspector/Inspector");
const { editorStore } = await import("@/editor/store");

const PROJECT = {
  version: 1,
  name: "Take",
  createdAtUnixMs: 0,
  recording: {
    source: { kind: "display", displayId: 1 },
    bounds: { x: 0, y: 0, width: 1368, height: 954 },
    width: 1368,
    height: 954,
    scaleFactor: 1,
    fps: 60,
    durationMs: 10_000,
    video: { file: "screen.mp4", codec: "hevc", durationMs: 10_000 },
    systemAudio: null,
    mic: { file: "mic.m4a", offsetMs: 0 },
    eventsFile: "events.msgpack",
    cursorsDir: "cursors",
    recovered: false,
  },
} as unknown as Project;

const TABS = [
  "background",
  "cursor",
  "zoom",
  "clicks",
  "audio",
  "export",
] as const;

const CONTROLS = [
  '[role="slider"]',
  '[role="switch"]',
  '[role="combobox"]',
  '[role="radio"]',
  'input[type="color"]',
  "button",
].join(",");

function show(tab: (typeof TABS)[number], edit?: (s: Settings) => void) {
  const s = settings();
  edit?.(s);
  editorStore.getState().load(s, 10_000, [segment(1000, 3000)]);
  return render(
    <TooltipProvider>
      <Inspector project={PROJECT} tab={tab} onTabChange={() => {}} />
    </TooltipProvider>,
  );
}

/** The inspector controls that have no hint text next to them. */
function controlsWithoutHint(container: HTMLElement) {
  const panel = container.querySelector(
    '[role="tabpanel"][data-state="active"]',
  )!;
  return [...panel.querySelectorAll<HTMLElement>(CONTROLS)].filter(
    (control) =>
      !control
        .closest('[data-slot="field"]')
        ?.querySelector('[data-slot="hint"]')
        ?.textContent?.trim(),
  );
}

beforeEach(() => vi.clearAllMocks());

describe("Inspector", () => {
  it.each(TABS)("gives every %s option a hint", (tab) => {
    const { container } = show(tab);
    const panel = container.querySelector(
      '[role="tabpanel"][data-state="active"]',
    )!;
    expect(panel.querySelectorAll(CONTROLS).length).toBeGreaterThan(0);
    expect(controlsWithoutHint(container)).toEqual([]);
  });

  it.each([
    ["solid", { kind: "solid", color: "#000000" }],
    ["image", { kind: "image", path: "/tmp/sky.png" }],
  ] as const)("gives the %s fill options hints", (_, fill) => {
    const { container } = show("background", (s) => {
      s.background.fill = fill;
    });
    expect(controlsWithoutHint(container)).toEqual([]);
  });

  it("gives the options of a selected zoom segment hints", () => {
    const { container } = show("zoom", (s) => {
      s.zoom.auto = false;
      s.zoom.segments = [
        { ...segment(1000, 3000), focus: { kind: "point", x: 0.5, y: 0.5 } },
      ];
    });
    act(() => editorStore.getState().select(0));
    expect(screen.getByText("Horizontal")).toBeTruthy();
    expect(controlsWithoutHint(container)).toEqual([]);
  });

  it("warns when a preset enlarges the recording", () => {
    show("export", (s) => {
      s.export.resolution = "4k";
    });
    expect(screen.getByRole("status").textContent).toContain(
      "The recording is 1368 × 954 px. 4K enlarges it 2.0×",
    );
  });

  it("does not warn at Auto", () => {
    show("export", (s) => {
      s.export.resolution = "auto";
    });
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.getByText(/\(1520 × 1106\)/)).toBeTruthy();
  });

  it("previews the chosen sound pack at the chosen volume", async () => {
    const started = vi.fn();
    const gain = { gain: { value: 0 }, connect: vi.fn((next) => next) };
    class FakeAudioContext {
      destination = {};
      decodeAudioData = vi.fn(async () => "decoded");
      createGain = () => gain;
      createBufferSource = () => ({
        buffer: null,
        connect: vi.fn((next) => next),
        start: started,
      });
    }
    vi.stubGlobal("AudioContext", FakeAudioContext);
    commands.clickSoundPreview.mockResolvedValue([82, 73, 70, 70]);
    show("clicks", (s) => {
      s.sounds.pack = "pop";
      s.sounds.volume = 0.4;
    });
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Play the sound" }));
    await vi.waitFor(() => expect(started).toHaveBeenCalled());
    expect(commands.clickSoundPreview).toHaveBeenCalledWith("pop");
    expect(gain.gain.value).toBe(0.4);
    vi.unstubAllGlobals();
  });
});
