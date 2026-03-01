use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;

use crate::{backend, EventLoop, Result};

pub struct Timer {
    pub(crate) state: Rc<backend::TimerState>,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Timer {
    pub fn repeat<F>(event_loop: &EventLoop, duration: Duration, handler: F) -> Result<Timer>
    where
        F: FnMut() + 'static,
    {
        let state = backend::TimerState::repeat(event_loop, duration, handler)?;

        Ok(Timer {
            state,
            _marker: PhantomData,
        })
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.state.cancel();
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Timer").finish_non_exhaustive()
    }
}
