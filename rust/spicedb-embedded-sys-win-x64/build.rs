fn main() {
    spicedb_embedded_native_build::run("win-x64", Some(env!("CARGO_PKG_VERSION")));
}
