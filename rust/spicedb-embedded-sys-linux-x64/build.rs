fn main() {
    spicedb_embedded_native_build::run("linux-x64", Some(env!("CARGO_PKG_VERSION")));
}
