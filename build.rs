//! Builds `swift/` (`ParaBridge`, which links FluidAudio's real Swift ASR
//! library directly — see `specs/004-native-coreml-backend/research.md`)
//! and links the resulting static library plus the Apple frameworks it
//! needs into the `para` binary. Mirrors `fluidaudio-rs`'s own build.rs,
//! which proved this exact `swift build` + `cargo:rustc-link-*` approach
//! works for the same dependency.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR is always set by cargo for build scripts");
    let swift_dir = Path::new(&manifest_dir).join("swift");

    println!(
        "cargo:rerun-if-changed={}",
        swift_dir.join("Package.swift").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        swift_dir.join("Sources").display()
    );

    let status = std::process::Command::new("swift")
        .args(["build", "-c", "release"])
        .current_dir(&swift_dir)
        .status()
        .unwrap_or_else(|e| {
            eprintln!(
                "error: failed to run `swift build` in {} — is the Swift toolchain (Xcode Command \
                 Line Tools) installed? ({e})",
                swift_dir.display()
            );
            std::process::exit(1);
        });
    if !status.success() {
        eprintln!("error: `swift build -c release` failed for the ParaBridge package");
        std::process::exit(1);
    }

    let lib_dir = swift_dir.join(".build").join("release");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ParaBridge");

    for framework in ["Foundation", "AVFoundation", "CoreML", "Accelerate"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    println!("cargo:rustc-link-lib=dylib=swiftCore");
    println!("cargo:rustc-link-lib=c++");

    // The Swift runtime dylibs (libswiftCore, libswift_Concurrency, ...)
    // live only in the dyld shared cache on modern macOS, not as on-disk
    // files — `swiftc`-built binaries reference them by the absolute path
    // `/usr/lib/swift/lib*.dylib`, which dyld resolves specially even
    // without a real file there. Cargo's linker otherwise emits `@rpath`
    // references that never point at this directory, so it must be added
    // explicitly.
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
