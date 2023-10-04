use std::any::Any;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};
use std::time::Duration;
use std::{mem, ptr, result};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::UI::WindowsAndMessaging::{
    self as msg, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PeekMessageW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    TranslateMessage, UnregisterClassW, HCURSOR, HICON, HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW, WNDCLASS_STYLES,
};

use super::{class_name, hinstance, to_wstring};

use super::dpi::DpiFns;
use super::timer::{TimerHandleInner, Timers};
use super::window;
use crate::{App, AppContext, AppMode, AppOptions, Error, IntoInnerError, Result};

fn register_message_class() -> Result<PCWSTR> {
    let class_name = to_wstring(&class_name("message-"));

    let wnd_class = WNDCLASSW {
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(message_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance(),
        hIcon: HICON(0),
        hCursor: HCURSOR(0),
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

unsafe fn unregister_message_class(class: PCWSTR) {
    let _ = UnregisterClassW(class, hinstance());
}

pub unsafe extern "system" fn message_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let app_state_ptr = GetWindowLongPtrW(hwnd, msg::GWLP_USERDATA) as *mut AppState;
    if !app_state_ptr.is_null() {
        let app_state_weak = Weak::from_raw(app_state_ptr);
        let app_state = app_state_weak.clone();
        let _ = app_state_weak.into_raw();

        match msg {
            msg::WM_TIMER => {
                if let Some(app_state) = app_state.upgrade() {
                    app_state.timers.handle_timer(&app_state, wparam.0);
                }
            }
            msg::WM_DESTROY => {
                drop(Weak::from_raw(app_state_ptr));
                SetWindowLongPtrW(hwnd, msg::GWLP_USERDATA, 0);
            }
            _ => {}
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

pub struct AppState {
    pub message_class: PCWSTR,
    pub message_hwnd: HWND,
    pub window_class: PCWSTR,
    pub dpi: DpiFns,
    pub timers: Timers,
    pub data: RefCell<Option<Box<dyn Any>>>,
}

impl Drop for AppState {
    fn drop(&mut self) {
        self.timers.kill_timers(&self);

        unsafe {
            window::unregister_class(self.window_class);

            let _ = DestroyWindow(self.message_hwnd);
            unregister_message_class(self.message_class);
        }
    }
}

pub struct AppInner<T> {
    pub state: Rc<AppState>,
    _marker: PhantomData<T>,
}

impl<T: 'static> AppInner<T> {
    pub fn new<F>(options: &AppOptions, build: F) -> Result<AppInner<T>>
    where
        F: FnOnce(&AppContext<T>) -> Result<T>,
        T: 'static,
    {
        let message_class = register_message_class()?;

        let message_hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                message_class,
                PCWSTR(ptr::null()),
                WINDOW_STYLE(0),
                msg::CW_USEDEFAULT,
                msg::CW_USEDEFAULT,
                0,
                0,
                HWND(0),
                HMENU(0),
                hinstance(),
                None,
            )
        };
        if message_hwnd == HWND(0) {
            return Err(windows::core::Error::from_win32().into());
        }

        let window_class = window::register_class()?;

        let dpi = DpiFns::load();
        if options.mode == AppMode::Owner {
            dpi.set_dpi_aware();
        }

        let timers = Timers::new();

        let state = Rc::new(AppState {
            message_class,
            message_hwnd,
            window_class,
            dpi,
            timers,
            data: RefCell::new(None),
        });

        let state_ptr = Weak::into_raw(Rc::downgrade(&state));
        unsafe {
            SetWindowLongPtrW(message_hwnd, msg::GWLP_USERDATA, state_ptr as isize);
        }

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
    }

    pub fn run(&mut self) -> Result<()> {
        if self.state.data.try_borrow().is_err() {
            return Err(Error::InsideEventHandler);
        }

        loop {
            unsafe {
                let mut msg: MSG = mem::zeroed();

                let result = GetMessageW(&mut msg, HWND(0), 0, 0);
                if result.0 < 0 {
                    return Err(windows::core::Error::from_win32().into());
                } else if result.0 == 0 {
                    return Ok(());
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    pub fn poll(&mut self) -> Result<()> {
        if self.state.data.try_borrow().is_err() {
            return Err(Error::InsideEventHandler);
        }

        loop {
            unsafe {
                let mut msg: MSG = mem::zeroed();

                let result = PeekMessageW(&mut msg, HWND(0), 0, 0, msg::PM_REMOVE);
                if result.0 == 0 {
                    return Ok(());
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    pub fn into_inner(self) -> result::Result<T, IntoInnerError<App<T>>> {
        if let Ok(mut data) = self.state.data.try_borrow_mut() {
            if let Some(data) = data.take() {
                return Ok(*data.downcast().unwrap());
            }
        }

        Err(IntoInnerError::new(
            Error::InsideEventHandler,
            App::from_inner(self),
        ))
    }
}

impl<T> Drop for AppInner<T> {
    fn drop(&mut self) {
        if let Ok(mut data) = self.state.data.try_borrow_mut() {
            drop(data.take());
        }
    }
}

pub struct AppContextInner<'a, T> {
    pub state: &'a Rc<AppState>,
    pub _marker: PhantomData<T>,
}

impl<'a, T: 'static> AppContextInner<'a, T> {
    pub(super) fn new(state: &'a Rc<AppState>) -> AppContextInner<'a, T> {
        AppContextInner {
            state,
            _marker: PhantomData,
        }
    }

    pub fn set_timer<H>(&self, duration: Duration, handler: H) -> TimerHandleInner
    where
        H: 'static,
        H: FnMut(&mut T, &AppContext<T>),
    {
        self.state.timers.set_timer(self.state, duration, handler)
    }

    pub fn exit(&self) {
        unsafe {
            PostQuitMessage(0);
        }
    }
}
