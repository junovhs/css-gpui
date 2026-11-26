use std::{borrow::Cow, env, fs};

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
    let width = env::args()
        .nth(1)
        .map(|arg| arg.parse::<f32>().expect("window width must be a number"))
        .unwrap_or(720.0);
    let height = env::var("CSS_GPUI_HOST_HEIGHT")
        .ok()
        .map(|arg| arg.parse::<f32>().expect("window height must be a number"))
        .unwrap_or(100.0);
    Application::new()
        .with_assets(generated::scene_assets())
        .run(move |cx: &mut App| {
            if let Some(directory) = env::var_os("CSS_GPUI_HOST_FONTS") {
                let fonts = fs::read_dir(directory)
                    .expect("read host font directory")
                    .map(|entry| {
                        let path = entry.expect("read font entry").path();
                        Cow::Owned(fs::read(path).expect("read font bytes"))
                    })
                    .collect();
                cx.text_system()
                    .add_fonts(fonts)
                    .expect("register fonts in GPUI");
            }
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
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
