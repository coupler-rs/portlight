use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::{EventLoop, Result};

pub struct TimerState {
    timer_id: Cell<Option<usize>>,
    event_loop: EventLoop,
    handler: RefCell<Box<dyn FnMut()>>,
}

impl TimerState {
    pub fn repeat<F>(
        event_loop: &EventLoop,
        duration: Duration,
        handler: F,
    ) -> Result<Rc<TimerState>>
    where
        F: FnMut() + 'static,
    {
        let timers = &event_loop.state.timers;

        let timer_id = timers.next_id.get();
        timers.next_id.set(timer_id + 1);

        let state = Rc::new(TimerState {
            timer_id: Cell::new(Some(timer_id)),
            event_loop: event_loop.clone(),
            handler: RefCell::new(Box::new(handler)),
        });

        timers.timers.borrow_mut().insert(timer_id, Rc::clone(&state));

        unsafe {
            let millis = duration.as_millis() as u32;
            SetTimer(event_loop.state.message_hwnd, timer_id, millis, None);
        }

        Ok(state)
    }

    pub fn cancel(&self) {
        if let Some(timer_id) = self.timer_id.take() {
            self.event_loop.state.timers.timers.borrow_mut().remove(&timer_id);
            let _ = unsafe { KillTimer(self.event_loop.state.message_hwnd, timer_id) };
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

    pub fn handle_timer(&self, timer_id: usize) -> Option<()> {
        let timer_state = self.timers.borrow().get(&timer_id).cloned();
        if let Some(timer_state) = timer_state {
            timer_state.handler.borrow_mut()();
        }

        Some(())
    }
}
