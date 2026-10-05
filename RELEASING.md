# Releasing Recast

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed. The
workflow builds an ad-hoc signed `Recast.app`, a DMG and a signed updater archive, then
publishes them with `latest.json` to a public releases repo. The app checks that repo's
`latest.json` for updates.

## One-time setup

1. Create a public repo for the releases, for example `<owner>/recast-releases`. It can
   be empty.
2. Generate the updater key pair on your Mac:

   ```sh
   make updater-key
   ```

   It writes the private key to `~/.tauri/recast.key` (choose a password when asked)
   and prints the public key. Back the private key up; without it, installed copies
   can't be updated. Never commit it.
3. Create a fine-grained token that can write the releases repo: Contents read and
   write, on that repo only.
4. In this repo's Settings → Secrets and variables → Actions, add:

   | Kind     | Name                                 | Value                                         |
   | -------- | ------------------------------------ | --------------------------------------------- |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY`          | the contents of `~/.tauri/recast.key`         |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password (leave out if it has none)       |
   | Secret   | `RELEASES_TOKEN`                     | the token from step 3                         |
   | Variable | `RELEASES_OWNER`                     | the releases repo owner                       |
   | Variable | `RELEASES_REPO`                      | the releases repo name, e.g. `recast-releases` |
   | Variable | `UPDATER_PUBKEY`                     | the public key printed in step 2              |
   | Variable | `MACOS_RUNNER` (optional)            | runner label; defaults to `macos-15`          |

The updater endpoint and public key in `apps/desktop/src-tauri/tauri.conf.json` are
placeholders (`<owner>`, `REPLACE_WITH_UPDATER_PUBLIC_KEY`). Release builds replace them
from these variables, and builds that keep them don't check for updates. To point local
builds at a real endpoint, put the values in `plugins.updater` there instead.

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
