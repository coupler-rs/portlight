use std::fmt;

mod display_links;
mod event_loop;
mod ffi;
mod surface;
mod timer;
mod window;

pub use event_loop::EventLoopState;
pub use timer::TimerState;
pub use window::WindowState;

#[derive(Debug)]
pub enum OsError {
    Other(&'static str),
}

impl fmt::Display for OsError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OsError::Other(err) => write!(fmt, "{}", err),
        }
    }
}
