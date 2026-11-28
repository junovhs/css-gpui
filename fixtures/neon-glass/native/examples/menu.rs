use std::{env, error::Error};

use css_gpui_neon_reference::menu::{MenuView, register_fonts, scene_assets};
use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let shift: f32 = args.next().map_or(Ok(0.0), |value| value.parse())?;
    if !shift.is_finite() || shift.abs() > 90.0 {
        return Err("backdrop offset must be between -90 and 90".into());
    }
    let (width, height) = match (args.next().as_deref(), args.next()) {
        (None, None) => (1100.0, 760.0),
        (Some("--narrow"), None) => (390.0, 844.0),
        _ => return Err("usage: menu [backdrop-offset [--narrow]]".into()),
    };
    let view = MenuView::new(shift)?;
    Application::new()
        .with_assets(scene_assets())
        .run(move |cx: &mut App| {
            register_fonts(cx).expect("load embedded Neon Glass fonts");
            let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Neon Glass Context Menu by Simey (@simeydotme)".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|_| view),
            )
            .expect("open Neon Glass");
            cx.activate(true);
        });
    Ok(())
}
