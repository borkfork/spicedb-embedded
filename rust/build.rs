//! Build script for spicedb-embedded.
//!
//! Prefers a prebuilt C-shared library in `prebuilds/<rid>/` (same layout as stage-all-prebuilds.sh).
//! Otherwise builds from Go source in `shared/c` (crates.io publish) or `../shared/c` (in-repo).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo");
    let out_path = Path::new(&out_dir);
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo"),
    );
    let (lib_filename, lib_path) = lib_artifact_name_and_path_out(out_path);

    // Prefer staged prebuild (same convention as C#/Node/Java/Python)
    if let Some(rid) = target_rid() {
        let prebuild = manifest_dir.join("prebuilds").join(rid).join(&lib_filename);
        if prebuild.exists() {
            println!("cargo:rerun-if-changed={}", prebuild.display());
            assert!(
                std::fs::copy(&prebuild, &lib_path).is_ok(),
                "Failed to copy prebuild from {}",
                prebuild.display()
            );
            #[cfg(target_os = "windows")]
            {
                let prebuild_dir = manifest_dir.join("prebuilds").join(rid);
                generate_import_lib_windows(&prebuild_dir, out_path);
            }
            println!("cargo:rustc-link-search=native={}", out_path.display());
            println!("cargo:rustc-link-lib=dylib=spicedb");
            copy_lib_to_target(&lib_path, &lib_filename);
            if let Some(target_dir) = out_path.ancestors().nth(3) {
                emit_rpath(target_dir);
            }
            return;
        }
    }

    let (shared_c_dir, use_crate_shared) = resolve_shared_c_dir();
    emit_rerun_if_changed(use_crate_shared);

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

/// Maps `target_os` + `target_arch` to our RID (same as C# / stage-all-prebuilds).
const fn target_rid() -> Option<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Some("linux-x64");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Some("linux-arm64");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Some("osx-x64");
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Some("osx-arm64");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Some("win-x64");
    #[allow(unreachable_code)]
    None
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
    } else {
        println!("cargo:rerun-if-changed=../shared/c/main.go");
        println!("cargo:rerun-if-changed=../shared/c/go.mod");
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

    #[cfg(target_os = "windows")]
    generate_import_lib_windows(shared_c_dir, out_dir);
}

#[cfg(target_os = "windows")]
fn generate_import_lib_windows(def_dir: &Path, out_dir: &Path) {
    let def_file = def_dir.join("spicedb.def");
    let lib_file = out_dir.join("spicedb.lib");
    if !def_file.exists() {
        return;
    }
    if try_generate_import_lib_win(&def_file, &lib_file).is_err() {
        panic!(
            "Failed to generate spicedb.lib. MSVC needs the import library to link the DLL. \
            Install Visual Studio Build Tools. From Git Bash you can run: scripts/generate-dll-import-lib.sh {} {}",
            def_file.display(),
            out_dir.display()
        );
    }
    println!("cargo:warning=Generated spicedb.lib for MSVC linking");
}

#[cfg(target_os = "windows")]
fn try_generate_import_lib_win(def_file: &Path, lib_file: &Path) -> Result<(), ()> {
    let lib_exe = find_msvc_lib_exe().ok_or(())?;
    let def_arg = format!("/def:{}", def_file.display());
    let out_arg = format!("/out:{}", lib_file.display());
    let status = Command::new(&lib_exe)
        .arg(def_arg)
        .arg(out_arg)
        .arg("/machine:x64")
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(()),
    }
}

#[cfg(target_os = "windows")]
fn find_msvc_lib_exe() -> Option<PathBuf> {
    let vswhere = r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe";
    let vswhere = Path::new(vswhere);
    if !vswhere.exists() {
        return None;
    }
    let out = Command::new(vswhere)
        .args(["-latest", "-property", "installationPath"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let vs_path = std::str::from_utf8(&out.stdout).ok()?.trim_end();
    let vs_path = Path::new(vs_path);
    let vc_tools = vs_path.join("VC").join("Tools").join("MSVC");
    if let Ok(entries) = std::fs::read_dir(&vc_tools) {
        for entry in entries.flatten() {
            let path = entry
                .path()
                .join("bin")
                .join("Hostx64")
                .join("x64")
                .join("lib.exe");
            if path.exists() {
                return Some(path);
            }
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
