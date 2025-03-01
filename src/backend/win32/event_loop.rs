use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{self, AssertUnwindSafe};
use std::rc::{Rc, Weak};
use std::{mem, ptr};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{HBRUSH, HMONITOR};
use windows::Win32::UI::WindowsAndMessaging::{
    self as msg, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PeekMessageW, PostQuitMessage, RegisterClassW, SetWindowLongPtrW,
    TranslateMessage, UnregisterClassW, HCURSOR, HICON, HMENU, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW, WNDCLASS_STYLES,
};

use super::dpi::DpiFns;
use super::timer::Timers;
use super::vsync::VsyncThreads;
use super::window::{self, WindowState};
use super::{class_name, hinstance, to_wstring, WM_USER_VBLANK};
use crate::{Error, EventLoopMode, EventLoopOptions, Result};

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
    let event_loop_state_ptr = GetWindowLongPtrW(hwnd, msg::GWLP_USERDATA) as *mut EventLoopState;
    if event_loop_state_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }

    let event_loop_state_weak = Weak::from_raw(event_loop_state_ptr);
    let event_loop_state = event_loop_state_weak.clone();
    let _ = event_loop_state_weak.into_raw();

    let Some(event_loop_state) = event_loop_state.upgrade() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| match msg {
        msg::WM_TIMER => {
            event_loop_state.timers.handle_timer(wparam.0);
        }
        WM_USER_VBLANK => {
            event_loop_state
                .vsync_threads
                .handle_vblank(&event_loop_state, HMONITOR(lparam.0));
        }
        msg::WM_DESTROY => {
            SetWindowLongPtrW(hwnd, msg::GWLP_USERDATA, 0);
            drop(Rc::from_raw(event_loop_state_ptr));
        }
        _ => {}
    }));

    if let Err(panic) = result {
        event_loop_state.propagate_panic(panic);
    }

    // If a panic occurs while dropping the Rc<EventLoopState>, the only thing left to do is abort.
    if let Err(_panic) = panic::catch_unwind(AssertUnwindSafe(move || drop(event_loop_state))) {
        std::process::abort();
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

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
    pub message_class: PCWSTR,
    pub message_hwnd: HWND,
    pub window_class: PCWSTR,
    pub dpi: DpiFns,
    pub timers: Timers,
    pub vsync_threads: VsyncThreads,
    pub windows: RefCell<HashMap<isize, Rc<WindowState>>>,
}

impl EventLoopState {
    pub(crate) fn propagate_panic(&self, panic: Box<dyn Any + Send + 'static>) {
        // If we own the event loop, exit and propagate the panic upwards. Otherwise, just abort.
        if self.running.get() {
            self.panic.set(Some(panic));
            unsafe { PostQuitMessage(0) };
        } else {
            std::process::abort();
        }
    }
}

impl Drop for EventLoopState {
    fn drop(&mut self) {
        unsafe { window::unregister_class(self.window_class) };

        self.vsync_threads.join_all();

        unsafe {
            let _ = DestroyWindow(self.message_hwnd);
            unregister_message_class(self.message_class);
        }
    }
}

impl EventLoopState {
    pub fn new(options: &EventLoopOptions) -> Result<Rc<EventLoopState>> {
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
        if options.mode == EventLoopMode::Owner {
            dpi.set_dpi_aware();
        }

        let timers = Timers::new();

        let vsync_threads = VsyncThreads::new();

        let state = Rc::new(EventLoopState {
            running: Cell::new(false),
            panic: Cell::new(None),
            message_class,
            message_hwnd,
            window_class,
            dpi,
            timers,
            vsync_threads,
            windows: RefCell::new(HashMap::new()),
        });

        let state_ptr = Weak::into_raw(Rc::downgrade(&state));
        unsafe {
            SetWindowLongPtrW(message_hwnd, msg::GWLP_USERDATA, state_ptr as isize);
        }

        state.vsync_threads.init(&state);

        Ok(state)
    }

    pub fn run(&self) -> Result<()> {
        let _run_guard = RunGuard::new(&self.running)?;

        let result = loop {
            unsafe {
                let mut msg: MSG = mem::zeroed();

                let result = GetMessageW(&mut msg, HWND(0), 0, 0);
                #[allow(clippy::comparison_chain)]
                if result.0 < 0 {
                    break Err(windows::core::Error::from_win32().into());
                } else if result.0 == 0 {
                    break Ok(());
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };

        if let Some(panic) = self.panic.take() {
            panic::resume_unwind(panic);
        }

        result
    }

    pub fn exit(&self) {
        if self.running.get() {
            unsafe { PostQuitMessage(0) };
        }
    }

    pub fn poll(&self) -> Result<()> {
        let _run_guard = RunGuard::new(&self.running)?;

        loop {
            unsafe {
                let mut msg: MSG = mem::zeroed();

                let result = PeekMessageW(&mut msg, HWND(0), 0, 0, msg::PM_REMOVE);
                if result.0 == 0 {
                    break;
                }

                if msg.message == msg::WM_QUIT {
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        if let Some(panic) = self.panic.take() {
            panic::resume_unwind(panic);
        }

        Ok(())
    }
}
