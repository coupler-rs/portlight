use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ops::{Deref, DerefMut};
use std::panic::{self, AssertUnwindSafe};
use std::rc::{Rc, Weak};

use objc2::declare::ClassBuilder;
use objc2::encode::Encoding;
use objc2::rc::{autoreleasepool, Allocated, Id};
use objc2::runtime::{AnyClass, Bool, MessageReceiver, Sel};
use objc2::{class, msg_send, msg_send_id, sel};
use objc2::{ClassType, Message, RefEncode};

use objc_sys::{objc_class, objc_disposeClassPair};

use objc2_app_kit::{
    NSBackingStoreType, NSCursor, NSEvent, NSScreen, NSTrackingArea, NSTrackingAreaOptions, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSInteger, NSPoint, NSRect, NSSize, NSString};

use super::surface::Surface;
use super::OsError;
use crate::{
    Bitmap, Context, Cursor, Error, Event, EventLoop, Key, MouseButton, Point, RawWindow, Rect,
    Response, Result, Size, Task, WindowEvent, WindowOptions,
};

fn class_name() -> String {
    use std::fmt::Write;

    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).unwrap();

    let mut name = "window-".to_string();
    for byte in bytes {
        write!(&mut name, "{:x}", byte).unwrap();
    }

    name
}

fn mouse_button_from_number(button_number: NSInteger) -> Option<MouseButton> {
    match button_number {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Right),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Back),
        4 => Some(MouseButton::Forward),
        _ => None,
    }
}

#[repr(C)]
pub struct View {
    superclass: NSView,
}

unsafe impl RefEncode for View {
    const ENCODING_REF: Encoding = NSView::ENCODING_REF;
}

unsafe impl Message for View {}

impl Deref for View {
    type Target = NSView;

    fn deref(&self) -> &Self::Target {
        &self.superclass
    }
}

impl DerefMut for View {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.superclass
    }
}

impl View {
    pub fn register_class() -> Result<&'static AnyClass> {
        let name = class_name();
        let Some(mut builder) = ClassBuilder::new(&name, class!(NSView)) else {
            return Err(Error::Os(OsError::Other(
                "could not declare NSView subclass",
            )));
        };

        builder.add_ivar::<Cell<*mut c_void>>("windowState");

        unsafe {
            builder.add_method(
                sel!(acceptsFirstMouse:),
                Self::accepts_first_mouse as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(
                sel!(isFlipped),
                Self::is_flipped as unsafe extern "C" fn(_, _) -> _,
            );
            builder.add_method(
                sel!(mouseEntered:),
                Self::mouse_entered as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(mouseExited:),
                Self::mouse_exited as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(mouseMoved:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(mouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(rightMouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(otherMouseDragged:),
                Self::mouse_moved as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(mouseDown:),
                Self::mouse_down as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(mouseUp:),
                Self::mouse_up as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(rightMouseDown:),
                Self::right_mouse_down as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(rightMouseUp:),
                Self::right_mouse_up as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(otherMouseDown:),
                Self::other_mouse_down as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(otherMouseUp:),
                Self::other_mouse_up as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(scrollWheel:),
                Self::scroll_wheel as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(cursorUpdate:),
                Self::cursor_update as unsafe extern "C" fn(_, _, _),
            );
            builder.add_method(
                sel!(windowShouldClose:),
                Self::window_should_close as unsafe extern "C" fn(_, _, _) -> _,
            );
            builder.add_method(sel!(dealloc), View::dealloc as unsafe extern "C" fn(_, _));
        }

        Ok(builder.register())
    }

    pub unsafe fn unregister_class(class: &'static AnyClass) {
        objc_disposeClassPair(class as *const _ as *mut objc_class);
    }

    fn state_ivar(&self) -> &Cell<*mut c_void> {
        let ivar = self.class().instance_variable("windowState").unwrap();
        unsafe { ivar.load::<Cell<*mut c_void>>(self) }
    }

    fn state(&self) -> &WindowState {
        unsafe { &*(self.state_ivar().get() as *const WindowState) }
    }

    fn catch_unwind<F: FnOnce()>(&self, f: F) {
        let result = panic::catch_unwind(AssertUnwindSafe(f));

        if let Err(panic) = result {
            self.state().event_loop.state.propagate_panic(panic);
        }
    }

    pub fn retain(&self) -> Id<View> {
        unsafe { Id::retain(self as *const View as *mut View) }.unwrap()
    }

    unsafe extern "C" fn accepts_first_mouse(&self, _: Sel, _event: Option<&NSEvent>) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn is_flipped(&self, _: Sel) -> Bool {
        Bool::YES
    }

    unsafe extern "C" fn mouse_entered(&self, _: Sel, _event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            self.state().handle_event(WindowEvent::MouseEnter);
        });
    }

    unsafe extern "C" fn mouse_exited(&self, _: Sel, _event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            self.state().handle_event(WindowEvent::MouseExit);
        });
    }

    unsafe extern "C" fn mouse_moved(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let Some(event) = event else {
                return;
            };

            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            self.state().handle_event(WindowEvent::MouseMove(Point {
                x: point.x,
                y: point.y,
            }));
        });
    }

    unsafe extern "C" fn mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.state().handle_event(WindowEvent::MouseDown(MouseButton::Left));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), mouseDown: event];
            }
        });
    }

    unsafe extern "C" fn mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.state().handle_event(WindowEvent::MouseUp(MouseButton::Left));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), mouseUp: event];
            }
        });
    }

    unsafe extern "C" fn right_mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.state().handle_event(WindowEvent::MouseDown(MouseButton::Right));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), rightMouseDown: event];
            }
        });
    }

    unsafe extern "C" fn right_mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.state().handle_event(WindowEvent::MouseUp(MouseButton::Right));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), rightMouseUp: event];
            }
        });
    }

    unsafe extern "C" fn other_mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let Some(event) = event else {
                return;
            };

            let button_number = event.buttonNumber();
            let result = if let Some(button) = mouse_button_from_number(button_number) {
                self.state().handle_event(WindowEvent::MouseDown(button))
            } else {
                None
            };

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), otherMouseDown: event];
            }
        });
    }

    unsafe extern "C" fn other_mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let Some(event) = event else {
                return;
            };

            let button_number = event.buttonNumber();
            let result = if let Some(button) = mouse_button_from_number(button_number) {
                self.state().handle_event(WindowEvent::MouseUp(button))
            } else {
                None
            };

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), otherMouseUp: event];
            }
        });
    }

    unsafe extern "C" fn scroll_wheel(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let Some(event) = event else {
                return;
            };

            let dx = event.scrollingDeltaX();
            let dy = event.scrollingDeltaY();
            let delta = if event.hasPreciseScrollingDeltas() {
                Point::new(dx, dy)
            } else {
                Point::new(32.0 * dx, 32.0 * dy)
            };
            let result = self.state().handle_event(WindowEvent::Scroll(delta));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), scrollWheel: event];
            }
        });
    }

    unsafe extern "C" fn cursor_update(&self, _: Sel, _event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            self.state().update_cursor();
        });
    }

    unsafe extern "C" fn window_should_close(&self, _: Sel, _sender: &NSWindow) -> Bool {
        self.catch_unwind(|| {
            self.state().handle_event(WindowEvent::Close);
        });

        Bool::NO
    }

    unsafe extern "C" fn dealloc(this: *mut Self, _: Sel) {
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            drop(Rc::from_raw(
                (*this).state_ivar().get() as *const WindowState
            ));
        }));

        // If a panic occurs while dropping the Rc<WindowState>, the only thing left to do is
        // abort.
        if let Err(_panic) = result {
            std::process::abort();
        }

        let () = msg_send![super(this, NSView::class()), dealloc];
    }
}

pub struct WindowState {
    view: RefCell<Option<Id<View>>>,
    window: RefCell<Option<Id<NSWindow>>>,
    surface: RefCell<Option<Surface>>,
    cursor: Cell<Cursor>,
    event_loop: EventLoop,
    handler: Weak<RefCell<dyn Task>>,
    key: Key,
}

impl WindowState {
    pub fn view(&self) -> Option<Id<View>> {
        self.view.borrow().as_ref().map(|view| view.retain())
    }

    pub fn window(&self) -> Option<Id<NSWindow>> {
        self.window.borrow().clone()
    }

    pub fn handle_event(&self, event: WindowEvent) -> Option<Response> {
        let task_ref = self.handler.upgrade()?;
        let mut handler = task_ref.try_borrow_mut().ok()?;
        let cx = Context::new(&self.event_loop, &task_ref);
        Some(handler.event(&cx, self.key, Event::Window(event)))
    }

    fn update_cursor(&self) {
        fn try_get_cursor(selector: Sel) -> Id<NSCursor> {
            unsafe {
                let class = NSCursor::class();
                if objc2::msg_send![class, respondsToSelector: selector] {
                    let cursor: *mut NSCursor = class.send_message(selector, ());
                    if let Some(cursor) = Id::retain(cursor) {
                        return cursor;
                    }
                }

                NSCursor::arrowCursor()
            }
        }

        let cursor = self.cursor.get();

        let ns_cursor = match cursor {
            Cursor::Arrow => NSCursor::arrowCursor(),
            Cursor::Crosshair => NSCursor::crosshairCursor(),
            Cursor::Hand => NSCursor::pointingHandCursor(),
            Cursor::IBeam => NSCursor::IBeamCursor(),
            Cursor::No => NSCursor::operationNotAllowedCursor(),
            Cursor::SizeNs => try_get_cursor(sel!(_windowResizeNorthSouthCursor)),
            Cursor::SizeWe => try_get_cursor(sel!(_windowResizeEastWestCursor)),
            Cursor::SizeNesw => try_get_cursor(sel!(_windowResizeNorthEastSouthWestCursor)),
            Cursor::SizeNwse => try_get_cursor(sel!(_windowResizeNorthWestSouthEastCursor)),
            Cursor::Wait => try_get_cursor(sel!(_waitCursor)),
            Cursor::None => self.event_loop.state.empty_cursor.clone(),
        };

        unsafe {
            ns_cursor.set();
        }
    }

    pub fn open(options: &WindowOptions, context: &Context, key: Key) -> Result<Rc<WindowState>> {
        autoreleasepool(|_| {
            let event_loop = context.event_loop;

            let event_loop_state = &event_loop.state;

            let parent_view = if let Some(parent) = options.parent {
                if let RawWindow::Cocoa(parent_view) = parent {
                    Some(parent_view as *const NSView)
                } else {
                    return Err(Error::InvalidWindowHandle);
                }
            } else {
                None
            };

            let origin = options.position.unwrap_or(Point::new(0.0, 0.0));
            let frame = NSRect::new(
                NSPoint::new(origin.x, origin.y),
                NSSize::new(options.size.width, options.size.height),
            );

            let state = Rc::new(WindowState {
                view: RefCell::new(None),
                window: RefCell::new(None),
                surface: RefCell::new(None),
                cursor: Cell::new(Cursor::Arrow),
                event_loop: event_loop.clone(),
                handler: Rc::downgrade(context.task),
                key,
            });

            let view: Allocated<View> = unsafe { msg_send_id![event_loop_state.class, alloc] };
            let view: Id<View> = unsafe { msg_send_id![view, initWithFrame: frame] };
            view.state_ivar().set(Rc::into_raw(Rc::clone(&state)) as *mut c_void);

            state.view.replace(Some(view.retain()));

            let tracking_options = NSTrackingAreaOptions::NSTrackingMouseEnteredAndExited
                | NSTrackingAreaOptions::NSTrackingMouseMoved
                | NSTrackingAreaOptions::NSTrackingCursorUpdate
                | NSTrackingAreaOptions::NSTrackingActiveAlways
                | NSTrackingAreaOptions::NSTrackingInVisibleRect
                | NSTrackingAreaOptions::NSTrackingEnabledDuringMouseDrag;

            unsafe {
                let tracking_area = NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
                    tracking_options,
                    Some(&view),
                    None,
                );
                view.addTrackingArea(&tracking_area);
            }

            if let Some(parent_view) = parent_view {
                unsafe {
                    view.setHidden(true);
                    (*parent_view).addSubview(&view);
                }
            } else {
                let origin = options.position.unwrap_or(Point::new(0.0, 0.0));
                let content_rect = NSRect::new(
                    NSPoint::new(origin.x, origin.y),
                    NSSize::new(options.size.width, options.size.height),
                );

                let style_mask = NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable;

                let window = unsafe {
                    NSWindow::initWithContentRect_styleMask_backing_defer(
                        event_loop_state.mtm.alloc::<NSWindow>(),
                        content_rect,
                        style_mask,
                        NSBackingStoreType::NSBackingStoreBuffered,
                        false,
                    )
                };

                unsafe {
                    window.setReleasedWhenClosed(false);

                    window.setTitle(&NSString::from_str(&options.title));

                    let () = msg_send![&*window, setDelegate: &*view];
                    window.setContentView(Some(&view));

                    if options.position.is_none() {
                        window.center();
                    }
                }

                state.window.replace(Some(window));
            }

            event_loop_state
                .windows
                .borrow_mut()
                .insert(Id::as_ptr(&view), Rc::clone(&state));

            let scale = state.scale();

            let surface = Surface::new(
                (scale * options.size.width).round() as usize,
                (scale * options.size.height).round() as usize,
            )?;

            unsafe {
                let () = msg_send![&*view, setLayer: &*surface.layer];
                view.setWantsLayer(true);

                surface.layer.setContentsScale(scale);
            }

            state.surface.replace(Some(surface));

            Ok(state)
        })
    }

    pub fn show(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.window() {
                window.orderFront(None);
            }

            if let Some(view) = self.view() {
                view.setHidden(false);
            }
        })
    }

    pub fn hide(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.window() {
                window.orderOut(None);
            }

            if let Some(view) = self.view() {
                view.setHidden(true);
            }
        })
    }

    pub fn size(&self) -> Size {
        autoreleasepool(|_| {
            if let Some(view) = self.view() {
                let frame = view.frame();

                Size::new(frame.size.width, frame.size.height)
            } else {
                Size::new(0.0, 0.0)
            }
        })
    }

    pub fn scale(&self) -> f64 {
        autoreleasepool(|_| {
            let mtm = self.event_loop.state.mtm;

            if let Some(view) = self.view() {
                if let Some(window) = view.window() {
                    return window.backingScaleFactor();
                } else if let Some(screen) = NSScreen::screens(mtm).get(0) {
                    return screen.backingScaleFactor();
                }
            }

            1.0
        })
    }

    pub fn present(&self, bitmap: Bitmap) {
        autoreleasepool(|_| {
            if let Some(surface) = &mut *self.surface.borrow_mut() {
                let width = surface.width;
                let height = surface.height;
                let copy_width = bitmap.width().min(width);
                let copy_height = bitmap.height().min(height);

                surface.with_buffer(|buffer| {
                    for row in 0..copy_height {
                        let src =
                            &bitmap.data()[row * bitmap.width()..row * bitmap.width() + copy_width];
                        let dst = &mut buffer[row * width..row * width + copy_width];
                        dst.copy_from_slice(src);
                    }
                });

                surface.present();
            }
        })
    }

    pub fn present_partial(&self, bitmap: Bitmap, _rects: &[Rect]) {
        self.present(bitmap);
    }

    pub fn set_cursor(&self, cursor: Cursor) {
        autoreleasepool(|_| {
            self.cursor.set(cursor);
            self.update_cursor();
        })
    }

    pub fn set_mouse_position(&self, _position: Point) {}

    pub fn close(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.window.take() {
                window.close();
            }

            if let Some(view) = self.view.take() {
                self.event_loop.state.windows.borrow_mut().remove(&Id::as_ptr(&view));
                unsafe { view.removeFromSuperview() };
            }
        })
    }

    pub fn as_raw(&self) -> Result<RawWindow> {
        if let Some(view) = self.view.borrow().as_ref() {
            Ok(RawWindow::Cocoa(Id::as_ptr(view) as *mut c_void))
        } else {
            Err(Error::WindowClosed)
        }
    }
}
