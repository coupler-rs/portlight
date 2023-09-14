use std::ffi::OsStr;
use std::fmt;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{HMODULE, WIN32_ERROR};
use windows_sys::Win32::System::SystemServices::IMAGE_DOS_HEADER;

mod app;
mod timer;
mod window;

pub use app::{AppContextInner, AppInner};
pub use timer::TimerHandleInner;
pub use window::WindowInner;

fn hinstance() -> HMODULE {
    extern "C" {
        static __ImageBase: IMAGE_DOS_HEADER;
    }

    unsafe { &__ImageBase as *const IMAGE_DOS_HEADER as HMODULE }
}

fn to_wstring<S: AsRef<OsStr> + ?Sized>(str: &S) -> Vec<u16> {
    let mut wstr: Vec<u16> = str.as_ref().encode_wide().collect();
    wstr.push(0);
    wstr
}

fn class_name(prefix: &str) -> String {
    use std::fmt::Write;

    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).unwrap();

    let mut name = prefix.to_string();
    for byte in bytes {
        write!(&mut name, "{:x}", byte).unwrap();
    }

    name
}

#[derive(Debug)]
pub struct OsError {
    code: WIN32_ERROR,
}

impl fmt::Display for OsError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.code)
    }
}
