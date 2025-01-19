use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ops::{Deref, DerefMut};
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

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
    Bitmap, Cursor, Error, Event, EventLoopHandle, MouseButton, Point, RawWindow, Rect, Response,
    Result, Size, Window, WindowContext, WindowOptions,
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
            self.state().event_loop.inner.state.propagate_panic(panic);
        }
    }

    pub fn handle_event(&self, event: Event) -> Option<Response> {
        let state_rc = unsafe { Rc::from_raw(self.state_ivar().get() as *const WindowState) };
        let state = Rc::clone(&state_rc);
        let _ = Rc::into_raw(state_rc);

        let window = Window::from_inner(WindowInner { state });
        let cx = WindowContext::new(&window.inner.state.event_loop, &window);

        window.inner.state.handle_event(&cx, event)
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
            self.handle_event(Event::MouseEnter);
        });
    }

    unsafe extern "C" fn mouse_exited(&self, _: Sel, _event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            self.handle_event(Event::MouseExit);
        });
    }

    unsafe extern "C" fn mouse_moved(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let Some(event) = event else {
                return;
            };

            let point = self.convertPoint_fromView(event.locationInWindow(), None);
            self.handle_event(Event::MouseMove(Point {
                x: point.x,
                y: point.y,
            }));
        });
    }

    unsafe extern "C" fn mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.handle_event(Event::MouseDown(MouseButton::Left));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), mouseDown: event];
            }
        });
    }

    unsafe extern "C" fn mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.handle_event(Event::MouseUp(MouseButton::Left));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), mouseUp: event];
            }
        });
    }

    unsafe extern "C" fn right_mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.handle_event(Event::MouseDown(MouseButton::Right));

            if result != Some(Response::Capture) {
                let () = msg_send![super(self, NSView::class()), rightMouseDown: event];
            }
        });
    }

    unsafe extern "C" fn right_mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        self.catch_unwind(|| {
            let result = self.handle_event(Event::MouseUp(MouseButton::Right));

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
                self.handle_event(Event::MouseDown(button))
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
                self.handle_event(Event::MouseUp(button))
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
            let result = self.handle_event(Event::Scroll(delta));

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
            self.handle_event(Event::Close);
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
    event_loop: EventLoopHandle,
    handler: RefCell<Box<dyn FnMut(&WindowContext, Event) -> Response>>,
}

impl WindowState {
    pub fn view(&self) -> Option<Id<View>> {
        self.view.borrow().as_ref().map(|view| view.retain())
    }

    pub fn window(&self) -> Option<Id<NSWindow>> {
        self.window.borrow().clone()
    }

    pub fn handle_event(&self, cx: &WindowContext, event: Event) -> Option<Response> {
        if let Ok(mut handler) = self.handler.try_borrow_mut() {
            return Some(handler(cx, event));
        }

        None
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
            Cursor::None => self.event_loop.inner.state.empty_cursor.clone(),
        };

        unsafe {
            ns_cursor.set();
        }
    }

    pub fn close(&self) {
        if let Some(window) = self.window.take() {
            window.close();
        }

        if let Some(view) = self.view.take() {
            unsafe { view.removeFromSuperview() };
        }
    }
}

#[derive(Clone)]
pub struct WindowInner {
    pub(super) state: Rc<WindowState>,
}

impl WindowInner {
    pub fn from_state(state: Rc<WindowState>) -> WindowInner {
        WindowInner { state }
    }

    pub fn open<H>(
        options: &WindowOptions,
        event_loop: &EventLoopHandle,
        handler: H,
    ) -> Result<WindowInner>
    where
        H: FnMut(&WindowContext, Event) -> Response + 'static,
    {
        autoreleasepool(|_| {
            if !event_loop.inner.state.open.get() {
                return Err(Error::EventLoopDropped);
            }

            let event_loop_state = &event_loop.inner.state;

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
                handler: RefCell::new(Box::new(handler)),
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

            let inner = WindowInner { state };

            let scale = inner.scale();

            let surface = Surface::new(
                (scale * options.size.width).round() as usize,
                (scale * options.size.height).round() as usize,
            )?;

            unsafe {
                let () = msg_send![&*view, setLayer: &*surface.layer];
                view.setWantsLayer(true);

                surface.layer.setContentsScale(scale);
            }

            inner.state.surface.replace(Some(surface));

            Ok(inner)
        })
    }

    pub fn show(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.state.window() {
                window.orderFront(None);
            }

            if let Some(view) = self.state.view() {
                view.setHidden(false);
            }
        })
    }

    pub fn hide(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.state.window() {
                window.orderOut(None);
            }

            if let Some(view) = self.state.view() {
                view.setHidden(true);
            }
        })
    }

    pub fn size(&self) -> Size {
        autoreleasepool(|_| {
            if let Some(view) = self.state.view() {
                let frame = view.frame();

                Size::new(frame.size.width, frame.size.height)
            } else {
                Size::new(0.0, 0.0)
            }
        })
    }

    pub fn scale(&self) -> f64 {
        autoreleasepool(|_| {
            let mtm = self.state.event_loop.inner.state.mtm;

            if let Some(view) = self.state.view() {
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
            if let Some(surface) = &mut *self.state.surface.borrow_mut() {
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
            self.state.cursor.set(cursor);
            self.state.update_cursor();
        })
    }

    pub fn set_mouse_position(&self, _position: Point) {}

    pub fn close(&self) {
        autoreleasepool(|_| {
            if let Some(view) = self.state.view.borrow().as_ref() {
                self.state.event_loop.inner.state.windows.borrow_mut().remove(&Id::as_ptr(view));
            }

            self.state.close();
        })
    }

    pub fn as_raw(&self) -> Result<RawWindow> {
        if let Some(view) = self.state.view.borrow().as_ref() {
            Ok(RawWindow::Cocoa(Id::as_ptr(view) as *mut c_void))
        } else {
            Err(Error::WindowClosed)
        }
    }
}
