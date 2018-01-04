// cbindgen can't add a prefix to everything, so we have Loginw* names here :(

#[repr(C)]
pub union LoginwData {
    pub bytes: [u8; 128],
    pub u64: u64,
    pub boolean: bool,
}

impl Default for LoginwData {
    fn default() -> LoginwData {
        LoginwData { bytes: [0; 128] }
    }
}

#[repr(u16)]
#[derive(Debug, PartialEq)]
pub enum LoginwRequestType {
    /// bytes -> fd -- Open an input (evdev) device fd (by full path)
    LoginwOpenInput = 0,
    /// bytes -> fd -- Open a DRM device fd (by full path)
    LoginwOpenDrm = 1,

    /// void -> u64 + fd -- Initialize a new virtual terminal, returns vt number and passes tty fd
    LoginwAcquireVt = 100,
    /// uint -> void -- Switch to a given virtual terminal (by number)
    LoginwSwitchVt = 101,

    /// void -> void -- Shuts down the machine
    LoginwPowerOff = 200,
    /// void -> void -- Reboots the machine
    LoginwReboot = 201,
    /// void -> void -- Suspends the machine
    LoginwSuspend = 202,
    /// void -> void -- Hibernates the machine
    LoginwHibernate = 203,

    /// void -> boolean -- Checks whether suspending is possible
    LoginwCanSuspend = 302,
    /// void -> boolean -- Checks whether hibernation is possible
    LoginwCanHibernate = 303,
}

#[repr(C)]
pub struct LoginwRequest {
    pub typ: LoginwRequestType,
    pub dat: LoginwData,
}

impl LoginwRequest {
    pub fn new(typ: LoginwRequestType) -> LoginwRequest {
        LoginwRequest { typ, dat: LoginwData::default() }
    }
}

#[repr(u16)]
#[derive(Debug, PartialEq)]
pub enum LoginwResponseType {
    LoginwError = 0,
    LoginwDone = 1,
    LoginwPassedFd = 2,

    // Notifications (not actually responses)
    LoginwActivated = 100,
    LoginwDeactivated = 101,
}

#[repr(C)]
pub struct LoginwResponse {
    pub typ: LoginwResponseType,
    pub dat: LoginwData,
}

impl LoginwResponse {
    pub fn new(typ: LoginwResponseType) -> LoginwResponse {
        LoginwResponse { typ, dat: LoginwData::default() }
    }
}

#[allow(private_no_mangle_fns)]
#[no_mangle]
pub extern "C" fn _cbindgen_helper(
    _a: LoginwData,
    _b: LoginwRequestType,
    _c: LoginwRequest,
    _d: LoginwResponseType,
    _e: LoginwResponse,
) {
}
