mod backend;
mod error;
mod event_loop;
mod task;
mod timer;
mod window;

#[cfg(feature = "_test")]
pub mod tests;

pub use error::{Error, Result};
pub use event_loop::{EventLoop, EventLoopMode, EventLoopOptions};
pub use task::{Context, Event, Key, Response, Task, TaskHandle};
pub use timer::Timer;
pub use window::{
    Bitmap, Cursor, MouseButton, Point, RawWindow, Rect, Size, Window, WindowEvent, WindowOptions,
};
