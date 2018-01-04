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
ioctl!(read vt_openqry with VT_IOC_MAGIC, 1; libc::c_int);
ioctl!(write_buf vt_setmode with VT_IOC_MAGIC, 2; VtMode);
ioctl!(write_int vt_reldisp with VT_IOC_MAGIC, 4);
ioctl!(read vt_getmode with VT_IOC_MAGIC, 3; VtMode);
ioctl!(read vt_getindex with VT_IOC_MAGIC, 8; libc::c_int);

const KD_IOC_MAGIC: char = 'K';
const K_RAW: libc::c_int = 0;
const KD_TEXT: libc::c_int = 0;
const KD_GRAPHICS: libc::c_int = 1;
ioctl!(read kdgkbmode with KD_IOC_MAGIC, 6; libc::c_int);
ioctl!(write_int kdskbmode with KD_IOC_MAGIC, 7);
ioctl!(read kdgetmode with KD_IOC_MAGIC, 9; libc::c_int);
ioctl!(write_int kdsetmode with KD_IOC_MAGIC, 10);

pub struct Vt {
    pub tty_fd: RawFd,
    pub vt_num: libc::c_int,
    original_kb_mode: libc::c_int,
}

impl Drop for Vt {
    fn drop(&mut self) {
        unsafe { kdskbmode(self.tty_fd, self.original_kb_mode) }.expect("kdskbmode");
        unsafe { kdsetmode(self.tty_fd, KD_TEXT) }.expect("kdsetmode");
        let mut tios = termios::tcgetattr(self.tty_fd).expect("tcgetattr");
        termios::cfmakesane(&mut tios);
        termios::tcsetattr(self.tty_fd, termios::SetArg::TCSAFLUSH, &tios).expect("tcsetattr");
        let mode = VtMode { mode: VT_AUTO, waitv: 0, relsig: 0, acqsig: 0, frsig: 0 };
        unsafe { vt_setmode(self.tty_fd, &[mode]) }.expect("vt_setmode");
        let _ = unistd::close(self.tty_fd);
    }
}

impl Vt {
    pub fn new(tty_fd: RawFd) -> Vt {
        // vt number is tty number + 1, but get it the proper way anyway
        let mut vt_num = 0;
        unsafe { vt_getindex(tty_fd, &mut vt_num) }.expect("vt_getindex");

        // Set raw mode to mute the console, otherwise everything typed in the compositor
        // could also end up displayed there, including passwords :)
        let mut original_kb_mode = 0;
        unsafe { kdgkbmode(tty_fd, &mut original_kb_mode) }.expect("kdgkbmode");
        unsafe { kdskbmode(tty_fd, K_RAW) }.expect("kdskbmode");
        let mut tios = termios::tcgetattr(tty_fd).expect("tcgetattr");
        termios::cfmakeraw(&mut tios);
        termios::tcsetattr(tty_fd, termios::SetArg::TCSAFLUSH, &tios).expect("tcsetattr");

        // Set graphics mode and take control!
        unsafe { kdsetmode(tty_fd, KD_GRAPHICS) }.expect("kdsetmode");
        let mode = VtMode {
            mode: VT_PROCESS,
            waitv: 0,
            relsig: Signal::SIGUSR1 as libc::c_short,
            acqsig: Signal::SIGUSR2 as libc::c_short,
            frsig: Signal::SIGIO as libc::c_short,
        };
        unsafe { vt_setmode(tty_fd, &[mode]) }.expect("vt_setmode");

        Vt { tty_fd, vt_num, original_kb_mode }
    }

    pub fn ack_release(&self) {
        unsafe { vt_reldisp(self.tty_fd, VT_TRUE) }.expect("vt_reldisp");
    }

    pub fn ack_acquire(&self) {
        unsafe { vt_reldisp(self.tty_fd, VT_ACKACQ) }.expect("vt_reldisp");
    }
}

pub fn open_tty(dev_dir: RawFd, tty_num: libc::c_int) -> nix::Result<RawFd> {
    let tty_num = find_free_tty(dev_dir)?;
    fcntl::openat(
        dev_dir,
        &format!("ttyv{}", tty_num) as &str,
        OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC,
        stat::Mode::empty(),
    )
}

pub fn find_free_tty(dev_dir: RawFd) -> nix::Result<libc::c_int> {
    let tty0 = fcntl::openat(dev_dir, "ttyv0", OFlag::O_RDWR | OFlag::O_NOCTTY | OFlag::O_CLOEXEC, stat::Mode::empty())?;
    let mut vt_num = 0;
    unsafe { vt_openqry(tty0, &mut vt_num) }?;
    unistd::close(tty0)?;
    Ok(vt_num - 1)
}
