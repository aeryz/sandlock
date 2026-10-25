#[path = "../sandlock-core/build_support.rs"]
mod build_support;

use build_support::{build_static, target_arch, target_compilers};
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();

    if !build_static(
        &manifest_dir.join("rootfs-helper.c"),
        &out_dir.join("rootfs-helper"),
        &target_compilers(&target, &host, target_arch(&target), &["musl-gcc", "cc"]),
        &["-static", "-O2"],
    ) {
        println!(
            "cargo:warning=cannot compile rootfs-helper: chroot tests will \
             fail. Install musl-tools or static libc."
        );
    }
}
