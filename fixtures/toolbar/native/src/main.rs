use gpui::prelude::*;
use gpui::{
    App, Application, Bounds, Context, Render, Window, WindowBounds, WindowOptions, px, size,
};

mod generated;

struct Toolbar;

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        generated::render_scene(|_, _, _| {})
    }
}

fn main() {
    let width = std::env::args()
        .nth(1)
        .map(|arg| arg.parse::<f32>().expect("window width must be a number"))
        .unwrap_or(720.0);
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(width), px(100.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_resizable: false,
                ..Default::default()
            },
            |_, cx| cx.new(|_| Toolbar),
        )
        .expect("open toolbar window");
        cx.activate(true);
    });
}
