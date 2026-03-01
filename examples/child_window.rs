use std::cell::RefCell;
use std::rc::Rc;

use portlight::{
    Bitmap, EventLoop, EventLoopMode, EventLoopOptions, Point, Response, Size, Window, WindowEvent,
    WindowOptions,
};

struct ParentState {
    event_loop: EventLoop,
    framebuffer: Vec<u32>,
    window: Option<Window>,
}

impl ParentState {
    fn handle_event(&mut self, event: WindowEvent) -> Response {
        match event {
            WindowEvent::Frame => {
                let window = &self.window.as_ref().unwrap();

                let scale = window.scale();
                let size = window.size();
                let width = (scale * size.width) as usize;
                let height = (scale * size.height) as usize;
                self.framebuffer.resize(width * height, 0xFF00FFFF);
                window.present(Bitmap::new(&self.framebuffer, width, height));
            }
            WindowEvent::Close => {
                self.event_loop.exit();
            }
            _ => {}
        }

        Response::Ignore
    }
}

struct ChildState {
    framebuffer: Vec<u32>,
    window: Option<Window>,
}

impl ChildState {
    fn handle_event(&mut self, event: WindowEvent) -> Response {
        match event {
            WindowEvent::Frame => {
                let window = &self.window.as_ref().unwrap();

                let scale = window.scale();
                let size = window.size();
                let width = (scale * size.width) as usize;
                let height = (scale * size.height) as usize;
                self.framebuffer.resize(width * height, 0xFFFF00FF);
                window.present(Bitmap::new(&self.framebuffer, width, height));
            }
            _ => {}
        }

        Response::Ignore
    }
}

fn main() {
    let parent_event_loop = EventLoop::new().unwrap();

    let parent_state = Rc::new(RefCell::new(ParentState {
        event_loop: parent_event_loop.clone(),
        framebuffer: Vec::new(),
        window: None,
    }));

    let window = WindowOptions::new()
        .title("parent window")
        .size(Size::new(512.0, 512.0))
        .open(&parent_event_loop, {
            let state = Rc::downgrade(&parent_state);
            move |event| state.upgrade().unwrap().borrow_mut().handle_event(event)
        })
        .unwrap();

    window.show();

    let parent_window_raw = window.as_raw().unwrap();
    parent_state.borrow_mut().window = Some(window);

    let child_event_loop = EventLoopOptions::new().mode(EventLoopMode::Guest).build().unwrap();

    let child_state = Rc::new(RefCell::new(ChildState {
        framebuffer: Vec::new(),
        window: None,
    }));

    let mut window_opts = WindowOptions::new();
    unsafe {
        window_opts.raw_parent(parent_window_raw);
    }

    let window = window_opts
        .position(Point::new(128.0, 128.0))
        .size(Size::new(256.0, 256.0))
        .open(&child_event_loop, {
            let state = Rc::downgrade(&child_state);
            move |event| state.upgrade().unwrap().borrow_mut().handle_event(event)
        })
        .unwrap();

    window.show();

    child_state.borrow_mut().window = Some(window);

    parent_event_loop.run().unwrap();

    child_state.borrow_mut().window = None;

    parent_state.borrow_mut().window = None;
}
