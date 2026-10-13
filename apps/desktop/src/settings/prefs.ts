import type {
  AppearanceSettings,
  AppSettings,
  RecordingSettings,
  ScreenshotSettings,
  ShortcutSettings,
  UpdateSettings,
} from "@/bindings";
import { DEFAULT_APPEARANCE } from "@/theme/theme";

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
  appearance: Required<AppearanceSettings>;
  onboardingCompleted: boolean;
};

export const prefs = (settings: AppSettings) =>
  ({
    ...settings,
    appearance: { ...DEFAULT_APPEARANCE, ...settings.appearance },
  }) as Prefs;

export const DEFAULT_RECORDINGS_FOLDER = "~/Movies/Recast";
export const DEFAULT_SCREENSHOTS_FOLDER = "~/Pictures/Recast";
