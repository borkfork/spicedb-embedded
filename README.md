# spicedb-embedded

Embedded [SpiceDB](https://authzed.com/spicedb) for use in application tests and development. This repository provides an in-memory SpiceDB server that you can communicate with over gRPC using Unix sockets.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Your Application (Rust, Python, etc.)                          │
├─────────────────────────────────────────────────────────────────┤
│  Language bindings (rust/, python/, ...)                         │
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

| Language | README | Build & Test |
|----------|--------|--------------|
| **Rust** | [rust/README.md](rust/README.md) | `cd rust && cargo build && cargo test` |
| **Java** | [java/README.md](java/README.md) | `mise run shared-c-build && cd java && mvn test` |
| **Python** | [python/README.md](python/README.md) | `mise run python-test` |

## Prerequisites

- **Go** 1.23+ with CGO enabled (for building shared/c)
- **Rust**, **Java 17+**, or **Python 3.10+** (depending on language)

## Building shared/c

```bash
mise run shared-c-build
# or manually:
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

## Directory Structure

| Directory | Description |
|-----------|-------------|
| `shared/c/` | Go/CGO library that embeds SpiceDB. Exposes `spicedb_start`, `spicedb_dispose`, `spicedb_free` via C FFI. |
| `rust/`    | Rust crate — see [rust/README.md](rust/README.md). Thin FFI + [spicedb-grpc](https://docs.rs/spicedb-grpc) clients. |
| `java/`    | Java library — see [java/README.md](java/README.md). JNA FFI + [authzed](https://central.sonatype.com/artifact/com.authzed.api/authzed) gRPC clients. |
| `python/`  | Python package — see [python/README.md](python/README.md). ctypes FFI + [authzed](https://pypi.org/project/authzed/) gRPC clients. |

## License

Apache-2.0
