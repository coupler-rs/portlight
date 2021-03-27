use window::{Application, Rect, Window, WindowHandler, WindowOptions};

struct Handler;

impl WindowHandler for Handler {
    fn open(&self, _window: &Window) {
        println!("open");
    }

    fn display(&self, window: &Window) {
        println!("display");
        window.update_contents(&[0xFF00FF; 1920 * 1920], 1920, 1920);
    }

    fn request_close(&self, window: &Window) {
        window.close().unwrap();
    }

    fn close(&self, window: &Window) {
        println!("close");
        window.application().stop();
    }
}

impl Drop for Handler {
    fn drop(&mut self) {
        println!("drop");
    }
}

fn main() {
    let app = Application::open().unwrap();

    Window::open(
        &app,
        WindowOptions {
            title: "window".to_string(),
            rect: Rect { x: 0.0, y: 0.0, w: 500.0, h: 500.0 },
            handler: Some(Box::new(Handler)),
            ..WindowOptions::default()
        },
    )
    .unwrap();

    app.start().unwrap();
    app.close().unwrap();
}
