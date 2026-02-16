//! If `src/generated/authzed.api.v1.rs` exists (checked-in for published crate), skip buf and codegen.
//! Otherwise run buf export (get .protos) + tonic_build::compile_protos (invokes protoc) so developers can regenerate.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-check-cfg=cfg(proto_checked_in)");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let checked_in = manifest
        .join("src")
        .join("generated")
        .join("authzed.api.v1.rs");

    if checked_in.exists() {
        println!("cargo:rustc-cfg=proto_checked_in");
        println!("cargo:rerun-if-changed=src/generated/");
        return Ok(());
    }

    run_buf_and_tonic(&manifest)
}

fn run_buf_and_tonic(manifest: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    let proto_dir = manifest.join("proto");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let export_dir = out_dir.join("buf_export");

    let status = Command::new("buf")
        .args(["dep", "update"])
        .current_dir(&proto_dir)
        .status()?;
    if !status.success() {
        return Err("buf dep update failed".into());
    }

    let status = Command::new("buf")
        .args(["export", ".", "--output", export_dir.to_str().unwrap()])
        .current_dir(&proto_dir)
        .status()?;
    if !status.success() {
        return Err("buf export failed".into());
    }

    let protos: Vec<PathBuf> = walkdir::WalkDir::new(&export_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "proto"))
        .map(|e| e.path().to_path_buf())
        .collect();

    tonic_build::configure()
        .build_server(false)
        .compile_protos(&protos, &[&export_dir])?;

    println!("cargo:rerun-if-changed=proto/");
    Ok(())
}
