use std::cell::RefCell;
use std::rc::Rc;

use crate::{EventLoop, Response, Size, Window, WindowEvent, WindowOptions};

pub fn leak() {
    struct State {
        window: Option<Window>,
    }

    impl State {
        fn handle_event(&mut self, _event: WindowEvent) -> Response {
            Response::Ignore
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let event_loop_weak = Rc::downgrade(&event_loop.state);

    let state = Rc::new(RefCell::new(State { window: None }));
    let state_weak = Rc::downgrade(&state);

    let window = WindowOptions::new()
        .size(Size::new(1.0, 1.0))
        .open(&event_loop, {
            let state = Rc::downgrade(&state);
            move |event| state.upgrade().unwrap().borrow_mut().handle_event(event)
        })
        .unwrap();
    let window_weak = Rc::downgrade(&window.state);
    state.borrow_mut().window = Some(window);

    assert!(event_loop_weak.upgrade().is_some());
    assert!(state_weak.upgrade().is_some());
    assert!(window_weak.upgrade().is_some());

    drop(event_loop);
    drop(state);

    assert!(event_loop_weak.upgrade().is_none());
    assert!(state_weak.upgrade().is_none());
    assert!(window_weak.upgrade().is_none());
}
