//! No build-time steps: the C shared library is provided by the platform-specific
//! crates (spicedb-embedded-sys-linux-x64, spicedb-embedded-sys-osx-arm64, etc.), which
//! bundle the prebuilt binary and set up linking.

fn main() {}
