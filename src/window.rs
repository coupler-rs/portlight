use std::ffi::{c_ulong, c_void};
use std::fmt;
use std::marker::PhantomData;

use crate::{backend, AppHandle, Result};

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    #[inline]
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[inline]
    pub fn scale(self, scale: f64) -> Point {
        Point::new(self.x * scale, self.y * scale)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    #[inline]
    pub fn new(width: f64, height: f64) -> Size {
        Size { width, height }
    }

    #[inline]
    pub fn scale(self, scale: f64) -> Size {
        Size::new(self.width * scale, self.height * scale)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    #[inline]
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    pub fn scale(self, scale: f64) -> Rect {
        Rect::new(
            self.x * scale,
            self.y * scale,
            self.width * scale,
            self.height * scale,
        )
    }
}

pub struct Bitmap<'a> {
    data: &'a [u32],
    width: usize,
    height: usize,
}

impl<'a> Bitmap<'a> {
    #[inline]
    pub fn new(data: &'a [u32], width: usize, height: usize) -> Bitmap<'a> {
        assert!(width * height == data.len(), "invalid bitmap dimensions");

        Bitmap {
            data,
            width,
            height,
        }
    }

    #[inline]
    pub fn data(&self) -> &'a [u32] {
        self.data
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Cursor {
    Arrow,
    Crosshair,
    Hand,
    IBeam,
    No,
    SizeNs,
    SizeWe,
    SizeNesw,
    SizeNwse,
    Wait,
    None,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Event<'a> {
    Expose(&'a [Rect]),
    Frame,
    Close,
    MouseEnter,
    MouseExit,
    MouseMove(Point),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Scroll(Point),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Response {
    Capture,
    Ignore,
}

#[derive(Copy, Clone, Debug)]
pub enum RawWindow {
    Win32(*mut c_void),
    Cocoa(*mut c_void),
    X11(c_ulong),
}

#[derive(Clone, Debug)]
pub struct WindowOptions {
    pub(crate) title: String,
    pub(crate) position: Option<Point>,
    pub(crate) size: Size,
    pub(crate) parent: Option<RawWindow>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        WindowOptions {
            title: String::new(),
            position: None,
            size: Size::new(0.0, 0.0),
            parent: None,
        }
    }
}

impl WindowOptions {
    pub fn new() -> WindowOptions {
        Self::default()
    }

    pub fn title<S: AsRef<str>>(&mut self, title: S) -> &mut Self {
        self.title = title.as_ref().to_string();
        self
    }

    pub fn position(&mut self, position: Point) -> &mut Self {
        self.position = Some(position);
        self
    }

    pub fn size(&mut self, size: Size) -> &mut Self {
        self.size = size;
        self
    }

    pub unsafe fn raw_parent(&mut self, parent: RawWindow) -> &mut Self {
        self.parent = Some(parent);
        self
    }

    pub fn open<H>(&self, app: &AppHandle, handler: H) -> Result<Window>
    where
        H: FnMut(&WindowContext, Event) -> Response + 'static,
    {
        Ok(Window::from_inner(backend::WindowInner::open(
            self, app, handler,
        )?))
    }
}

pub struct WindowContext<'a> {
    app: &'a AppHandle,
    window: &'a Window,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl<'a> WindowContext<'a> {
    pub(crate) fn new(app: &'a AppHandle, window: &'a Window) -> WindowContext<'a> {
        WindowContext {
            app,
            window,
            _marker: PhantomData,
        }
    }

    pub fn app(&self) -> &AppHandle {
        self.app
    }

    pub fn window(&self) -> &Window {
        self.window
    }
}

#[derive(Clone)]
pub struct Window {
    pub(crate) inner: backend::WindowInner,
    // ensure !Send and !Sync on all platforms
    _marker: PhantomData<*mut ()>,
}

impl Window {
    pub(crate) fn from_inner(inner: backend::WindowInner) -> Window {
        Window {
            inner,
            _marker: PhantomData,
        }
    }

    pub fn show(&self) {
        self.inner.show();
    }

    pub fn hide(&self) {
        self.inner.hide();
    }

    pub fn size(&self) -> Size {
        self.inner.size()
    }

    pub fn scale(&self) -> f64 {
        self.inner.scale()
    }

    pub fn present(&self, bitmap: Bitmap) {
        self.inner.present(bitmap);
    }

    pub fn present_partial(&self, bitmap: Bitmap, rects: &[Rect]) {
        self.inner.present_partial(bitmap, rects);
    }

    pub fn set_cursor(&self, cursor: Cursor) {
        self.inner.set_cursor(cursor);
    }

    pub fn set_mouse_position(&self, position: Point) {
        self.inner.set_mouse_position(position);
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn as_raw(&self) -> Result<RawWindow> {
        self.inner.as_raw()
    }
}

impl fmt::Debug for Window {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Window").finish_non_exhaustive()
    }
}
