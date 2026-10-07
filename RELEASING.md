# Releasing Recast

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed. The
workflow builds an ad-hoc signed `Recast.app`, a DMG and a signed updater archive, then
publishes them with `latest.json` as a GitHub release of this repo. The app checks the
latest release's `latest.json` for updates, so the repo must be public.

## One-time setup

1. Generate the updater key pair on your Mac:

   ```sh
   make updater-key
   ```

   It writes the private key to `~/.tauri/recast.key` (choose a password when asked)
   and prints the public key. Back the private key up; without it, installed copies
   can't be updated. Never commit it.
2. In this repo's Settings → Secrets and variables → Actions, add:

   | Kind     | Name                                 | Value                                   |
   | -------- | ------------------------------------ | --------------------------------------- |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY`          | the contents of `~/.tauri/recast.key`   |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password (leave out if it has none) |
   | Variable | `UPDATER_PUBKEY`                     | the public key printed in step 1        |
   | Variable | `MACOS_RUNNER` (optional)            | runner label; defaults to `macos-15`    |

The updater public key in `apps/desktop/src-tauri/tauri.conf.json` is a placeholder
(`REPLACE_WITH_UPDATER_PUBLIC_KEY`). Release builds replace it from `UPDATER_PUBKEY`,
and builds that keep it don't check for updates. To check for updates in local builds,
put the public key in `plugins.updater` there instead.

## Making a release

1. Set the new version in `apps/desktop/src-tauri/tauri.conf.json` and commit it.
2. Tag the commit and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

The workflow fails early when the tag doesn't match the version or a secret or
variable is missing.

## Installing an unnotarized build

Releases are signed ad hoc and not notarized, so macOS blocks the first launch. Open the
DMG, drag Recast to Applications and open it, then go to System Settings → Privacy &
Security and click **Open Anyway** next to Recast. Updates installed from inside Recast
don't need this.

An ad-hoc signature changes with every build, so macOS may ask for Screen Recording and
Input Monitoring again after an update.
