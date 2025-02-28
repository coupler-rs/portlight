use std::fmt;
use std::marker::PhantomData;

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
        Ok(EventLoop::from_inner(backend::EventLoopInner::new(self)?))
    }
}

#[derive(Clone)]
pub struct EventLoop {
    pub(crate) inner: backend::EventLoopInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl EventLoop {
    pub(crate) fn from_inner(inner: backend::EventLoopInner) -> EventLoop {
        EventLoop {
            inner,
            _marker: PhantomData,
        }
    }

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
        self.inner.run()
    }

    pub fn poll(&self) -> Result<()> {
        self.inner.poll()
    }

    pub fn exit(&self) {
        self.inner.exit();
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
        self.inner.as_raw_fd()
    }
}
