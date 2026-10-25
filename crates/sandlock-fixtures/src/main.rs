//! Prints fixture paths for the Go and Python suites, which cannot read OUT_DIR.

fn main() {
    println!("{}", sandlock_fixtures::ROOTFS_HELPER);
}
