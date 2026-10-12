#!/usr/bin/env node
// Release helpers for CI.
//
//   node scripts/release.mjs check-version <tag>
//     Fails unless <tag> is v<version> from tauri.conf.json.
//   node scripts/release.mjs config
//     Prints the Tauri config overrides of a release build: updater artifacts on,
//     the endpoint from UPDATER_ENDPOINT and the public key from UPDATER_PUBKEY.
//   node scripts/release.mjs assets <owner/repo> <out-dir>
//     Copies the DMG of each build and the signed updater archives of the Apple
//     Silicon and Intel builds to <out-dir> and writes the updater manifest
//     latest.json and the release notes notes.md next to them.

import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConfig = JSON.parse(
  readFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"),
);
const version = tauriConfig.version;
const targetDir = process.env.TARGET_DIR ?? join(root, "target");
// The universal build ships only as a DMG; its installs update to the build of
// their own architecture.
const BUILDS = [
  { name: "aarch64", target: "aarch64-apple-darwin", updater: "darwin-aarch64" },
  { name: "x86_64", target: "x86_64-apple-darwin", updater: "darwin-x86_64" },
  { name: "universal", target: "universal-apple-darwin" },
];

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

function required(name) {
  const value = process.env[name]?.trim();
  if (!value) fail(`${name} is not set`);
  return value;
}

function checkVersion(tag) {
  if (tag !== `v${version}`) {
    fail(`tag ${tag} does not match version ${version} in tauri.conf.json`);
  }
}

function config() {
  const endpoint = required("UPDATER_ENDPOINT");
  const pubkey = required("UPDATER_PUBKEY");
  if (!endpoint.startsWith("https://")) fail("UPDATER_ENDPOINT must use https");
  if (endpoint.includes("<") || pubkey.startsWith("REPLACE_")) {
    fail("UPDATER_ENDPOINT and UPDATER_PUBKEY must not be placeholders");
  }
  return {
    bundle: { createUpdaterArtifacts: true },
    plugins: { updater: { endpoints: [endpoint], pubkey } },
  };
}

function only(dir, suffix) {
  const found = readdirSync(dir).filter((name) => name.endsWith(suffix));
  if (found.length !== 1) {
    fail(`expected one *${suffix} in ${dir}, found ${found.length}`);
  }
  return join(dir, found[0]);
}

function notes() {
  return `Recast ${version} for Macs running macOS 14 or later. Download the DMG for your Mac:

- \`Recast_${version}_aarch64.dmg\`: Apple Silicon (M1 and later).
- \`Recast_${version}_x86_64.dmg\`: Intel.
- \`Recast_${version}_universal.dmg\`: both, at about twice the size.

Recast is not notarized. The first time you open it, macOS blocks it:

1. Open the DMG and drag Recast to Applications.
2. Open Recast. When macOS says it can't check it for malicious software, click Done.
3. Open System Settings → Privacy & Security, scroll to Security and click Open Anyway next to Recast.

Updates from inside Recast install without these steps.
`;
}

function assets(repo, outDir) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo ?? "")) fail("pass the releases repo as owner/repo");
  mkdirSync(outDir, { recursive: true });
  const platforms = {};
  for (const { name, target, updater } of BUILDS) {
    const bundleDir = join(targetDir, target, "release/bundle");
    copyFileSync(only(join(bundleDir, "dmg"), ".dmg"), join(outDir, `Recast_${version}_${name}.dmg`));
    if (!updater) continue;

    const archiveName = `Recast_${version}_${name}.app.tar.gz`;
    const archive = only(join(bundleDir, "macos"), ".app.tar.gz");
    const signature = readFileSync(`${archive}.sig`, "utf8").trim();
    copyFileSync(archive, join(outDir, archiveName));
    copyFileSync(`${archive}.sig`, join(outDir, `${archiveName}.sig`));

    const url = `https://github.com/${repo}/releases/download/v${version}/${archiveName}`;
    platforms[updater] = { signature, url };
    platforms[`${updater}-app`] = { signature, url };
  }

  const manifest = {
    version,
    notes: `Recast ${version}`,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms,
  };
  writeFileSync(join(outDir, "latest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  writeFileSync(join(outDir, "notes.md"), notes());
}

const [command, ...args] = process.argv.slice(2);
switch (command) {
  case "check-version":
    checkVersion(args[0]);
    break;
  case "config":
    process.stdout.write(`${JSON.stringify(config(), null, 2)}\n`);
    break;
  case "assets":
    assets(args[0], resolve(args[1] ?? "target/release-assets"));
    break;
  default:
    fail("usage: release.mjs check-version <tag> | config | assets <owner/repo> <out-dir>");
}
