DESKTOP := apps/desktop
PNPM := pnpm --dir $(DESKTOP)
# The local Apple Development certificate keeps macOS permissions across rebuilds.
SIGNING_IDENTITY ?= Apple Development

.PHONY: help install dev build bindings test test-rust test-ui lint lint-rust lint-ui lint-workflows fmt record simulate export bench-preview update-goldens sounds unfinished recover discard permissions updater-key release-check release-bundle release-assets clean

help:
	@echo "install   install JS dependencies"
	@echo "dev       run the desktop app in development mode"
	@echo "build     build the desktop app bundle (Recast.app)"
	@echo "bindings  regenerate the TypeScript IPC bindings"
	@echo "test      run Rust and UI tests"
	@echo "lint      clippy (-D warnings), rustfmt check, eslint, prettier check, tsc"
	@echo "fmt       format Rust and UI sources"
	@echo "record    record from the CLI: make record ARGS='--seconds 5 --display'"
	@echo "simulate  record generated video/audio/input (no permissions needed): make simulate ARGS='--seconds 5'"
	@echo "export    export a bundle to MP4: make export BUNDLE=path ARGS='--set export.fps=30 --out out.mp4'"
	@echo "bench-preview measure editor preview seek latency and frame rate: make bench-preview BUNDLE=path [PROFILE=dev]"
	@echo "update-goldens  re-render the golden images of the compositor and markup tests"
	@echo "sounds    regenerate the bundled click sounds"
	@echo "unfinished list bundles left by a crash"
	@echo "recover   recover a bundle from the CLI: make recover BUNDLE=path"
	@echo "discard   move a crashed bundle to the Trash: make discard BUNDLE=path"
	@echo "permissions show Screen Recording / Input Monitoring / Microphone status"
	@echo "updater-key generate the updater signing key pair in ~/.tauri/recast.key and print its public key"
	@echo "lint-workflows check the GitHub Actions workflows with actionlint"
	@echo "release-check   CI: check that TAG matches the app version"
	@echo "release-bundle  CI: build the ad-hoc signed app, DMG and signed updater archive"
	@echo "release-assets  CI: collect the release files and latest.json: make release-assets REPO=owner/repo"

install:
	pnpm install --frozen-lockfile

dev: install
	$(PNPM) tauri dev

build: install bindings
	APPLE_SIGNING_IDENTITY="$(SIGNING_IDENTITY)" $(PNPM) tauri build --bundles app

bindings:
	cargo run --quiet -p recast-desktop --example export-bindings

# Runs every suite, even after a failing one, and fails if any failed.
test:
	@status=0; $(MAKE) test-rust || status=1; $(MAKE) test-ui || status=1; exit $$status

test-rust:
	cargo test --workspace --all-targets --all-features --no-fail-fast

test-ui: install
	$(PNPM) test

lint: lint-rust lint-ui

lint-rust:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --all-features -- -D warnings

lint-ui: install
	$(PNPM) lint
	$(PNPM) typecheck
	$(PNPM) format:check

fmt: install
	cargo fmt --all
	$(PNPM) format

CLI := cargo run --quiet -p recast-desktop --features synthetic --example recast-cli --

record:
	$(CLI) record $(ARGS)

simulate:
	$(CLI) simulate $(ARGS)

export:
	$(CLI) export "$(BUNDLE)" $(ARGS)

PROFILE ?= release

bench-preview:
	cargo run --quiet --profile $(PROFILE) -p recast-desktop --example preview-bench -- "$(BUNDLE)"

update-goldens: install
	UPDATE_GOLDENS=1 cargo test -p recast-render --test golden
	UPDATE_GOLDENS=1 $(PNPM) test src/markup/render.test.ts

sounds:
	cargo run --quiet -p recast-export --example generate-sounds

unfinished:
	$(CLI) unfinished $(ARGS)

recover:
	$(CLI) recover "$(BUNDLE)"

discard:
	$(CLI) discard "$(BUNDLE)"

permissions:
	$(CLI) permissions

UPDATER_KEY ?= $(HOME)/.tauri/recast.key

updater-key:
	@if [ -e "$(UPDATER_KEY)" ]; then echo "$(UPDATER_KEY) already exists, keeping it."; exit 1; fi
	$(PNPM) tauri signer generate --write-keys "$(UPDATER_KEY)"
	@echo
	@echo "Public key (UPDATER_PUBKEY):"
	@cat "$(UPDATER_KEY).pub"
	@echo

lint-workflows:
	@if command -v actionlint >/dev/null; then actionlint; else echo "actionlint is not installed, skipping (brew install actionlint)."; fi

RELEASE_CONFIG := $(CURDIR)/target/release.conf.json

release-check:
	node scripts/release.mjs check-version "$(TAG)"

# Needs UPDATER_ENDPOINT, UPDATER_PUBKEY and TAURI_SIGNING_PRIVATE_KEY (and its
# password, if any) in the environment. The app is signed ad hoc.
release-bundle: install bindings
	@mkdir -p target
	node scripts/release.mjs config > "$(RELEASE_CONFIG)"
	APPLE_SIGNING_IDENTITY=- $(PNPM) tauri build --bundles app,dmg --config "$(RELEASE_CONFIG)"

release-assets:
	node scripts/release.mjs assets "$(REPO)" target/release-assets

clean:
	cargo clean
	rm -rf $(DESKTOP)/dist
