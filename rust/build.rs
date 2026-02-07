//! Build script for spicedb-embedded.
//!
//! Builds a C-shared library from Go source and links it.

use std::{path::Path, process::Command};

fn main() {
    let shared_c_dir = Path::new("../shared/c");
    let main_go_path = shared_c_dir.join("main.go");

    // Determine output library name based on OS
    let (lib_name, lib_ext) = if cfg!(target_os = "macos") {
        ("libspicedb", "dylib")
    } else if cfg!(target_os = "windows") {
        ("spicedb", "dll")
    } else {
        ("libspicedb", "so")
    };
    let lib_filename = format!("{lib_name}.{lib_ext}");
    let lib_path = shared_c_dir.join(&lib_filename);

    // Rebuild if Go source changes
    println!("cargo:rerun-if-changed=../shared/c/main.go");
    println!("cargo:rerun-if-changed=../shared/c/go.mod");
    // Note: C library now uses spicedb_start (no schema/relationships)

    // Check if library needs to be built
    let lib_needs_build = if lib_path.exists() {
        if main_go_path.exists() {
            let go_modified = std::fs::metadata(&main_go_path)
                .and_then(|m| m.modified())
                .ok();
            let lib_modified = std::fs::metadata(&lib_path).and_then(|m| m.modified()).ok();
            matches!((go_modified, lib_modified), (Some(go_time), Some(lib_time)) if go_time > lib_time)
        } else {
            false
        }
    } else {
        true
    };

    if lib_needs_build {
        println!("cargo:warning=Building {lib_filename} from Go source...");

        let output = Command::new("go")
            .args(["build", "-buildmode=c-shared", "-o", &lib_filename, "."])
            .current_dir(shared_c_dir)
            .output();

        // On macOS, fix the install name so the library can be found at runtime
        // via @rpath without needing DYLD_LIBRARY_PATH
        #[cfg(target_os = "macos")]
        if let Ok(ref result) = output {
            if result.status.success() {
                let _ = Command::new("install_name_tool")
                    .args(["-id", "@rpath/libspicedb.dylib", &lib_filename])
                    .current_dir(shared_c_dir)
                    .output();
            }
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
                panic!(
                    "Go not found ({e}). Install Go with 'mise install' and ensure CGO is enabled."
                );
            }
        }
    }

    let lib_abs_path = shared_c_dir.canonicalize().unwrap();

    // Tell cargo to link the library
    println!("cargo:rustc-link-search=native={}", lib_abs_path.display());
    println!("cargo:rustc-link-lib=dylib=spicedb");

    // Copy library to target directory so it's found at runtime
    // This is more reliable than rpath across different build scenarios
    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let out_path = Path::new(&out_dir);
        // OUT_DIR is something like target/debug/build/spicedb-embedded-xxx/out
        // We want to copy to target/debug
        if let Some(target_dir) = out_path.ancestors().nth(3) {
            let dest = target_dir.join(&lib_filename);
            // Always copy if source is newer than destination or destination doesn't exist
            let needs_copy = if dest.exists() {
                let src_modified = std::fs::metadata(&lib_path).and_then(|m| m.modified()).ok();
                let dest_modified = std::fs::metadata(&dest).and_then(|m| m.modified()).ok();
                matches!((src_modified, dest_modified), (Some(src_time), Some(dest_time)) if src_time > dest_time)
            } else {
                true
            };
            if needs_copy {
                if let Err(e) = std::fs::copy(&lib_path, &dest) {
                    println!(
                        "cargo:warning=Failed to copy library to {}: {}",
                        dest.display(),
                        e
                    );
                }
            }
        }
    }

    // Set rpath as a fallback
    if cfg!(target_os = "macos") {
        // Use @executable_path for relative lookup
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_abs_path.display());
    }

    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_abs_path.display());
    }
}
