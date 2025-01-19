use std::fmt;
use std::marker::PhantomData;
use std::time::Duration;

use crate::{backend, Result, Timer, TimerContext};

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

pub struct EventLoop {
    handle: EventLoopHandle,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl EventLoop {
    pub(crate) fn from_inner(inner: backend::EventLoopInner) -> EventLoop {
        EventLoop {
            handle: EventLoopHandle::from_inner(inner),
            _marker: PhantomData,
        }
    }

    pub fn new() -> Result<EventLoop> {
        EventLoopOptions::default().build()
    }

    pub fn handle(&self) -> &EventLoopHandle {
        &self.handle
    }

    pub fn run(&self) -> Result<()> {
        self.handle.inner.run()
    }

    pub fn poll(&self) -> Result<()> {
        self.handle.inner.poll()
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        self.handle.inner.shutdown();
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
        self.handle.inner.as_raw_fd()
    }
}

#[derive(Clone)]
pub struct EventLoopHandle {
    pub(crate) inner: backend::EventLoopInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl EventLoopHandle {
    pub(crate) fn from_inner(inner: backend::EventLoopInner) -> EventLoopHandle {
        EventLoopHandle {
            inner,
            _marker: PhantomData,
        }
    }

    pub fn set_timer<H>(&self, duration: Duration, handler: H) -> Result<Timer>
    where
        H: FnMut(&TimerContext) + 'static,
    {
        Ok(Timer::from_inner(self.inner.set_timer(duration, handler)?))
    }

    pub fn exit(&self) {
        self.inner.exit();
    }
}

impl fmt::Debug for EventLoopHandle {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventLoopHandle").finish_non_exhaustive()
    }
}
