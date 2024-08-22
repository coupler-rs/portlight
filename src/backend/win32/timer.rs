use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use super::app::AppState;
use crate::{AppHandle, Timer, TimerContext};

struct TimerState {
    timer_id: Cell<Option<usize>>,
    app_state: Rc<AppState>,
    handler: RefCell<Box<dyn FnMut(&TimerContext)>>,
}

impl TimerState {
    fn cancel(&self) {
        if let Some(timer_id) = self.timer_id.take() {
            let _ = unsafe { KillTimer(self.app_state.message_hwnd, timer_id) };
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

    pub fn set_timer<H>(
        &self,
        app_state: &Rc<AppState>,
        duration: Duration,
        handler: H,
    ) -> TimerInner
    where
        H: FnMut(&TimerContext) + 'static,
    {
        let timer_id = self.next_id.get();
        self.next_id.set(timer_id + 1);

        let state = Rc::new(TimerState {
            timer_id: Cell::new(Some(timer_id)),
            app_state: Rc::clone(app_state),
            handler: RefCell::new(Box::new(handler)),
        });

        self.timers.borrow_mut().insert(timer_id, Rc::clone(&state));

        unsafe {
            let millis = duration.as_millis() as u32;
            SetTimer(app_state.message_hwnd, timer_id, millis, None);
        }

        TimerInner { state }
    }

    pub fn handle_timer(&self, app: &AppHandle, timer_id: usize) {
        let timer_state = app.inner.state.timers.timers.borrow().get(&timer_id).cloned();
        if let Some(timer_state) = timer_state {
            let timer = Timer::from_inner(TimerInner { state: timer_state });
            let cx = TimerContext::new(&app, &timer);
            timer.inner.state.handler.borrow_mut()(&cx);
        }
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
        if let Some(timer_id) = self.state.timer_id.get() {
            self.state.app_state.timers.timers.borrow_mut().remove(&timer_id);
        }

        self.state.cancel();
    }
}
