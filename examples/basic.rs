use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use portlight::{Bitmap, Event, EventLoop, Response, Size, Timer, Window, WindowOptions};

const WIDTH: usize = 512;
const HEIGHT: usize = 512;

struct State {
    event_loop: EventLoop,
    window: Option<Window>,
    framebuffer: Vec<u32>,
    width: usize,
    height: usize,
    timer: Option<Timer>,
}

impl Drop for State {
    fn drop(&mut self) {
        println!("drop");
    }
}

impl State {
    fn handle_event(&mut self, event: Event) -> Response {
        match event {
            Event::Expose(rects) => {
                println!("expose: {:?}", rects);
            }
            Event::Frame => {
                println!("frame");

                let window = self.window.as_ref().unwrap();

                let scale = window.scale();
                self.width = (WIDTH as f64 * scale) as usize;
                self.height = (HEIGHT as f64 * scale) as usize;
                self.framebuffer.resize(self.width * self.height, 0xFFFF00FF);

                window.present(Bitmap::new(&self.framebuffer, self.width, self.height));
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
                self.event_loop.exit();
            }
        }

        Response::Ignore
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let state = Rc::new(RefCell::new(State {
        event_loop: event_loop.clone(),
        window: None,
        framebuffer: Vec::new(),
        width: 0,
        height: 0,
        timer: None,
    }));

    let window = WindowOptions::new()
        .title("window")
        .size(Size::new(512.0, 512.0))
        .open(&event_loop, {
            let state = Rc::downgrade(&state);
            move |event| state.upgrade().unwrap().borrow_mut().handle_event(event)
        })
        .unwrap();

    window.show();

    state.borrow_mut().window = Some(window);

    state.borrow_mut().timer = Some(
        Timer::repeat(&event_loop, Duration::from_millis(1000), || {
            println!("timer")
        })
        .unwrap(),
    );

    event_loop.run().unwrap();
}
