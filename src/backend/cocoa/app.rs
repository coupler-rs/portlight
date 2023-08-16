use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::result;
use std::time::Duration;

use objc::rc::autoreleasepool;
use objc::runtime::Class;

use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicyRegular};
use cocoa::base::nil;

use super::window::{register_class, unregister_class};
use super::TimerHandleInner;
use crate::{App, AppContext, IntoInnerError, Result};

pub struct AppState {
    pub class: *mut Class,
    pub data: RefCell<Option<Box<dyn Any>>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        unsafe {
            unregister_class(self.class);
        }
    }
}

pub struct AppInner<T> {
    pub state: Rc<AppState>,
    _marker: PhantomData<T>,
}

impl<T> AppInner<T> {
    pub fn new<F>(build: F) -> Result<AppInner<T>>
    where
        F: FnOnce(&AppContext<T>) -> Result<T>,
        T: 'static,
    {
        autoreleasepool(|| {
            let class = register_class()?;

            let state = Rc::new(AppState {
                class,
                data: RefCell::new(None),
            });

            let cx = AppContext::from_inner(AppContextInner {
                state: &state,
                _marker: PhantomData,
            });
            let data = build(&cx)?;

            state.data.replace(Some(Box::new(data)));

            Ok(AppInner {
                state,
                _marker: PhantomData,
            })
        })
    }

    pub fn run(&mut self) -> Result<()> {
        autoreleasepool(|| unsafe {
            let app = NSApp();
            app.setActivationPolicy_(NSApplicationActivationPolicyRegular);
            app.run();

            Ok(())
        })
    }

    pub fn poll(&mut self) -> Result<()> {
        Ok(())
    }

    pub fn into_inner(self) -> result::Result<T, IntoInnerError<App<T>>> {
        unimplemented!()
    }
}

pub struct AppContextInner<'a, T> {
    pub state: &'a Rc<AppState>,
    _marker: PhantomData<T>,
}

impl<'a, T> AppContextInner<'a, T> {
    pub fn set_timer<H>(&self, duration: Duration, handler: H) -> TimerHandleInner
    where
        H: 'static,
        H: FnMut(&mut T, &AppContext<T>),
    {
        TimerHandleInner {}
    }

    pub fn exit(&self) {
        unsafe {
            NSApp().stop_(nil);
        }
    }
}
