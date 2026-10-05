// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { AppSettings, ShortcutStatus } from "@/bindings";

const SETTINGS: AppSettings = {
  screenshots: { copyToClipboard: true, saveToDisk: true, folder: null },
  beautify: {},
  recording: {
    countdown: true,
    mic: false,
    systemAudio: true,
    folder: "/Volumes/Work/Takes",
  },
  shortcuts: {
    record: "Alt+Shift+Cmd+KeyR",
    captureArea: "Alt+Shift+Cmd+KeyS",
    captureWindow: null,
  },
  updates: { checkAutomatically: true },
  onboardingCompleted: true,
};

const STATUSES: ShortcutStatus[] = [
  {
    action: "record",
    shortcut: "Alt+Shift+Cmd+KeyR",
    symbols: "⌥⇧⌘R",
    error: null,
  },
  {
    action: "captureArea",
    shortcut: "Alt+Shift+Cmd+KeyS",
    symbols: "⌥⇧⌘S",
    error: "macOS didn't accept ⌥⇧⌘S",
  },
  { action: "captureWindow", shortcut: null, symbols: null, error: null },
];

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const commands = vi.hoisted(() => ({
  getSettings: vi.fn(),
  shortcutStatuses: vi.fn(),
  getLaunchAtLogin: vi.fn(),
  appInfo: vi.fn(),
  setRecordingSettings: vi.fn(),
  setScreenshotSettings: vi.fn(),
  setUpdateSettings: vi.fn(),
  setShortcut: vi.fn(),
  suspendShortcuts: vi.fn(),
  setLaunchAtLogin: vi.fn(),
  openLoginItemsSettings: vi.fn(),
  openWindow: vi.fn(),
  checkForUpdates: vi.fn(),
  openLogsFolder: vi.fn(),
  copyDiagnostics: vi.fn(),
}));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock("@/bindings", () => ({ commands }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const { Settings } = await import("@/settings/Settings");

beforeEach(() => {
  vi.clearAllMocks();
  commands.getSettings.mockReturnValue(ok(SETTINGS));
  commands.shortcutStatuses.mockResolvedValue(STATUSES);
  commands.getLaunchAtLogin.mockResolvedValue("disabled");
  commands.appInfo.mockResolvedValue({
    version: "0.2.0",
    logsDir: "/Users/ada/Library/Logs/Recast",
  });
  commands.suspendShortcuts.mockResolvedValue(STATUSES);
  for (const save of [
    commands.setRecordingSettings,
    commands.setScreenshotSettings,
    commands.setUpdateSettings,
  ]) {
    save.mockImplementation(() => ok(SETTINGS));
  }
});

async function openTab(name: string) {
  const user = userEvent.setup();
  render(<Settings />);
  await user.click(await screen.findByRole("tab", { name }));
  return user;
}

describe("Settings", () => {
  it("turns the countdown off and keeps the other recording options", async () => {
    const user = await openTab("Recording");
    const countdown = screen.getByRole("switch", { name: "Countdown" });
    expect(countdown.getAttribute("aria-checked")).toBe("true");
    await user.click(countdown);
    expect(commands.setRecordingSettings).toHaveBeenCalledWith({
      countdown: false,
      mic: false,
      systemAudio: true,
      folder: "/Volumes/Work/Takes",
    });
  });

  it("chooses a recordings folder and goes back to the default", async () => {
    const user = await openTab("Recording");
    expect(screen.getByText("/Volumes/Work/Takes")).toBeTruthy();
    dialog.open.mockResolvedValue("/Users/ada/Desktop");
    await user.click(screen.getByRole("button", { name: "Change…" }));
    expect(commands.setRecordingSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ folder: "/Users/ada/Desktop" }),
    );
    await user.click(screen.getByRole("button", { name: "Use Default" }));
    expect(commands.setRecordingSettings).toHaveBeenLastCalledWith(
      expect.objectContaining({ folder: null }),
    );
  });

  it("keeps the screenshot toggles and shows the default folder", async () => {
    const user = await openTab("Screenshots");
    expect(screen.getByText("~/Pictures/Recast")).toBeTruthy();
    await user.click(
      screen.getByRole("switch", { name: "Copy to the clipboard" }),
    );
    expect(commands.setScreenshotSettings).toHaveBeenCalledWith({
      copyToClipboard: false,
      saveToDisk: true,
      folder: null,
    });
  });

  it("shows shortcut failures and saves a new shortcut", async () => {
    const user = await openTab("Shortcuts");
    expect(screen.getByRole("alert").textContent).toBe(
      "macOS didn't accept ⌥⇧⌘S",
    );
    commands.setShortcut.mockReturnValue(
      ok(
        STATUSES.map((s) =>
          s.action === "captureWindow"
            ? { ...s, shortcut: "Ctrl+Alt+KeyW", symbols: "⌃⌥W" }
            : s,
        ),
      ),
    );
    const recorder = screen.getByRole("button", {
      name: "Capture window shortcut",
    });
    expect(recorder.textContent).toBe("Record Shortcut");
    await user.click(recorder);
    expect(commands.suspendShortcuts).toHaveBeenLastCalledWith(true);
    await user.keyboard("{Control>}{Alt>}[KeyW]{/Alt}{/Control}");
    expect(commands.setShortcut).toHaveBeenCalledWith(
      "captureWindow",
      "Ctrl+Alt+KeyW",
    );
    expect(commands.suspendShortcuts).toHaveBeenLastCalledWith(false);
    expect(
      (await screen.findByRole("button", { name: "Capture window shortcut" }))
        .textContent,
    ).toBe("⌃⌥W");
  });

  it("reports a conflicting shortcut without changing it", async () => {
    const user = await openTab("Shortcuts");
    commands.setShortcut.mockReturnValue(
      Promise.resolve({
        status: "error",
        error: "⌥⇧⌘S is already used for Capture area.",
      }),
    );
    await user.click(
      screen.getByRole("button", { name: "Start or stop recording shortcut" }),
    );
    await user.keyboard("{Alt>}{Shift>}{Meta>}[KeyS]{/Meta}{/Shift}{/Alt}");
    expect(
      (await screen.findByText("⌥⇧⌘S is already used for Capture area."))
        .textContent,
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Start or stop recording shortcut" })
        .textContent,
    ).toBe("⌥⇧⌘R");
  });

  it("turns on launch at login and points to Login Items when approval is needed", async () => {
    commands.setLaunchAtLogin.mockReturnValue(ok("needsApproval"));
    const user = await openTab("General");
    await user.click(screen.getByRole("switch", { name: "Launch at login" }));
    expect(commands.setLaunchAtLogin).toHaveBeenCalledWith(true);
    await user.click(
      await screen.findByRole("button", { name: "Open Login Items" }),
    );
    expect(commands.openLoginItemsSettings).toHaveBeenCalled();
  });

  it("turns off automatic update checks and checks on demand", async () => {
    const user = await openTab("Updates");
    await user.click(
      screen.getByRole("switch", { name: "Check for updates automatically" }),
    );
    expect(commands.setUpdateSettings).toHaveBeenCalledWith({
      checkAutomatically: false,
    });
    await user.click(screen.getByRole("button", { name: "Check Now" }));
    expect(commands.checkForUpdates).toHaveBeenCalled();
  });

  it("copies diagnostics from About", async () => {
    commands.copyDiagnostics.mockReturnValue(ok(null));
    const user = await openTab("About");
    expect(screen.getByText("Version 0.2.0")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Copy Diagnostics" }));
    expect(commands.copyDiagnostics).toHaveBeenCalled();
    expect(await screen.findByRole("button", { name: "Copied" })).toBeTruthy();
  });
});
