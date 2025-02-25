use std::time::Duration;

use portlight::{
    Bitmap, Context, Event, EventLoop, Key, Response, Size, Task, Timer, Window, WindowEvent,
    WindowOptions,
};

const WIDTH: usize = 512;
const HEIGHT: usize = 512;

struct State {
    window: Option<Window>,
    framebuffer: Vec<u32>,
    width: usize,
    height: usize,
}

impl Drop for State {
    fn drop(&mut self) {
        println!("drop");
    }
}

impl Task for State {
    fn event(&mut self, cx: &Context, _key: Key, event: Event) -> Response {
        if let Event::Window(event) = event {
            match event {
                WindowEvent::Expose(rects) => {
                    println!("expose: {:?}", rects);
                }
                WindowEvent::Frame => {
                    println!("frame");

                    let window = self.window.as_ref().unwrap();

                    let scale = window.scale();
                    self.width = (WIDTH as f64 * scale) as usize;
                    self.height = (HEIGHT as f64 * scale) as usize;
                    self.framebuffer.resize(self.width * self.height, 0xFFFF00FF);

                    window.present(Bitmap::new(&self.framebuffer, self.width, self.height));
                }
                WindowEvent::GainFocus => {
                    println!("gain focus");
                }
                WindowEvent::LoseFocus => {
                    println!("lose focus");
                }
                WindowEvent::MouseEnter => {
                    println!("mouse enter");
                }
                WindowEvent::MouseExit => {
                    println!("mouse exit");
                }
                WindowEvent::MouseMove(pos) => {
                    println!("mouse move: {:?}", pos);
                }
                WindowEvent::MouseDown(btn) => {
                    println!("mouse down: {:?}", btn);
                    return Response::Capture;
                }
                WindowEvent::MouseUp(btn) => {
                    println!("mouse up: {:?}", btn);
                    return Response::Capture;
                }
                WindowEvent::Scroll(delta) => {
                    println!("scroll: {:?}", delta);
                    return Response::Capture;
                }
                WindowEvent::Close => {
                    cx.event_loop().exit();
                }
            }
        } else if let Event::Timer = event {
            println!("timer");
        }

        Response::Ignore
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let state = event_loop.spawn(State {
        window: None,
        framebuffer: Vec::new(),
        width: 0,
        height: 0,
    });

    state.with(|state, cx| {
        let window = WindowOptions::new()
            .title("window")
            .size(Size::new(512.0, 512.0))
            .open(cx, Key(0))
            .unwrap();

        window.show();
        state.window = Some(window);

        Timer::repeat(Duration::from_millis(1000), cx, Key(0)).unwrap();
    });

    event_loop.run().unwrap();
}
