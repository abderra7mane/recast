DESKTOP := apps/desktop
PNPM := pnpm --dir $(DESKTOP)
# The local Apple Development certificate keeps macOS permissions across rebuilds.
SIGNING_IDENTITY ?= Apple Development

.PHONY: help install dev build bindings test test-rust test-ui lint lint-rust lint-ui fmt record simulate export bench-preview update-goldens sounds unfinished recover discard permissions clean

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
	@echo "update-goldens  re-render the golden frames of the compositor tests"
	@echo "sounds    regenerate the bundled click sounds"
	@echo "unfinished list bundles left by a crash"
	@echo "recover   recover a bundle from the CLI: make recover BUNDLE=path"
	@echo "discard   move a crashed bundle to the Trash: make discard BUNDLE=path"
	@echo "permissions show Screen Recording / Input Monitoring / Microphone status"

install:
	pnpm install --frozen-lockfile

dev: install
	$(PNPM) tauri dev

build: install bindings
	APPLE_SIGNING_IDENTITY="$(SIGNING_IDENTITY)" $(PNPM) tauri build --bundles app

bindings:
	cargo run --quiet -p recast-desktop --example export-bindings

test: test-rust test-ui

test-rust:
	cargo test --workspace --all-targets --all-features

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

update-goldens:
	UPDATE_GOLDENS=1 cargo test -p recast-render --test golden

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

clean:
	cargo clean
	rm -rf $(DESKTOP)/dist
