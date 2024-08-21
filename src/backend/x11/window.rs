use std::cell::{Cell, RefCell};
use std::ffi::{c_int, c_ulong, c_void};
use std::rc::Rc;
use std::{mem, ptr, slice};

use x11rb::connection::Connection;
use x11rb::protocol::present::{self, ConnectionExt as _};
use x11rb::protocol::shm::{ConnectionExt as _, Seg};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClipOrdering, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, Gcontext, ImageFormat, PropMode, Rectangle, Window, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

use super::app::{AppInner, AppState};
use super::OsError;
use crate::{
    AppHandle, Bitmap, Cursor, Error, Event, Point, RawWindow, Rect, Response, Result, Size,
    WindowContext, WindowOptions,
};

pub struct ShmState {
    shm_id: c_int,
    seg_id: Seg,
    ptr: *mut c_void,
    width: usize,
    height: usize,
}

pub struct PresentState {
    event_id: present::Event,
}

pub struct WindowState {
    pub window_id: Cell<Option<Window>>,
    pub gc_id: Cell<Option<Gcontext>>,
    pub shm_state: RefCell<Option<ShmState>>,
    pub present_state: RefCell<Option<PresentState>>,
    pub expose_rects: RefCell<Vec<Rect>>,
    pub app_state: Rc<AppState>,
    pub handler: RefCell<Box<dyn FnMut(&WindowContext, Event) -> Response>>,
}

impl WindowState {
    fn init_shm(app_state: &AppState, width: usize, height: usize) -> Result<Option<ShmState>> {
        if !app_state.shm_supported {
            return Ok(None);
        }

        let shm_id = unsafe {
            let shm_id = libc::shmget(
                libc::IPC_PRIVATE,
                width * height * mem::size_of::<u32>(),
                libc::IPC_CREAT | 0o600,
            );
            if shm_id == -1 {
                return Err(Error::Os(OsError::Message(
                    "failed to create shared memory segment",
                )));
            }

            shm_id
        };

        let ptr = unsafe {
            let ptr = libc::shmat(shm_id, ptr::null(), 0);
            if ptr == usize::MAX as *mut c_void {
                libc::shmctl(shm_id, libc::IPC_RMID, ptr::null_mut());
                return Err(Error::Os(OsError::Message(
                    "failed to attach shared memory segment",
                )));
            }

            ptr
        };

        let seg_id = app_state.connection.generate_id()?;
        app_state.connection.shm_attach(seg_id, shm_id as u32, false)?;

        Ok(Some(ShmState {
            shm_id,
            seg_id,
            ptr,
            width,
            height,
        }))
    }

    fn deinit_shm(&self) {
        if let Some(shm_state) = self.shm_state.take() {
            let _ = self.app_state.connection.shm_detach(shm_state.seg_id);

            unsafe {
                libc::shmdt(shm_state.ptr);
                libc::shmctl(shm_state.shm_id, libc::IPC_RMID, ptr::null_mut());
            }
        }
    }

    pub fn handle_event(self: &Rc<WindowState>, event: Event) -> Response {
        let app = AppHandle::from_inner(AppInner {
            state: Rc::clone(&self.app_state),
        });
        let window = crate::Window::from_inner(WindowInner {
            state: self.clone(),
        });
        let cx = WindowContext::new(&app, &window);
        self.handler.borrow_mut()(&cx, event)
    }

    pub fn close(&self) {
        if let Some(window_id) = self.window_id.take() {
            let connection = &self.app_state.connection;

            if let Some(gc_id) = self.gc_id.take() {
                let _ = connection.free_gc(gc_id);
            }

            self.deinit_shm();

            if let Some(present_state) = self.present_state.take() {
                let _ = connection.present_select_input(
                    present_state.event_id,
                    window_id,
                    present::EventMask::NO_EVENT,
                );
            }

            let _ = connection.destroy_window(window_id);
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

        let app_state = &app.inner.state;
        let connection = &app_state.connection;

        let window_id = connection.generate_id()?;

        let parent_id = if let Some(parent) = options.parent {
            if let RawWindow::X11(parent_id) = parent {
                parent_id as Window
            } else {
                return Err(Error::InvalidWindowHandle);
            }
        } else {
            connection.setup().roots[app_state.screen_index].root
        };

        let position = options.position.unwrap_or(Point::new(0.0, 0.0));
        let position_physical = position.scale(app_state.scale);

        let size_physical = options.size.scale(app_state.scale);

        let event_mask = EventMask::EXPOSURE
            | EventMask::ENTER_WINDOW
            | EventMask::LEAVE_WINDOW
            | EventMask::POINTER_MOTION
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE;
        let aux = CreateWindowAux::new().event_mask(event_mask);

        connection.create_window(
            x11rb::COPY_FROM_PARENT as u8,
            window_id,
            parent_id,
            position_physical.x.round() as i16,
            position_physical.y.round() as i16,
            size_physical.width.round() as u16,
            size_physical.height.round() as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &aux,
        )?;

        connection.change_property8(
            PropMode::REPLACE,
            window_id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            options.title.as_bytes(),
        )?;
        connection.change_property8(
            PropMode::REPLACE,
            window_id,
            app_state.atoms._NET_WM_NAME,
            app_state.atoms.UTF8_STRING,
            options.title.as_bytes(),
        )?;
        connection.change_property32(
            PropMode::REPLACE,
            window_id,
            app_state.atoms.WM_PROTOCOLS,
            AtomEnum::ATOM,
            &[app_state.atoms.WM_DELETE_WINDOW],
        )?;

        let gc_id = connection.generate_id()?;
        connection.create_gc(gc_id, window_id, &CreateGCAux::default())?;

        let shm_state = WindowState::init_shm(
            &app_state,
            size_physical.width.round() as usize,
            size_physical.height.round() as usize,
        )?;

        let present_state = if app_state.present_supported {
            let event_id = connection.generate_id()?;
            connection.present_select_input(
                event_id,
                window_id,
                present::EventMask::COMPLETE_NOTIFY,
            )?;

            connection.present_notify_msc(window_id, 0, 0, 1, 0)?;

            Some(PresentState { event_id })
        } else {
            None
        };

        connection.flush()?;

        let state = Rc::new(WindowState {
            window_id: Cell::new(Some(window_id)),
            gc_id: Cell::new(Some(gc_id)),
            shm_state: RefCell::new(shm_state),
            present_state: RefCell::new(present_state),
            expose_rects: RefCell::new(Vec::new()),
            app_state: Rc::clone(&app_state),
            handler: RefCell::new(Box::new(handler)),
        });

        app_state.windows.borrow_mut().insert(window_id, Rc::clone(&state));

        Ok(WindowInner { state })
    }

    pub fn show(&self) {
        if let Some(window_id) = self.state.window_id.get() {
            let _ = self.state.app_state.connection.map_window(window_id);
            let _ = self.state.app_state.connection.flush();
        }
    }

    pub fn hide(&self) {
        if let Some(window_id) = self.state.window_id.get() {
            let _ = self.state.app_state.connection.unmap_window(window_id);
            let _ = self.state.app_state.connection.flush();
        }
    }

    pub fn size(&self) -> Size {
        self.size_inner().unwrap_or(Size::new(0.0, 0.0))
    }

    fn size_inner(&self) -> Result<Size> {
        let window_id = self.state.window_id.get().ok_or(Error::WindowClosed)?;
        let geom = self.state.app_state.connection.get_geometry(window_id)?.reply()?;
        let size_physical = Size::new(geom.width as f64, geom.height as f64);

        Ok(size_physical.scale(self.state.app_state.scale.recip()))
    }

    pub fn scale(&self) -> f64 {
        self.state.app_state.scale
    }

    pub fn present(&self, bitmap: Bitmap) {
        let _ = self.present_inner(bitmap, None);
    }

    pub fn present_partial(&self, bitmap: Bitmap, rects: &[Rect]) {
        let _ = self.present_inner(bitmap, Some(rects));
    }

    fn present_inner(&self, bitmap: Bitmap, rects: Option<&[Rect]>) -> Result<()> {
        let connection = &self.state.app_state.connection;
        let window_id = self.state.window_id.get().ok_or(Error::WindowClosed)?;
        let gc_id = self.state.gc_id.get().ok_or(Error::WindowClosed)?;

        if let Some(rects) = rects {
            let mut x_rects = Vec::with_capacity(rects.len());
            for rect in rects {
                let rect_physical = rect.scale(self.state.app_state.scale);

                x_rects.push(Rectangle {
                    x: rect_physical.x.round() as i16,
                    y: rect_physical.y.round() as i16,
                    width: rect_physical.width.round() as u16,
                    height: rect_physical.height.round() as u16,
                });
            }

            connection.set_clip_rectangles(ClipOrdering::UNSORTED, gc_id, 0, 0, &x_rects)?;
        }

        if let Some(ref shm_state) = *self.state.shm_state.borrow() {
            // SAFETY: ptr is page-aligned and thus u32-aligned
            let data = unsafe {
                slice::from_raw_parts_mut(
                    shm_state.ptr as *mut u32,
                    shm_state.width * shm_state.height * std::mem::size_of::<u32>(),
                )
            };

            let copy_width = bitmap.width().min(shm_state.width);
            let copy_height = bitmap.height().min(shm_state.height);
            for row in 0..copy_height {
                let src = &bitmap.data()[row * bitmap.width()..row * bitmap.width() + copy_width];
                let dst = &mut data[row * shm_state.width..row * shm_state.width + copy_width];
                dst.copy_from_slice(src);
            }

            connection.shm_put_image(
                window_id,
                gc_id,
                shm_state.width as u16,
                shm_state.height as u16,
                0,
                0,
                shm_state.width as u16,
                shm_state.height as u16,
                0,
                0,
                24,
                ImageFormat::Z_PIXMAP.into(),
                false,
                shm_state.seg_id,
                0,
            )?;
        } else {
            let (_, bytes, _) = unsafe { bitmap.data().align_to::<u8>() };
            connection.put_image(
                ImageFormat::Z_PIXMAP,
                window_id,
                gc_id,
                bitmap.width() as u16,
                bitmap.height() as u16,
                0,
                0,
                0,
                24,
                bytes,
            )?;
        }

        if rects.is_some() {
            connection.set_clip_rectangles(ClipOrdering::UNSORTED, gc_id, 0, 0, &[])?;
        }

        connection.flush()?;

        Ok(())
    }

    pub fn set_cursor(&self, cursor: Cursor) {
        let _ = self.set_cursor_inner(cursor);
    }

    fn set_cursor_inner(&self, cursor: Cursor) -> Result<()> {
        let connection = &self.state.app_state.connection;
        let cursor_cache = &self.state.app_state.cursor_cache;
        let window_id = self.state.window_id.get().ok_or(Error::WindowClosed)?;

        let cursor_id = if let Some(cursor_id) = cursor_cache.borrow_mut().get(&cursor) {
            *cursor_id
        } else {
            if cursor == Cursor::None {
                let cursor_id = connection.generate_id()?;
                let pixmap_id = connection.generate_id()?;
                let root = connection.setup().roots[self.state.app_state.screen_index].root;
                connection.create_pixmap(1, pixmap_id, root, 1, 1)?;
                connection
                    .create_cursor(cursor_id, pixmap_id, pixmap_id, 0, 0, 0, 0, 0, 0, 0, 0)?;
                connection.free_pixmap(pixmap_id)?;

                cursor_id
            } else {
                let cursor_name = match cursor {
                    Cursor::Arrow => "left_ptr",
                    Cursor::Crosshair => "crosshair",
                    Cursor::Hand => "hand2",
                    Cursor::IBeam => "text",
                    Cursor::No => "crossed_circle",
                    Cursor::SizeNs => "v_double_arrow",
                    Cursor::SizeWe => "h_double_arrow",
                    Cursor::SizeNesw => "fd_double_arrow",
                    Cursor::SizeNwse => "bd_double_arrow",
                    Cursor::Wait => "watch",
                    Cursor::None => unreachable!(),
                };
                self.state.app_state.cursor_handle.load_cursor(connection, cursor_name)?
            }
        };

        connection.change_window_attributes(
            window_id,
            &ChangeWindowAttributesAux::new().cursor(cursor_id),
        )?;
        self.state.app_state.connection.flush()?;

        Ok(())
    }

    pub fn set_mouse_position(&self, position: Point) {
        if let Some(window_id) = self.state.window_id.get() {
            let position_physical = position.scale(self.state.app_state.scale);

            let _ = self.state.app_state.connection.warp_pointer(
                x11rb::NONE,
                window_id,
                0,
                0,
                0,
                0,
                position_physical.x.round() as i16,
                position_physical.y.round() as i16,
            );
            let _ = self.state.app_state.connection.flush();
        }
    }

    pub fn close(&self) {
        if let Some(window_id) = self.state.window_id.get() {
            self.state.app_state.windows.borrow_mut().remove(&window_id);
        }

        self.state.close();

        let _ = self.state.app_state.connection.flush();
    }

    pub fn as_raw(&self) -> Result<RawWindow> {
        if let Some(window_id) = self.state.window_id.get() {
            Ok(RawWindow::X11(window_id as c_ulong))
        } else {
            Err(Error::WindowClosed)
        }
    }
}
