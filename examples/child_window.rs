use portlight::{
    Bitmap, Context, Event, EventLoop, EventLoopMode, EventLoopOptions, Key, Point, Response, Size,
    Task, Window, WindowEvent, WindowOptions,
};

struct ParentState {
    framebuffer: Vec<u32>,
    window: Option<Window>,
}

impl Task for ParentState {
    fn event(&mut self, cx: &Context, _key: Key, event: Event) -> Response {
        if let Event::Window(event) = event {
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
                    cx.event_loop().exit();
                }
                _ => {}
            }
        }

        Response::Ignore
    }
}

struct ChildState {
    framebuffer: Vec<u32>,
    window: Option<Window>,
}

impl Task for ChildState {
    fn event(&mut self, _cx: &Context, _key: Key, event: Event) -> Response {
        if let Event::Window(event) = event {
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
        }

        Response::Ignore
    }
}

fn main() {
    let parent_event_loop = EventLoop::new().unwrap();

    let parent_state = parent_event_loop.spawn(ParentState {
        framebuffer: Vec::new(),
        window: None,
    });
    let parent_window_raw = parent_state.with(|state, cx| {
        let window = WindowOptions::new()
            .title("parent window")
            .size(Size::new(512.0, 512.0))
            .open(cx, Key(0))
            .unwrap();

        window.show();

        let parent_window_raw = window.as_raw().unwrap();
        state.window = Some(window);

        parent_window_raw
    });

    let child_event_loop = EventLoopOptions::new().mode(EventLoopMode::Guest).build().unwrap();

    let child_state = child_event_loop.spawn(ChildState {
        framebuffer: Vec::new(),
        window: None,
    });
    child_state.with(|state, cx| {
        let mut window_opts = WindowOptions::new();
        unsafe {
            window_opts.raw_parent(parent_window_raw);
        }

        let window = window_opts
            .position(Point::new(128.0, 128.0))
            .size(Size::new(256.0, 256.0))
            .open(cx, Key(0))
            .unwrap();

        window.show();

        state.window = Some(window);
    });

    parent_event_loop.run().unwrap();

    child_state.with(|state, _| {
        state.window = None;
    });

    parent_state.with(|state, _| {
        state.window = None;
    });
}
