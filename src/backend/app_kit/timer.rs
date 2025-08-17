use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};
use std::time::Duration;

use objc2_core_foundation::{
    kCFRunLoopCommonModes, CFAbsoluteTimeGetCurrent, CFRetained, CFRunLoop, CFRunLoopTimer,
    CFRunLoopTimerContext,
};

use crate::{Context, Event, EventLoop, Key, Result, Task};

extern "C-unwind" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const TimerState) };

    info
}

extern "C-unwind" fn release(info: *const c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        unsafe { Rc::decrement_strong_count(info as *const TimerState) };
    }));

    // If a panic occurs while dropping the Rc<TimerState>, the only thing left to do is abort.
    if let Err(_panic) = result {
        std::process::abort();
    }
}

extern "C-unwind" fn callback(_timer: *mut CFRunLoopTimer, info: *mut c_void) {
    let state = unsafe { &*(info as *mut TimerState) };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        state.handle_timer();
    }));

    if let Err(panic) = result {
        state.event_loop.state.propagate_panic(panic);
    }
}

pub struct TimerState {
    timer: Cell<Option<CFRetained<CFRunLoopTimer>>>,
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
            timer: Cell::new(None),
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

        let now = CFAbsoluteTimeGetCurrent();
        let interval = duration.as_secs_f64();

        let timer = unsafe {
            CFRunLoopTimer::new(
                None,
                now + interval,
                interval,
                0,
                0,
                Some(callback),
                &mut context,
            )
        }
        .unwrap();

        let timer_ptr = CFRetained::as_ptr(&timer);
        event_loop_state.timers.timers.borrow_mut().insert(timer_ptr, Rc::clone(&state));

        let run_loop = CFRunLoop::main().unwrap();
        run_loop.add_timer(Some(&timer), unsafe { kCFRunLoopCommonModes });

        state.timer.set(Some(timer));

        Ok(state)
    }

    pub fn cancel(&self) {
        if let Some(timer) = self.timer.take() {
            let timer_ptr = CFRetained::as_ptr(&timer);
            self.event_loop.state.timers.timers.borrow_mut().remove(&timer_ptr);

            timer.invalidate();
        }
    }
}

pub struct Timers {
    timers: RefCell<HashMap<NonNull<CFRunLoopTimer>, Rc<TimerState>>>,
}

impl Timers {
    pub fn new() -> Timers {
        Timers {
            timers: RefCell::new(HashMap::new()),
        }
    }
}
