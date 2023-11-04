use std::{error, fmt, result};

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Os(crate::backend::OsError),
    AppDropped,
    WindowClosed,
    InsideEventHandler,
    InvalidWindowHandle,
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Os(err) => write!(fmt, "os error: {}", err),
            Error::AppDropped => write!(fmt, "app dropped"),
            Error::WindowClosed => write!(fmt, "window closed"),
            Error::InsideEventHandler => {
                write!(fmt, "operation not supported inside an event handler")
            }
            Error::InvalidWindowHandle => write!(fmt, "invalid window handle"),
        }
    }
}
