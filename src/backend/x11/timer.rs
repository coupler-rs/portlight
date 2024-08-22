use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::app::{AppInner, AppState};
use crate::{AppHandle, Timer, TimerContext};

pub type TimerId = usize;

struct TimerState {
    timer_id: TimerId,
    duration: Duration,
    app: AppHandle,
    handler: RefCell<Box<dyn FnMut(&TimerContext)>>,
}

#[derive(Clone)]
struct QueueEntry {
    time: Instant,
    timer_id: TimerId,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.time.cmp(&other.time).reverse())
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.time.cmp(&other.time).reverse()
    }
}

pub struct Timers {
    next_id: Cell<TimerId>,
    timers: RefCell<HashMap<usize, Rc<TimerState>>>,
    queue: RefCell<BinaryHeap<QueueEntry>>,
}

impl Timers {
    pub fn new() -> Timers {
        Timers {
            next_id: Cell::new(0),
            timers: RefCell::new(HashMap::new()),
            queue: RefCell::new(BinaryHeap::new()),
        }
    }

    pub fn next_time(&self) -> Option<Instant> {
        self.queue.borrow().peek().map(|e| e.time)
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
        let now = Instant::now();

        let timer_id = self.next_id.get();
        self.next_id.set(timer_id + 1);

        let state = Rc::new(TimerState {
            timer_id,
            duration,
            app: AppHandle::from_inner(AppInner::from_state(Rc::clone(app_state))),
            handler: RefCell::new(Box::new(handler)),
        });

        self.timers.borrow_mut().insert(timer_id, Rc::clone(&state));

        self.queue.borrow_mut().push(QueueEntry {
            time: now + duration,
            timer_id,
        });

        TimerInner { state }
    }

    pub fn poll(&self) {
        let now = Instant::now();

        // Check with < and not <= so that we don't process a timer twice during this loop
        while self.next_time().map_or(false, |t| t < now) {
            let next = self.queue.borrow_mut().pop().unwrap();

            // If we don't find the timer in `self.timers`, it has been canceled
            let timer_state = self.timers.borrow().get(&next.timer_id).cloned();
            if let Some(timer_state) = timer_state {
                let timer = Timer::from_inner(TimerInner { state: timer_state });
                let cx = TimerContext::new(&timer.inner.state.app, &timer);
                timer.inner.state.handler.borrow_mut()(&cx);

                // If we fall behind by more than one timer interval, reset the timer's phase
                let next_time = (next.time + timer.inner.state.duration).max(now);

                self.queue.borrow_mut().push(QueueEntry {
                    time: next_time,
                    timer_id: next.timer_id,
                })
            }
        }
    }

    pub fn shutdown(&self) {
        drop(self.timers.take());
        drop(self.queue.take());
    }
}

#[derive(Clone)]
pub struct TimerInner {
    state: Rc<TimerState>,
}

impl TimerInner {
    pub fn cancel(&self) {
        let timers = &self.state.app.inner.state.timers;
        timers.timers.borrow_mut().remove(&self.state.timer_id);
    }
}
