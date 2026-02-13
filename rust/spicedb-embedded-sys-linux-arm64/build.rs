fn main() {
    spicedb_embedded_native_build::run("linux-arm64", Some(env!("CARGO_PKG_VERSION")));
}
