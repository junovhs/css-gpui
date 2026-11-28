use std::{error::Error, path::Path, sync::Arc};

use gpui::{
    App, AppContext, Application, Bounds, Context, IntoElement, Render, RenderImage, Styled,
    Window, WindowBounds, WindowOptions, canvas, px, size,
};
use image::{Frame, RgbaImage};

#[path = "../planar_route.rs"]
mod route;

struct Preview(Arc<RenderImage>);

impl Render for Preview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let image = self.0.clone();
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                window
                    .paint_image(bounds, Default::default(), image, 0, false)
                    .expect("paint projected scene in GPUI");
                println!("painted");
            },
        )
        .size_full()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let [mode, bundle] = args.as_slice()
        && mode == "--route"
    {
        return route::run(Path::new(bundle));
    }
    let [width, height, rgba] = args.as_slice() else {
        return Err(
            "usage: css-gpui-planar-preview WIDTH HEIGHT FRAME.rgba | --route BUNDLE.json".into(),
        );
    };
    let (width, height) = (width.parse::<u32>()?, height.parse::<u32>()?);
    let image = read_frame(width, height, Path::new(rgba))?;
    Application::new().run(move |app: &mut App| {
        let bounds = Bounds::centered(None, size(px(width as f32), px(height as f32)), app);
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_resizable: false,
                ..Default::default()
            },
            move |_, app| app.new(|_| Preview(image)),
        )
        .expect("open GPUI planar preview");
        app.activate(true);
    });
    Ok(())
}

fn read_frame(width: u32, height: u32, rgba: &Path) -> Result<Arc<RenderImage>, Box<dyn Error>> {
    let mut rgba = std::fs::read(rgba)?;
    if !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
        || rgba.len() != width as usize * height as usize * 4
    {
        return Err("frame byte length does not match viewport".into());
    }
    // GPUI's uploaded sprite texture expects BGRA byte order.
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let image = RgbaImage::from_raw(width, height, rgba).ok_or("invalid frame")?;
    Ok(Arc::new(RenderImage::new(vec![Frame::new(image)])))
}
