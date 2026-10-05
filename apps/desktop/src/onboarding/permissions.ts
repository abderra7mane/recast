import type { Permission, Permissions, PermissionState } from "@/bindings";

export type Need = "required" | "recommended" | "optional";

export type PermissionInfo = {
  id: Permission;
  title: string;
  need: Need;
  why: string;
  /** What stops working without it. */
  without: string;
};

export const PERMISSIONS: PermissionInfo[] = [
  {
    id: "screenRecording",
    title: "Screen Recording",
    need: "required",
    why: "Records and captures your screen.",
    without: "Recast can't record or take screenshots.",
  },
  {
    id: "inputMonitoring",
    title: "Input Monitoring",
    need: "recommended",
    why: "Sees your clicks to zoom in on them and draw click effects.",
    without:
      "Recordings work, but without auto-zoom, click effects or click sounds.",
  },
  {
    id: "microphone",
    title: "Microphone",
    need: "optional",
    why: "Records your voice when you turn the microphone on.",
    without: "Recordings have no microphone audio.",
  },
];

export type Action = "none" | "request" | "openSettings";

/**
 * The microphone can be asked for once; Screen Recording and Input Monitoring only
 * report granted or not, so they always go through System Settings.
 */
export function actionFor(
  id: Permission,
  state: PermissionState | undefined,
): Action {
  if (state === undefined || state === "granted") return "none";
  if (id === "microphone" && state === "notDetermined") return "request";
  return "openSettings";
}

export const canFinish = (permissions: Permissions | null) =>
  permissions?.screenRecording === "granted";

export const STATE_LABEL: Record<PermissionState, string> = {
  granted: "Granted",
  denied: "Not granted",
  notDetermined: "Not asked",
};
