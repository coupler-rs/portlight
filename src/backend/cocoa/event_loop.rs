use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic;
use std::rc::Rc;

use objc2::rc::{autoreleasepool, Id};
use objc2::runtime::AnyClass;
use objc2::ClassType;

use objc2_app_kit::{
    self, NSApplication, NSApplicationActivationPolicy, NSCursor, NSEvent, NSEventModifierFlags,
    NSEventType, NSImage,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSSize};

use super::display_links::DisplayLinks;
use super::timer::Timers;
use super::window::{View, WindowState};
use crate::{Error, EventLoopMode, EventLoopOptions, Result};

struct RunGuard<'a> {
    running: &'a Cell<bool>,
}

impl<'a> RunGuard<'a> {
    fn new(running: &'a Cell<bool>) -> Result<RunGuard<'a>> {
        if running.get() {
            return Err(Error::AlreadyRunning);
        }

        running.set(true);

        Ok(RunGuard { running })
    }
}

impl<'a> Drop for RunGuard<'a> {
    fn drop(&mut self) {
        self.running.set(false);
    }
}

pub struct EventLoopState {
    pub running: Cell<bool>,
    pub panic: Cell<Option<Box<dyn Any + Send>>>,
    pub class: &'static AnyClass,
    pub empty_cursor: Id<NSCursor>,
    pub timers: Timers,
    pub display_links: DisplayLinks,
    pub windows: RefCell<HashMap<*const View, Rc<WindowState>>>,
    pub mtm: MainThreadMarker,
}

impl EventLoopState {
    pub(crate) fn propagate_panic(&self, panic: Box<dyn Any + Send + 'static>) {
        // If we own the event loop, exit and propagate the panic upwards. Otherwise, just abort.
        if self.running.get() {
            self.panic.set(Some(panic));
            self.exit();
        } else {
            std::process::abort();
        }
    }
}

impl Drop for EventLoopState {
    fn drop(&mut self) {
        unsafe {
            View::unregister_class(self.class);
        }
    }
}

impl EventLoopState {
    pub fn new(options: &EventLoopOptions) -> Result<Rc<EventLoopState>> {
        autoreleasepool(|_| {
            let mtm =
                MainThreadMarker::new().expect("EventLoop must be created on the main thread");

            let class = View::register_class()?;

            let empty_cursor = unsafe {
                let empty_cursor_image =
                    NSImage::initWithSize(NSImage::alloc(), NSSize::new(1.0, 1.0));
                let empty_cursor = NSCursor::initWithImage_hotSpot(
                    NSCursor::alloc(),
                    &empty_cursor_image,
                    NSPoint::new(0.0, 0.0),
                );

                empty_cursor
            };

            let state = Rc::new(EventLoopState {
                running: Cell::new(false),
                panic: Cell::new(None),
                class,
                empty_cursor,
                timers: Timers::new(),
                display_links: DisplayLinks::new(),
                windows: RefCell::new(HashMap::new()),
                mtm,
            });

            state.display_links.init(&state);

            if options.mode == EventLoopMode::Owner {
                let app = NSApplication::sharedApplication(mtm);
                app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
                #[allow(deprecated)]
                app.activateIgnoringOtherApps(true);
            }

            Ok(state)
        })
    }

    pub fn run(&self) -> Result<()> {
        autoreleasepool(|_| {
            let _run_guard = RunGuard::new(&self.running)?;

            let app = NSApplication::sharedApplication(self.mtm);
            unsafe {
                app.run();
            }

            if let Some(panic) = self.panic.take() {
                panic::resume_unwind(panic);
            }

            Ok(())
        })
    }

    pub fn exit(&self) {
        autoreleasepool(|_| {
            if self.running.get() {
                let app = NSApplication::sharedApplication(self.mtm);
                app.stop(None);

                let event = unsafe {
                    // Post an NSEvent to ensure that the call to [NSApplication stop] takes effect
                    // immediately, in case we're inside a CFRunLoopTimer or CFRunLoopSource callback.
                    NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                        NSEventType::ApplicationDefined,
                        NSPoint::new(0.0, 0.0),
                        NSEventModifierFlags::empty(),
                        0.0,
                        0,
                        None,
                        0,
                        0,
                        0,
                    ).unwrap()
                };
                app.postEvent_atStart(&event, true);
            }
        })
    }

    pub fn poll(&self) -> Result<()> {
        let _run_guard = RunGuard::new(&self.running)?;

        // TODO: poll events

        if let Some(panic) = self.panic.take() {
            panic::resume_unwind(panic);
        }

        Ok(())
    }
}
