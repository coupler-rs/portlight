use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::{Context, Event, EventLoop, Key, Result, Task};

pub type TimerId = usize;

struct TimerState {
    timer_id: TimerId,
    duration: Duration,
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

    pub fn poll(&self) {
        let now = Instant::now();

        // Check with < and not <= so that we don't process a timer twice during this loop
        while self.next_time().map_or(false, |t| t < now) {
            let next = self.queue.borrow_mut().pop().unwrap();

            // If we don't find the timer in `self.timers`, it has been canceled
            let timer_state = self.timers.borrow().get(&next.timer_id).cloned();
            if let Some(timer_state) = timer_state {
                timer_state.handle_timer();

                // If we fall behind by more than one timer interval, reset the timer's phase
                let next_time = (next.time + timer_state.duration).max(now);

                self.queue.borrow_mut().push(QueueEntry {
                    time: next_time,
                    timer_id: next.timer_id,
                })
            }
        }
    }
}

#[derive(Clone)]
pub struct TimerInner {
    state: Rc<TimerState>,
}

impl TimerInner {
    pub fn repeat(duration: Duration, context: &Context, key: Key) -> Result<TimerInner> {
        let event_loop_state = &context.event_loop.state;

        let now = Instant::now();

        let timer_id = event_loop_state.timers.next_id.get();
        event_loop_state.timers.next_id.set(timer_id + 1);

        let state = Rc::new(TimerState {
            timer_id,
            duration,
            event_loop: context.event_loop.clone(),
            handler: Rc::downgrade(context.task),
            key,
        });

        event_loop_state.timers.timers.borrow_mut().insert(timer_id, Rc::clone(&state));

        event_loop_state.timers.queue.borrow_mut().push(QueueEntry {
            time: now + duration,
            timer_id,
        });

        Ok(TimerInner { state })
    }

    pub fn cancel(&self) {
        let timers = &self.state.event_loop.state.timers;
        timers.timers.borrow_mut().remove(&self.state.timer_id);
    }
}
