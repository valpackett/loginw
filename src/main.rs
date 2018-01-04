extern crate libc;
#[macro_use]
extern crate nix;

mod protocol;
mod socket;
mod vt;

use std::{env, str};
use std::io::Write;
use std::ffi::{CStr, OsString};
use std::process::Command;
use std::os::unix::process::CommandExt;
use std::os::unix::io::RawFd;
use nix::{errno, unistd};
use nix::fcntl::{self, OFlag};
use nix::sys::stat;
use nix::sys::event::*;
use nix::sys::signal::*;
use nix::sys::socket::{socketpair, AddressFamily, SockFlag, SockType};
use protocol::*;
use socket::*;

struct Loginw {
    kq: RawFd,
    dev_dir: RawFd,
    child_pd: RawFd,
    sock: Socket,
    vt: Option<vt::Vt>,
    input_devs: Vec<RawFd>,
    drm_dev: Option<RawFd>,
}

impl Drop for Loginw {
    fn drop(&mut self) {
        if let Some(drm_dev) = self.drm_dev {
            unsafe { drmDropMaster(drm_dev) };
            let _ = unistd::close(drm_dev);
        }
        let _ = unistd::close(self.kq);
        let _ = unistd::close(self.dev_dir);
    }
}

impl Loginw {
    fn new(sock: Socket, child_pd: RawFd) -> Loginw {
        Loginw {
            kq: kqueue().expect("kqueue"),
            dev_dir: fcntl::open("/dev", OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK, stat::Mode::empty())
                .expect("open"),
            child_pd,
            sock,
            vt: None,
            input_devs: Vec::new(),
            drm_dev: None,
        }
    }

    fn send_str(&mut self, typ: LoginwResponseType, e: &str) {
        let mut resp = LoginwResponse::new(typ);
        write!(unsafe { &mut resp.dat.bytes[..] }, "{}", e).expect("write!");
        self.sock.sendmsg(&resp, None).expect(".sendmsg");
    }

    fn process(&mut self, mut req: LoginwRequest) {
        let last = unsafe { req.dat.bytes }.len() - 1;
        unsafe {
            req.dat.bytes[last] = 0;
        } // ensure CStr doesn't overread
        let dat_str = unsafe { CStr::from_ptr(&req.dat.bytes[0] as *const u8 as *const _) }
            .to_str()
            .expect("to_str");
        match req.typ {
            LoginwRequestType::LoginwOpenInput => {
                if !dat_str.starts_with("/dev/input") {
                    self.send_str(LoginwResponseType::LoginwError, &format!("Not an input device path: {}", dat_str));
                    return;
                }
                match fcntl::openat(
                    self.dev_dir,
                    &dat_str.replace("/dev/", "") as &str,
                    OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
                    stat::Mode::empty(),
                ) {
                    Ok(rfd) => {
                        self.input_devs.push(rfd);
                        self.sock
                            .sendmsg(&LoginwResponse::new(LoginwResponseType::LoginwPassedFd), Some(rfd))
                            .expect(".sendmsg");
                    },
                    Err(e) => {
                        self.send_str(LoginwResponseType::LoginwError, &format!("{:?}", e));
                    },
                }
            },
            LoginwRequestType::LoginwOpenDrm => {
                if self.drm_dev.is_some() {
                    println!("WARNING: opening more than one DRM device");
                }
                if !(dat_str.starts_with("/dev/dri") || dat_str.starts_with("/dev/drm")) {
                    self.send_str(LoginwResponseType::LoginwError, &format!("Not a DRM device path: {}", dat_str));
                    return;
                }
                match fcntl::openat(
                    self.dev_dir,
                    &dat_str.replace("/dev/", "") as &str,
                    OFlag::O_RDWR | OFlag::O_CLOEXEC | OFlag::O_NOCTTY | OFlag::O_NONBLOCK,
                    stat::Mode::empty(),
                ) {
                    Ok(rfd) => {
                        self.drm_dev = Some(rfd);
                        self.sock
                            .sendmsg(&LoginwResponse::new(LoginwResponseType::LoginwPassedFd), Some(rfd))
                            .expect(".sendmsg");
                    },
                    Err(e) => {
                        self.send_str(LoginwResponseType::LoginwError, &format!("{:?}", e));
                    },
                }
            },
            LoginwRequestType::LoginwAcquireVt => {
                if self.vt.is_none() {
                    let tty_num = vt::find_free_tty(self.dev_dir).expect("find_free_tty");
                    let tty_fd = vt::open_tty(self.dev_dir, tty_num).expect("open_tty");
                    self.vt = Some(vt::Vt::new(tty_fd));
                }
                if let Some(ref vt) = self.vt {
                    let mut resp = LoginwResponse::new(LoginwResponseType::LoginwPassedFd);
                    resp.dat.u64 = vt.vt_num as u64;
                    self.sock.sendmsg(&resp, Some(vt.tty_fd)).expect(".sendmsg");
                }
            },
            _ => println!("not implemented: {:?}", req.typ),
        }
    }

    fn on_sock_event(&mut self) -> bool {
        match self.sock.recvmsg::<LoginwRequest>() {
            Ok((req, _)) => self.process(req),
            Err(nix::Error::Sys(errno::Errno::ENOMSG)) => {
                // Other end died
                return false;
            },
            Err(e) => panic!("recvmsg: {}", e),
        }
        return true;
    }

    fn on_signal_event(&mut self, signal: Signal) -> bool {
        match signal {
            Signal::SIGTERM | Signal::SIGINT => {
                let _ = unsafe { libc::pdkill(self.child_pd, signal as libc::c_int) };
            },
            Signal::SIGUSR1 => {
                self.sock
                    .sendmsg(&LoginwResponse::new(LoginwResponseType::LoginwDeactivated), None)
                    .expect(".sendmsg");
                for fd in self.input_devs.iter() {
                    let _ = unistd::close(*fd);
                }
                if let Some(drm_dev) = self.drm_dev {
                    unsafe { drmDropMaster(drm_dev) };
                }
                if let Some(ref vt) = self.vt {
                    vt.ack_release();
                }
            },
            Signal::SIGUSR2 => {
                if let Some(ref vt) = self.vt {
                    vt.ack_acquire();
                }
                if let Some(drm_dev) = self.drm_dev {
                    unsafe { drmSetMaster(drm_dev) };
                }
                self.sock
                    .sendmsg(&LoginwResponse::new(LoginwResponseType::LoginwActivated), None)
                    .expect(".sendmsg");
            },
            _ => {},
        }
        return true;
    }

    fn on_proc_event(&mut self, exit_status: libc::c_int) -> bool {
        return false;
    }

    fn mainloop(&mut self) {
        let add = EventFlag::EV_ADD | EventFlag::EV_ENABLE;
        let filt = FilterFlag::empty();
        kevent(
            self.kq,
            &vec![
                KEvent::new(self.sock.fd as usize, EventFilter::EVFILT_READ, add, filt, 0, 0),
                KEvent::new(self.child_pd as usize, EventFilter::EVFILT_PROCDESC, add, filt, 0, 0),
                KEvent::new(Signal::SIGINT as usize, EventFilter::EVFILT_SIGNAL, add, filt, 0, 0),
                KEvent::new(Signal::SIGTERM as usize, EventFilter::EVFILT_SIGNAL, add, filt, 0, 0),
                KEvent::new(Signal::SIGUSR1 as usize, EventFilter::EVFILT_SIGNAL, add, filt, 0, 0),
                KEvent::new(Signal::SIGUSR2 as usize, EventFilter::EVFILT_SIGNAL, add, filt, 0, 0),
            ],
            &mut vec![],
            0,
        ).expect("kevent");
        let mut sigs = SigSet::empty();
        sigs.add(Signal::SIGINT);
        sigs.add(Signal::SIGTERM);
        sigs.add(Signal::SIGUSR1);
        sigs.add(Signal::SIGUSR2);
        let _ign = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), sigs);
        loop {
            let mut eventlist = vec![KEvent::new(0, EventFilter::EVFILT_READ, EventFlag::empty(), FilterFlag::empty(), 0, 0)];
            kevent_ts(self.kq, &vec![], &mut eventlist, None).expect("kevent");
            match eventlist[0].filter() {
                EventFilter::EVFILT_READ => if !self.on_sock_event() {
                    break;
                },
                EventFilter::EVFILT_SIGNAL => if !self.on_signal_event(
                    Signal::from_c_int(eventlist[0].data() as libc::c_int).expect("signal from_c_int"),
                ) {
                    break;
                },
                EventFilter::EVFILT_PROCDESC => if !self.on_proc_event(eventlist[0].data() as libc::c_int) {
                    break;
                },
                _ => {},
            }
        }
    }
}

fn main() {
    let args: Vec<OsString> = env::args_os().collect();
    if args.len() < 2 {
        panic!("No args");
    }
    let (sock_parent, sock_child) =
        socketpair(AddressFamily::Unix, SockType::SeqPacket, None, SockFlag::empty()).expect("socketpair()");
    let mut child_pd = -1;
    let pid = unsafe { libc::pdfork(&mut child_pd, 0) };
    if pid < 0 {
        panic!("pdfork");
    } else if pid > 0 {
        // Parent
        drop(Socket::new(sock_child));
        let mut server = Loginw::new(Socket::new(sock_parent), child_pd);
        if unsafe { cap_enter() } != 0 {
            panic!("cap_enter");
        }
        server.mainloop();
    } else {
        // Child
        drop(Socket::new(sock_parent));
        Command::new(&args[1])
            .args(&args[2..])
            .uid(libc::uid_t::from(unistd::getuid()) as _)
            .gid(libc::uid_t::from(unistd::getgid()) as _)
            .env("LOGINW_FD", format!("{}", sock_child))
            .exec();
    }
}

#[link(name = "c")]
extern "C" {
    fn cap_enter() -> libc::c_int;
}

#[link(name = "drm")]
extern "C" {
    fn drmSetMaster(fd: RawFd) -> libc::c_int;
    fn drmDropMaster(fd: RawFd) -> libc::c_int;
}
