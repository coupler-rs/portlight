use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::rc::Rc;

use objc2::rc::Id;

use objc2_app_kit::NSScreen;
use objc2_foundation::{ns_string, NSNumber};

use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation::runloop::*;

use super::event_loop::{EventLoopInner, EventLoopState};
use super::ffi::display_link::*;
use super::window::{View, WindowInner};
use crate::{Event, EventLoopHandle, Window, WindowContext};

fn display_from_screen(screen: &NSScreen) -> Option<CGDirectDisplayID> {
    unsafe {
        let number = screen.deviceDescription().objectForKey(ns_string!("NSScreenNumber"))?;
        let id = Id::cast::<NSNumber>(number).unsignedIntegerValue() as CGDirectDisplayID;

        Some(id)
    }
}

fn display_from_view(view: &View) -> Option<CGDirectDisplayID> {
    let screen = view.window()?.screen()?;
    display_from_screen(&*screen)
}

#[allow(non_snake_case)]
extern "C" fn callback(
    _displayLink: CVDisplayLinkRef,
    _inNow: *const CVTimeStamp,
    _inOutputTime: *const CVTimeStamp,
    _flagsIn: CVOptionFlags,
    _flagsOut: *mut CVOptionFlags,
    displayLinkContext: *mut c_void,
) -> CVReturn {
    let source = displayLinkContext as CFRunLoopSourceRef;
    unsafe {
        CFRunLoopSourceSignal(source);
        CFRunLoopWakeUp(CFRunLoopGetMain());
    }

    kCVReturnSuccess
}

extern "C" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const DisplayState) };

    info
}

extern "C" fn release(info: *const c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        unsafe { Rc::decrement_strong_count(info as *const DisplayState) };
    }));

    // If a panic occurs while dropping the Rc<DisplayState>, the only thing left to do is abort.
    if let Err(_panic) = result {
        std::process::abort();
    }
}

extern "C" fn perform(info: *const c_void) {
    let state = unsafe { &*(info as *mut DisplayState) };
    let event_loop_state = &state.event_loop.inner.state;

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let windows: Vec<*const View> = event_loop_state.windows.borrow().keys().copied().collect();
        for ptr in windows {
            let window_state = event_loop_state.windows.borrow().get(&ptr).cloned();
            if let Some(window_state) = window_state {
                if let Some(view) = window_state.view() {
                    let display = display_from_view(&*view);
                    if display == Some(state.display_id) {
                        let window = Window::from_inner(WindowInner::from_state(window_state));
                        let cx = WindowContext::new(&state.event_loop, &window);
                        window.inner.state.handle_event(&cx, Event::Frame);
                    }
                }
            }
        }
    }));

    if let Err(panic) = result {
        event_loop_state.propagate_panic(panic);
    }
}

struct DisplayState {
    display_id: CGDirectDisplayID,
    event_loop: EventLoopHandle,
}

struct Display {
    link: CVDisplayLinkRef,
    source: CFRunLoopSourceRef,
}

impl Display {
    pub fn new(event_loop_state: &Rc<EventLoopState>, display_id: CGDirectDisplayID) -> Display {
        let state = Rc::new(DisplayState {
            display_id,
            event_loop: EventLoopHandle::from_inner(EventLoopInner::from_state(Rc::clone(
                event_loop_state,
            ))),
        });

        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: Rc::as_ptr(&state) as *mut c_void,
            retain: Some(retain),
            release: Some(release),
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform,
        };

        let source = unsafe { CFRunLoopSourceCreate(ptr::null(), 0, &mut context) };
        unsafe {
            let run_loop = CFRunLoopGetMain();
            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
        }

        let mut link = ptr::null();
        unsafe {
            CVDisplayLinkCreateWithCGDisplay(display_id, &mut link);
            CVDisplayLinkSetOutputCallback(link, callback, source as *mut c_void);
            CVDisplayLinkStart(link);
        }

        Display { link, source }
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        unsafe {
            CVDisplayLinkStop(self.link);
            CVDisplayLinkRelease(self.link);

            CFRunLoopSourceInvalidate(self.source);
            CFRelease(self.source as CFTypeRef);
        }
    }
}

pub struct DisplayLinks {
    displays: RefCell<HashMap<CGDirectDisplayID, Display>>,
}

impl DisplayLinks {
    pub fn new() -> DisplayLinks {
        DisplayLinks {
            displays: RefCell::new(HashMap::new()),
        }
    }

    pub fn init(&self, event_loop_state: &Rc<EventLoopState>) {
        for screen in NSScreen::screens(event_loop_state.mtm) {
            if let Some(id) = display_from_screen(&*screen) {
                self.displays.borrow_mut().insert(id, Display::new(event_loop_state, id));
            }
        }
    }

    pub fn shutdown(&self) {
        drop(self.displays.take());
    }
}
