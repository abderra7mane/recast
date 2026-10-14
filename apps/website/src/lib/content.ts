export const FEATURES = [
  {
    id: "record",
    title: "Record",
    body: "Record an area, a window or a whole display, with system audio and the microphone. A countdown, a control bar and the menu bar icon stop, restart or cancel the recording.",
  },
  {
    id: "edit",
    title: "Edit",
    body: "Trim, auto zoom on clicks (adjust or add zooms on the timeline), cursor size and smoothing, click effects and sound packs, audio levels, and a background with padding, rounded corners and a shadow.",
  },
  {
    id: "export",
    title: "Export",
    body: "Export to MP4 (H.264 or HEVC) at the recording's own size or at 1080p, 1440p or 4K.",
  },
  {
    id: "capture",
    title: "Capture",
    body: "Capture an area, a window or a display. The screenshot is copied and saved, and a thumbnail opens it in the markup editor: arrows, shapes, lines, text, highlight, blur and pixelate, crop, and Beautify.",
  },
  {
    id: "library",
    title: "Library",
    body: "The Library and the menu's Recent Recordings and Recent Screenshots keep your work close. Recordings that stopped in a crash can be recovered.",
  },
  {
    id: "shortcuts",
    title: "Shortcuts",
    body: "Global shortcuts for every record and capture mode, set in Settings.",
  },
] as const;

export const SHORTCUTS = [
  { action: "Record Area", keys: ["⌥", "⇧", "⌘", "R"] },
  { action: "Capture Area", keys: ["⌥", "⇧", "⌘", "S"] },
];

export const PERMISSIONS = [
  { name: "Screen Recording", why: "to capture the screen" },
  { name: "Input Monitoring", why: "to record clicks and the cursor" },
  { name: "Microphone", why: "only when you record it" },
];

export const INSTALL_STEPS = [
  "Open the download and drag Recast to Applications.",
  "Open Recast. Builds are not notarized, so macOS blocks the first launch.",
  "In System Settings → Privacy & Security, click Open Anyway next to Recast.",
  "That's it. Recast updates itself from then on.",
];

export const REQUIREMENTS = "macOS 14 or later";
