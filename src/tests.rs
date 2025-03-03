use std::rc::Rc;

use crate::{Context, Event, EventLoop, Key, Response, Size, Task, Window, WindowOptions};

pub fn leak() {
    struct TestTask {
        window: Option<Window>,
    }

    impl Task for TestTask {
        fn event(&mut self, _cx: &Context, _key: Key, _event: Event) -> Response {
            Response::Ignore
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let event_loop_weak = Rc::downgrade(&event_loop.state);

    let task = event_loop.spawn(TestTask { window: None });
    let task_weak = Rc::downgrade(&task.task);

    let window_weak = task.with(|task, cx| {
        let window = WindowOptions::new().size(Size::new(1.0, 1.0)).open(cx, Key(0)).unwrap();
        let window_weak = Rc::downgrade(&window.state);
        task.window = Some(window);
        window_weak
    });

    assert!(event_loop_weak.upgrade().is_some());
    assert!(task_weak.upgrade().is_some());
    assert!(window_weak.upgrade().is_some());

    drop(event_loop);
    drop(task);

    assert!(event_loop_weak.upgrade().is_none());
    assert!(task_weak.upgrade().is_none());
    assert!(window_weak.upgrade().is_none());
}
