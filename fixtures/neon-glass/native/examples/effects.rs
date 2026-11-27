use std::{env, error::Error, fs, path::Path, sync::Arc};

use css_gpui::{ir::SceneIr, paint::Rect};
use css_gpui_neon_reference::effects::{GlassComposer, gpu_texture};
use gpui::{
    App, Application, Bounds, Context, Render, RenderImage, Window, WindowBounds, WindowOptions,
    canvas, div, prelude::*, px, size,
};
use image::{Rgba, RgbaImage};
use serde_json::Value;

const SCENE: &str = include_str!(concat!(env!("OUT_DIR"), "/scene-1100.json"));

struct Case {
    dimensions: (u32, u32),
    shift: f32,
    panel: Rect,
}

impl Case {
    fn read(data: &Value) -> Self {
        let width = data["width"].as_u64().expect("reference width") as u32;
        let height = data["height"].as_u64().expect("reference height") as u32;
        let shift = data["shift"].as_f64().expect("reference offset") as f32;
        let coordinate = |key| {
            data["panel"][key]
                .as_f64()
                .expect("panel coordinate")
                .round() as i32
        };
        Self {
            dimensions: (width, height),
            shift,
            panel: Rect {
                x: coordinate("x"),
                y: coordinate("y"),
                width: coordinate("width") as u32,
                height: coordinate("height") as u32,
            },
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let reference = env::var_os("NEON_GLASS_REFERENCE").ok_or("set NEON_GLASS_REFERENCE")?;
    let reference = Path::new(&reference);
    let painter = GlassComposer::load(SceneIr::from_json(SCENE)?, reference)?;
    let cases: Value = serde_json::from_str(&fs::read_to_string(reference.join("effects.json"))?)?;
    let cases: Vec<_> = cases
        .as_array()
        .ok_or("reference expects an array")?
        .iter()
        .map(Case::read)
        .collect();
    assert_eq!(cases.len(), 2);
    if env::args().any(|argument| argument == "--window") {
        let first = cases.into_iter().next().unwrap();
        let frame = painter.rasterize(first.dimensions, first.panel, (first.shift, 0.0))?;
        present(painter, first, frame.effects);
        return Ok(());
    }

    for (index, case) in cases.into_iter().enumerate() {
        let rendered = painter.rasterize(case.dimensions, case.panel, (case.shift, 0.0))?;
        assert_eq!(
            rendered.glow_layers, 6,
            "expected the six authored glow groups"
        );
        rendered
            .stage
            .save(reference.join(format!("gpui-stage-{index}.png")))?;
        rendered
            .effects
            .save(reference.join(format!("gpui-effects-{index}.png")))?;
        let chrome =
            image::open(reference.join(format!("chrome-effects-{index}.png")))?.into_rgba8();
        let stage_chrome =
            image::open(reference.join(format!("chrome-stage-{index}.png")))?.into_rgba8();
        let (width, height) = case.dimensions;
        let rect = case.panel;
        let stage = measure(
            &rendered.stage,
            &stage_chrome,
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
        );
        let interior = measure(
            &rendered.effects,
            &chrome,
            Rect {
                x: rect.x + 26,
                y: rect.y + 26,
                width: rect.width - 52,
                height: rect.height - 52,
            },
        );
        let perimeter = measure(
            &rendered.effects,
            &chrome,
            Rect {
                x: (rect.x - 30).max(0),
                y: (rect.y - 30).max(0),
                width: rect.width + 60,
                height: rect.height + 60,
            },
        );
        let report = serde_json::json!({
            "case": index, "shift": case.shift, "glowLayers": rendered.glow_layers,
            "stage": stage.json(), "glassInterior": interior.json(), "panelAndGlow": perimeter.json(),
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        fs::write(
            reference.join(format!("effects-diff-{index}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        let delta = RgbaImage::from_fn(width, height, |x, y| {
            let [ar, ag, ab, _] = rendered.effects.get_pixel(x, y).0;
            let [br, bg, bb, _] = chrome.get_pixel(x, y).0;
            Rgba([
                ar.abs_diff(br).saturating_mul(4),
                ag.abs_diff(bg).saturating_mul(4),
                ab.abs_diff(bb).saturating_mul(4),
                255,
            ])
        });
        delta.save(reference.join(format!("effects-diff-{index}.png")))?;
        assert!(
            stage.mean <= 1.5 && interior.mean <= 0.8 && perimeter.mean <= 2.5,
            "RGB error exceeded the effect comparison tolerances"
        );
    }
    Ok(())
}

struct Diff {
    mean: f64,
    maximum: u8,
    channels: u64,
}

impl Diff {
    fn json(&self) -> Value {
        serde_json::json!({ "meanRgbError": self.mean, "maximumChannelError": self.maximum, "channels": self.channels })
    }
}

fn measure(left: &RgbaImage, right: &RgbaImage, bounds: Rect) -> Diff {
    assert_eq!(left.dimensions(), right.dimensions());
    let mut total = 0_u64;
    let mut maximum = 0_u8;
    let mut channels = 0_u64;
    for y in (bounds.y as u32)..(bounds.y as u32 + bounds.height).min(left.height()) {
        for x in (bounds.x as u32)..(bounds.x as u32 + bounds.width).min(left.width()) {
            for (&left, &right) in left.get_pixel(x, y).0[..3]
                .iter()
                .zip(&right.get_pixel(x, y).0[..3])
            {
                let delta = left.abs_diff(right);
                total += u64::from(delta);
                maximum = maximum.max(delta);
                channels += 1;
            }
        }
    }
    Diff {
        mean: total as f64 / channels as f64,
        maximum,
        channels,
    }
}

struct Preview {
    composer: GlassComposer,
    case: Case,
    image: Arc<RenderImage>,
}

impl Render for Preview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image = Arc::clone(&self.image);
        div()
            .size_full()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, _| {
                        window
                            .paint_image(bounds, Default::default(), image, 0, false)
                            .unwrap();
                    },
                )
                .size_full(),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.case.shift = if this.case.shift == 90.0 {
                        0.0
                    } else {
                        this.case.shift + 30.0
                    };
                    let raster = this.composer.rasterize(
                        this.case.dimensions,
                        this.case.panel,
                        (this.case.shift, 0.0),
                    );
                    if let Ok(frame) = raster {
                        let old = std::mem::replace(&mut this.image, gpu_texture(frame.effects));
                        window.drop_image(old).ok();
                        cx.notify();
                    }
                }),
            )
    }
}

fn present(composer: GlassComposer, case: Case, frame: RgbaImage) {
    let (width, height) = case.dimensions;
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(width as f32), px(height as f32)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Preview {
                    composer,
                    case,
                    image: gpu_texture(frame),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, MouseButton, TestAppContext, point};

    #[gpui::test]
    #[ignore = "run capture.mjs --effects first"]
    fn repeated_paint_uses_the_moved_backdrop(cx: &mut TestAppContext) {
        let reference = env::var_os("NEON_GLASS_REFERENCE").unwrap();
        let reference = Path::new(&reference);
        let scene = SceneIr::from_json(SCENE).unwrap();
        let composer = GlassComposer::load(scene, reference).unwrap();
        let data: Value =
            serde_json::from_str(&fs::read_to_string(reference.join("effects.json")).unwrap())
                .unwrap();
        let case = Case::read(&data[0]);
        let dimensions = case.dimensions;
        let panel = case.panel;
        let initial = composer.rasterize(dimensions, panel, (0.0, 0.0)).unwrap();

        let (view, cx) = cx.add_window_view(|_, _| Preview {
            composer,
            case,
            image: gpu_texture(initial.effects),
        });
        cx.simulate_resize(size(px(dimensions.0 as f32), px(dimensions.1 as f32)));
        cx.update(|window, app| window.draw(app).clear());
        let before = cx.read(|app| Arc::clone(&view.read(app).image));
        cx.simulate_mouse_down(
            point(px(32.0), px(32.0)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|window, app| window.draw(app).clear());
        cx.read(|app| {
            let view = view.read(app);
            assert_eq!(view.case.shift, 30.0);
            assert_eq!(view.case.panel, panel);
            let before = before.as_bytes(0).unwrap();
            let after = view.image.as_bytes(0).unwrap();
            let pixels = (panel.y as usize + 12..panel.y as usize + panel.height as usize - 12)
                .flat_map(|y| {
                    (panel.x as usize + 12..panel.x as usize + panel.width as usize - 12)
                        .map(move |x| 4 * (y * dimensions.0 as usize + x))
                });
            assert!(
                pixels
                    .into_iter()
                    .any(|offset| before[offset..offset + 3] != after[offset..offset + 3])
            );
        });
    }
}
