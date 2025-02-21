use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::{error, fmt};

use crate::window::WindowEvent;
use crate::EventLoopHandle;

pub struct Context<'a> {
    event_loop: &'a EventLoopHandle,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Context<'_> {
    fn new(event_loop: &EventLoopHandle) -> Context {
        Context {
            event_loop,
            _marker: PhantomData,
        }
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
    event_loop: EventLoopHandle,
    task: Rc<RefCell<T>>,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl<T: Task + 'static> TaskHandle<T> {
    pub(crate) fn spawn(event_loop: &EventLoopHandle, task: T) -> TaskHandle<T> {
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
            f(&mut *task, &Context::new(&self.event_loop))
        } else {
            panic!("already mutably borrowed")
        }
    }

    pub fn try_with<F, R>(&self, f: F) -> Result<R, BorrowMutError>
    where
        F: FnOnce(&mut T, &Context) -> R,
    {
        if let Ok(mut task) = self.task.try_borrow_mut() {
            Ok(f(&mut *task, &Context::new(&self.event_loop)))
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
