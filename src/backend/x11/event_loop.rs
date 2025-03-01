use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::time::Instant;

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::present::{self, ConnectionExt as _};
use x11rb::protocol::shm;
use x11rb::protocol::xproto::{self, Button, ConnectionExt as _, Window as WindowId};
use x11rb::rust_connection::RustConnection;
use x11rb::{cursor, protocol, resource_manager};

use super::timer::Timers;
use super::window::WindowState;
use crate::{
    Context, Cursor, Error, Event, EventLoopOptions, MouseButton, Point, Rect, Response, Result,
    WindowEvent,
};

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

pub struct EventLoopState {
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
    pub windows: RefCell<HashMap<WindowId, Rc<WindowState>>>,
    pub timers: Timers,
}

impl Drop for EventLoopState {
    fn drop(&mut self) {
        for (_, cursor) in self.cursor_cache.take() {
            let _ = self.connection.free_cursor(cursor);
        }
        let _ = self.connection.flush();
    }
}

impl EventLoopState {
    pub fn new(_options: &EventLoopOptions) -> Result<Rc<EventLoopState>> {
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

        let state = Rc::new(EventLoopState {
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

        Ok(state)
    }

    pub fn run(&self) -> Result<()> {
        let _run_guard = RunGuard::new(&self.run_state)?;

        let fd = self.as_raw_fd();

        loop {
            self.drain_events()?;
            self.timers.poll();
            self.drain_events()?;

            if self.run_state.get() == RunState::Exiting {
                break;
            }

            let mut fds = [libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            }];

            let timeout = if let Some(next_time) = self.timers.next_time() {
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
        self.run_state.set(RunState::Exiting);
    }

    pub fn poll(&self) -> Result<()> {
        if self.run_state.get() != RunState::Stopped {
            return Err(Error::AlreadyRunning);
        }

        let _run_guard = RunGuard::new(&self.run_state)?;

        self.drain_events()?;
        self.timers.poll();
        self.drain_events()?;

        Ok(())
    }

    fn get_window(&self, id: WindowId) -> Option<Rc<WindowState>> {
        self.windows.borrow().get(&id).cloned()
    }

    fn handle_event(&self, state: &WindowState, event: WindowEvent) -> Option<Response> {
        let task_ref = state.handler.upgrade()?;
        let mut handler = task_ref.try_borrow_mut().ok()?;
        let cx = Context::new(&state.event_loop, &task_ref);
        Some(handler.event(&cx, state.key, Event::Window(event)))
    }

    fn drain_events(&self) -> Result<()> {
        loop {
            if self.run_state.get() == RunState::Exiting {
                break;
            }

            let Some(event) = self.connection.poll_for_event()? else {
                break;
            };

            match event {
                protocol::Event::Expose(event) => {
                    if let Some(window) = self.get_window(event.window) {
                        let rect_physical = Rect {
                            x: event.x as f64,
                            y: event.y as f64,
                            width: event.width as f64,
                            height: event.height as f64,
                        };
                        let rect = rect_physical.scale(self.scale.recip());

                        let expose_rects = &window.expose_rects;
                        expose_rects.borrow_mut().push(rect);

                        if event.count == 0 {
                            let rects = expose_rects.take();
                            self.handle_event(&window, WindowEvent::Expose(&rects));
                        }
                    }
                }
                protocol::Event::ClientMessage(event) => {
                    if event.format == 32
                        && event.data.as_data32()[0] == self.atoms.WM_DELETE_WINDOW
                    {
                        if let Some(window) = self.get_window(event.window) {
                            self.handle_event(&window, WindowEvent::Close);
                        }
                    }
                }
                protocol::Event::EnterNotify(event) => {
                    if let Some(window) = self.get_window(event.event) {
                        self.handle_event(&window, WindowEvent::MouseEnter);

                        let point = Point {
                            x: event.event_x as f64,
                            y: event.event_y as f64,
                        };
                        self.handle_event(&window, WindowEvent::MouseMove(point));
                    }
                }
                protocol::Event::LeaveNotify(event) => {
                    if let Some(window) = self.get_window(event.event) {
                        self.handle_event(&window, WindowEvent::MouseExit);
                    }
                }
                protocol::Event::MotionNotify(event) => {
                    if let Some(window) = self.get_window(event.event) {
                        let point = Point {
                            x: event.event_x as f64,
                            y: event.event_y as f64,
                        };

                        self.handle_event(&window, WindowEvent::MouseMove(point));
                    }
                }
                protocol::Event::ButtonPress(event) => {
                    if let Some(window) = self.get_window(event.event) {
                        if let Some(button) = mouse_button_from_code(event.detail) {
                            self.handle_event(&window, WindowEvent::MouseDown(button));
                        } else if let Some(delta) = scroll_delta_from_code(event.detail) {
                            self.handle_event(&window, WindowEvent::Scroll(delta));
                        }
                    }
                }
                protocol::Event::ButtonRelease(event) => {
                    if let Some(window) = self.get_window(event.event) {
                        if let Some(button) = mouse_button_from_code(event.detail) {
                            self.handle_event(&window, WindowEvent::MouseUp(button));
                        }
                    }
                }
                protocol::Event::PresentCompleteNotify(event) => {
                    if let Some(window) = self.get_window(event.window) {
                        self.handle_event(&window, WindowEvent::Frame);

                        self.connection.present_notify_msc(event.window, 0, 0, 1, 0)?;
                        self.connection.flush()?;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}

impl AsRawFd for EventLoopState {
    fn as_raw_fd(&self) -> RawFd {
        self.connection.stream().as_raw_fd()
    }
}
