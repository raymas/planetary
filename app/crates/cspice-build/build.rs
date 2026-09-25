//! Builds the CSPICE C library from the source tree shipped in `spice/cspice`.
//!
//! The source directory can be overridden with the `CSPICE_SRC_DIR` environment
//! variable; by default it resolves to `<repo root>/spice/cspice`.
//!
//! CSPICE is plain portable C (partly f2c-translated FORTRAN), so it compiles
//! with cc/clang/MSVC for every target we care about: x86_64/aarch64
//! Windows/macOS/Linux plus Android and iOS.

use std::path::{Path, PathBuf};

fn find_source_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CSPICE_SRC_DIR") {
        return PathBuf::from(dir);
    }
    // app/crates/cspice-build -> repo root
    let mut dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("manifest dir depth")
        .to_path_buf();
    dir.push("spice");
    dir.push("cspice");
    dir
}

fn main() {
    let cspice = find_source_dir();
    let src = cspice.join("src").join("cspice");
    let include = cspice.join("include");

    if !src.is_dir() {
        panic!(
            "CSPICE source not found at {}. Run from the repository or set CSPICE_SRC_DIR.",
            src.display()
        );
    }

    println!("cargo:rerun-if-env-changed=CSPICE_SRC_DIR");

    let mut sources: Vec<PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("read {}: {e}", src.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "c").unwrap_or(false))
        .collect();
    sources.sort();

    let mut build = cc::Build::new();
    build
        .files(&sources)
        .include(&src)
        .include(&include)
        // f2c-translated code relies on common-symbol merging, uses K&R-era
        // constructs that modern compilers reject by default, and is
        // extremely noisy; gnu89 turns the hard errors into warnings and -w
        // silences everything.
        .flag_if_supported("-w")
        .flag_if_supported("-fcommon")
        .flag_if_supported("-std=gnu89");

    // The shipped SpiceZpl.h is pre-configured for one platform; the macro it
    // defines only selects a 32-bit SpiceInt, which is what we want on every
    // 64-bit target. Define it explicitly per target so the build does not
    // depend on the pre-set header.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let platform = match (target_os.as_str(), target_arch.as_str()) {
        ("linux", _) => "CSPICE_PC_LINUX_64BIT_GCC",
        ("macos", "x86_64") => "CSPICE_MAC_OSX_INTEL_64BIT_GCC",
        ("macos", "aarch64") => "CSPICE_MAC_OSX_M1_64BIT_CLANG",
        // Windows: SpiceInt falls back to `long`, which is 32-bit there —
        // same ABI we declare in the FFI.
        _ => "",
    };
    if !platform.is_empty() {
        build.define(platform, None);
    }

    build.compile("cspice");
}
