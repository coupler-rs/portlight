use std::time::Duration;

use portlight::{Bitmap, EventLoop, Response, Size, WindowContext, WindowEvent, WindowOptions};

const WIDTH: usize = 512;
const HEIGHT: usize = 512;

struct State {
    framebuffer: Vec<u32>,
    width: usize,
    height: usize,
}

impl Drop for State {
    fn drop(&mut self) {
        println!("drop");
    }
}

impl State {
    fn handle_event(&mut self, cx: &WindowContext, event: WindowEvent) -> Response {
        match event {
            WindowEvent::Expose(rects) => {
                println!("expose: {:?}", rects);
            }
            WindowEvent::Frame => {
                println!("frame");

                let scale = cx.window().scale();
                self.width = (WIDTH as f64 * scale) as usize;
                self.height = (HEIGHT as f64 * scale) as usize;
                self.framebuffer.resize(self.width * self.height, 0xFFFF00FF);

                cx.window().present(Bitmap::new(&self.framebuffer, self.width, self.height));
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

        Response::Ignore
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let mut state = State {
        framebuffer: Vec::new(),
        width: 0,
        height: 0,
    };

    let window = WindowOptions::new()
        .title("window")
        .size(Size::new(512.0, 512.0))
        .open(event_loop.handle(), move |cx, event| {
            state.handle_event(cx, event)
        })
        .unwrap();

    event_loop
        .handle()
        .set_timer(Duration::from_millis(1000), |_| {
            println!("timer");
        })
        .unwrap();

    window.show();

    event_loop.run().unwrap();
}
