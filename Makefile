# NextPlane build. `make build` gives one binary in bin/nextplane with the web app inside.
COMMIT     ?= $(shell c=$$(git rev-parse --short HEAD 2>/dev/null) && { git diff --quiet HEAD -- 2>/dev/null || c="$$c-dirty"; echo "$$c"; })
BUILD_DATE ?= $(shell date -u +%Y-%m-%dT%H:%M:%SZ)
DATA       ?= $(CURDIR)/.devdata
LISTEN     ?= 127.0.0.1:12313
WEB_PORT   ?= 15173

export NEXTPLANE_COMMIT := $(COMMIT)
export NEXTPLANE_BUILD_DATE := $(BUILD_DATE)

.PHONY: all web build test test-rust test-web lint fmt dev clean

all: build

web:
	cd web && npm ci && npm run build

build: web
	cargo build --release --locked -p nextplane
	mkdir -p bin && cp target/release/nextplane bin/nextplane
	@echo "built bin/nextplane"

test: test-rust test-web

test-rust:
	cargo test --workspace --locked

# Needs web/node_modules: `cd web && npm ci` once.
test-web:
	cd web && npm run typecheck && npm test

# Needs rustfmt and clippy (rustup component add rustfmt clippy).
lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked -- -D warnings

fmt:
	cargo fmt --all

# Server on $(LISTEN) with a separate data folder, plus Vite on :$(WEB_PORT) with hot reload.
# Open http://localhost:$(WEB_PORT). Ctrl-C stops both.
dev:
	@DATA="$(DATA)" LISTEN="$(LISTEN)" WEB_PORT="$(WEB_PORT)" ./scripts/dev.sh

# Removes only build output. The dev data folder (.devdata) is never touched.
clean:
	cargo clean
	rm -f bin/nextplane
