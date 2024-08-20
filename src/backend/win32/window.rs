use std::alloc::{alloc, dealloc, Layout};
use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_void};
use std::mem::MaybeUninit;
use std::rc::Rc;
use std::{mem, ptr, slice};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{FALSE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{self as gdi, HBRUSH};
use windows::Win32::UI::Controls::{HOVER_DEFAULT, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    self as msg, AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetWindowLongPtrW, LoadCursorW, RegisterClassW, SetCursor, SetCursorPos, SetWindowLongPtrW,
    ShowWindow, UnregisterClassW, CREATESTRUCTW, HCURSOR, HICON, HMENU, WINDOW_EX_STYLE, WNDCLASSW,
};

use super::app::{AppInner, AppState};
use super::{class_name, hinstance, to_wstring};
use crate::{
    AppHandle, Bitmap, Cursor, Error, Event, MouseButton, Point, RawWindow, Rect, Response, Result,
    Size, Window, WindowContext, WindowOptions,
};

#[allow(non_snake_case)]
fn LOWORD(l: u32) -> u16 {
    (l & 0xffff) as u16
}

#[allow(non_snake_case)]
fn HIWORD(l: u32) -> u16 {
    ((l >> 16) & 0xffff) as u16
}

#[allow(non_snake_case)]
fn GET_X_LPARAM(lp: LPARAM) -> i16 {
    LOWORD(lp.0 as u32) as i16
}

#[allow(non_snake_case)]
fn GET_Y_LPARAM(lp: LPARAM) -> i16 {
    HIWORD(lp.0 as u32) as i16
}

#[allow(non_snake_case)]
fn GET_XBUTTON_WPARAM(wParam: WPARAM) -> u16 {
    HIWORD(wParam.0 as u32)
}

#[allow(non_snake_case)]
fn GET_WHEEL_DELTA_WPARAM(wParam: WPARAM) -> i16 {
    HIWORD(wParam.0 as u32) as i16
}

const WHEEL_DELTA: u16 = 120;

pub fn register_class() -> Result<PCWSTR> {
    let class_name = to_wstring(&class_name("window-"));

    let wnd_class = WNDCLASSW {
        style: msg::CS_HREDRAW | msg::CS_VREDRAW | msg::CS_OWNDC,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance(),
        hIcon: HICON(0),
        hCursor: unsafe { LoadCursorW(HINSTANCE(0), msg::IDC_ARROW)? },
        hbrBackground: HBRUSH(0),
        lpszMenuName: PCWSTR(ptr::null()),
        lpszClassName: PCWSTR(class_name.as_ptr()),
    };

    let class = unsafe { RegisterClassW(&wnd_class) };
    if class == 0 {
        return Err(windows::core::Error::from_win32().into());
    }

    Ok(PCWSTR(class as *const u16))
}

pub unsafe fn unregister_class(class: PCWSTR) {
    let _ = UnregisterClassW(class, hinstance());
}

pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == msg::WM_NCCREATE {
        let create_struct = &*(lparam.0 as *const CREATESTRUCTW);
        let app_state = &*(create_struct.lpCreateParams as *const AppState);

        #[allow(non_snake_case)]
        if let Some(EnableNonClientDpiScaling) = app_state.dpi.EnableNonClientDpiScaling {
            EnableNonClientDpiScaling(hwnd);
        }

        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let state_ptr = GetWindowLongPtrW(hwnd, msg::GWLP_USERDATA) as *const WindowState;
    if !state_ptr.is_null() {
        let app_state = Rc::clone(&(*state_ptr).app_state);

        match msg {
            msg::WM_SETCURSOR => {
                if LOWORD(lparam.0 as u32) == msg::HTCLIENT as u16 {
                    app_state.catch_unwind(|| {
                        let state = WindowState::from_raw(state_ptr);
                        state.update_cursor();
                    });
                    return LRESULT(1);
                }
            }
            msg::WM_ERASEBKGND => {
                return LRESULT(1);
            }
            msg::WM_PAINT => {
                app_state.catch_unwind(|| {
                    let mut rects = Vec::new();

                    let rgn = gdi::CreateRectRgn(0, 0, 0, 0);
                    gdi::GetUpdateRgn(hwnd, rgn, false);
                    let size = gdi::GetRegionData(rgn, 0, None);
                    if size != 0 {
                        let align = mem::align_of::<gdi::RGNDATA>();
                        let layout = Layout::from_size_align(size as usize, align).unwrap();
                        let ptr = alloc(layout) as *mut gdi::RGNDATA;

                        let result = gdi::GetRegionData(rgn, size, Some(ptr));
                        if result == size {
                            let count = (*ptr).rdh.nCount as usize;

                            let buffer_ptr = ptr::addr_of!((*ptr).Buffer) as *const RECT;
                            let buffer = slice::from_raw_parts(buffer_ptr, count);

                            rects.reserve_exact(count);
                            for rect in buffer {
                                rects.push(Rect {
                                    x: rect.left as f64,
                                    y: rect.top as f64,
                                    width: (rect.right - rect.left) as f64,
                                    height: (rect.bottom - rect.top) as f64,
                                });
                            }
                        }

                        dealloc(ptr as *mut u8, layout);
                    }
                    gdi::DeleteObject(rgn);

                    // Only validate the dirty region if we successfully invoked the event handler.
                    let state = WindowState::from_raw(state_ptr);
                    if state.handle_event(Event::Expose(&rects)).is_some() {
                        gdi::ValidateRgn(hwnd, gdi::HRGN(0));
                    }
                });

                return LRESULT(0);
            }
            msg::WM_MOUSEMOVE => {
                app_state.catch_unwind(|| {
                    let state = WindowState::from_raw(state_ptr);

                    if !state.mouse_in_window.get() {
                        state.mouse_in_window.set(true);
                        state.handle_event(Event::MouseEnter);

                        let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                            cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: HOVER_DEFAULT,
                        });
                    }

                    let point_physical = Point {
                        x: GET_X_LPARAM(lparam) as f64,
                        y: GET_Y_LPARAM(lparam) as f64,
                    };
                    let point = point_physical.scale(state.scale().recip());

                    state.handle_event(Event::MouseMove(point));
                });

                return LRESULT(0);
            }
            WM_MOUSELEAVE => {
                app_state.catch_unwind(|| {
                    let state = WindowState::from_raw(state_ptr);
                    state.mouse_in_window.set(false);
                    state.handle_event(Event::MouseExit);
                });
            }
            msg::WM_LBUTTONDOWN
            | msg::WM_LBUTTONUP
            | msg::WM_MBUTTONDOWN
            | msg::WM_MBUTTONUP
            | msg::WM_RBUTTONDOWN
            | msg::WM_RBUTTONUP
            | msg::WM_XBUTTONDOWN
            | msg::WM_XBUTTONUP => {
                let mut result = None;

                app_state.catch_unwind(|| {
                    let button = match msg {
                        msg::WM_LBUTTONDOWN | msg::WM_LBUTTONUP => Some(MouseButton::Left),
                        msg::WM_MBUTTONDOWN | msg::WM_MBUTTONUP => Some(MouseButton::Middle),
                        msg::WM_RBUTTONDOWN | msg::WM_RBUTTONUP => Some(MouseButton::Right),
                        msg::WM_XBUTTONDOWN | msg::WM_XBUTTONUP => {
                            match GET_XBUTTON_WPARAM(wparam) {
                                msg::XBUTTON1 => Some(MouseButton::Back),
                                msg::XBUTTON2 => Some(MouseButton::Forward),
                                _ => None,
                            }
                        }
                        _ => None,
                    };

                    if let Some(button) = button {
                        let event = match msg {
                            msg::WM_LBUTTONDOWN
                            | msg::WM_MBUTTONDOWN
                            | msg::WM_RBUTTONDOWN
                            | msg::WM_XBUTTONDOWN => Some(Event::MouseDown(button)),
                            msg::WM_LBUTTONUP
                            | msg::WM_MBUTTONUP
                            | msg::WM_RBUTTONUP
                            | msg::WM_XBUTTONUP => Some(Event::MouseUp(button)),
                            _ => None,
                        };

                        if let Some(event) = event {
                            let state = WindowState::from_raw(state_ptr);

                            match event {
                                Event::MouseDown(_) => {
                                    state.mouse_down_count.set(state.mouse_down_count.get() + 1);
                                    if state.mouse_down_count.get() == 1 {
                                        SetCapture(hwnd);
                                    }
                                }
                                Event::MouseUp(_) => {
                                    state.mouse_down_count.set(state.mouse_down_count.get() - 1);
                                    if state.mouse_down_count.get() == 0 {
                                        let _ = ReleaseCapture();
                                    }
                                }
                                _ => {}
                            }

                            result = state.handle_event(event);
                        }
                    }
                });

                if result == Some(Response::Capture) {
                    return LRESULT(0);
                }
            }
            msg::WM_MOUSEWHEEL | msg::WM_MOUSEHWHEEL => {
                let mut result = None;

                app_state.catch_unwind(|| {
                    let delta = GET_WHEEL_DELTA_WPARAM(wparam) as f64 / WHEEL_DELTA as f64;
                    let point = match msg {
                        msg::WM_MOUSEWHEEL => Point::new(0.0, delta),
                        msg::WM_MOUSEHWHEEL => Point::new(delta, 0.0),
                        _ => unreachable!(),
                    };

                    let state = WindowState::from_raw(state_ptr);
                    result = state.handle_event(Event::Scroll(point));
                });

                if result == Some(Response::Capture) {
                    return LRESULT(0);
                }
            }
            msg::WM_CLOSE => {
                app_state.catch_unwind(|| {
                    let state = WindowState::from_raw(state_ptr);
                    state.handle_event(Event::Close);
                });
                return LRESULT(0);
            }
            msg::WM_DESTROY => {
                SetWindowLongPtrW(hwnd, msg::GWLP_USERDATA, 0);
                app_state.catch_unwind(|| {
                    drop(Rc::from_raw(state_ptr));
                });
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub struct WindowState {
    hwnd: Cell<Option<HWND>>,
    mouse_down_count: Cell<isize>,
    mouse_in_window: Cell<bool>,
    cursor: Cell<Cursor>,
    app_state: Rc<AppState>,
    handler: RefCell<Box<dyn FnMut(&WindowContext, Event) -> Response>>,
}

impl WindowState {
    unsafe fn from_raw(ptr: *const WindowState) -> Rc<WindowState> {
        let state_rc = Rc::from_raw(ptr);
        let state = Rc::clone(&state_rc);
        let _ = Rc::into_raw(state_rc);

        state
    }

    fn update_cursor(&self) {
        unsafe {
            let hcursor = match self.cursor.get() {
                Cursor::Arrow => LoadCursorW(HINSTANCE(0), msg::IDC_ARROW),
                Cursor::Crosshair => LoadCursorW(HINSTANCE(0), msg::IDC_CROSS),
                Cursor::Hand => LoadCursorW(HINSTANCE(0), msg::IDC_HAND),
                Cursor::IBeam => LoadCursorW(HINSTANCE(0), msg::IDC_IBEAM),
                Cursor::No => LoadCursorW(HINSTANCE(0), msg::IDC_NO),
                Cursor::SizeNs => LoadCursorW(HINSTANCE(0), msg::IDC_SIZENS),
                Cursor::SizeWe => LoadCursorW(HINSTANCE(0), msg::IDC_SIZEWE),
                Cursor::SizeNesw => LoadCursorW(HINSTANCE(0), msg::IDC_SIZENESW),
                Cursor::SizeNwse => LoadCursorW(HINSTANCE(0), msg::IDC_SIZENWSE),
                Cursor::Wait => LoadCursorW(HINSTANCE(0), msg::IDC_WAIT),
                Cursor::None => Ok(HCURSOR(0)),
            };

            if let Ok(hcursor) = hcursor {
                SetCursor(hcursor);
            }
        }
    }

    pub fn scale(&self) -> f64 {
        if let Some(hwnd) = self.hwnd.get() {
            let dpi = unsafe { self.app_state.dpi.dpi_for_window(hwnd) };

            dpi as f64 / msg::USER_DEFAULT_SCREEN_DPI as f64
        } else {
            1.0
        }
    }

    pub fn handle_event(self: &Rc<WindowState>, event: Event) -> Option<Response> {
        if let Ok(mut handler) = self.handler.try_borrow_mut() {
            let app = AppHandle::from_inner(AppInner {
                state: Rc::clone(&self.app_state),
            });
            let window = Window::from_inner(WindowInner {
                state: Rc::clone(self),
            });
            let cx = WindowContext::new(&app, &window);
            return Some(handler(&cx, event));
        }

        None
    }

    pub fn close(&self) {
        if let Some(hwnd) = self.hwnd.take() {
            let _ = unsafe { DestroyWindow(hwnd) };
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
        if !app.inner.state.open.get() {
            return Err(Error::AppDropped);
        }

        unsafe {
            let window_name = to_wstring(&options.title);

            let mut style = msg::WS_CLIPCHILDREN | msg::WS_CLIPSIBLINGS;

            if options.parent.is_some() {
                style |= msg::WS_CHILD;
            } else {
                style |= msg::WS_CAPTION
                    | msg::WS_SIZEBOX
                    | msg::WS_SYSMENU
                    | msg::WS_MINIMIZEBOX
                    | msg::WS_MAXIMIZEBOX;
            }

            let parent = if let Some(parent) = options.parent {
                if let RawWindow::Win32(hwnd) = parent {
                    HWND(hwnd as isize)
                } else {
                    return Err(Error::InvalidWindowHandle);
                }
            } else {
                HWND(0)
            };

            let dpi = if options.parent.is_some() {
                app.inner.state.dpi.dpi_for_window(parent)
            } else {
                app.inner.state.dpi.dpi_for_primary_monitor()
            };
            let scale = dpi as f64 / msg::USER_DEFAULT_SCREEN_DPI as f64;

            let position_physical = options.position.unwrap_or(Point::new(0.0, 0.0)).scale(scale);
            let size_physical = options.size.scale(scale);

            let mut rect = RECT {
                left: position_physical.x.round() as i32,
                top: position_physical.y.round() as i32,
                right: (position_physical.x + size_physical.width).round() as i32,
                bottom: (position_physical.y + size_physical.height).round() as i32,
            };
            let _ = AdjustWindowRectEx(&mut rect, style, FALSE, WINDOW_EX_STYLE(0));

            let (x, y) = if options.position.is_some() {
                (rect.top, rect.left)
            } else {
                (msg::CW_USEDEFAULT, msg::CW_USEDEFAULT)
            };

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                app.inner.state.window_class,
                PCWSTR(window_name.as_ptr()),
                style,
                x,
                y,
                rect.right - rect.left,
                rect.bottom - rect.top,
                parent,
                HMENU(0),
                hinstance(),
                Some(Rc::as_ptr(&app.inner.state) as *const c_void),
            );
            if hwnd == HWND(0) {
                return Err(windows::core::Error::from_win32().into());
            }

            let state = Rc::new(WindowState {
                hwnd: Cell::new(Some(hwnd)),
                mouse_down_count: Cell::new(0),
                mouse_in_window: Cell::new(false),
                cursor: Cell::new(Cursor::Arrow),
                app_state: Rc::clone(&app.inner.state),
                handler: RefCell::new(Box::new(handler)),
            });

            let state_ptr = Rc::into_raw(Rc::clone(&state));
            SetWindowLongPtrW(hwnd, msg::GWLP_USERDATA, state_ptr as isize);

            app.inner.state.windows.borrow_mut().insert(hwnd.0, Rc::clone(&state));

            Ok(WindowInner { state })
        }
    }

    pub fn show(&self) {
        if let Some(hwnd) = self.state.hwnd.get() {
            unsafe { ShowWindow(hwnd, msg::SW_SHOWNORMAL) };
        }
    }

    pub fn hide(&self) {
        if let Some(hwnd) = self.state.hwnd.get() {
            unsafe { ShowWindow(hwnd, msg::SW_HIDE) };
        }
    }

    pub fn size(&self) -> Size {
        if let Some(hwnd) = self.state.hwnd.get() {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            unsafe {
                let _ = GetClientRect(hwnd, &mut rect);
            }

            let size_physical = Size::new(
                (rect.right - rect.left) as f64,
                (rect.bottom - rect.top) as f64,
            );
            size_physical.scale(self.scale().recip())
        } else {
            Size::new(0.0, 0.0)
        }
    }

    pub fn scale(&self) -> f64 {
        self.state.scale()
    }

    pub fn present(&self, bitmap: Bitmap) {
        self.present_inner(bitmap, None);
    }

    pub fn present_partial(&self, bitmap: Bitmap, rects: &[Rect]) {
        self.present_inner(bitmap, Some(rects));
    }

    fn present_inner(&self, bitmap: Bitmap, rects: Option<&[Rect]>) {
        if let Some(hwnd) = self.state.hwnd.get() {
            unsafe {
                let hdc = gdi::GetDC(hwnd);
                if hdc != gdi::HDC(0) {
                    if let Some(rects) = rects {
                        let (layout, _) = Layout::new::<gdi::RGNDATAHEADER>()
                            .extend(Layout::array::<RECT>(rects.len()).unwrap())
                            .unwrap();
                        let ptr = alloc(layout) as *mut gdi::RGNDATA;

                        let buffer_ptr = ptr::addr_of!((*ptr).Buffer) as *mut MaybeUninit<RECT>;
                        let buffer = slice::from_raw_parts_mut(buffer_ptr, rects.len());
                        for (src, dst) in rects.iter().zip(buffer.iter_mut()) {
                            dst.write(RECT {
                                left: src.x.round() as i32,
                                top: src.y.round() as i32,
                                right: (src.x + src.width).round() as i32,
                                bottom: (src.y + src.height).round() as i32,
                            });
                        }

                        let buffer = slice::from_raw_parts(buffer_ptr as *const RECT, rects.len());
                        let bounds = if buffer.is_empty() {
                            RECT {
                                left: 0,
                                top: 0,
                                right: 0,
                                bottom: 0,
                            }
                        } else {
                            let mut bounds = buffer[0];
                            for rect in buffer {
                                bounds.left = bounds.left.min(rect.left);
                                bounds.top = bounds.top.min(rect.top);
                                bounds.right = bounds.right.max(rect.right);
                                bounds.bottom = bounds.bottom.max(rect.bottom);
                            }
                            bounds
                        };

                        (*ptr).rdh = gdi::RGNDATAHEADER {
                            dwSize: mem::size_of::<gdi::RGNDATAHEADER>() as u32,
                            iType: gdi::RDH_RECTANGLES,
                            nCount: rects.len() as u32,
                            nRgnSize: layout.size() as u32,
                            rcBound: bounds,
                        };

                        let rgn = gdi::ExtCreateRegion(None, layout.size() as u32, ptr);
                        gdi::SelectClipRgn(hdc, rgn);
                        gdi::DeleteObject(rgn);

                        dealloc(ptr as *mut u8, layout);
                    }

                    let bitmap_info = gdi::BITMAPINFO {
                        bmiHeader: gdi::BITMAPINFOHEADER {
                            biSize: mem::size_of::<gdi::BITMAPINFOHEADER>() as u32,
                            biWidth: bitmap.width() as i32,
                            biHeight: -(bitmap.height() as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: gdi::BI_RGB.0,
                            ..mem::zeroed()
                        },
                        ..mem::zeroed()
                    };

                    gdi::SetDIBitsToDevice(
                        hdc,
                        0,
                        0,
                        bitmap.width() as u32,
                        bitmap.height() as u32,
                        0,
                        0,
                        0,
                        bitmap.height() as u32,
                        bitmap.data().as_ptr() as *const c_void,
                        &bitmap_info,
                        gdi::DIB_RGB_COLORS,
                    );

                    if rects.is_some() {
                        gdi::SelectClipRgn(hdc, gdi::HRGN(0));
                    }

                    gdi::ReleaseDC(hwnd, hdc);
                }
            }
        }
    }

    pub fn set_cursor(&self, cursor: Cursor) {
        self.state.cursor.set(cursor);
        self.state.update_cursor();
    }

    pub fn set_mouse_position(&self, position: Point) {
        if let Some(hwnd) = self.state.hwnd.get() {
            let position_physical = position.scale(self.scale());

            let mut point = POINT {
                x: position_physical.x as c_int,
                y: position_physical.y as c_int,
            };
            unsafe {
                gdi::ClientToScreen(hwnd, &mut point);
                let _ = SetCursorPos(point.x, point.y);
            }
        }
    }

    pub fn close(&self) {
        if let Some(hwnd) = self.state.hwnd.get() {
            self.state.app_state.windows.borrow_mut().remove(&hwnd.0);
        }

        self.state.close();
    }

    pub fn as_raw(&self) -> Result<RawWindow> {
        if let Some(hwnd) = self.state.hwnd.get() {
            Ok(RawWindow::Win32(hwnd.0 as *mut c_void))
        } else {
            Err(Error::WindowClosed)
        }
    }
}
