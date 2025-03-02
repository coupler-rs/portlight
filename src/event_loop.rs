use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::{backend, Result, Task, TaskHandle};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EventLoopMode {
    Owner,
    Guest,
}

#[derive(Clone, Debug)]
pub struct EventLoopOptions {
    pub(crate) mode: EventLoopMode,
}

impl Default for EventLoopOptions {
    fn default() -> Self {
        EventLoopOptions {
            mode: EventLoopMode::Owner,
        }
    }
}

impl EventLoopOptions {
    pub fn new() -> EventLoopOptions {
        Self::default()
    }

    pub fn mode(&mut self, mode: EventLoopMode) -> &mut Self {
        self.mode = mode;
        self
    }

    pub fn build(&self) -> Result<EventLoop> {
        Ok(EventLoop {
            state: backend::EventLoopState::new(self)?,
            _marker: PhantomData,
        })
    }
}

#[derive(Clone)]
pub struct EventLoop {
    pub(crate) state: Rc<backend::EventLoopState>,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl EventLoop {
    pub fn new() -> Result<EventLoop> {
        EventLoopOptions::default().build()
    }

    pub fn spawn<T>(&self, task: T) -> TaskHandle<T>
    where
        T: Task + 'static,
    {
        TaskHandle::spawn(&self, task)
    }

    pub fn run(&self) -> Result<()> {
        self.state.run()
    }

    pub fn poll(&self) -> Result<()> {
        self.state.poll()
    }

    pub fn exit(&self) {
        self.state.exit();
    }
}

impl fmt::Debug for EventLoop {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventLoop").finish_non_exhaustive()
    }
}

#[cfg(target_os = "linux")]
use std::os::unix::io::{AsRawFd, RawFd};

#[cfg(target_os = "linux")]
impl AsRawFd for EventLoop {
    fn as_raw_fd(&self) -> RawFd {
        self.state.as_raw_fd()
    }
}
