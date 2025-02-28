use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::event_loop::EventLoopState;
use crate::{Context, Event, EventLoop, Key, Result, Task};

struct TimerState {
    timer_id: Cell<Option<usize>>,
    event_loop_state: Rc<EventLoopState>,
    handler: Weak<RefCell<dyn Task>>,
    key: Key,
}

impl TimerState {
    fn cancel(&self) {
        if let Some(timer_id) = self.timer_id.take() {
            let _ = unsafe { KillTimer(self.event_loop_state.message_hwnd, timer_id) };
        }
    }
}

pub struct Timers {
    next_id: Cell<usize>,
    timers: RefCell<HashMap<usize, Rc<TimerState>>>,
}

impl Timers {
    pub fn new() -> Timers {
        Timers {
            next_id: Cell::new(0),
            timers: RefCell::new(HashMap::new()),
        }
    }

    pub fn handle_timer(&self, event_loop: &EventLoop, timer_id: usize) -> Option<()> {
        let timer_state = event_loop.inner.state.timers.timers.borrow().get(&timer_id).cloned();
        if let Some(timer_state) = timer_state {
            let task_ref = timer_state.handler.upgrade()?;
            let mut handler = task_ref.try_borrow_mut().ok()?;
            let cx = Context::new(event_loop, &task_ref);
            handler.event(&cx, timer_state.key, Event::Timer);
        }

        Some(())
    }
}

#[derive(Clone)]
pub struct TimerInner {
    state: Rc<TimerState>,
}

impl TimerInner {
    pub fn repeat(duration: Duration, context: &Context, key: Key) -> Result<TimerInner> {
        let event_loop_state = &context.event_loop.inner.state;

        let timer_id = event_loop_state.timers.next_id.get();
        event_loop_state.timers.next_id.set(timer_id + 1);

        let state = Rc::new(TimerState {
            timer_id: Cell::new(Some(timer_id)),
            event_loop_state: Rc::clone(event_loop_state),
            handler: Rc::downgrade(context.task),
            key,
        });

        event_loop_state.timers.timers.borrow_mut().insert(timer_id, Rc::clone(&state));

        unsafe {
            let millis = duration.as_millis() as u32;
            SetTimer(event_loop_state.message_hwnd, timer_id, millis, None);
        }

        Ok(TimerInner { state })
    }

    pub fn cancel(&self) {
        if let Some(timer_id) = self.state.timer_id.get() {
            self.state.event_loop_state.timers.timers.borrow_mut().remove(&timer_id);
        }

        self.state.cancel();
    }
}
