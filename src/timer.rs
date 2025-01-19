use std::fmt;
use std::marker::PhantomData;

use crate::{backend, EventLoopHandle};

pub struct TimerContext<'a> {
    event_loop: &'a EventLoopHandle,
    timer: &'a Timer,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl<'a> TimerContext<'a> {
    pub(crate) fn new(event_loop: &'a EventLoopHandle, timer: &'a Timer) -> TimerContext<'a> {
        TimerContext {
            event_loop,
            timer,
            _marker: PhantomData,
        }
    }

    pub fn event_loop(&self) -> &EventLoopHandle {
        self.event_loop
    }

    pub fn timer(&self) -> &Timer {
        self.timer
    }
}

#[derive(Clone)]
pub struct Timer {
    pub(crate) inner: backend::TimerInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Timer {
    pub(crate) fn from_inner(inner: backend::TimerInner) -> Timer {
        Timer {
            inner,
            _marker: PhantomData,
        }
    }

    pub fn cancel(&self) {
        self.inner.cancel();
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Timer").finish_non_exhaustive()
    }
}
