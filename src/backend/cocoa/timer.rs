use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::rc::{Rc, Weak};
use std::time::Duration;

use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation::date::CFAbsoluteTimeGetCurrent;
use core_foundation::runloop::*;

use crate::{Context, Event, EventLoop, Key, Result, Task};

extern "C" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const TimerState) };

    info
}

extern "C" fn release(info: *const c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        unsafe { Rc::decrement_strong_count(info as *const TimerState) };
    }));

    // If a panic occurs while dropping the Rc<TimerState>, the only thing left to do is abort.
    if let Err(_panic) = result {
        std::process::abort();
    }
}

extern "C" fn callback(_timer: CFRunLoopTimerRef, info: *mut c_void) {
    let state = unsafe { &*(info as *mut TimerState) };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        state.handle_timer();
    }));

    if let Err(panic) = result {
        state.event_loop.state.propagate_panic(panic);
    }
}

pub struct TimerState {
    timer_ref: Cell<Option<CFRunLoopTimerRef>>,
    event_loop: EventLoop,
    handler: Weak<RefCell<dyn Task>>,
    key: Key,
}

impl TimerState {
    fn handle_timer(&self) -> Option<()> {
        let task_ref = self.handler.upgrade()?;
        let mut handler = task_ref.try_borrow_mut().ok()?;
        let cx = Context::new(&self.event_loop, &task_ref);
        handler.event(&cx, self.key, Event::Timer);
        Some(())
    }

    pub fn repeat(duration: Duration, context: &Context, key: Key) -> Result<Rc<TimerState>> {
        let event_loop_state = &context.event_loop.state;

        let state = Rc::new(TimerState {
            timer_ref: Cell::new(None),
            event_loop: context.event_loop.clone(),
            handler: Rc::downgrade(context.task),
            key,
        });

        let mut context = CFRunLoopTimerContext {
            version: 0,
            info: Rc::as_ptr(&state) as *mut c_void,
            retain: Some(retain),
            release: Some(release),
            copyDescription: None,
        };

        let now = unsafe { CFAbsoluteTimeGetCurrent() };
        let interval = duration.as_secs_f64();

        let timer_ref = unsafe {
            CFRunLoopTimerCreate(
                ptr::null(),
                now + interval,
                interval,
                0,
                0,
                callback,
                &mut context,
            )
        };
        state.timer_ref.set(Some(timer_ref));

        event_loop_state.timers.timers.borrow_mut().insert(timer_ref, Rc::clone(&state));

        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            CFRunLoopAddTimer(run_loop, timer_ref, kCFRunLoopCommonModes);
        }

        Ok(state)
    }

    pub fn cancel(&self) {
        if let Some(timer_ref) = self.timer_ref.take() {
            self.event_loop.state.timers.timers.borrow_mut().remove(&timer_ref);

            unsafe {
                CFRunLoopTimerInvalidate(timer_ref);
                CFRelease(timer_ref as CFTypeRef);
            }
        }
    }
}

pub struct Timers {
    timers: RefCell<HashMap<CFRunLoopTimerRef, Rc<TimerState>>>,
}

impl Timers {
    pub fn new() -> Timers {
        Timers {
            timers: RefCell::new(HashMap::new()),
        }
    }
}
