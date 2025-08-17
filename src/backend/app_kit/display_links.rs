use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::ptr;
use std::rc::{Rc, Weak};

use objc2_app_kit::NSScreen;
use objc2_core_foundation::{
    kCFRunLoopCommonModes, CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext,
};
use objc2_foundation::{ns_string, NSNumber};

use super::event_loop::EventLoopState;
use super::ffi::display_link::*;
use super::window::View;
use crate::WindowEvent;

fn display_from_screen(screen: &NSScreen) -> Option<CGDirectDisplayID> {
    let number = screen.deviceDescription().objectForKey(ns_string!("NSScreenNumber"))?;
    let number = number.downcast::<NSNumber>().ok()?;
    Some(number.unsignedIntegerValue() as CGDirectDisplayID)
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
    let source = unsafe { &*(displayLinkContext as *const CFRunLoopSource) };
    source.signal();

    let run_loop = CFRunLoop::main().unwrap();
    run_loop.wake_up();

    kCVReturnSuccess
}

extern "C-unwind" fn retain(info: *const c_void) -> *const c_void {
    unsafe { Rc::increment_strong_count(info as *const DisplayState) };

    info
}

extern "C-unwind" fn release(info: *const c_void) {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        unsafe { Rc::decrement_strong_count(info as *const DisplayState) };
    }));

    // If a panic occurs while dropping the Rc<DisplayState>, the only thing left to do is abort.
    if let Err(_panic) = result {
        std::process::abort();
    }
}

extern "C-unwind" fn perform(info: *mut c_void) {
    let state = unsafe { &*(info as *mut DisplayState) };

    let Some(event_loop_state) = state.event_loop_state.upgrade() else {
        return;
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        let windows: Vec<*const View> = event_loop_state.windows.borrow().keys().copied().collect();
        for ptr in windows {
            let window_state = event_loop_state.windows.borrow().get(&ptr).cloned();
            if let Some(window_state) = window_state {
                if let Some(view) = window_state.view() {
                    let display = display_from_view(&*view);
                    if display == Some(state.display_id) {
                        window_state.handle_event(WindowEvent::Frame);
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
    event_loop_state: Weak<EventLoopState>,
}

struct Display {
    link: CVDisplayLinkRef,
    source: CFRetained<CFRunLoopSource>,
}

impl Display {
    pub fn new(event_loop_state: &Rc<EventLoopState>, display_id: CGDirectDisplayID) -> Display {
        let state = Rc::new(DisplayState {
            display_id,
            event_loop_state: Rc::downgrade(event_loop_state),
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
            perform: Some(perform),
        };

        let source = unsafe { CFRunLoopSource::new(None, 0, &mut context) }.unwrap();

        let run_loop = CFRunLoop::main().unwrap();
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });

        let source_ptr = CFRetained::as_ptr(&source).as_ptr();

        let mut link = ptr::null();
        unsafe {
            CVDisplayLinkCreateWithCGDisplay(display_id, &mut link);
            CVDisplayLinkSetOutputCallback(link, callback, source_ptr as *mut c_void);
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

            self.source.invalidate();
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
}
