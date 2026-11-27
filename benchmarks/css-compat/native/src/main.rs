use gpui::prelude::*;
use gpui::{
    App, Application, Bounds, Context, Render, Window, WindowBounds, WindowOptions, px, size,
};

mod generated {
    include!(env!("CSS_GPUI_GENERATED"));
}

struct CompatibilityScreen;

impl Render for CompatibilityScreen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        generated::render_scene(|_, _, _| {})
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let mut dimension = || {
        arguments
            .next()
            .expect("width and height required")
            .parse::<f32>()
            .expect("width and height must be numbers")
    };
    let (width, height) = (dimension(), dimension());
    Application::new()
        .with_assets(generated::scene_assets())
        .run(move |app: &mut App| {
            let bounds = Bounds::centered(None, size(px(width), px(height)), app);
            app.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: None,
                    is_resizable: false,
                    ..Default::default()
                },
                |_, app| app.new(|_| CompatibilityScreen),
            )
            .expect("open GPUI compatibility window");
            app.activate(true);
        });
}
