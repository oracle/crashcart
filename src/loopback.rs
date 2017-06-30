use libc;
use nix::{Errno, Result};
use std::os::unix::io::RawFd;

const LOOP_MAJOR: u64 = 7;

#[cfg(target_env = "musl")]
const LOOP_SET_FD: libc::c_int = 0x4C00;
#[cfg(target_env = "musl")]
const LOOP_CTL_GET_FREE: libc::c_int = 0x4C82;
#[cfg(not(target_env = "musl"))]
const LOOP_SET_FD: libc::c_ulong = 0x4C00;
#[cfg(not(target_env = "musl"))]
const LOOP_CTL_GET_FREE: libc::c_ulong = 0x4C82;

pub fn loop_ctl_get_free(fd: RawFd) -> Result<i32> {
    let devnr = unsafe { libc::ioctl(fd, LOOP_CTL_GET_FREE) };
    Errno::result(devnr)
}

pub fn loop_set_fd(fd: RawFd, source: RawFd) -> Result<()> {
    let res = unsafe { libc::ioctl(fd, LOOP_SET_FD, source) };
    Errno::result(res).map(drop)
}

pub fn makedev(major: u64, minor: u64) -> u64 {
    (minor & 0xff) | ((major & 0xfff) << 8) | ((minor & !0xff) << 12) | ((major & !0xfff) << 32)
}

pub fn loopdev(devnr: i32) -> u64 {
    makedev(LOOP_MAJOR, devnr as u64)
}
