use std::os::unix::io::RawFd;
use libc;
use nix::{self, unistd};
use nix::sys::{stat, termios};
use nix::sys::signal::Signal;
use nix::fcntl::{self, OFlag};

#[repr(C)]
pub struct VtMode {
    /// switching controlled by: auto/process/kernel
    mode: libc::c_char,
    /// Not implemented
    waitv: libc::c_char,
    /// Release signal
    relsig: libc::c_short,
    /// Acquire signal
    acqsig: libc::c_short,
    /// Not implemented, but HAS TO BE SET
    frsig: libc::c_short,
}

const VT_IOC_MAGIC: char = 'v';
const VT_AUTO: libc::c_char = 0;
const VT_PROCESS: libc::c_char = 1;
const VT_TRUE: libc::c_int = 1;
const VT_ACKACQ: libc::c_int = 2;
ioctl_read!(vt_openqry, VT_IOC_MAGIC, 1, libc::c_int);
ioctl_write_buf!(vt_setmode, VT_IOC_MAGIC, 2, VtMode);
ioctl_write_int!(vt_reldisp, VT_IOC_MAGIC, 4);
ioctl_write_int!(vt_activate, VT_IOC_MAGIC, 5);
ioctl_write_int!(vt_waitactive, VT_IOC_MAGIC, 6);
ioctl_read!(vt_getmode, VT_IOC_MAGIC, 3, VtMode);
ioctl_read!(vt_getactive, VT_IOC_MAGIC, 7, libc::c_int);
ioctl_read!(vt_getindex, VT_IOC_MAGIC, 8, libc::c_int);

const KD_IOC_MAGIC: char = 'K';
const K_RAW: libc::c_int = 0;
const KD_TEXT: libc::c_int = 0;
const KD_GRAPHICS: libc::c_int = 1;
ioctl_read!(kdgkbmode, KD_IOC_MAGIC, 6, libc::c_int);
ioctl_write_int!(kdskbmode, KD_IOC_MAGIC, 7);
ioctl_read!(kdgetmode, KD_IOC_MAGIC, 9, libc::c_int);
ioctl_write_int!(kdsetmode, KD_IOC_MAGIC, 10);

pub struct Vt {
    pub tty_fd: RawFd,
    pub vt_num: libc::c_int,
    original_kb_mode: libc::c_int,
    original_vt_num: libc::c_int,
}

impl Drop for Vt {
    fn drop(&mut self) {
        debug!("setting kbd original mode {}", self.original_kb_mode);
        unsafe { kdskbmode(self.tty_fd, self.original_kb_mode) }.expect("kdskbmode");
        debug!("setting text mode");
        unsafe { kdsetmode(self.tty_fd, KD_TEXT) }.expect("kdsetmode");
        debug!("setting termios sane mode");
        let mut tios = termios::tcgetattr(self.tty_fd).expect("tcgetattr");
        termios::cfmakesane(&mut tios);
        termios::tcsetattr(self.tty_fd, termios::SetArg::TCSAFLUSH, &tios).expect("tcsetattr");
        let mode = VtMode { mode: VT_AUTO, waitv: 0, relsig: 0, acqsig: 0, frsig: 0 };
        debug!("setting vt mode");
        unsafe { vt_setmode(self.tty_fd, &[mode]) }.expect("vt_setmode");
        switch_to(self.tty_fd, self.original_vt_num);
        let _ = unistd::close(self.tty_fd);
    }
}

impl Vt {
    pub fn new(tty_fd: RawFd) -> Vt {
        // vt number is tty number + 1, but get it the proper way anyway
        let mut vt_num = 0;
        unsafe { vt_getindex(tty_fd, &mut vt_num) }.expect("vt_getindex");
        info!("VT index: {}", vt_num);

        // Set raw mode to mute the console, otherwise everything typed in the compositor
        // could also end up displayed there, including passwords :)
        let mut original_kb_mode = -1;
        unsafe { kdgkbmode(tty_fd, &mut original_kb_mode) }.expect("kdgkbmode");
        debug!("VT original kb mode: {}", original_kb_mode);
        debug!("setting kbd raw mode");
        unsafe { kdskbmode(tty_fd, K_RAW) }.expect("kdskbmode");
        debug!("setting termios raw mode");
        let mut tios = termios::tcgetattr(tty_fd).expect("tcgetattr");
        termios::cfmakeraw(&mut tios);
        termios::tcsetattr(tty_fd, termios::SetArg::TCSAFLUSH, &tios).expect("tcsetattr");

        // Set graphics mode and take control!
        debug!("setting graphics mode");
        unsafe { kdsetmode(tty_fd, KD_GRAPHICS) }.expect("kdsetmode");
        let mode = VtMode {
            mode: VT_PROCESS,
            waitv: 0,
            relsig: Signal::SIGUSR1 as libc::c_short,
            acqsig: Signal::SIGUSR1 as libc::c_short,
            frsig: Signal::SIGIO as libc::c_short,
        };
        debug!("setting vt mode");
        unsafe { vt_setmode(tty_fd, &[mode]) }.expect("vt_setmode");

        let mut original_vt_num = 0;
        unsafe { vt_getactive(tty_fd, &mut original_vt_num) }.expect("vt_getactive");
        debug!("old active vt number: {}", original_vt_num);
        switch_to(tty_fd, vt_num);

        Vt { tty_fd, vt_num, original_kb_mode, original_vt_num }
    }

    pub fn ack_release(&self) {
        debug!("acknowledging vt release");
        unsafe { vt_reldisp(self.tty_fd, VT_TRUE) }.expect("vt_reldisp");
    }

    pub fn ack_acquire(&self) {
        debug!("acknowledging vt acquire");
        unsafe { vt_reldisp(self.tty_fd, VT_ACKACQ) }.expect("vt_reldisp");
    }
}

fn switch_to(tty_fd: RawFd, vt_num: libc::c_int) {
    debug!("activating vt {}", vt_num);
    unsafe { vt_activate(tty_fd, vt_num) }.expect("vt_activate");
    debug!("waiting for vt {} activation", vt_num);
    unsafe { vt_waitactive(tty_fd, vt_num) }.expect("vt_waitactive");
}

pub fn open_tty(dev_dir: RawFd, tty_num: libc::c_int) -> nix::Result<RawFd> {
    debug!("opening ttyv{}", tty_num);
    fcntl::openat(
        dev_dir,
        &format!("ttyv{}", tty_num) as &str,
        OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC,
        stat::Mode::empty(),
    )
}

pub fn find_free_tty(dev_dir: RawFd) -> nix::Result<libc::c_int> {
    debug!("finding free tty");
    let tty0 = fcntl::openat(dev_dir, "ttyv0", OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC, stat::Mode::empty())?;
    let mut vt_num = 0;
    unsafe { vt_openqry(tty0, &mut vt_num) }?;
    debug!("found free vt {} (tty {})", vt_num, vt_num - 1);
    unistd::close(tty0)?;
    Ok(vt_num - 1)
}
