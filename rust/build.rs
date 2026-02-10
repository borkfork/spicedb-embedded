//! Build script for spicedb-embedded.
//!
//! Builds a C-shared library from Go source and links it.
//! Uses `CARGO_MANIFEST_DIR` so the crate works when published to crates.io (`shared/c`
//! is copied into the package at publish time). In-repo development falls back to `../shared/c`.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo");
    let out_path = Path::new(&out_dir);

    let (shared_c_dir, use_crate_shared) = resolve_shared_c_dir();
    emit_rerun_if_changed(use_crate_shared);
    let (lib_filename, lib_path) = lib_artifact_name_and_path_out(out_path);

    if lib_needs_build(&lib_path, &shared_c_dir) {
        build_go_lib_to(&shared_c_dir, out_path, &lib_filename);
    }

    println!("cargo:rustc-link-search=native={}", out_path.display());
    println!("cargo:rustc-link-lib=dylib=spicedb");
    copy_lib_to_target(&lib_path, &lib_filename);
    if let Some(target_dir) = out_path.ancestors().nth(3) {
        emit_rpath(target_dir);
    }
}

fn resolve_shared_c_dir() -> (PathBuf, bool) {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo");
    let manifest_path = Path::new(&manifest_dir);
    let shared_in_crate = manifest_path.join("shared").join("c");
    let use_crate_shared = shared_in_crate.join("main.go").exists();
    let dir = if use_crate_shared {
        shared_in_crate.canonicalize().expect("shared/c in crate")
    } else {
        manifest_path
            .join("..")
            .join("shared")
            .join("c")
            .canonicalize()
            .expect("shared/c at repo root")
    };
    (dir, use_crate_shared)
}

fn emit_rerun_if_changed(use_crate_shared: bool) {
    if use_crate_shared {
        println!("cargo:rerun-if-changed=shared/c/main.go");
        println!("cargo:rerun-if-changed=shared/c/go.mod");
        println!("cargo:rerun-if-changed=shared/c/go.sum");
        if cfg!(target_os = "windows") {
            println!("cargo:rerun-if-changed=shared/c/spicedb.def");
        }
    } else {
        println!("cargo:rerun-if-changed=../shared/c/main.go");
        println!("cargo:rerun-if-changed=../shared/c/go.mod");
        println!("cargo:rerun-if-changed=../shared/c/go.sum");
        if cfg!(target_os = "windows") {
            println!("cargo:rerun-if-changed=../shared/c/spicedb.def");
        }
    }
}

fn lib_artifact_name_and_path_out(out_dir: &Path) -> (String, PathBuf) {
    let (lib_name, lib_ext) = if cfg!(target_os = "macos") {
        ("libspicedb", "dylib")
    } else if cfg!(target_os = "windows") {
        ("spicedb", "dll")
    } else {
        ("libspicedb", "so")
    };
    let lib_filename = format!("{lib_name}.{lib_ext}");
    let lib_path = out_dir.join(&lib_filename);
    (lib_filename, lib_path)
}

fn lib_needs_build(lib_path: &Path, shared_c_dir: &Path) -> bool {
    let main_go = shared_c_dir.join("main.go");
    if !lib_path.exists() {
        return true;
    }
    if !main_go.exists() {
        return false;
    }
    let go_modified = std::fs::metadata(&main_go).and_then(|m| m.modified()).ok();
    let lib_modified = std::fs::metadata(lib_path).and_then(|m| m.modified()).ok();
    matches!(
        (go_modified, lib_modified),
        (Some(go_time), Some(lib_time)) if go_time > lib_time
    )
}

fn build_go_lib_to(shared_c_dir: &Path, out_dir: &Path, lib_filename: &str) {
    println!("cargo:warning=Building {lib_filename} from Go source...");
    let out_lib = out_dir.join(lib_filename);
    let output = Command::new("go")
        .args([
            "build",
            "-buildmode=c-shared",
            "-o",
            out_lib.to_str().expect("out path is utf-8"),
            ".",
        ])
        .current_dir(shared_c_dir)
        .output();

    #[cfg(target_os = "macos")]
    if let Ok(ref result) = output
        && result.status.success()
    {
        let _ = Command::new("install_name_tool")
            .args([
                "-id",
                "@rpath/libspicedb.dylib",
                out_lib.to_str().expect("path is utf-8"),
            ])
            .output();
    }

    match output {
        Ok(result) if result.status.success() => {
            println!("cargo:warning=Successfully built {lib_filename}");
            #[cfg(target_os = "windows")]
            generate_windows_import_lib(shared_c_dir, out_dir);
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            panic!(
                "Failed to build {lib_filename}: {stderr}\n\
                Make sure CGO is enabled and Go is installed."
            );
        }
        Err(e) => {
            panic!("Go not found ({e}). Install Go with 'mise install' and ensure CGO is enabled.");
        }
    }
}

#[cfg(target_os = "windows")]
fn generate_windows_import_lib(shared_c_dir: &Path, out_dir: &Path) {
    println!("cargo:warning=Generating spicedb.lib import library for MSVC...");
    let def_file = shared_c_dir.join("spicedb.def");
    let dll_file = out_dir.join("spicedb.dll");
    let lib_file = out_dir.join("spicedb.lib");

    if !def_file.exists() {
        println!("cargo:warning=spicedb.def not found, skipping import library generation");
        return;
    }
    if !dll_file.exists() {
        println!("cargo:warning=spicedb.dll not found, skipping import library generation");
        return;
    }

    // Try to find lib.exe from Visual Studio
    let lib_exe = find_msvc_lib_exe();
    let Some(lib_exe) = lib_exe else {
        println!("cargo:warning=lib.exe not found. Install Visual Studio Build Tools for MSVC linking support.");
        return;
    };

    let result = Command::new(&lib_exe)
        .args([
            format!("/def:{}", def_file.to_str().expect("path is utf-8")),
            format!("/out:{}", lib_file.to_str().expect("path is utf-8")),
            "/machine:x64".to_string(),
        ])
        .output();

    match result {
        Ok(result) if result.status.success() => {
            println!("cargo:warning=Successfully generated spicedb.lib");
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            println!("cargo:warning=Failed to generate import library: {stderr}");
        }
        Err(e) => {
            println!("cargo:warning=Failed to run lib.exe: {e}");
        }
    }
}

#[cfg(target_os = "windows")]
fn find_msvc_lib_exe() -> Option<PathBuf> {
    use std::env;

    // Try vswhere first
    let vswhere = Path::new("C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe");
    if vswhere.exists() {
        if let Ok(output) = Command::new(vswhere)
            .args(["-latest", "-property", "installationPath"])
            .output()
        {
            if output.status.success() {
                let vs_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Some(lib_exe) = find_lib_exe_in_vs_path(&vs_path) {
                    return Some(lib_exe);
                }
            }
        }
    }

    // Fallback to common locations
    let common_paths = [
        "C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise",
        "C:\\Program Files\\Microsoft Visual Studio\\2022\\Professional",
        "C:\\Program Files\\Microsoft Visual Studio\\2022\\Community",
        "C:\\Program Files\\Microsoft Visual Studio\\2022\\BuildTools",
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Enterprise",
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Professional",
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\Community",
        "C:\\Program Files (x86)\\Microsoft Visual Studio\\2019\\BuildTools",
    ];

    for vs_path in &common_paths {
        if let Some(lib_exe) = find_lib_exe_in_vs_path(vs_path) {
            return Some(lib_exe);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn find_lib_exe_in_vs_path(vs_path: &str) -> Option<PathBuf> {
    let vc_tools = Path::new(vs_path).join("VC\\Tools\\MSVC");
    if !vc_tools.exists() {
        return None;
    }

    // Find the latest MSVC version (sort descending by path name)
    // Note: This relies on MSVC version numbers being lexicographically sortable (e.g., "14.40.33807")
    let mut versions: Vec<_> = std::fs::read_dir(&vc_tools)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    
    versions.sort_by(|a, b| b.path().cmp(&a.path()));

    for version_dir in versions {
        let lib_exe = version_dir.path().join("bin\\Hostx64\\x64\\lib.exe");
        if lib_exe.exists() {
            return Some(lib_exe);
        }
    }

    None
}

fn copy_lib_to_target(lib_path: &Path, lib_filename: &str) {
    let Some(out_dir) = std::env::var("OUT_DIR").ok() else {
        return;
    };
    let out_path = Path::new(&out_dir);
    let Some(target_dir) = out_path.ancestors().nth(3) else {
        return;
    };
    let dest = target_dir.join(lib_filename);
    let needs_copy = if dest.exists() {
        let src_modified = std::fs::metadata(lib_path).and_then(|m| m.modified()).ok();
        let dest_modified = std::fs::metadata(&dest).and_then(|m| m.modified()).ok();
        matches!(
            (src_modified, dest_modified),
            (Some(src_time), Some(dest_time)) if src_time > dest_time
        )
    } else {
        true
    };
    if needs_copy && let Err(e) = std::fs::copy(lib_path, &dest) {
        println!(
            "cargo:warning=Failed to copy library to {}: {}",
            dest.display(),
            e
        );
    }

    // On Windows, also copy the .lib import library if it exists
    #[cfg(target_os = "windows")]
    {
        let lib_import = out_path.join("spicedb.lib");
        if lib_import.exists() {
            let dest_lib = target_dir.join("spicedb.lib");
            if let Err(e) = std::fs::copy(&lib_import, &dest_lib) {
                println!(
                    "cargo:warning=Failed to copy import library to {}: {}",
                    dest_lib.display(),
                    e
                );
            }
        }
    }
}

fn emit_rpath(runtime_lib_dir: &Path) {
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            runtime_lib_dir.display()
        );
    }
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            runtime_lib_dir.display()
        );
    }
}
