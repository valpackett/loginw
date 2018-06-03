use libc;

#[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
pub fn make_realtime() -> bool {
    unsafe {
        let mut rtp = libc::rtprio {
            type_: libc::RTP_PRIO_REALTIME,
            prio: 1,
        };
        libc::rtprio(libc::RTP_SET, libc::getpid(), &mut rtp) != 0
    }
}

#[allow(dead_code)]
fn as_u64(x: u8) -> u64 { x as u64 }

#[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
pub fn make_normal() -> bool {
    unsafe {
        let mut rtp = libc::rtprio {
            type_: libc::RTP_PRIO_NORMAL,
            prio: 0,
        };
        libc::rtprio(libc::RTP_SET, libc::getpid(), &mut rtp) != 0
    }
}

#[cfg(not(any(target_os = "freebsd", target_os = "dragonfly")))]
pub fn make_realtime() -> bool {
    true
}

#[cfg(not(any(target_os = "freebsd", target_os = "dragonfly")))]
pub fn make_normal() -> bool {
    true
}
