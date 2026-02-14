use std::{path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")?;
    let proto_dir = PathBuf::from(&manifest).join("proto");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let export_dir = out_dir.join("buf_export");

    // Resolve deps (no-op if buf.lock is current); required so export includes BSR deps.
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
