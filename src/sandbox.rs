#[cfg(target_os = "freebsd")]
use libc;

#[cfg(target_os = "freebsd")]
pub fn sandbox() {
    if unsafe { cap_enter() } != 0 {
        panic!("cap_enter");
    }
}

#[cfg(not(target_os = "freebsd"))]
pub fn sandbox() {
}

#[cfg(target_os = "freebsd")]
#[link(name = "c")]
extern "C" {
    fn cap_enter() -> libc::c_int;
}
