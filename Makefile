.PHONY: all build release debug test run clean docs docs-serve help

BINARY := shlack
RELEASE_BIN := target/release/$(BINARY)
DEBUG_BIN := target/debug/$(BINARY)

all: release

build: release

release:
	cargo build --release

debug:
	cargo build

test:
	cargo test

run: release
	./$(RELEASE_BIN)

run-debug: debug
	./$(DEBUG_BIN)

clean:
	cargo clean

docs:
	bash scripts/docs.sh
	mkdocs build

docs-serve:
	bash scripts/docs.sh
	mkdocs serve

help:
	@echo "shlack — common targets"
	@echo ""
	@echo "  make            build release binary (default)"
	@echo "  make release    cargo build --release"
	@echo "  make debug      cargo build (dev profile)"
	@echo "  make test       run unit tests"
	@echo "  make run        build release and run ./$(RELEASE_BIN)"
	@echo "  make run-debug  build debug and run ./$(DEBUG_BIN)"
	@echo "  make clean      cargo clean"
	@echo "  make docs       stage docs and mkdocs build"
	@echo "  make docs-serve stage docs and mkdocs serve"
