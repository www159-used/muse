.DEFAULT_GOAL := help

CARGO ?= cargo
CARGO_RELEASE_VERSION := 1.1.2
LEVEL ?= patch
E2E_FILTER ?=
LIBRARY ?= ./library.muse
TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p')

.PHONY: help fmt fmt-check interface-check lint test e2e e2e-list build release ci run clean reset-library package release-tools release-dry-run release-tag

help:
	@printf '%s\n' \
	  'make ci                         Check formatting, Clippy, tests and release build' \
	  'make fmt                        Format Rust sources' \
	  'make e2e                        Run import stories with Cucumber step definitions' \
	  'make e2e E2E_FILTER=twice        Filter import stories by scenario name' \
	  'make e2e-list                   List all import scenarios' \
	  'make build / make release       Build debug / release binary' \
	  'make run                        Run muse' \
	  'make reset-library              Delete LIBRARY and its SQLite journals; exit muse first' \
	  'make package TAG=v0.1.0         Package a release binary for TARGET' \
	  'make release-tools              Install the pinned cargo-release version' \
	  'make release-dry-run LEVEL=patch Preview a version release' \
	  'make release-tag LEVEL=patch    Commit a version bump and create a local tag'

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

interface-check:
	$(CARGO) check -p muse-playback --locked --no-default-features

lint:
	$(CARGO) clippy --workspace --locked --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --workspace --locked --all-targets --all-features
	$(CARGO) test --workspace --locked --doc --all-features

e2e:
	$(CARGO) test -p muse-e2e --locked --test import $(if $(strip $(E2E_FILTER)),-- --name "$(E2E_FILTER)")

e2e-list:
	$(CARGO) test -p muse-e2e --locked --test import -- --list

build:
	$(CARGO) build --workspace --locked

release:
	$(CARGO) build --workspace --locked --release

ci: fmt-check interface-check lint test release

run:
	$(CARGO) run -p muse --locked

clean:
	$(CARGO) clean

reset-library:
	rm -f -- "$(LIBRARY)" "$(LIBRARY)-wal" "$(LIBRARY)-shm" "$(LIBRARY)-journal"

package:
	$(CARGO) build --locked --release --target "$(TARGET)"
	python3 scripts/package-release.py --target "$(TARGET)" --tag "$(TAG)"

release-tools:
	$(CARGO) install cargo-release --version "$(CARGO_RELEASE_VERSION)" --locked

release-dry-run:
	$(CARGO) release "$(LEVEL)" -p muse

release-tag:
	$(CARGO) release "$(LEVEL)" -p muse --execute
