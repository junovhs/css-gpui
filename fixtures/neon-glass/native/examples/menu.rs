use std::{env, error::Error};

use css_gpui_neon_reference::menu::{MenuView, register_fonts, scene_assets};
use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};

fn main() -> Result<(), Box<dyn Error>> {
    let shift: f32 = env::args().nth(1).map_or(Ok(0.0), |value| value.parse())?;
    if !shift.is_finite() || shift.abs() > 90.0 {
        return Err("backdrop offset must be between -90 and 90".into());
    }
    let view = MenuView::new(shift)?;
    Application::new()
        .with_assets(scene_assets())
        .run(move |cx: &mut App| {
            register_fonts(cx).expect("load embedded Neon Glass fonts");
            let bounds = Bounds::centered(None, size(px(1100.0), px(760.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| view),
            )
            .expect("open Neon Glass");
            cx.activate(true);
        });
    Ok(())
}
