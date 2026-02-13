//! Build script: detect target, then obtain C shared lib (local prebuild, build from Go, or download).

use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let rid = target_rid();
    run(rid, Some(env!("CARGO_PKG_VERSION")));
}

fn target_rid() -> &'static str {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    match (os.as_str(), arch.as_str()) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "aarch64") => "osx-arm64",
        ("windows", "x86_64") => "win-x64",
        _ => panic!(
            "spicedb-embedded-sys does not support target {}-{}. \
            Supported: linux-x64, linux-arm64, osx-arm64, win-x64.",
            os, arch
        ),
    }
}

fn run(rid: &str, release_version: Option<&str>) {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo"),
    );
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo"));
    let (lib_filename, lib_path) = lib_artifact_name_and_path(rid, &out_dir);

    let prebuild_crate = manifest_dir.join("prebuilds").join(rid).join(&lib_filename);
    let prebuild_repo = manifest_dir
        .join("..")
        .join("prebuilds")
        .join(rid)
        .join(&lib_filename);

    #[allow(unused_variables)]
    let (prebuild, prebuild_dir) = if prebuild_crate.exists() {
        (
            prebuild_crate.clone(),
            manifest_dir.join("prebuilds").join(rid),
        )
    } else if prebuild_repo.exists() {
        (
            prebuild_repo.clone(),
            manifest_dir.join("..").join("prebuilds").join(rid),
        )
    } else if try_build_from_source(&manifest_dir, &out_dir, &lib_filename) {
        let prebuild = out_dir.join(&lib_filename);
        (prebuild.clone(), out_dir.clone())
    } else {
        let env_version = std::env::var("SPICEDB_EMBEDDED_RELEASE_VERSION").ok();
        let version = release_version.or(env_version.as_deref());
        if let Some(version) = version {
            download_from_release(rid, version, &out_dir);
            let prebuild = out_dir.join(&lib_filename);
            (prebuild.clone(), out_dir.clone())
        } else {
            panic!(
                "Prebuild not found for {}. Looked in {} and {}. \
                Build from source: install Go (CGO enabled) and build from repo (mise run shared-c-build), \
                or run ./scripts/stage-all-prebuilds.sh. \
                When using the published crate, the build script uses CARGO_PKG_VERSION to download the lib from GitHub Release.",
                rid,
                prebuild_crate.display(),
                prebuild_repo.display()
            );
        }
    };

    if prebuild.exists() {
        println!("cargo:rerun-if-changed={}", prebuild.display());
        std::fs::copy(&prebuild, &lib_path).expect("copy prebuild to OUT_DIR");
    }

    #[cfg(target_os = "windows")]
    {
        generate_import_lib_windows(&prebuild_dir, &out_dir);
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=dylib=spicedb");
    copy_lib_to_target(&lib_path, &lib_filename);
    if let Some(target_dir) = out_dir.ancestors().nth(3) {
        emit_rpath(target_dir);
    }
}

fn try_build_from_source(manifest_dir: &Path, out_dir: &Path, lib_filename: &str) -> bool {
    // Build from shared/c inside the -sys crate (copy from repo first if not present).
    let in_crate = manifest_dir.join("shared").join("c");
    let shared_c = if in_crate.join("go.mod").exists() {
        in_crate
    } else {
        // Copy repo shared/c into the -sys crate directory, then build from there.
        let repo_root = match manifest_dir.ancestors().nth(2) {
            Some(r) => r.to_path_buf(),
            None => return false,
        };
        let repo_shared_c = repo_root.join("shared").join("c");
        if !repo_shared_c.join("go.mod").exists() {
            return false;
        }
        let dest = manifest_dir.join("shared");
        if let Err(e) = copy_dir_recursive(&repo_shared_c, &dest.join("c")) {
            eprintln!("cargo:warning=copy shared/c into -sys crate: {}", e);
            return false;
        }
        dest.join("c")
    };
    let _ = std::fs::create_dir_all(out_dir);
    let out_lib = out_dir.join(lib_filename);
    let status = Command::new("go")
        .args([
            "build",
            "-buildmode=c-shared",
            "-o",
            out_lib.to_str().unwrap(),
            ".",
        ])
        .current_dir(&shared_c)
        .env("CGO_ENABLED", "1")
        .status();
    let ok = match status {
        Ok(s) => s.success(),
        Err(_) => false,
    };
    if !ok {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        let def_src = shared_c.join("spicedb.def");
        if def_src.exists() {
            let _ = std::fs::copy(&def_src, out_dir.join("spicedb.def"));
        }
    }
    if out_lib.exists() {
        println!("cargo:rerun-if-changed={}", shared_c.display());
        true
    } else {
        false
    }
}

/// Copy a directory recursively. Creates dest parent if needed.
fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

fn download_from_release(rid: &str, version: &str, out_dir: &Path) {
    let url = format!(
        "https://github.com/borkfork/spicedb-embedded/releases/download/v{}/libspicedb-{}.tar.gz",
        version, rid
    );
    let archive = out_dir.join("libspicedb.tar.gz");
    let status = Command::new("curl")
        .args(["-sSL", "-o", archive.to_str().unwrap(), &url])
        .status();
    let status = status.unwrap_or_else(|e| {
        panic!(
            "Failed to run curl to download {}: {}. Install curl or use a local prebuild.",
            url, e
        )
    });
    if !status.success() {
        panic!(
            "curl failed ({}). Check network and that release v{} exists with asset libspicedb-{}.tar.gz",
            status, version, rid
        );
    }
    let status = Command::new("tar")
        .args([
            "-xzf",
            archive.to_str().unwrap(),
            "-C",
            out_dir.to_str().unwrap(),
        ])
        .status();
    let status = status
        .unwrap_or_else(|e| panic!("Failed to run tar to extract {}: {}", archive.display(), e));
    if !status.success() {
        panic!("tar failed ({}) extracting {}", status, archive.display());
    }
    let _ = std::fs::remove_file(&archive);
}

fn lib_artifact_name_and_path(rid: &str, out_dir: &Path) -> (String, PathBuf) {
    let (name, ext) = if rid.starts_with("osx-") {
        ("libspicedb", "dylib")
    } else if rid.starts_with("win-") {
        ("spicedb", "dll")
    } else {
        ("libspicedb", "so")
    };
    let lib_filename = format!("{name}.{ext}");
    (lib_filename.clone(), out_dir.join(&lib_filename))
}

fn copy_lib_to_target(lib_path: &Path, lib_filename: &str) {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo");
    let out_path = Path::new(&out_dir);
    if let Some(target_dir) = out_path.ancestors().nth(3) {
        let _ = std::fs::copy(lib_path, target_dir.join(lib_filename));
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
            Install Visual Studio Build Tools. From Git Bash: scripts/generate-dll-import-lib.sh {} {}",
            def_file.display(),
            out_dir.display()
        );
    }
}

#[cfg(target_os = "windows")]
fn try_generate_import_lib_win(def_file: &Path, lib_file: &Path) -> Result<(), ()> {
    let lib_exe = find_msvc_lib_exe().ok_or(())?;
    let status = Command::new(&lib_exe)
        .arg(format!("/def:{}", def_file.display()))
        .arg(format!("/out:{}", lib_file.display()))
        .arg("/machine:x64")
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(()),
    }
}

#[cfg(target_os = "windows")]
fn find_msvc_lib_exe() -> Option<PathBuf> {
    let vswhere =
        Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
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
    let vc_tools = Path::new(vs_path).join("VC").join("Tools").join("MSVC");
    for entry in std::fs::read_dir(&vc_tools).ok()?.flatten() {
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
    None
}
