use std::time::Duration;

use portlight::{App, Bitmap, Event, Response, Size, WindowContext, WindowOptions};

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
    fn handle_event(&mut self, cx: &WindowContext, event: Event) -> Response {
        match event {
            Event::Expose(rects) => {
                println!("expose: {:?}", rects);
            }
            Event::Frame => {
                println!("frame");

                let scale = cx.window().scale();
                self.width = (WIDTH as f64 * scale) as usize;
                self.height = (HEIGHT as f64 * scale) as usize;
                self.framebuffer.resize(self.width * self.height, 0xFFFF00FF);

                cx.window().present(Bitmap::new(&self.framebuffer, self.width, self.height));
            }
            Event::GainFocus => {
                println!("gain focus");
            }
            Event::LoseFocus => {
                println!("lose focus");
            }
            Event::MouseEnter => {
                println!("mouse enter");
            }
            Event::MouseExit => {
                println!("mouse exit");
            }
            Event::MouseMove(pos) => {
                println!("mouse move: {:?}", pos);
            }
            Event::MouseDown(btn) => {
                println!("mouse down: {:?}", btn);
                return Response::Capture;
            }
            Event::MouseUp(btn) => {
                println!("mouse up: {:?}", btn);
                return Response::Capture;
            }
            Event::Scroll(delta) => {
                println!("scroll: {:?}", delta);
                return Response::Capture;
            }
            Event::Close => {
                cx.app().exit();
            }
        }

        Response::Ignore
    }
}

fn main() {
    let app = App::new().unwrap();

    let mut state = State {
        framebuffer: Vec::new(),
        width: 0,
        height: 0,
    };

    let window = WindowOptions::new()
        .title("window")
        .size(Size::new(512.0, 512.0))
        .open(app.handle(), move |cx, event| state.handle_event(cx, event))
        .unwrap();

    app.handle()
        .set_timer(Duration::from_millis(1000), |_| {
            println!("timer");
        })
        .unwrap();

    window.show();

    app.run().unwrap();
}
