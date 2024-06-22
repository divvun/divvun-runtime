#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::{collections::HashMap, ffi::c_void, os::raw::c_char};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct os_log_s {
    _unused: [u8; 0],
}

pub type os_log_t = *mut os_log_s;
pub type os_log_type_t = u8;

pub const OS_LOG_TYPE_DEFAULT: os_log_type_t = 0;
pub const OS_LOG_TYPE_INFO: os_log_type_t = 1;
pub const OS_LOG_TYPE_DEBUG: os_log_type_t = 2;
pub const OS_LOG_TYPE_ERROR: os_log_type_t = 16;
pub const OS_LOG_TYPE_FAULT: os_log_type_t = 17;

// Provided by the OS.
extern "C" {
    pub fn os_log_create(subsystem: *const c_char, category: *const c_char) -> os_log_t;
    pub fn os_release(object: *mut c_void);
    pub fn os_log_type_enabled(log: os_log_t, level: os_log_type_t) -> bool;
}

// Wrappers defined in wrapper.c because most of the os_log_* APIs are macros.
extern "C" {
    pub fn wrapped_get_default_log() -> os_log_t;
    pub fn wrapped_os_log_with_type(log: os_log_t, log_type: os_log_type_t, message: *const c_char);
    pub fn wrapped_os_log_debug(log: os_log_t, message: *const c_char);
    pub fn wrapped_os_log_info(log: os_log_t, message: *const c_char);
    pub fn wrapped_os_log_default(log: os_log_t, message: *const c_char);
    pub fn wrapped_os_log_error(log: os_log_t, message: *const c_char);
    pub fn wrapped_os_log_fault(log: os_log_t, message: *const c_char);
}

use std::ffi::CString;

#[inline]
fn to_cstr(message: &str) -> CString {
    let fixed = message.replace('\0', "(null)");
    CString::new(fixed).unwrap()
}

#[repr(u8)]
pub enum Level {
    Debug = OS_LOG_TYPE_DEBUG,
    Info = OS_LOG_TYPE_INFO,
    Default = OS_LOG_TYPE_DEFAULT,
    Error = OS_LOG_TYPE_ERROR,
    Fault = OS_LOG_TYPE_FAULT,
}

#[cfg(feature = "logger")]
impl From<log::Level> for Level {
    fn from(other: log::Level) -> Self {
        match other {
            log::Level::Trace => Self::Debug,
            log::Level::Debug => Self::Info,
            log::Level::Info => Self::Default,
            log::Level::Warn => Self::Error,
            log::Level::Error => Self::Fault,
        }
    }
}

pub struct OsLog {
    inner: os_log_t,
}

unsafe impl Send for OsLog {}
unsafe impl Sync for OsLog {}

impl Drop for OsLog {
    fn drop(&mut self) {
        unsafe {
            if self.inner != wrapped_get_default_log() {
                os_release(self.inner as *mut c_void);
            }
        }
    }
}

impl OsLog {
    #[inline]
    pub fn new(subsystem: &str, category: &str) -> Self {
        let subsystem = to_cstr(subsystem);
        let category = to_cstr(category);

        let inner = unsafe { os_log_create(subsystem.as_ptr(), category.as_ptr()) };

        assert!(!inner.is_null(), "Unexpected null value from os_log_create");

        Self { inner }
    }

    #[inline]
    pub fn global() -> Self {
        let inner = unsafe { wrapped_get_default_log() };

        assert!(!inner.is_null(), "Unexpected null value for OS_DEFAULT_LOG");

        Self { inner }
    }

    #[inline]
    pub fn with_level(&self, level: Level, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_with_type(self.inner, level as u8, message.as_ptr()) }
    }

    #[inline]
    pub fn debug(&self, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_debug(self.inner, message.as_ptr()) }
    }

    #[inline]
    pub fn info(&self, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_info(self.inner, message.as_ptr()) }
    }

    #[inline]
    pub fn default(&self, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_default(self.inner, message.as_ptr()) }
    }

    #[inline]
    pub fn error(&self, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_error(self.inner, message.as_ptr()) }
    }

    #[inline]
    pub fn fault(&self, message: &str) {
        let message = to_cstr(message);
        unsafe { wrapped_os_log_fault(self.inner, message.as_ptr()) }
    }

    #[inline]
    pub fn level_is_enabled(&self, level: Level) -> bool {
        unsafe { os_log_type_enabled(self.inner, level as u8) }
    }
}
