// @vitest-environment jsdom
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Permissions } from "@/bindings";

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const commands = vi.hoisted(() => ({
  checkPermissions: vi.fn(),
  requestPermission: vi.fn(),
  resetPermission: vi.fn(),
  openPrivacySettings: vi.fn(),
  relaunch: vi.fn(),
  completeOnboarding: vi.fn(),
}));

vi.mock("@/bindings", () => ({ commands }));

const { Onboarding } = await import("@/onboarding/Onboarding");

const DENIED: Permissions = {
  screenRecording: "denied",
  inputMonitoring: "granted",
  microphone: "notDetermined",
};

beforeEach(() => {
  vi.clearAllMocks();
  commands.checkPermissions.mockResolvedValue(DENIED);
  commands.requestPermission.mockResolvedValue("denied");
  commands.resetPermission.mockReturnValue(ok(null));
  commands.openPrivacySettings.mockReturnValue(ok(null));
});

describe("Onboarding", () => {
  it("offers a reset only for permissions that read as denied", async () => {
    render(<Onboarding />);
    expect(
      await screen.findByRole("button", { name: "Reset Screen Recording" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Reset Input Monitoring" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Reset Microphone" }),
    ).toBeNull();
  });

  it("resets a stale permission, asks again and opens System Settings", async () => {
    const user = userEvent.setup();
    render(<Onboarding />);
    await user.click(
      await screen.findByRole("button", { name: "Reset Screen Recording" }),
    );
    await waitFor(() =>
      expect(commands.openPrivacySettings).toHaveBeenCalledWith(
        "screenRecording",
      ),
    );
    expect(commands.resetPermission).toHaveBeenCalledWith("screenRecording");
    expect(commands.requestPermission).toHaveBeenCalledWith("screenRecording");
    const order = (fn: { mock: { invocationCallOrder: number[] } }) =>
      fn.mock.invocationCallOrder[0];
    expect(order(commands.resetPermission)).toBeLessThan(
      order(commands.requestPermission),
    );
    expect(order(commands.requestPermission)).toBeLessThan(
      order(commands.openPrivacySettings),
    );
  });

  it("explains the manual fix when the reset fails", async () => {
    commands.resetPermission.mockReturnValue(
      Promise.resolve({ status: "error" as const, error: "tccutil failed" }),
    );
    const user = userEvent.setup();
    render(<Onboarding />);
    await user.click(
      await screen.findByRole("button", { name: "Reset Screen Recording" }),
    );
    expect(await screen.findByText(/remove it with the − button/)).toBeTruthy();
    expect(commands.requestPermission).not.toHaveBeenCalled();
    expect(commands.openPrivacySettings).not.toHaveBeenCalled();
  });
});
