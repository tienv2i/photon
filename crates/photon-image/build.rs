//! Probes system `libopenjp2` and `libheif` via `pkg-config` so `photon-image`
//! can decode JPEG 2000 and HEIC/HEIF natively when headers/libraries are present,
//! while remaining portable on systems that only have CLI decoders or neither.

use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_openjp2)");
    println!("cargo:rustc-check-cfg=cfg(has_libheif)");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    if probe_pkg_config("libopenjp2") {
        println!("cargo:rustc-cfg=has_openjp2");
    }
    if probe_pkg_config("libheif") {
        println!("cargo:rustc-cfg=has_libheif");
    }
}

fn probe_pkg_config(pkg: &str) -> bool {
    let Ok(out) = Command::new("pkg-config").args(["--libs", pkg]).output() else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let flags = String::from_utf8_lossy(&out.stdout);
    for token in flags.split_whitespace() {
        if let Some(lib) = token.strip_prefix("-l") {
            if !lib.is_empty() {
                println!("cargo:rustc-link-lib={lib}");
            }
        } else if let Some(dir) = token.strip_prefix("-L")
            && !dir.is_empty()
        {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }
    true
}
