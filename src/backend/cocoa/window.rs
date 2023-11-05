use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use objc2::declare::{ClassBuilder, Ivar, IvarEncode, IvarType};
use objc2::encode::Encoding;
use objc2::rc::{autoreleasepool, Allocated, Id};
use objc2::runtime::{AnyClass, Bool, ProtocolObject, Sel};
use objc2::{class, msg_send, msg_send_id, sel};
use objc2::{ClassType, Message, MessageReceiver, RefEncode};

use objc_sys::{objc_class, objc_disposeClassPair};

use icrate::AppKit::{
    NSCursor, NSEvent, NSScreen, NSTrackingArea, NSView, NSWindow, NSWindowDelegate,
};
use icrate::Foundation::{NSInteger, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use super::app::{AppInner, AppState};
use super::surface::Surface;
use super::OsError;
use crate::{
    AppHandle, Bitmap, Cursor, Error, Event, MouseButton, Point, RawParent, Rect, Response, Result,
    Size, Window, WindowContext, WindowOptions,
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

struct StateIvar;

unsafe impl IvarType for StateIvar {
    type Type = IvarEncode<Cell<*mut c_void>>;
    const NAME: &'static str = "windowState";
}

#[repr(C)]
pub struct View {
    superclass: NSView,
    state: Ivar<StateIvar>,
}

unsafe impl RefEncode for View {
    const ENCODING_REF: Encoding = NSView::ENCODING_REF;
}

unsafe impl Message for View {}

unsafe impl NSObjectProtocol for View {}
unsafe impl NSWindowDelegate for View {}

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
            return Err(Error::Os(OsError::Other("could not declare NSView subclass")));
        };

        builder.add_static_ivar::<StateIvar>();

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

    fn state(&self) -> &WindowState {
        unsafe { &*(self.state.get() as *const WindowState) }
    }

    pub fn handle_event(&self, event: Event) -> Option<Response> {
        let state_rc = unsafe { Rc::from_raw(self.state.get() as *const WindowState) };
        let state = Rc::clone(&state_rc);
        let _ = Rc::into_raw(state_rc);

        let app = AppHandle::from_inner(AppInner {
            state: Rc::clone(&state.app_state),
        });
        let window = Window::from_inner(WindowInner { state });
        let cx = WindowContext::new(&app, &window);

        if let Ok(mut handler) = window.inner.state.handler.try_borrow_mut() {
            return Some(handler(&cx, event));
        }

        None
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

    unsafe extern "C" fn mouse_moved(&self, _: Sel, event: Option<&NSEvent>) {
        let Some(event) = event else { return; };

        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        self.handle_event(Event::MouseMove(Point {
            x: point.x,
            y: point.y,
        }));
    }

    unsafe extern "C" fn mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        let result = self.handle_event(Event::MouseDown(MouseButton::Left));

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), mouseDown: event];
        }
    }

    unsafe extern "C" fn mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        let result = self.handle_event(Event::MouseUp(MouseButton::Left));

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), mouseUp: event];
        }
    }

    unsafe extern "C" fn right_mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        let result = self.handle_event(Event::MouseDown(MouseButton::Right));

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), rightMouseDown: event];
        }
    }

    unsafe extern "C" fn right_mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        let result = self.handle_event(Event::MouseUp(MouseButton::Right));

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), rightMouseUp: event];
        }
    }

    unsafe extern "C" fn other_mouse_down(&self, _: Sel, event: Option<&NSEvent>) {
        let Some(event) = event else { return; };

        let button_number = event.buttonNumber();
        let result = if let Some(button) = mouse_button_from_number(button_number) {
            self.handle_event(Event::MouseDown(button))
        } else {
            None
        };

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), otherMouseDown: event];
        }
    }

    unsafe extern "C" fn other_mouse_up(&self, _: Sel, event: Option<&NSEvent>) {
        let Some(event) = event else { return; };

        let button_number = event.buttonNumber();
        let result = if let Some(button) = mouse_button_from_number(button_number) {
            self.handle_event(Event::MouseUp(button))
        } else {
            None
        };

        if result != Some(Response::Capture) {
            let () = msg_send![super(self, NSView::class()), otherMouseUp: event];
        }
    }

    unsafe extern "C" fn scroll_wheel(&self, _: Sel, event: Option<&NSEvent>) {
        let Some(event) = event else { return; };

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
    }

    unsafe extern "C" fn cursor_update(&self, _: Sel, _event: Option<&NSEvent>) {
        self.state().update_cursor();
    }

    unsafe extern "C" fn window_should_close(&self, _: Sel, _sender: &NSWindow) -> Bool {
        self.handle_event(Event::Close);

        Bool::NO
    }

    unsafe extern "C" fn dealloc(&self, _: Sel) {
        drop(Rc::from_raw(self.state.get() as *mut WindowState));

        let () = msg_send![super(self, NSView::class()), dealloc];
    }
}

pub struct WindowState {
    view: RefCell<Option<Id<View>>>,
    window: RefCell<Option<Id<NSWindow>>>,
    surface: RefCell<Option<Surface>>,
    cursor: Cell<Cursor>,
    app_state: Rc<AppState>,
    handler: RefCell<Box<dyn FnMut(&WindowContext, Event) -> Response>>,
}

impl WindowState {
    pub fn view(&self) -> Option<Id<View>> {
        self.view.borrow().as_ref().map(|view| view.retain())
    }

    pub fn window(&self) -> Option<Id<NSWindow>> {
        self.window.borrow().clone()
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
            Cursor::Arrow => unsafe { NSCursor::arrowCursor() },
            Cursor::Crosshair => unsafe { NSCursor::crosshairCursor() },
            Cursor::Hand => unsafe { NSCursor::pointingHandCursor() },
            Cursor::IBeam => unsafe { NSCursor::IBeamCursor() },
            Cursor::No => unsafe { NSCursor::operationNotAllowedCursor() },
            Cursor::SizeNs => try_get_cursor(sel!(_windowResizeNorthSouthCursor)),
            Cursor::SizeWe => try_get_cursor(sel!(_windowResizeEastWestCursor)),
            Cursor::SizeNesw => try_get_cursor(sel!(_windowResizeNorthEastSouthWestCursor)),
            Cursor::SizeNwse => try_get_cursor(sel!(_windowResizeNorthWestSouthEastCursor)),
            Cursor::Wait => try_get_cursor(sel!(_waitCursor)),
            Cursor::None => self.app_state.empty_cursor.clone(),
        };

        unsafe {
            ns_cursor.set();
        }
    }

    pub fn close(&self) {
        if let Some(window) = self.window.take() {
            unsafe { window.close() };
        }

        if let Some(view) = self.view.take() {
            unsafe { view.removeFromSuperview() };
        }
    }
}

#[derive(Clone)]
pub struct WindowInner {
    state: Rc<WindowState>,
}

impl WindowInner {
    pub fn open<H>(options: &WindowOptions, app: &AppHandle, handler: H) -> Result<WindowInner>
    where
        H: FnMut(&WindowContext, Event) -> Response + 'static,
    {
        autoreleasepool(|_| {
            if !app.inner.state.open.get() {
                return Err(Error::AppDropped);
            }

            let app_state = &app.inner.state;

            let parent_view = if let Some(parent) = options.parent {
                if let RawParent::Cocoa(parent_view) = parent {
                    Some(parent_view as *const NSView)
                } else {
                    return Err(Error::InvalidWindowHandle);
                }
            } else {
                None
            };

            let origin = if options.parent.is_some() {
                Point::new(0.0, 0.0)
            } else {
                options.position.unwrap_or(Point::new(0.0, 0.0))
            };
            let frame = NSRect::new(
                NSPoint::new(origin.x, origin.y),
                NSSize::new(options.size.width, options.size.height),
            );

            let state = Rc::new(WindowState {
                view: RefCell::new(None),
                window: RefCell::new(None),
                surface: RefCell::new(None),
                cursor: Cell::new(Cursor::Arrow),
                app_state: Rc::clone(app_state),
                handler: RefCell::new(Box::new(handler)),
            });

            let view: Option<Allocated<View>> = unsafe { msg_send_id![app_state.class, alloc] };
            let view: Id<View> = unsafe { msg_send_id![view, initWithFrame: frame] };
            view.state.set(Rc::into_raw(Rc::clone(&state)) as *mut c_void);

            state.view.replace(Some(view.retain()));

            let tracking_options = icrate::AppKit::NSTrackingMouseEnteredAndExited
                | icrate::AppKit::NSTrackingMouseMoved
                | icrate::AppKit::NSTrackingCursorUpdate
                | icrate::AppKit::NSTrackingActiveAlways
                | icrate::AppKit::NSTrackingInVisibleRect
                | icrate::AppKit::NSTrackingEnabledDuringMouseDrag;

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

                let style_mask = icrate::AppKit::NSWindowStyleMaskTitled
                    | icrate::AppKit::NSWindowStyleMaskClosable
                    | icrate::AppKit::NSWindowStyleMaskMiniaturizable
                    | icrate::AppKit::NSWindowStyleMaskResizable;

                let window = unsafe {
                    NSWindow::initWithContentRect_styleMask_backing_defer(
                        NSWindow::alloc(),
                        content_rect,
                        style_mask,
                        icrate::AppKit::NSBackingStoreBuffered,
                        false,
                    )
                };

                unsafe {
                    window.setReleasedWhenClosed(false);

                    window.setTitle(&NSString::from_str(&options.title));

                    let delegate = ProtocolObject::<dyn NSWindowDelegate>::from_ref(&*view);
                    window.setDelegate(Some(&delegate));
                    window.setContentView(Some(&view));

                    if options.position.is_none() {
                        window.center();
                    }
                }

                state.window.replace(Some(window));
            }

            app_state.windows.borrow_mut().insert(Id::as_ptr(&view), Rc::clone(&state));

            let inner = WindowInner { state };

            let scale = inner.scale();

            let surface = Surface::new(
                (scale * options.size.width).round() as usize,
                (scale * options.size.height).round() as usize,
            );

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
                unsafe { window.orderFront(None) };
            }

            if let Some(view) = self.state.view() {
                unsafe { view.setHidden(false) };
            }
        })
    }

    pub fn hide(&self) {
        autoreleasepool(|_| {
            if let Some(window) = self.state.window() {
                unsafe { window.orderOut(None) };
            }

            if let Some(view) = self.state.view() {
                unsafe { view.setHidden(true) };
            }
        })
    }

    pub fn size(&self) -> Size {
        autoreleasepool(|_| {
            if let Some(view) = self.state.view() {
                let frame = unsafe { view.frame() };

                Size::new(frame.size.width, frame.size.height)
            } else {
                Size::new(0.0, 0.0)
            }
        })
    }

    pub fn scale(&self) -> f64 {
        autoreleasepool(|_| {
            if let Some(view) = self.state.view() {
                if let Some(window) = unsafe { view.window() } {
                    return unsafe { window.backingScaleFactor() };
                } else if let Some(screen) = unsafe { NSScreen::screens() }.get(0) {
                    return unsafe { screen.backingScaleFactor() };
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
                self.state.app_state.windows.borrow_mut().remove(&Id::as_ptr(view));
            }

            self.state.close();
        })
    }
}
