# Releasing Recast

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed. The
workflow builds a `Recast.app` signed with a self-signed certificate, a DMG and a signed
updater archive, then publishes them with `latest.json` as a GitHub release of this repo.
The app checks the latest release's `latest.json` for updates, so the repo must be public.

## One-time setup

1. Generate the updater key pair on your Mac:

   ```sh
   make updater-key
   ```

   It writes the private key to `~/.tauri/recast.key` (choose a password when asked)
   and prints the public key. Back the private key up; without it, installed copies
   can't be updated. Never commit it.
2. Create the code signing certificate:

   ```sh
   make signing-cert
   ```

   It writes a self-signed certificate to `~/.tauri/recast-signing.p12` (choose a
   password when asked) and prints the `gh secret set` commands for step 3. macOS ties
   Screen Recording and Input Monitoring grants to this certificate, so they survive
   updates. Back it up with its password and never commit it: releases signed with
   another certificate make every user grant the permissions again.
3. In this repo's Settings → Secrets and variables → Actions, add:

   | Kind     | Name                                 | Value                                   |
   | -------- | ------------------------------------ | --------------------------------------- |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY`          | the contents of `~/.tauri/recast.key`   |
   | Secret   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password (leave out if it has none) |
   | Secret   | `MACOS_CERTIFICATE`                  | `base64 -i ~/.tauri/recast-signing.p12` |
   | Secret   | `MACOS_CERTIFICATE_PASSWORD`         | the certificate's password              |
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

Releases are self-signed and not notarized, so macOS blocks the first launch. Open the
DMG, drag Recast to Applications and open it, then go to System Settings → Privacy &
Security and click **Open Anyway** next to Recast. Updates installed from inside Recast
don't need this.

Releases up to 0.1.5 were signed ad hoc, and macOS ties permissions to the exact build.
The first update to a self-signed release asks for Screen Recording and Input Monitoring
once more. If System Settings already shows Recast as allowed, click **Reset** next to
the permission in Recast's setup window and turn it on again; after that, updates keep
the permissions.

## Local release builds

`make release-bundle` signs with the certificate from `MACOS_CERTIFICATE` and
`MACOS_CERTIFICATE_PASSWORD`, imported into a temporary keychain, or with
`RELEASE_SIGNING_IDENTITY`, an identity already in your keychain
(`make signing-cert ARGS=--import` adds it to the login keychain). Without either, it
signs ad hoc and warns; in CI it fails instead.
