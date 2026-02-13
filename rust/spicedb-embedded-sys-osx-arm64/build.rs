fn main() {
    spicedb_embedded_native_build::run("osx-arm64", Some(env!("CARGO_PKG_VERSION")));
}
