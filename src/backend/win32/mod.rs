use std::ffi::OsStr;
use std::fmt;
use std::os::windows::ffi::OsStrExt;

use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::SystemServices::IMAGE_DOS_HEADER;
use windows::Win32::UI::WindowsAndMessaging::WM_USER;

mod dpi;
mod event_loop;
mod timer;
mod vsync;
mod window;

pub use event_loop::EventLoopInner;
pub use timer::TimerInner;
pub use window::WindowInner;

use crate::Error;

const WM_USER_VBLANK: u32 = WM_USER;

fn hinstance() -> HINSTANCE {
    extern "C" {
        static __ImageBase: IMAGE_DOS_HEADER;
    }

    unsafe { HINSTANCE(&__ImageBase as *const IMAGE_DOS_HEADER as isize) }
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
    error: windows::core::Error,
}

impl fmt::Display for OsError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.error)
    }
}

impl From<windows::core::Error> for Error {
    fn from(err: windows::core::Error) -> Error {
        Error::Os(OsError { error: err })
    }
}
