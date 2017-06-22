extern crate caps;
#[macro_use] extern crate error_chain;
extern crate getopts;
extern crate glob;
extern crate libc;
#[macro_use] extern crate log;
extern crate nix;
#[macro_use] extern crate scopeguard;

mod errors;
mod logger;
mod loopback;

use errors::*;
use getopts::Options;
use glob::glob;
use nix::c_int;
use nix::fcntl::{open, OFlag, O_RDWR, O_CREAT, flock, FlockArg};
use nix::mount::{mount, umount, MS_RDONLY, MsFlags};
use nix::sched::{CloneFlags, CLONE_NEWUSER, CLONE_NEWNET, CLONE_NEWCGROUP};
use nix::sched::{setns, CLONE_NEWNS, CLONE_NEWPID, CLONE_NEWIPC, CLONE_NEWUTS};
use nix::sys::signal::{sigaction, kill};
use nix::sys::signal::{SigAction, SigHandler, SaFlags, SigSet, Signal};
use nix::sys::stat::{mknod, S_IFBLK, Mode, fstat};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{close, fork, ForkResult, execvp, setresgid, setresuid};
use nix::Errno;
use std::env;
use std::fs::{read_link, create_dir_all, remove_file, remove_dir};
use std::fs::{File, canonicalize};
use std::io::prelude::*;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::ffi::CString;


const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options] ID [--] [CMD]", program);
    print!("{}", opts.usage(&brief));
}


fn mount_image(image: &str, link: &str) -> Result<i32> {
    // get free loop device
    let cfd = open("/dev/loop-control", OFlag::empty(), Mode::empty())
        .chain_err(|| "failed to open /dev/loop-control")?;
    let devnr = loopback::loop_ctl_get_free(cfd)
        .chain_err(|| "failed to get free device")?;
    defer!(close(cfd).unwrap());

    // set backing file for loop device to image
    let lp = format!("/dev/loop{}", devnr);
    let lfd = open(&*lp, OFlag::empty(), Mode::empty())
        .chain_err(|| format!("failed to open {}", lp))?;
    defer!(close(lfd).unwrap());

    let ifd = open(image, OFlag::empty(), Mode::empty())
        .chain_err(|| format!("failed to open {}", image))?;
    defer!(close(ifd).unwrap());

    loopback::loop_set_fd(lfd, ifd)
        .chain_err(|| format!("failed to set backing file to {}", image))?;

    symlink(&lp, &link)
        .chain_err(|| format!("failed to symlink from {} to {}", link, lp))?;

    info!("backed /dev/loop{} to {}", devnr, image);
    Ok(devnr)
}

macro_rules! maybe {
    ($e:expr) => (match $e {
        Ok(val) => val,
        Err(_) => {
            return false;
        },
    });
}

fn is_backing(devnr: i32, image: &str) -> bool {
    let path = format!("/sys/block/loop{}/loop/backing_file", devnr);
    let mut f = maybe!(File::open(&path));
    let mut backing = String::new();
    maybe!(f.read_to_string(&mut backing));
    let image_path = maybe!(canonicalize(&image));
    let backing_path = maybe!(canonicalize(&backing.trim()));
    image_path == backing_path
}

fn make_device(image: &str) -> Result<i32> {
    // create lock file
    let lockp = format!("{}.lock", image);
    let lockfd = open(&*lockp,
                      O_RDWR|O_CREAT,
                      Mode::from_bits_truncate(0o644))
        .chain_err(|| format!("failed to open {}", lockp))?;
    defer!(close(lockfd).unwrap());

    flock(lockfd, FlockArg::LockExclusive)
        .chain_err(|| format!("could not get lock on {}", lockp))?;
    defer!(flock(lockfd, FlockArg::Unlock).unwrap());
    defer!(remove_file(&lockp).unwrap());

    let link = format!("{}.link", image);
    match read_link(&link) {
        Ok(m) => {
            let devnr = m.to_str().unwrap()["/dev/loop".len()..]
                .parse::<i32>().unwrap();
            if !is_backing(devnr, image) {
                remove_file(&link)
                    .chain_err(|| format!("could not delete {}", link))?;
                return mount_image(&image, &link);
            };
            info!("{} is backed to /dev/loop{}", image, devnr);
            Ok(devnr)
        },
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("could not read {}", image);
                Err(e).chain_err(|| msg)
            } else {
                mount_image(&image, &link)
            }
        }
    }
}

const PID_GLOBS: &'static [&'static str] = &[
    "/var/run/docker/libcontainerd/containerd/{}*/init/pid",
    "/var/lib/rkt/pods/run/{}*/pid",
];

fn get_pid(id: &str) -> Result<u64> {
    // NOTE: An alternative option for finding docker pids is to find the
    // docker cgroup hierarchy and read the first pid in the tasks file
    // tasks file associated with he container id, for example:
    // /sys/fs/cgroup/memory/docker/*/tasks
    let mut out = id.to_owned();
    let mut pid_file = String::new();
    for entry in PID_GLOBS{
        let results = glob(&entry.replace("{}", id))
            .chain_err(|| format!("invalid glob for id {}", id))?
            .map(|s| s.unwrap().to_str().unwrap().to_string())
            .collect::<Vec<String>>();
        match results.len() {
            0 => (),
            1 => pid_file = results[0].to_owned(),
            _ => bail!("id {} is ambiguous", id),
        }
    }
    if !pid_file.is_empty() {
        info!("Found pid_file at {}", pid_file);
        let mut f = File::open(&pid_file)
            .chain_err(|| format!("could not open {}", pid_file))?;
        out = String::new();
        f.read_to_string(&mut out)
            .chain_err(|| format!("could not read {}", pid_file))?;
    }
    out.parse::<u64>()
        .chain_err(|| format!("{} is not a valid pid", out))
}

const NAMESPACES: &[(CloneFlags, &'static str)] = &[
    (CLONE_NEWIPC, "ipc"),
    (CLONE_NEWUTS, "uts"),
    (CLONE_NEWNET, "net"),
    (CLONE_NEWPID, "pid"),
    (CLONE_NEWNS, "mnt"),
    (CLONE_NEWCGROUP, "cgroup"),
    (CLONE_NEWUSER, "user"),
];

fn enter_namespaces(pid: u64, namespaces: CloneFlags) -> Result<()> {
    let mut to_enter = Vec::new();
    for &(space, name) in NAMESPACES {
        if namespaces.contains(space) {
            debug!("entering {} namespace of {}", name, pid);
            let oldpath = format!("/proc/self/ns/{}", name);
            let oldfd = match open(&*oldpath, OFlag::empty(), Mode::empty()) {
                Err(e) => {
                    if e.errno() == Errno::ENOENT {
                        continue;
                    }
                    let msg = format!("failed to open {}", oldpath);
                    return Err(e).chain_err(|| msg)?;
                },
                Ok(fd) => fd,
            };
            let stat = fstat(oldfd)
                .chain_err(|| "failed to stat")?;
            close(oldfd).unwrap();
            let newpath = format!("/proc/{}/ns/{}", pid, name);
            let fd = match open(&*newpath, OFlag::empty(), Mode::empty()) {
                Err(e) => {
                    if e.errno() == Errno::ENOENT {
                        continue;
                    }
                    let msg = format!("failed to open {}", oldpath);
                    return Err(e).chain_err(|| msg);
                },
                Ok(fd) => fd,
            };
            let nstat = fstat(fd)
                .chain_err(|| "failed to stat")?;
            if stat.st_dev == nstat.st_dev && stat.st_ino == nstat.st_ino {
                close(fd).unwrap();
            } else {
                to_enter.push((space, fd));
            }
        }
    }
    for &(space, fd) in to_enter.iter() {
        setns(fd, space)
            .chain_err(|| "failed to enter")?;
        close(fd).unwrap();
        if space == CLONE_NEWUSER {
            setresgid(0, 0, 0)
                .chain_err(|| "failed to setgid")?;
            setresuid(0, 0, 0)
                .chain_err(|| "failed to setuid")?;
        }
    }
    Ok(())
}

fn enter_mount_ns(pid: u64) -> Result<Box<(Fn() -> Result<()>)>> {
    let origpath = "/proc/self/ns/mnt";
    let ofd = open(origpath, OFlag::empty(), Mode::empty())
        .chain_err(|| format!("failed to open {}", origpath))?;

    // enter ns and return closure to reset
    let cwd = env::current_dir()
        .chain_err(|| "failed to get cwd")?;
    enter_namespaces(pid, CLONE_NEWNS)?;
    Ok(Box::new(move || {
        setns(ofd, CLONE_NEWNS)
            .chain_err(|| "failed to setns")?;
        close(ofd)
            .chain_err(|| format!("failed to close {}", origpath))?;
        env::set_current_dir(&cwd)
            .chain_err(|| "failed to set cwd")?;
        Ok(())
    }))
}

fn enter_pid_ns(pid: u64) -> Result<Box<(Fn() -> Result<()>)>> {
    let origpath = "/proc/self/ns/pid";
    let ofd = open(origpath, OFlag::empty(), Mode::empty())
        .chain_err(|| format!("failed to open {}", origpath))?;

    // enter ns and return closure to reset
    enter_namespaces(pid, CLONE_NEWPID)?;
    Ok(Box::new(move || {
        setns(ofd, CLONE_NEWPID)
            .chain_err(|| "failed to setns")?;
        close(ofd)
            .chain_err(|| format!("failed to close {}", origpath))?;
        Ok(())
    }))
}

fn find_root(path: &str) -> Result<u32> {
    let mut file = match File::open(path) {
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
             return Ok(0);
            }
            let msg = format!("failed to open {}", path);
            return Err(e).chain_err(|| msg);
        },
        Ok(f) => f,
    };
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    for ref line in contents.split("\n") {
        let words: Vec<&str> = line.trim().split_whitespace().collect();
        if words.len() < 2 {
            continue;
        }
        if words[0] == "0" {
            return Ok(words[1].parse::<u32>()
                .chain_err(|| "failed to parse root")?);
        }
    }
    Ok(0)
}

fn set_fsids(pid: u64) -> Result<Box<(Fn() -> ())>> {
    let uid = find_root(&format!{"/proc/{}/uid_map", pid})?;
    let gid = find_root(&format!{"/proc/{}/gid_map", pid})?;
    if uid == 0 && gid == 0 {
        return Ok(Box::new(|| {}));
    }
    // set the filesystem ids
    unsafe {
        libc::setfsgid(gid);
        libc::setfsuid(uid);
    }
    // reset capabilities (to get CAP_MKNOD back)
    let mut all = caps::CapsHashSet::new();
    for c in caps::Capability::iter_variants() {
        all.insert(c);
    }
    caps::set(None, caps::CapSet::Effective, all)
        .chain_err(|| "failed to set capabilities")?;
    Ok(Box::new(|| {
        unsafe {
            libc::setfsgid(0);
            libc::setfsuid(0);
        }
    }))
}

fn is_mounted(path: &str) -> Result<bool> {
    let fd = match open(path, OFlag::empty(), Mode::empty()) {
        Err(e) => {
            if e.errno() == Errno::ENOENT {
                return Ok(false);
            }
            let msg = format!("failed to open {}", path);
            return Err(e).chain_err(|| msg)
        },
        Ok(fd) => fd,
    };
    let stat = fstat(fd)
        .chain_err(|| "failed to stat")?;
    close(fd).unwrap();
    let ppath = match Path::new(path).parent() {
        None => {
            return Ok(false);
        },
        Some(p) => p,
    };
    let pfd = match open(ppath, OFlag::empty(), Mode::empty()) {
        Err(e) => {
            if e.errno() == Errno::ENOENT {
                return Ok(false);
            }
            let msg = format!("failed to open {:?}", ppath);
            return Err(e).chain_err(|| msg);
        },
        Ok(fd) => fd,
    };
    let pstat = fstat(pfd)
        .chain_err(|| "failed to stat")?;
    close(fd).unwrap();
    Ok(stat.st_dev != pstat.st_dev)
}

const CC_LOOP_TMP: &'static str = "/dev/cc-loop";
const CC_MOUNT_PATH: &'static str = "/dev/crashcart";

fn do_mount(pid: u64, image: &str) -> Result<()> {
    let devnr = make_device(&image)?;
    // if we are in a userns, make sure that we have the right fsids
    let reset_fsids = set_fsids(pid)?;
    defer!(reset_fsids());
    let exit_mount_ns = enter_mount_ns(pid)?;
    defer!(exit_mount_ns().unwrap());

    // TODO: if /dev is marked ro, we need to remount it rw. The userns
    //       must be entered first so don't mess up the permissions on
    //       remount. We also need to place a sentinel file so we know
    //       to remount it ro when we teardown the mount.

    // NOTE: the default dev device inside a user namespace can not hold
    //       loopback devices, so we create a new tmpfs mount from the
    //       init_user_ns to hold the device
    if !is_mounted(CC_LOOP_TMP)? {
        create_dir_all(CC_LOOP_TMP)
            .chain_err(|| format!("failed to create {}", CC_LOOP_TMP))?;
        if let Err(e) = mount(Some("tmpfs"),
            CC_LOOP_TMP,
            Some("tmpfs"),
            MsFlags::empty(),
            None::<&str>) {
            if e.errno() != Errno::EBUSY {
                let msg = format!("could not mount tmpfs to {}",
                       CC_LOOP_TMP);
                Err(e).chain_err(|| msg)?;
            }
        }
    }
    let ccimage = format!("{}/loop{}", CC_LOOP_TMP, devnr);
    if let Err(e) = mknod(&*ccimage,
        S_IFBLK,
        Mode::from_bits_truncate(0o660),
        loopback::loopdev(devnr)) {
        if e.errno() != Errno::EEXIST {
            let msg = format!("could not mknod {}", ccimage);
            Err(e).chain_err(|| msg)?;
        }
    }
    if !is_mounted(CC_MOUNT_PATH)? {
        create_dir_all(CC_MOUNT_PATH)
            .chain_err(|| format!("failed to create {}", CC_MOUNT_PATH))?;
        if let Err(e) = mount(Some(&*ccimage),
            CC_MOUNT_PATH,
            Some("ext3"),
            MS_RDONLY,
            None::<&str>) {
            if e.errno() != Errno::EBUSY {
                let msg = format!("could not mount {} to {}",
                       ccimage, CC_MOUNT_PATH);
                Err(e).chain_err(|| msg)?;
            }
        }
    }
    info!("{} is loaded into namespace of pid {}", image, pid);
    Ok(())
}

static mut CHILD_PID: i32 = 0;

extern fn signal_handler(signo: c_int) {
    // the unsafe is due to usage of CHILD_PID, although it is safe to use it
    // as mentioned below.
    unsafe {
        // ignoring errors since it isn't safe to do anything with them in the
        // signal handler.
        let _ = kill(CHILD_PID, Signal::from_c_int(signo).unwrap());
    }
}

const DEFAULT_ARGS: &'static [&'static str] = &[
    "/dev/crashcart/bin/bash",
    "--rcfile",
    "/dev/crashcart/.crashcartrc",
    "-i",
];

fn do_exec(pid: u64, docker_id: &str, args: &[&str]) -> Result<i32> {
    let mut a = args;
    if args.is_empty() {
        a = &DEFAULT_ARGS[..];
    }
    if !docker_id.is_empty() {
        let mut all = Vec::new();
        all.push(CString::new("docker").unwrap());
        all.push(CString::new("exec").unwrap());
        all.push(CString::new("-it").unwrap());
        all.push(CString::new(docker_id.to_string())
                 .chain_err(|| "invalid docker id")?);
        let mut other: Vec<CString> = a.iter()
            .map(|s| CString::new(s.to_string()).unwrap()).collect();
        all.append(&mut other);
        execvp(&all[0], &all)
            .chain_err(|| "failed to exec")?;
    }

    // enter pid namespace before fork
    let exit_pid_ns = enter_pid_ns(pid)?;

    match fork().chain_err(|| "failed to fork")? {
        ForkResult::Child => {
            // enter remaining namespaces
            enter_namespaces(pid,
                CLONE_NEWUSER|CLONE_NEWIPC|CLONE_NEWUTS|
                CLONE_NEWNS|CLONE_NEWCGROUP|CLONE_NEWNET)?;
            // child execs parameters or execs docker_exec
            let all: Vec<CString> = a.iter()
                .map(|s| CString::new(s.to_string()).unwrap()).collect();
            execvp(&all[0], &all)
                .chain_err(|| "failed to exec")?;
            Ok(-1)
        },
        ForkResult::Parent{child} => {
            // parent waits for child to exit, passing along signals
            unsafe {
                // NOTE: the child pid is only set once prior to setting up the
                // signal handler, so it should be safe to access it from the
                // signal handler.
                CHILD_PID = child;
                let a = SigAction::new(SigHandler::Handler(signal_handler),
                                       SaFlags::empty(),
                                       SigSet::all());
                sigaction(Signal::SIGTERM, &a)
                    .chain_err(|| "failed to sigaction")?;
                sigaction(Signal::SIGQUIT, &a)
                    .chain_err(|| "failed to sigaction")?;
                sigaction(Signal::SIGINT, &a)
                    .chain_err(|| "failed to sigaction")?;
                sigaction(Signal::SIGHUP, &a)
                    .chain_err(|| "failed to sigaction")?;
                sigaction(Signal::SIGUSR1, &a)
                    .chain_err(|| "failed to sigaction")?;
                sigaction(Signal::SIGUSR2, &a)
                    .chain_err(|| "failed to sigaction")?;
            }
            let mut exit_code = -1;
            while exit_code == -1 {
                let result = match waitpid(child, None) {
                    Err(e) => {
                        // ignore EINTR as it gets sent when we get a SIGCHLD
                        if e.errno() != Errno::EINTR {
                            let msg = format!("could not waitpid on {}", child);
                            Err(e).chain_err(|| msg)?;
                        }
                        WaitStatus::StillAlive
                    },
                    Ok(result) => result,

                };
                match result {
                    WaitStatus::Exited(_, code) => {
                        exit_code = code as i32
                    },
                    WaitStatus::Signaled(_, signal, _) => {
                        exit_code = signal as i32 + 128
                    },
                    _ => (),
                };
            };
            // reset pid namespace
            exit_pid_ns()
                .chain_err(|| "failed to exit pid ns")?;
            Ok(exit_code)
        },
    }
}

fn do_unmount_ns(pid: u64, devnr: i32) -> Result<()> {
    // if we are in a userns, make sure that we have the right fsids
    let reset_fsids = set_fsids(pid)?;
    defer!(reset_fsids());
    let exit_mount_ns = enter_mount_ns(pid)?;
    defer!(exit_mount_ns().unwrap());

    let ccimage = format!("{}/loop{}", CC_LOOP_TMP, devnr);
    if let Err(e) = umount(CC_MOUNT_PATH) {
        if e.errno() != Errno::ENOENT {
            let msg = format!("could not unmount {} from {}",
                   ccimage, CC_MOUNT_PATH);
            Err(e).chain_err(|| msg)?;
        }
    }
    if let Err(e) = remove_dir(CC_MOUNT_PATH) {
        if e.kind() != std::io::ErrorKind::NotFound {
            let msg = format!("could not delete {}", CC_MOUNT_PATH);
            Err(e).chain_err(|| msg)?;
        }
    }
    if let Err(e) = remove_file(&ccimage) {
        if e.kind() != std::io::ErrorKind::NotFound {
            let msg = format!("could not delete {}", &ccimage);
            Err(e).chain_err(|| msg)?;
        }
    }
    if let Err(e) = umount(CC_LOOP_TMP) {
        if e.errno() != Errno::ENOENT {
            let msg = format!("could not unmount tmpfs from {}",
                   CC_LOOP_TMP);
            Err(e).chain_err(|| msg)?;
        }
    }
    if let Err(e) = remove_dir(CC_LOOP_TMP) {
        if e.kind() != std::io::ErrorKind::NotFound {
            let msg = format!("could not delete {}", CC_LOOP_TMP);
            Err(e).chain_err(|| msg)?;
        }
    }
    Ok(())
}

fn do_unmount(pid: u64, image: &str) -> Result<()> {
    let link = format!("{}.link", image);
    match read_link(&link) {
        Ok(m) => {
            let devnr = m.to_str().unwrap()["/dev/loop".len()..]
                .parse::<i32>().unwrap();
            if is_backing(devnr, &image) {
                do_unmount_ns(pid, devnr)?;
            };
        },
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("could not read {}", image);
                Err(e).chain_err(|| msg)?;
            }
        }
    }
    info!("{} is unloaded from namespace of pid {}", image, pid);
    Ok(())
}

// only show backtrace in debug mode
#[cfg(not(debug_assertions))]
fn print_backtrace(_: &Error) {}

#[cfg(debug_assertions)]
fn print_backtrace(e: &Error) {
    match e.backtrace() {
        Some(backtrace) => error!("{:?}", backtrace),
        None => error!("to view backtrace, use RUST_BACKTRACE=1"),
    }
}

fn main() {
    if let Err(ref e) = run() {
        error!("{}", e);

        for e in e.iter().skip(1) {
            error!("caused by: {}", e);
        }

        print_backtrace(e);
        unsafe {
            if CHILD_PID != 0 {
                kill(CHILD_PID, Signal::SIGTERM).unwrap();
            }
        }
        ::std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let ref program = args[0];

    let mut opts = Options::new();
    opts.optopt("i", "image", "image to mount <crashcart.img>", "IMAGE");
    opts.optflag("h", "help", "display this help and exit");
    opts.optflag("m", "mount", "mount only (do not run command)");
    opts.optflag("e", "exec", "use docker exec instead of setns");
    opts.optflag("u", "unmount", "unmount only (do not run command)");
    opts.optflag("V", "version", "output version information and exit");
    opts.optflag("v", "verbose", "enable more verbose logging");

    let matches = opts.parse(&args[1..])
        .chain_err(|| "unable to parse options")?;

    if matches.opt_present("h") {
        println!("crashcart - mount crashcart image in container");
        println!("");
        print_usage(&program, opts);
        return Ok(());
    }

    if matches.opt_present("V") {
        println!("{} version: {}", program, VERSION.unwrap_or("unknown"));
        return Ok(());
    }

    let mut level = log::LogLevelFilter::Info;

    if matches.opt_present("v") {
        level = log::LogLevelFilter::Debug;
    }

    let _ = log::set_logger(|max_log_level| {
        max_log_level.set(level);
        Box::new(logger::SimpleLogger)
    });

    let image = matches.opt_str("i").unwrap_or("crashcart.img".to_string());

    let id = if !matches.free.is_empty() {
        matches.free[0].clone()
    } else {
        print_usage(&program, opts);
        return Ok(());
    };

    let pid = get_pid(&id)?;
    if !matches.opt_present("u") {
        do_mount(pid, &image)?;
    }

    let mut exit_code = 0;
    if !matches.opt_present("u") && !matches.opt_present("m") {
        let a: Vec<&str> = matches.free.iter().map(AsRef::as_ref).collect();
        let mut docker_id = String::new();
        if matches.opt_present("e") {
            docker_id = id;
        }
        exit_code = do_exec(pid, &docker_id, &a[1..])?;
    }


    if !matches.opt_present("m") {
        do_unmount(pid, &image)?;
    }
    ::std::process::exit(exit_code);
}
