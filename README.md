# Recast

A macOS menu bar app for screen recordings and screenshots. Recordings get automatic
zoom on clicks, click effects and sounds, a smooth cursor and a styled background,
all applied when you edit and export, never baked into the recording.

## Features

- **Record** an area, a window or a whole display, with system audio and the
  microphone. A countdown, a control bar and the menu bar icon stop, restart or
  cancel the recording.
- **Edit** in the editor: trim, auto zoom on clicks (adjust or add zooms on the
  timeline), cursor size and smoothing, click effects and sound packs, audio levels,
  and a background with padding, rounded corners and a shadow.
- **Export** to MP4 (H.264 or HEVC) at the recording's own size or at 1080p, 1440p
  or 4K.
- **Capture** an area, a window or a display. The screenshot is copied and saved, and
  a thumbnail opens it in the markup editor: arrows, shapes, lines, text, highlight,
  blur and pixelate, crop, and Beautify.
- **Find your work again** in the Library or the menu's Recent Recordings and Recent
  Screenshots. Recordings that stopped in a crash can be recovered.
- **Global shortcuts** for every record and capture mode, set in Settings.

Recordings are saved to `~/Movies/Recast` and screenshots to `~/Pictures/Recast`;
both folders can be changed in Settings.

## Install

Download the DMG from [Releases](https://github.com/abderra7mane/recast/releases),
drag Recast to Applications and open it. Builds are not notarized, so macOS blocks the
first launch: open System Settings → Privacy & Security and click **Open Anyway** next
to Recast. Recast updates itself from then on.

Recast asks for **Screen Recording** (to capture), **Input Monitoring** (to record
clicks and the cursor) and **Microphone** (only when you record it).

Requires macOS 14 or later.

## Build from source

You need Xcode's command line tools, [rustup](https://rustup.rs) (the toolchain is
pinned in `rust-toolchain.toml`), Node.js 24 and pnpm 10.

```sh
make install   # JS dependencies
make dev       # run in development mode
make build     # build Recast.app into target/release/bundle/macos
make test      # Rust and UI tests
make lint      # clippy, rustfmt, eslint, prettier and tsc
```

`make build` signs with your "Apple Development" certificate, which keeps macOS
permissions across rebuilds. Without one, sign ad hoc:
`make build SIGNING_IDENTITY=-`. Run `make help` for the other targets.

## Project layout

| Path                    | What it does                                                    |
| ----------------------- | --------------------------------------------------------------- |
| `apps/desktop`          | The Tauri app: React UI in `src`, Rust backend in `src-tauri`   |
| `crates/recast-capture` | Screen, window and audio capture through ScreenCaptureKit       |
| `crates/recast-input`   | Mouse, click and cursor shape recording                         |
| `crates/recast-project` | The `.recast` bundle: recording metadata, edits, crash recovery |
| `crates/recast-zoom`    | Auto zoom, camera motion, cursor smoothing and click effects    |
| `crates/recast-render`  | The wgpu compositor shared by preview and export                |
| `crates/recast-export`  | MP4 export: decode, render, mix audio and encode                |

See [RELEASING.md](RELEASING.md) for publishing a release.

## License

[MIT](LICENSE)
