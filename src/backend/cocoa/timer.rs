use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::rc::Rc;
use std::time::Duration;

use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation::date::CFAbsoluteTimeGetCurrent;
use core_foundation::runloop::*;

use super::event_loop::{EventLoopInner, EventLoopState};
use crate::{EventLoopHandle, Timer, TimerContext};

extern "C" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const TimerState) };

    info
}

extern "C" fn release(info: *const c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        unsafe { Rc::decrement_strong_count(info as *const TimerState) };
    }));

    // If a panic occurs while dropping the Rc<WindowState>, the only thing left to do is abort.
    if let Err(_panic) = result {
        std::process::abort();
    }
}

extern "C" fn callback(_timer: CFRunLoopTimerRef, info: *mut c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let state_rc = unsafe { Rc::from_raw(info as *const TimerState) };
        let state = Rc::clone(&state_rc);
        let _ = Rc::into_raw(state_rc);

        let timer = Timer::from_inner(TimerInner { state });
        let cx = TimerContext::new(&timer.inner.state.event_loop, &timer);
        timer.inner.state.handler.borrow_mut()(&cx);
    }));

    if let Err(panic) = result {
        let state = unsafe { &*(info as *const TimerState) };
        state.event_loop.inner.state.propagate_panic(panic);
    }
}

struct TimerState {
    timer_ref: Cell<Option<CFRunLoopTimerRef>>,
    event_loop: EventLoopHandle,
    handler: RefCell<Box<dyn FnMut(&TimerContext)>>,
}

impl TimerState {
    fn cancel(&self) {
        if let Some(timer_ref) = self.timer_ref.take() {
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

    pub fn set_timer<H>(
        &self,
        event_loop_state: &Rc<EventLoopState>,
        duration: Duration,
        handler: H,
    ) -> TimerInner
    where
        H: FnMut(&TimerContext) + 'static,
    {
        let state = Rc::new(TimerState {
            timer_ref: Cell::new(None),
            event_loop: EventLoopHandle::from_inner(EventLoopInner::from_state(Rc::clone(
                event_loop_state,
            ))),
            handler: RefCell::new(Box::new(handler)),
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

        TimerInner { state }
    }

    pub fn shutdown(&self) {
        for timer in self.timers.take().into_values() {
            timer.cancel();
        }
    }
}

#[derive(Clone)]
pub struct TimerInner {
    state: Rc<TimerState>,
}

impl TimerInner {
    pub fn cancel(&self) {
        if let Some(timer_ref) = self.state.timer_ref.get() {
            self.state.event_loop.inner.state.timers.timers.borrow_mut().remove(&timer_ref);
        }

        self.state.cancel();
    }
}
