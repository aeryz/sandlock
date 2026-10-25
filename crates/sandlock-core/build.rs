mod build_support;

use build_support::{build_static, target_arch, target_compilers};
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let host = std::env::var("HOST").unwrap_or_default();
    let target = std::env::var("TARGET").unwrap_or_default();
    let is_riscv64 = target.starts_with("riscv64");
    let arch = target_arch(&target);

    #[cfg(feature = "checkpoint")]
    {
        // restore-stub: a core component of the restore engine (the supervisor execs
        // it to reconstruct a checkpoint), freestanding, no libc, no PIE. It lives
        // next to the checkpoint code that owns it; its binary is built into OUT_DIR
        // and its path is handed to the crate via the RESTORE_STUB_PATH env var,
        // which embeds it: an installed sandlock has no build tree to exec from.
        //
        // The fixed load address must match `checkpoint::restore_blob::STUB_BASE`:
        // the stub reconstructs the checkpoint's layout around itself, so its own
        // text and stack have to sit outside the address range programs occupy. The
        // default -no-pie base (0x400000) is exactly where a static ET_EXEC workload
        // loads, so the checkpoint's own text would be mapped over the running stub.
        let stub_src = manifest_dir.join("src/checkpoint/restore-stub.c");
        let stub_bin = out_dir.join("restore-stub");
        // Checkpoint restore is claimed only on x86_64 and riscv64 (see
        // `restore_interactive`); on those arches a stub build failure is fatal, not
        // a silent skip — a green build with no stub is how regressions slip past CI.
        let is_restore_arch = target.starts_with("x86_64") || is_riscv64;
        let stub_ccs = target_compilers(&target, &host, arch, &["cc"]);
        let fail_msg = format!(
            "failed to compile restore-stub for {arch}: no working C compiler \
             (tried {}); checkpoint restore is unavailable",
            stub_ccs.join(", "),
        );
        // The link address must match restore_blob::STUB_BASE and must sit below
        // the Sv39 user ceiling (256 GiB) on riscv64.  x86_64 uses 0x300_0000_0000.
        let text_segment = if is_riscv64 {
            "-Wl,-Ttext-segment=0x3000000000"
        } else {
            "-Wl,-Ttext-segment=0x30000000000"
        };
        if !build_static(
            &stub_src,
            &stub_bin,
            &stub_ccs,
            &[
                "-static",
                "-nostdlib",
                "-no-pie",
                // Only PC-relative code can live at the link address below; a
                // compiler that defaults to non-PIE (manylinux's gcc-toolset)
                // emits 32-bit absolute .bss references that cannot reach it.
                "-fPIE",
                "-O2",
                "-ffreestanding",
                "-fno-tree-loop-distribute-patterns",
                // A compiler with default SSP (vanilla GCC; Ubuntu's exempts
                // -ffreestanding) reads the canary at %fs:0x28, and the stub's
                // thread pointer is zero until it restores the checkpoint's.
                "-fno-stack-protector",
                text_segment,
            ],
        ) {
            if is_restore_arch {
                panic!("{fail_msg}");
            }
            println!("cargo:warning={fail_msg}");
        }
        // The crate embeds the stub with include_bytes!, so an arch without restore
        // still needs a file there; the runtime treats an empty stub as unavailable.
        if !stub_bin.exists() {
            std::fs::write(&stub_bin, b"").unwrap();
        }
        // Emit the path every run (rustc-env is not cached across build-script runs),
        // whether or not the binary was just (re)built.
        println!("cargo:rustc-env=RESTORE_STUB_PATH={}", stub_bin.display());
    }

    // shebang-trampoline: runs a chroot script's #! interpreter (see the
    // source). Freestanding, so any compiler for the target arch will do.
    let tramp_src = manifest_dir.join("src/chroot/shebang-trampoline.c");
    let tramp_bin = out_dir.join("shebang-trampoline");
    let tramp_ccs = target_compilers(&target, &host, arch, &["cc"]);
    if !build_static(
        &tramp_src,
        &tramp_bin,
        &tramp_ccs,
        &[
            "-static",
            "-nostdlib",
            "-no-pie",
            "-O2",
            "-ffreestanding",
            "-fno-tree-loop-distribute-patterns",
            "-fno-stack-protector",
        ],
    ) {
        let msg = format!(
            "failed to compile shebang-trampoline for {arch}: no working C compiler \
             (tried {}); #! scripts cannot run under chroot",
            tramp_ccs.join(", "),
        );
        println!("cargo:warning={msg}");
    }
    if !tramp_bin.exists() {
        std::fs::write(&tramp_bin, b"").unwrap();
    }
    println!(
        "cargo:rustc-env=SHEBANG_TRAMPOLINE_PATH={}",
        tramp_bin.display()
    );
}
