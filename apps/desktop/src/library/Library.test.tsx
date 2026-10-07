// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const commands = vi.hoisted(() => ({
  listUnfinished: vi.fn(),
  listProjects: vi.fn(),
  listScreenshots: vi.fn(),
  editScreenshot: vi.fn(),
  revealInFinder: vi.fn(),
  recordingPhase: vi.fn(),
  startRecording: vi.fn(),
  stopRecording: vi.fn(),
  takeScreenshot: vi.fn(),
  openRecordingsFolder: vi.fn(),
  openWindow: vi.fn(),
}));
const events = vi.hoisted(() => ({
  recordingChanged: { listen: vi.fn(() => Promise.resolve(() => {})) },
}));

vi.mock("@/bindings", () => ({ commands, events }));

const { Library } = await import("@/library/Library");

beforeEach(() => {
  vi.clearAllMocks();
  commands.listUnfinished.mockReturnValue(ok([]));
  commands.listProjects.mockReturnValue(ok([]));
  commands.listScreenshots.mockReturnValue(ok([]));
  commands.editScreenshot.mockReturnValue(ok(null));
  commands.recordingPhase.mockReturnValue(ok({ kind: "idle" }));
  commands.startRecording.mockReturnValue(ok(null));
  commands.takeScreenshot.mockReturnValue(ok(null));
});

describe("Library", () => {
  it("records and captures each mode separately", async () => {
    const user = userEvent.setup();
    render(<Library />);
    for (const [label, mode] of [
      ["Area", "area"],
      ["Window", "window"],
      ["Display", "display"],
    ]) {
      await user.click(screen.getByRole("button", { name: `Record ${label}` }));
      expect(commands.startRecording).toHaveBeenLastCalledWith(mode);
      await user.click(
        screen.getByRole("button", { name: `Capture ${label}` }),
      );
      expect(commands.takeScreenshot).toHaveBeenLastCalledWith(mode);
    }
    expect(commands.startRecording).toHaveBeenCalledTimes(3);
    expect(commands.takeScreenshot).toHaveBeenCalledTimes(3);
  });

  it("lists screenshots to edit or show in Finder", async () => {
    const path = "/Users/ada/Pictures/Recast/Shot.png";
    commands.listScreenshots.mockReturnValue(
      ok([
        { path, name: "Shot", modifiedAtUnixMs: 1, width: 800, height: 600 },
      ]),
    );
    const user = userEvent.setup();
    render(<Library />);
    expect(await screen.findByText("800×600")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(commands.editScreenshot).toHaveBeenCalledWith(path);
    await user.click(
      screen.getByRole("button", { name: "Show Shot in Finder" }),
    );
    expect(commands.revealInFinder).toHaveBeenCalledWith(path);
  });

  it("says when there are no screenshots", async () => {
    render(<Library />);
    expect(await screen.findByText("No screenshots yet.")).toBeTruthy();
  });

  it("shows a failed start", async () => {
    commands.startRecording.mockReturnValue(
      Promise.resolve({ status: "error", error: "A recording is running." }),
    );
    const user = userEvent.setup();
    render(<Library />);
    await user.click(screen.getByRole("button", { name: "Record Window" }));
    expect(await screen.findByText("A recording is running.")).toBeTruthy();
  });
});
