use std::fmt;
use std::marker::PhantomData;
use std::time::Duration;

use crate::{backend, Context, Key, Result};

pub struct Timer {
    pub(crate) inner: backend::TimerInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Timer {
    pub fn repeat(duration: Duration, context: &Context, key: Key) -> Result<Timer> {
        let inner = backend::TimerInner::repeat(duration, context, key)?;

        Ok(Timer {
            inner,
            _marker: PhantomData,
        })
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.inner.cancel();
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Timer").finish_non_exhaustive()
    }
}
