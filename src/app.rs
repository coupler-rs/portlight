use std::fmt;
use std::marker::PhantomData;
use std::time::Duration;

use crate::{backend, Result, Timer, TimerContext};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AppMode {
    Owner,
    Guest,
}

#[derive(Clone, Debug)]
pub struct AppOptions {
    pub(crate) mode: AppMode,
}

impl Default for AppOptions {
    fn default() -> Self {
        AppOptions {
            mode: AppMode::Owner,
        }
    }
}

impl AppOptions {
    pub fn new() -> AppOptions {
        Self::default()
    }

    pub fn mode(&mut self, mode: AppMode) -> &mut Self {
        self.mode = mode;
        self
    }

    pub fn build(&self) -> Result<App> {
        Ok(App::from_inner(backend::AppInner::new(self)?))
    }
}

pub struct App {
    handle: AppHandle,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl App {
    pub(crate) fn from_inner(inner: backend::AppInner) -> App {
        App {
            handle: AppHandle::from_inner(inner),
            _marker: PhantomData,
        }
    }

    pub fn new() -> Result<App> {
        AppOptions::default().build()
    }

    pub fn handle(&self) -> &AppHandle {
        &self.handle
    }

    pub fn run(&self) -> Result<()> {
        self.handle.inner.run()
    }

    pub fn poll(&self) -> Result<()> {
        self.handle.inner.poll()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.handle.inner.shutdown();
    }
}

impl fmt::Debug for App {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("App").finish_non_exhaustive()
    }
}

#[cfg(target_os = "linux")]
use std::os::unix::io::{AsRawFd, RawFd};

#[cfg(target_os = "linux")]
impl AsRawFd for App {
    fn as_raw_fd(&self) -> RawFd {
        self.handle.inner.as_raw_fd()
    }
}

#[derive(Clone)]
pub struct AppHandle {
    pub(crate) inner: backend::AppInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl AppHandle {
    pub(crate) fn from_inner(inner: backend::AppInner) -> AppHandle {
        AppHandle {
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

impl fmt::Debug for AppHandle {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("AppHandle").finish_non_exhaustive()
    }
}
