extern crate loginw;
extern crate tiny_nix_ipc;
extern crate libc;
#[macro_use]
extern crate nix;

use std::{env, str, thread};
use std::ffi::CStr;
use std::io::Write;
use std::time::Duration;
use std::os::unix::io::{RawFd, FromRawFd};

use tiny_nix_ipc::Socket;
use loginw::protocol::*;

const EVDEV_IOC_MAGIC: char = 'E';
const EVDEV_IOC_GNAME: u8 = 0x06;
ioctl_read_buf!(evdev_name, EVDEV_IOC_MAGIC, EVDEV_IOC_GNAME, u8);

fn main() {
    let fd = env::var("LOGINW_FD").expect("No LOGINW_FD, launch under loginw");
    let mut sock = unsafe { Socket::from_raw_fd(fd.parse::<RawFd>().expect("parse::<RawFd>()")) };
    let mut req = LoginwRequest::new(LoginwRequestType::LoginwOpenInput);
    write!(unsafe { &mut req.dat.bytes[..] }, "/dev/input/event0").expect("write!()");
    sock.send_struct(&req, None).expect(".sendmsg()");
    let (resp, event0fd) = sock.recv_struct::<LoginwResponse, [RawFd; 1]>().expect(".recvmsg()");
    assert!(resp.typ == LoginwResponseType::LoginwPassedFd);
    let mut name_buf = [0u8; 128];
    println!("Read {} bytes from ioctl", unsafe { evdev_name(event0fd.unwrap()[0], &mut name_buf[..]).unwrap() });
    let name_str = unsafe { CStr::from_ptr(&name_buf[0] as *const u8 as *const _) };
    println!("/dev/input/event0 is a '{}'", str::from_utf8(name_str.to_bytes()).expect("from_utf8()"));
    let user_info = unsafe { &*libc::getpwuid(libc::getuid()) };
    println!("running as uid {} gid {}", user_info.pw_uid, user_info.pw_gid);
    thread::sleep(Duration::from_secs(2));
}
