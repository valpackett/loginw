use std::{mem, ptr, slice};
use std::os::unix::io::RawFd;
use nix::{self, errno, unistd};
use nix::sys::uio::IoVec;
use nix::sys::socket::{recvmsg, sendmsg, CmsgSpace, ControlMessage, MsgFlags};

pub struct Socket {
    pub fd: RawFd,
}

impl Socket {
    pub fn new(fd: RawFd) -> Socket {
        Socket { fd: fd }
    }

    pub fn recvmsg<T>(&self) -> nix::Result<(T, Option<RawFd>)> {
        let mut buf = vec![0u8; mem::size_of::<T>()];
        let iov = [IoVec::from_mut_slice(&mut buf[..])];
        let mut rfd = None;
        let mut cmsgspace: CmsgSpace<[RawFd; 1]> = CmsgSpace::new();
        let msg = recvmsg(self.fd, &iov, Some(&mut cmsgspace), MsgFlags::empty())?;
        if msg.bytes != mem::size_of::<T>() {
            return Err(nix::Error::Sys(errno::Errno::ENOMSG));
        }
        for cmsg in msg.cmsgs() {
            if let ControlMessage::ScmRights(fds) = cmsg {
                if fds.len() >= 1 {
                    rfd = Some(fds[0]);
                }
            }
        }
        Ok((unsafe { ptr::read(iov[0].as_slice().as_ptr() as *const _) }, rfd))
    }

    pub fn sendmsg<T>(&self, data: &T, fd: Option<RawFd>) -> nix::Result<usize> {
        let iov = [IoVec::from_slice(unsafe { slice::from_raw_parts((data as *const T) as *const u8, mem::size_of::<T>()) })];
        if let Some(rfd) = fd {
            sendmsg(self.fd, &iov, &[ControlMessage::ScmRights(&[rfd])], MsgFlags::empty(), None)
        } else {
            sendmsg(self.fd, &iov, &[], MsgFlags::empty(), None)
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unistd::close(self.fd).expect("close");
    }
}
