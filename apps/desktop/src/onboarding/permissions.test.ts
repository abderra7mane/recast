import { describe, expect, it } from "vitest";

import { PERMISSIONS, actionFor, canFinish } from "@/onboarding/permissions";

describe("onboarding permissions", () => {
  it("ranks Screen Recording required, Input Monitoring recommended", () => {
    expect(PERMISSIONS.map((p) => [p.id, p.need])).toEqual([
      ["screenRecording", "required"],
      ["inputMonitoring", "recommended"],
      ["microphone", "optional"],
    ]);
    expect(
      PERMISSIONS.find((p) => p.id === "inputMonitoring")?.without,
    ).toMatch(/auto-zoom/);
  });

  it("asks for the microphone once, then sends people to System Settings", () => {
    expect(actionFor("microphone", "notDetermined")).toBe("request");
    expect(actionFor("microphone", "denied")).toBe("openSettings");
    expect(actionFor("screenRecording", "denied")).toBe("openSettings");
    expect(actionFor("inputMonitoring", "denied")).toBe("openSettings");
    expect(actionFor("screenRecording", "granted")).toBe("none");
    expect(actionFor("microphone", undefined)).toBe("none");
  });

  it("finishes once Screen Recording is granted", () => {
    expect(canFinish(null)).toBe(false);
    expect(
      canFinish({
        screenRecording: "denied",
        inputMonitoring: "granted",
        microphone: "granted",
      }),
    ).toBe(false);
    expect(
      canFinish({
        screenRecording: "granted",
        inputMonitoring: "denied",
        microphone: "notDetermined",
      }),
    ).toBe(true);
  });
});
