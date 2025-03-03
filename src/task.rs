use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::{error, fmt};

use crate::window::WindowEvent;
use crate::EventLoop;

pub struct Context<'a> {
    pub(crate) event_loop: &'a EventLoop,
    pub(crate) task: &'a Rc<RefCell<dyn Task>>,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Context<'_> {
    pub(crate) fn new<'a>(
        event_loop: &'a EventLoop,
        task: &'a Rc<RefCell<dyn Task>>,
    ) -> Context<'a> {
        Context {
            event_loop,
            task,
            _marker: PhantomData,
        }
    }

    pub fn event_loop(&self) -> &EventLoop {
        self.event_loop
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Key(pub usize);

pub enum Event<'a> {
    Window(WindowEvent<'a>),
    Timer,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Response {
    Capture,
    Ignore,
}

pub trait Task {
    fn event(&mut self, cx: &Context, key: Key, event: Event) -> Response;
}

pub struct TaskHandle<T> {
    event_loop: EventLoop,
    pub(crate) task: Rc<RefCell<T>>,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl<T: Task + 'static> TaskHandle<T> {
    pub(crate) fn spawn(event_loop: &EventLoop, task: T) -> TaskHandle<T> {
        TaskHandle {
            event_loop: event_loop.clone(),
            task: Rc::new(RefCell::new(task)),
            _marker: PhantomData,
        }
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut T, &Context) -> R,
    {
        if let Ok(mut task) = self.task.try_borrow_mut() {
            let task_ref = Rc::clone(&self.task) as _;
            f(&mut *task, &Context::new(&self.event_loop, &task_ref))
        } else {
            panic!("already mutably borrowed")
        }
    }

    pub fn try_with<F, R>(&self, f: F) -> Result<R, BorrowMutError>
    where
        F: FnOnce(&mut T, &Context) -> R,
    {
        if let Ok(mut task) = self.task.try_borrow_mut() {
            let task_ref = Rc::clone(&self.task) as _;
            Ok(f(&mut *task, &Context::new(&self.event_loop, &task_ref)))
        } else {
            Err(BorrowMutError)
        }
    }
}

#[derive(Debug)]
pub struct BorrowMutError;

impl error::Error for BorrowMutError {}

impl fmt::Display for BorrowMutError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt("already mutably borrowed", f)
    }
}
