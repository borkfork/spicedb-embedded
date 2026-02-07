# Contributing to spicedb-embedded

## Prerequisites

1. Install [rustup](https://rustup.rs/) for Rust.
2. Install [mise](https://mise.jdx.dev/installing-mise.html) for managing tools.
3. Install [Go](https://go.dev/dl/) 1.23+ for building the shared/c library.
4. Ensure CGO is enabled for Go (it is by default on most systems).

## Workflow

1. Run `mise run check` to verify your changes pass CI checks (clippy, format, tests, cargo-deny).
2. For user-facing changes, run `mise run change` to create a Knope change file.
3. Use conventional commit messages for PRs: `feat:`, `fix:`, `docs:`, `test:`, `chore:`, etc.

## Project structure

- **shared/c/** — Go library that builds a C-shared library. All language bindings depend on this.
- **rust/** — Rust crate that links against shared/c and provides `EmbeddedSpiceDB`.

When adding support for a new language, add a new top-level directory (e.g. `python/`) that uses the C FFI from shared/c.
