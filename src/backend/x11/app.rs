use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::present::{self, ConnectionExt as _};
use x11rb::protocol::shm;
use x11rb::protocol::xproto::{self, Button, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;
use x11rb::{cursor, protocol, resource_manager};

use super::timer::{TimerInner, Timers};
use super::window::WindowState;
use crate::{AppOptions, Cursor, Error, Event, MouseButton, Point, Rect, Result, TimerContext};

fn mouse_button_from_code(code: Button) -> Option<MouseButton> {
    match code {
        1 => Some(MouseButton::Left),
        2 => Some(MouseButton::Middle),
        3 => Some(MouseButton::Right),
        8 => Some(MouseButton::Back),
        9 => Some(MouseButton::Forward),
        _ => None,
    }
}

fn scroll_delta_from_code(code: Button) -> Option<Point> {
    match code {
        4 => Some(Point::new(0.0, 1.0)),
        5 => Some(Point::new(0.0, -1.0)),
        6 => Some(Point::new(-1.0, 0.0)),
        7 => Some(Point::new(1.0, 0.0)),
        _ => None,
    }
}

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        WM_PROTOCOLS,
        WM_DELETE_WINDOW,
        _NET_WM_NAME,
        UTF8_STRING,
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum RunState {
    Stopped,
    Running,
    Exiting,
}

struct RunGuard<'a> {
    run_state: &'a Cell<RunState>,
}

impl<'a> RunGuard<'a> {
    fn new(run_state: &'a Cell<RunState>) -> Result<RunGuard<'a>> {
        if run_state.get() == RunState::Running {
            return Err(Error::AlreadyRunning);
        }

        run_state.set(RunState::Running);

        Ok(RunGuard { run_state })
    }
}

impl<'a> Drop for RunGuard<'a> {
    fn drop(&mut self) {
        self.run_state.set(RunState::Stopped);
    }
}

pub struct AppState {
    pub open: Cell<bool>,
    pub run_state: Cell<RunState>,
    pub connection: RustConnection,
    pub screen_index: usize,
    pub atoms: Atoms,
    pub shm_supported: bool,
    pub present_supported: bool,
    pub resources: resource_manager::Database,
    pub cursor_handle: cursor::Handle,
    pub cursor_cache: RefCell<HashMap<Cursor, xproto::Cursor>>,
    pub scale: f64,
    pub windows: RefCell<HashMap<Window, Rc<WindowState>>>,
    pub timers: Timers,
}

impl Drop for AppState {
    fn drop(&mut self) {
        for (_, cursor) in self.cursor_cache.take() {
            let _ = self.connection.free_cursor(cursor);
        }
        let _ = self.connection.flush();
    }
}

#[derive(Clone)]
pub struct AppInner {
    pub(super) state: Rc<AppState>,
}

impl AppInner {
    pub fn new(_options: &AppOptions) -> Result<AppInner> {
        let (connection, screen_index) = x11rb::connect(None)?;
        let atoms = Atoms::new(&connection)?.reply()?;
        let shm_supported = connection.extension_information(shm::X11_EXTENSION_NAME)?.is_some();
        let present_supported =
            connection.extension_information(present::X11_EXTENSION_NAME)?.is_some();
        let resources = resource_manager::new_from_default(&connection)?;
        let cursor_handle = cursor::Handle::new(&connection, screen_index, &resources)?.reply()?;

        let scale = if let Ok(Some(dpi)) = resources.get_value::<u32>("Xft.dpi", "") {
            dpi as f64 / 96.0
        } else {
            1.0
        };

        let state = Rc::new(AppState {
            open: Cell::new(true),
            run_state: Cell::new(RunState::Stopped),
            connection,
            screen_index,
            shm_supported,
            present_supported,
            atoms,
            resources,
            cursor_handle,
            cursor_cache: RefCell::new(HashMap::new()),
            scale,
            windows: RefCell::new(HashMap::new()),
            timers: Timers::new(),
        });

        let inner = AppInner { state };

        Ok(inner)
    }

    pub fn set_timer<H>(&self, duration: Duration, handler: H) -> Result<TimerInner>
    where
        H: FnMut(&TimerContext) + 'static,
    {
        if !self.state.open.get() {
            return Err(Error::AppDropped);
        }

        Ok(self.state.timers.set_timer(&self.state, duration, handler))
    }

    pub fn run(&self) -> Result<()> {
        if !self.state.open.get() {
            return Err(Error::AppDropped);
        }

        let _run_guard = RunGuard::new(&self.state.run_state)?;

        let fd = self.as_raw_fd();

        loop {
            self.drain_events()?;
            self.state.timers.poll(&self.state);
            self.drain_events()?;

            if self.state.run_state.get() == RunState::Exiting {
                break;
            }

            let mut fds = [libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            }];

            let timeout = if let Some(next_time) = self.state.timers.next_time() {
                let duration = next_time.saturating_duration_since(Instant::now());
                duration.as_millis() as i32
            } else {
                -1
            };

            unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as u64, timeout) };
        }

        Ok(())
    }

    pub fn exit(&self) {
        self.state.run_state.set(RunState::Exiting);
    }

    pub fn poll(&self) -> Result<()> {
        if !self.state.open.get() {
            return Err(Error::AppDropped);
        }

        if self.state.run_state.get() != RunState::Stopped {
            return Err(Error::AlreadyRunning);
        }

        let _run_guard = RunGuard::new(&self.state.run_state)?;

        self.drain_events()?;
        self.state.timers.poll(&self.state);
        self.drain_events()?;

        Ok(())
    }

    fn drain_events(&self) -> Result<()> {
        loop {
            if self.state.run_state.get() == RunState::Exiting {
                break;
            }

            let Some(event) = self.state.connection.poll_for_event()? else {
                break;
            };

            match event {
                protocol::Event::Expose(event) => {
                    let window = self.state.windows.borrow().get(&event.window).cloned();
                    if let Some(window) = window {
                        let rect_physical = Rect {
                            x: event.x as f64,
                            y: event.y as f64,
                            width: event.width as f64,
                            height: event.height as f64,
                        };
                        let rect = rect_physical.scale(self.state.scale.recip());

                        window.expose_rects.borrow_mut().push(rect);

                        if event.count == 0 {
                            let rects = window.expose_rects.take();
                            window.handle_event(Event::Expose(&rects));
                        }
                    }
                }
                protocol::Event::ClientMessage(event) => {
                    if event.format == 32
                        && event.data.as_data32()[0] == self.state.atoms.WM_DELETE_WINDOW
                    {
                        let window = self.state.windows.borrow().get(&event.window).cloned();
                        if let Some(window) = window {
                            window.handle_event(Event::Close);
                        }
                    }
                }
                protocol::Event::EnterNotify(event) => {
                    let window = self.state.windows.borrow().get(&event.event).cloned();
                    if let Some(window) = window {
                        window.handle_event(Event::MouseEnter);

                        let point = Point {
                            x: event.event_x as f64,
                            y: event.event_y as f64,
                        };
                        window.handle_event(Event::MouseMove(point));
                    }
                }
                protocol::Event::LeaveNotify(event) => {
                    let window = self.state.windows.borrow().get(&event.event).cloned();
                    if let Some(window) = window {
                        window.handle_event(Event::MouseExit);
                    }
                }
                protocol::Event::MotionNotify(event) => {
                    let window = self.state.windows.borrow().get(&event.event).cloned();
                    if let Some(window) = window {
                        let point = Point {
                            x: event.event_x as f64,
                            y: event.event_y as f64,
                        };

                        window.handle_event(Event::MouseMove(point));
                    }
                }
                protocol::Event::ButtonPress(event) => {
                    let window = self.state.windows.borrow().get(&event.event).cloned();
                    if let Some(window) = window {
                        if let Some(button) = mouse_button_from_code(event.detail) {
                            window.handle_event(Event::MouseDown(button));
                        } else if let Some(delta) = scroll_delta_from_code(event.detail) {
                            window.handle_event(Event::Scroll(delta));
                        }
                    }
                }
                protocol::Event::ButtonRelease(event) => {
                    let window = self.state.windows.borrow().get(&event.event).cloned();
                    if let Some(window) = window {
                        if let Some(button) = mouse_button_from_code(event.detail) {
                            window.handle_event(Event::MouseUp(button));
                        }
                    }
                }
                protocol::Event::PresentCompleteNotify(event) => {
                    let window = self.state.windows.borrow().get(&event.window).cloned();
                    if let Some(window) = window {
                        window.handle_event(Event::Frame);

                        self.state.connection.present_notify_msc(event.window, 0, 0, 1, 0)?;
                        self.state.connection.flush()?;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub fn shutdown(&self) {
        self.state.open.set(false);

        for window_state in self.state.windows.take().into_values() {
            window_state.close();
        }
        let _ = self.state.connection.flush();

        self.state.timers.shutdown();
    }
}

impl AsRawFd for AppInner {
    fn as_raw_fd(&self) -> RawFd {
        self.state.connection.stream().as_raw_fd()
    }
}
