# spicedb-embedded

Embedded [SpiceDB](https://authzed.com/spicedb) for use in application tests and development. This repository provides an in-memory SpiceDB server that you can communicate with over gRPC using Unix sockets.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Your Application (Rust, Java, Python, C#, etc.)                 │
├─────────────────────────────────────────────────────────────────┤
│  Language bindings (rust/, java/, python/, csharp/)               │
│  - FFI/cbindgen to shared/c                                     │
│  - Native gRPC (tonic/protobuf) over Unix socket                 │
├─────────────────────────────────────────────────────────────────┤
│  shared/c: C-shared library (Go/CGO)                             │
│  - Embeds SpiceDB server                                        │
│  - In-memory datastore, one instance per handle                  │
│  - Listens on unique Unix socket per instance                   │
└─────────────────────────────────────────────────────────────────┘
```

The **shared/c** library is the foundation—all language bindings build on top of it via C FFI. Each instance is independent, enabling parallel testing.

## Quick Start

Install [mise](https://mise.jdx.dev/installing-mise.html), then run `mise install` to install all tools (Go, Rust, Java, Maven, Python, .NET).

| Language | README | Build & Test |
|----------|--------|--------------|
| **Rust** | [rust/README.md](rust/README.md) | `mise run rust-test` |
| **Java** | [java/README.md](java/README.md) | `mise run java-test` |
| **Python** | [python/README.md](python/README.md) | `mise run python-test` |
| **C# / .NET** | [csharp/README.md](csharp/README.md) | `mise run csharp-test` |

Run all tests: `mise run test`

## Prerequisites

- **Go** 1.23+ with CGO enabled (for building shared/c)
- **Rust** 1.91.1, **Java 17** (Temurin), **Python 3.11**, or **.NET 9** (depending on language) — managed by mise

## Building shared/c

```bash
mise run shared-c-build
# or manually:
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

## Testing in Docker

To run all tests in a clean Linux environment:

```bash
mise run docker-test
```

Or: `docker build -t spicedb-embedded-test -f Dockerfile .`

## Directory Structure

| Directory | Description |
|-----------|-------------|
| `shared/c/` | Go/CGO library that embeds SpiceDB. Exposes `spicedb_start`, `spicedb_dispose`, `spicedb_free` via C FFI. |
| `rust/`    | Rust crate — see [rust/README.md](rust/README.md). Thin FFI + [spicedb-grpc](https://docs.rs/spicedb-grpc) clients. |
| `java/`    | Java library — see [java/README.md](java/README.md). JNA FFI + [authzed](https://central.sonatype.com/artifact/com.authzed.api/authzed) gRPC clients. |
| `python/`  | Python package — see [python/README.md](python/README.md). ctypes FFI + [authzed](https://pypi.org/project/authzed/) gRPC clients. |
| `csharp/`  | C# / .NET package — see [csharp/README.md](csharp/README.md). P/Invoke FFI + [Authzed.Net](https://www.nuget.org/packages/Authzed.Net) gRPC clients. |

## License

Apache-2.0
