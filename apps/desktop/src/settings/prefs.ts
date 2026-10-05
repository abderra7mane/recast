import type {
  AppSettings,
  RecordingSettings,
  ScreenshotSettings,
  ShortcutSettings,
  UpdateSettings,
} from "@/bindings";

/**
 * Rust always sends every field; the generated types mark them optional because of
 * serde defaults. Folders and shortcuts stay nullable: `null` means the default
 * folder or a turned-off shortcut.
 */
export type Prefs = {
  screenshots: Required<ScreenshotSettings>;
  recording: Required<RecordingSettings>;
  shortcuts: Required<ShortcutSettings>;
  updates: Required<UpdateSettings>;
  onboardingCompleted: boolean;
};

export const prefs = (settings: AppSettings) => settings as Prefs;

export const DEFAULT_RECORDINGS_FOLDER = "~/Movies/Recast";
export const DEFAULT_SCREENSHOTS_FOLDER = "~/Pictures/Recast";
