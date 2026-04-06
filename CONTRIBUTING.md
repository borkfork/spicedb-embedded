# Contributing to spicedb-embedded

## Architecture

```
┌───────────────────────────────────────────────────────────────────────┐
│  Your Application (Rust, Java, Python, C#, TypeScript, etc.)          │
├───────────────────────────────────────────────────────────────────────┤
│  Language bindings (rust/, java/, python/, csharp/, node/)            │
│  - FFI/cbindgen to shared/c                                           │
│  - Native gRPC (protobuf) over Unix socket / tcp                      │
├───────────────────────────────────────────────────────────────────────┤
│  shared/c: C-shared library (Go/CGO)                                  │
│  - Embeds SpiceDB server                                              │
│  - Datastore: memory (default), postgres, cockroachdb, spanner, mysql │
│  - Listens on unique Unix socket or TCP per instance                  │
└───────────────────────────────────────────────────────────────────────┘
```

All language bindings build on top of **shared/c** via C FFI. Each instance is independent, enabling parallel testing.

## Building shared/c

```bash
mise run shared-c-build
# or manually:
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

## Prerequisites

1. Install [mise](https://mise.jdx.dev/installing-mise.html) for managing tools.
2. Run `mise install` to install Go, Rust, Java, Maven, Node.js, Python, and .NET from `mise.toml`.
3. Ensure CGO is enabled for Go (it is by default on most systems).

When working on a single language, you can install only the tools needed:

- **Rust:** `mise install rust go github:EmbarkStudios/cargo-deny taplo` (add `github:knope-dev/knope` for `mise run change`; not available on linux-arm64)
- **Java:** `mise install java maven go`
- **Python:** `mise install python go`
- **C#:** `mise install dotnet go`
- **Node:** `mise install node go`

To prevent mise from auto-installing other tools when running tasks (e.g. `mise run csharp-format-check`), set `MISE_ENABLE_TOOLS` before running:

```bash
MISE_ENABLE_TOOLS=dotnet,go mise run csharp-format-check
MISE_ENABLE_TOOLS=dotnet,go mise run csharp-test
```

## Workflow

1. Run `mise run check` to verify your changes pass CI checks:
   - Format checks (Rust, TOML, Python, Java, C#, Node.js)
   - Clippy (Rust)
   - Cargo-deny (Rust)
   - Tests (all languages)

2. Run `mise run reformat` to format all languages before committing.

3. For user-facing changes, run `mise run change` to create a Knope change file.

4. Use conventional commit messages for PRs: `feat:`, `fix:`, `docs:`, `test:`, `chore:`, etc.

## Key tasks

| Task | Description |
|------|-------------|
| `mise run test` | Run tests for Rust, Java, Python, C#, Node.js |
| `mise run check` | Format checks, clippy, cargo-deny, tests |
| `mise run format-check` | Verify format for all languages |
| `mise run reformat` | Format all languages |
| `mise run docker-test` | Build and run all tests in Docker |

## Project structure

- **shared/c/** — Go library that builds a C-shared library. All language bindings depend on this.
- **rust/** — Rust crate. Thin FFI + spicedb-grpc.
- **java/** — Java library. JNA FFI + authzed gRPC clients.
- **python/** — Python package. ctypes FFI + authzed gRPC clients.
- **csharp/** — C# / .NET package. P/Invoke FFI + Authzed.Net.
- **node/** — Node.js package. koffi FFI + @authzed/authzed-node.

When adding support for a new language, add a new top-level directory that uses the C FFI from shared/c, plus a mise task for building and testing.
