//! C compilation shared by the build scripts of sandlock-core and sandlock-fixtures.

use std::path::Path;
use std::process::Command;

/// The target's arch as cross toolchains spell it (riscv64gc compilers are
/// installed as riscv64-*).
pub fn target_arch(target: &str) -> &str {
    let arch = target.split('-').next().unwrap_or_default();
    if arch.starts_with("riscv64") {
        "riscv64"
    } else {
        arch
    }
}

/// Compile `src` to `bin` with the first working compiler in `ccs`, skipping the
/// work when `bin` is newer than both `src` and this build script (the flags
/// live here, so a flag change must recompile). Returns `false` only when the
/// source is present, stale, and no compiler in `ccs` succeeded; a missing
/// source (a packaged crate) or an up-to-date `bin` reports success. The caller
/// decides whether that failure is a hard error or a warning.
pub fn build_static(src: &Path, bin: &Path, ccs: &[String], args: &[&str]) -> bool {
    println!("cargo:rerun-if-changed={}", src.display());
    if !src.exists() {
        return true;
    }
    let mtime = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
    let build_rs = Path::new(env!("CARGO_MANIFEST_DIR")).join("build.rs");
    if let (Some(bt), Some(st), Some(rt)) = (mtime(bin), mtime(src), mtime(&build_rs)) {
        if bt >= st && bt >= rt {
            return true;
        }
    }
    for cc in ccs {
        let mut words = cc.split_whitespace();
        let Some(program) = words.next() else {
            continue;
        };
        let ok = Command::new(program)
            .args(words)
            .args(args)
            .arg("-o")
            .arg(bin)
            .arg(src)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

/// C compilers to try for `target`. A configured one is the only candidate, so a
/// broken setting fails instead of silently falling back to another compiler.
pub fn target_compilers(target: &str, host: &str, arch: &str, native: &[&str]) -> Vec<String> {
    let mut vars = vec![
        format!("CC_{target}"),
        format!("CC_{}", target.replace('-', "_")),
        "TARGET_CC".to_string(),
    ];
    if host == target {
        vars.push("CC".to_string());
    }
    for var in &vars {
        println!("cargo:rerun-if-env-changed={var}");
    }
    if let Some(cc) = vars.iter().find_map(|v| std::env::var(v).ok()) {
        return vec![cc];
    }

    let mut ccs = Vec::new();
    if host.split('-').next() == target.split('-').next() {
        ccs.extend(native.iter().map(|cc| cc.to_string()));
    }
    let env = target.rsplit('-').next().unwrap_or_default();
    for env in [env, "gnu"] {
        for vendor in ["", "unknown-"] {
            let cc = format!("{arch}-{vendor}linux-{env}-gcc");
            if !ccs.contains(&cc) {
                ccs.push(cc);
            }
        }
    }
    ccs
}
