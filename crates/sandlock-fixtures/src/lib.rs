/// A static busybox-style binary that chroot tests drop into a rootfs.
pub const ROOTFS_HELPER: &str = concat!(env!("OUT_DIR"), "/rootfs-helper");
