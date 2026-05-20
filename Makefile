SHELL := /bin/sh

CARGO ?= cargo
NIDA ?= nida
PREFIX ?= $(HOME)/.local
JOBS ?= 4
SAMPLE_INPUTS ?= README.md Cargo.toml
SAMPLE_OUTPUT ?= /tmp/radd-sample.root
SAMPLE_SCRATCH ?= /tmp/radd-sample-scratch

.DEFAULT_GOAL := help

.PHONY: help
help:
	@printf '%s\n' 'radd make targets'
	@printf '%s\n' ''
	@printf '%s\n' 'Core:'
	@printf '%s\n' '  make check        Run fmt-check, clippy, test, and docs-check'
	@printf '%s\n' '  make fmt          Format Rust code'
	@printf '%s\n' '  make fmt-check    Check Rust formatting'
	@printf '%s\n' '  make clippy       Run clippy with warnings as errors'
	@printf '%s\n' '  make test         Run the full test suite'
	@printf '%s\n' '  make build        Build debug binary'
	@printf '%s\n' '  make release      Build release binary'
	@printf '%s\n' ''
	@printf '%s\n' 'Docs:'
	@printf '%s\n' '  make docs         Build docs with Nida'
	@printf '%s\n' '  make docs-check   Build docs with Nida'
	@printf '%s\n' '  make docs-serve   Serve docs locally with Nida'
	@printf '%s\n' '  make docs-clean   Remove generated docs/public'
	@printf '%s\n' ''
	@printf '%s\n' 'Run and smoke:'
	@printf '%s\n' '  make run ARGS="--help"'
	@printf '%s\n' '  make doctor       Run radd doctor from cargo'
	@printf '%s\n' '  make sample-plan  Run a sample plan using README.md and Cargo.toml'
	@printf '%s\n' '  make clean        Remove cargo build artifacts and docs output'
	@printf '%s\n' '  make install      Install release binary to PREFIX/bin'

.PHONY: fmt
fmt:
	$(CARGO) fmt

.PHONY: fmt-check
fmt-check:
	$(CARGO) fmt --check

.PHONY: clippy
clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

.PHONY: test
test:
	$(CARGO) test

.PHONY: build
build:
	$(CARGO) build

.PHONY: release
release:
	$(CARGO) build --release

.PHONY: check
check: fmt-check clippy test docs-check

.PHONY: docs
docs:
	$(NIDA) build --site ./docs

.PHONY: docs-check
docs-check: docs

.PHONY: docs-serve
docs-serve:
	$(NIDA) serve --site ./docs

.PHONY: docs-clean
docs-clean:
	rm -rf docs/public

.PHONY: run
run:
	$(CARGO) run -- $(ARGS)

.PHONY: doctor
doctor:
	$(CARGO) run -- doctor

.PHONY: sample-plan
sample-plan:
	$(CARGO) run -- plan $(SAMPLE_OUTPUT) $(SAMPLE_INPUTS) --jobs $(JOBS) --scratch $(SAMPLE_SCRATCH) --commands

.PHONY: clean
clean: docs-clean
	$(CARGO) clean

.PHONY: install
install: release
	install -d "$(PREFIX)/bin"
	install -m 0755 target/release/radd "$(PREFIX)/bin/radd"
