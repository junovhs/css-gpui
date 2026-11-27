use std::{
    borrow::Cow,
    error::Error,
    sync::{Arc, LazyLock},
    time::Duration,
};

use css_gpui::{
    ir::{Paint, SceneIr},
    paint::{Background, Rect, rounded_coverage},
};
use gpui::{
    App, AssetSource, Context, MouseButton, Pixels, Point, Render, RenderImage, SharedString,
    Window, canvas, div, prelude::*, px,
};
use image::{Rgba, RgbaImage};
use serde_json::Value;

use crate::effects::{GlassComposer, PreparedBackdrop, gpu_texture};

mod wide {
    pub(super) mod stage {
        include!(concat!(env!("OUT_DIR"), "/stage-1100.rs"));
    }
    pub(super) mod overlay {
        include!(concat!(env!("OUT_DIR"), "/overlay-1100.rs"));
    }
    pub(super) const CONFIG: &str = include_str!(concat!(env!("OUT_DIR"), "/live-1100.json"));
}

mod narrow {
    pub(super) mod stage {
        include!(concat!(env!("OUT_DIR"), "/stage-390.rs"));
    }
    pub(super) mod overlay {
        include!(concat!(env!("OUT_DIR"), "/overlay-390.rs"));
    }
    pub(super) const CONFIG: &str = include_str!(concat!(env!("OUT_DIR"), "/live-390.json"));
}

const SCENE: &str = include_str!(concat!(env!("OUT_DIR"), "/scene-1100.json"));
const BACKDROP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/abstract-light.jpg"));
const NOISE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/noise-base.png"));
include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

pub fn register_fonts(app: &App) -> gpui::Result<()> {
    app.text_system()
        .add_fonts(FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect())
}

const RESET_MS: u64 = 200;

struct Assets(Vec<Box<dyn AssetSource>>);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        for source in &self.0 {
            if let Some(data) = source.load(path)? {
                return Ok(Some(data));
            }
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut names = Vec::new();
        for source in &self.0 {
            names.extend(source.list(path)?);
        }
        names.sort();
        names.dedup();
        Ok(names)
    }
}

pub fn scene_assets() -> impl AssetSource {
    Assets(vec![
        Box::new(wide::stage::scene_assets()),
        Box::new(wide::overlay::scene_assets()),
        Box::new(narrow::stage::scene_assets()),
        Box::new(narrow::overlay::scene_assets()),
    ])
}

struct Row {
    #[cfg(test)]
    id: String,
    #[cfg(test)]
    label: String,
    bounds: [f32; 4],
    hover: Paint,
}

struct Config {
    menu: [f32; 4],
    #[cfg(test)]
    footer: Option<(String, [f32; 4])>,
    rows: Vec<Row>,
    fade: u64,
    hide: u64,
    easing: [f32; 4],
    initial_selection: Option<usize>,
}

impl Config {
    fn parse(input: &str) -> Self {
        let config: Value = serde_json::from_str(input).unwrap();
        let menu: [f32; 4] =
            serde_json::from_value(config["geometry"]["boxes"]["menu"].clone()).unwrap();
        let hover: Vec<(String, Paint)> = serde_json::from_value(config["hover"].clone()).unwrap();
        let rows = config["geometry"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .zip(hover)
            .map(|(row, (_id, hover))| {
                let mut bounds: [f32; 4] = serde_json::from_value(row["bounds"].clone()).unwrap();
                bounds[0] -= menu[0];
                bounds[1] -= menu[1];
                Row {
                    #[cfg(test)]
                    id: _id,
                    #[cfg(test)]
                    label: row["label"].as_str().unwrap().to_owned(),
                    bounds,
                    hover,
                }
            })
            .collect();
        let easing = serde_json::from_value(config["ease"].clone()).unwrap();
        Self {
            menu,
            #[cfg(test)]
            footer: config["geometry"]["boxes"]["footer"].as_array().map(|_| {
                (
                    config["footerId"].as_str().unwrap().to_owned(),
                    serde_json::from_value(config["geometry"]["boxes"]["footer"].clone()).unwrap(),
                )
            }),
            rows,
            fade: config["fadeMs"].as_f64().unwrap() as u64,
            hide: config["hideMs"].as_f64().unwrap() as u64,
            easing,
            initial_selection: config["initialSelection"].as_u64().map(|row| row as usize),
        }
    }
}

static CONFIGS: LazyLock<[Config; 2]> =
    LazyLock::new(|| [Config::parse(wide::CONFIG), Config::parse(narrow::CONFIG)]);

struct Stage {
    dimensions: (u32, u32),
    prepared: PreparedBackdrop,
    texture: Arc<RenderImage>,
}

struct Glass {
    panel: Rect,
    texture: Arc<RenderImage>,
}

pub struct MenuView {
    composer: GlassComposer,
    stage: Option<Stage>,
    glass: Option<Glass>,
    hover_texture: Option<(usize, Arc<RenderImage>)>,
    shift: f32,
    dimensions: (u32, u32),
    origin: (f32, f32),
    initialized: bool,
    visible: bool,
    accepts_input: bool,
    opacity: f32,
    hovered: Option<usize>,
    selected: Option<usize>,
    generation: u64,
}

impl MenuView {
    pub fn new(shift: f32) -> Result<Self, Box<dyn Error>> {
        let composer = GlassComposer::from_images(SceneIr::from_json(SCENE)?, |url| {
            let bytes = if url.ends_with("/abstract-light.jpg") {
                BACKDROP
            } else if url.ends_with("/noise-base.png") {
                NOISE
            } else {
                return Err(format!("unrecognized image {url}").into());
            };
            Ok(image::load_from_memory(bytes)?.into_rgba8())
        })?;
        Ok(Self {
            composer,
            stage: None,
            glass: None,
            hover_texture: None,
            shift,
            dimensions: (1100, 760),
            origin: (0.0, 0.0),
            initialized: false,
            visible: true,
            accepts_input: true,
            opacity: 1.0,
            hovered: None,
            selected: CONFIGS[0].initial_selection,
            generation: 0,
        })
    }

    fn narrow(&self) -> bool {
        self.dimensions.0 <= 480
    }

    fn config(&self) -> &'static Config {
        &CONFIGS[usize::from(self.narrow())]
    }

    fn panel(&self) -> Rect {
        let [_, _, width, height] = self.config().menu;
        Rect {
            x: self.origin.0.round() as i32,
            y: self.origin.1.round() as i32,
            width: width.round() as u32,
            height: height.round() as u32,
        }
    }

    fn inside(&self, position: Point<Pixels>) -> bool {
        self.accepts_input
            && contains(
                [
                    self.origin.0,
                    self.origin.1,
                    self.config().menu[2],
                    self.config().menu[3],
                ],
                (position.x.into(), position.y.into()),
            )
    }

    fn row_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.inside(position)
            .then(|| {
                self.config().rows.iter().position(|row| {
                    contains(
                        row.bounds,
                        (
                            f32::from(position.x) - self.origin.0,
                            f32::from(position.y) - self.origin.1,
                        ),
                    )
                })
            })
            .flatten()
    }

    fn place(&mut self, x: f32, y: f32) {
        let [_, _, width, height] = self.config().menu;
        self.origin = (
            x.clamp(0.0, (self.dimensions.0 as f32 - width - 30.0).max(0.0)),
            y.clamp(0.0, (self.dimensions.1 as f32 - height - 20.0).max(0.0)),
        );
    }

    fn animate(&mut self, open: bool, cx: &mut Context<Self>) {
        self.accepts_input = open;
        self.visible = true;
        self.generation += 1;
        let generation = self.generation;
        let start = self.opacity;
        let target = if open { 1.0 } else { 0.0 };
        let config = self.config();
        let fade = config.fade;
        let easing = config.easing;
        let end = if open {
            fade
        } else {
            config.hide.max(fade).max(RESET_MS)
        };
        let executor = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            let mut elapsed = 0;
            while elapsed < end {
                let mut next = (elapsed + 16).min(end);
                for boundary in [fade, RESET_MS] {
                    if elapsed < boundary {
                        next = next.min(boundary);
                    }
                }
                executor.timer(Duration::from_millis(next - elapsed)).await;
                elapsed = next;
                let keep = entity
                    .update(cx, |view, cx| {
                        if view.generation != generation {
                            return false;
                        }
                        let progress = (elapsed as f32 / fade.max(1) as f32).min(1.0);
                        view.opacity = start + (target - start) * ease(progress, easing);
                        if !open && elapsed >= RESET_MS {
                            view.selected = None;
                        }
                        if !open && elapsed == end {
                            view.visible = false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn images(&mut self, window: &mut Window) {
        let viewport = window.viewport_size();
        let dimensions = (
            f32::from(viewport.width).round().max(1.0) as u32,
            f32::from(viewport.height).round().max(1.0) as u32,
        );
        if dimensions != self.dimensions || !self.initialized {
            self.dimensions = dimensions;
            if self.initialized {
                self.place(self.origin.0, self.origin.1);
            } else {
                let [x, y, _, _] = self.config().menu;
                self.place(x, y);
                self.initialized = true;
            }
            if let Some((_, previous)) = self.hover_texture.take() {
                window.drop_image(previous).ok();
            }
        }
        if self
            .stage
            .as_ref()
            .is_none_or(|stage| stage.dimensions != dimensions)
        {
            let prepared = self
                .composer
                .prepare(dimensions, (self.shift, 0.0))
                .expect("paint authored backdrop");
            let texture = gpu_texture(prepared.stage());
            if let Some(previous) = self.stage.replace(Stage {
                dimensions,
                prepared,
                texture,
            }) {
                window.drop_image(previous.texture).ok();
            }
            if let Some(previous) = self.glass.take() {
                window.drop_image(previous.texture).ok();
            }
        }
        let active = self.panel();
        if self.visible
            && self
                .glass
                .as_ref()
                .is_none_or(|glass| glass.panel != active)
        {
            let frame = self
                .composer
                .rasterize_prepared(&self.stage.as_ref().unwrap().prepared, active)
                .expect("paint the authored glass");
            let mut overlay = frame.effects;
            for (front, behind) in overlay.pixels_mut().zip(frame.stage.pixels()) {
                if front == behind {
                    *front = Rgba([0; 4]);
                }
            }
            if let Some(previous) = self.glass.replace(Glass {
                panel: active,
                texture: gpu_texture(overlay),
            }) {
                window.drop_image(previous.texture).ok();
            }
        }

        let highlight = self.hovered.or(self.selected);
        if self.hover_texture.as_ref().map(|(index, _)| *index) != highlight {
            if let Some((_, previous)) = self.hover_texture.take() {
                window.drop_image(previous).ok();
            }
            if let Some(index) = highlight {
                let row = &self.config().rows[index];
                let width = row.bounds[2].round() as u32;
                let height = row.bounds[3].round() as u32;
                let radius = row.hover.border_radius.unwrap_or(0.0);
                let paint = Background::new(&row.hover, |_| None, (0.0, 0.0))
                    .expect("compile row hover fill");
                let image = RgbaImage::from_fn(width, height, |x, y| {
                    let x = x as f32 + 0.5;
                    let y = y as f32 + 0.5;
                    let mut pixel = paint.sample(x, y, width as f32, height as f32);
                    pixel[3] *= rounded_coverage(x, y, width as f32, height as f32, radius);
                    Rgba(pixel.map(|channel| (channel * 255.0).round() as u8))
                });
                self.hover_texture = Some((index, gpu_texture(image)));
            }
        }
    }
}

impl Render for MenuView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.images(window);
        let action = |_: &str, _: &mut Window, _: &mut App| {};
        let stage = if self.narrow() {
            narrow::stage::render_scene(action).into_any_element()
        } else {
            wide::stage::render_scene(action).into_any_element()
        };
        let mut root = div()
            .size_full()
            .child(
                texture(Arc::clone(&self.stage.as_ref().unwrap().texture))
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .child(stage);
        if self.visible {
            let glass = Arc::clone(&self.glass.as_ref().unwrap().texture);
            // The compositor emits viewport coordinates, including glow outside the panel.
            root = root.child(
                texture(glass)
                    .top_0()
                    .left_0()
                    .size_full()
                    .opacity(self.opacity),
            );
            if let Some((index, highlight)) = &self.hover_texture {
                let [x, y, width, height] = self.config().rows[*index].bounds;
                root = root.child(
                    texture(Arc::clone(highlight))
                        .left(px(x + self.origin.0))
                        .top(px(y + self.origin.1))
                        .w(px(width))
                        .h(px(height))
                        .opacity(self.opacity),
                );
            }
            let menu = if self.narrow() {
                narrow::overlay::render_scene(action).into_any_element()
            } else {
                wide::overlay::render_scene(action).into_any_element()
            };
            root = root.child(
                div()
                    .absolute()
                    .left(px(self.origin.0))
                    .top(px(self.origin.1))
                    .w(px(self.config().menu[2]))
                    .opacity(self.opacity)
                    .child(menu),
            );
        }
        root.on_mouse_down(
            MouseButton::Right,
            cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                if !view.inside(event.position) {
                    view.place(event.position.x.into(), event.position.y.into());
                    view.hovered = None;
                    view.animate(true, cx);
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                if view.inside(event.position) {
                    view.selected = view.row_at(event.position);
                    cx.notify();
                } else if view.accepts_input {
                    view.hovered = None;
                    view.animate(false, cx);
                }
            }),
        )
        .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, _, cx| {
            let row = view.row_at(event.position);
            if row != view.hovered {
                view.hovered = row;
                cx.notify();
            }
        }))
    }
}

fn texture(image: Arc<RenderImage>) -> gpui::Canvas<()> {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            window
                .paint_image(bounds, Default::default(), image, 0, false)
                .expect("paint scene texture");
        },
    )
    .absolute()
}

fn contains(bounds: [f32; 4], (x, y): (f32, f32)) -> bool {
    let [left, top, width, height] = bounds;
    (left..left + width).contains(&x) && (top..top + height).contains(&y)
}

fn ease(progress: f32, [x1, y1, x2, y2]: [f32; 4]) -> f32 {
    if progress <= 0.0 || progress >= 1.0 {
        return progress.clamp(0.0, 1.0);
    }
    let bezier = |t: f32, p1, p2| {
        3.0 * (1.0 - t).powi(2) * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t.powi(3)
    };
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..14 {
        let mid = (lo + hi) * 0.5;
        if bezier(mid, x1, x2) < progress {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier((lo + hi) * 0.5, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Modifiers, TestAppContext, VisualTestContext, point, size};

    #[gpui::test]
    fn startup_footer_matches_chromium_bottom_inset(cx: &mut TestAppContext) {
        for (width, height) in [(1100.0, 760.0), (390.0, 844.0)] {
            let view = MenuView::new(90.0).unwrap();
            let (view, cx) = cx.add_window_view(|_, _| view);
            cx.simulate_resize(size(px(width), px(height)));
            redraw(cx);
            let (footer, panel) = cx.read(|app| {
                let menu = view.read(app);
                (
                    menu.config()
                        .footer
                        .as_ref()
                        .expect("recapture --native with footer geometry"),
                    menu.panel(),
                )
            });
            let bounds = cx.debug_bounds(&footer.0).expect("generated native footer");
            let native_top = f32::from(bounds.origin.y);
            let native_bottom = native_top + f32::from(bounds.size.height);
            let browser_bottom = footer.1[1] + footer.1[3];
            println!(
                "{width}x{height} footer inset: GPUI {:.3}px, Chromium {:.3}px; native bounds {bounds:?}",
                height - native_bottom,
                height - browser_bottom,
            );
            assert!((native_bottom - browser_bottom).abs() <= 1.0);
            assert!(native_top >= panel.y as f32 + panel.height as f32);
        }
    }

    #[gpui::test]
    fn replays_native_menu_over_both_browser_backdrops(cx: &mut TestAppContext) {
        let mut samples = Vec::new();
        for (width, height, shift, x, y) in [
            (1100.0_f32, 760.0_f32, 0.0_f32, 80.0, 120.0),
            (1100.0, 760.0, 90.0, 1092.0, 752.0),
            (390.0, 844.0, 90.0, 382.0, 836.0),
        ] {
            let view = MenuView::new(shift).unwrap();
            let (view, cx) = cx.add_window_view(|_, _| view);
            cx.simulate_resize(size(px(width), px(height)));
            redraw(cx);
            // Match the browser replay, which clears the initial open state first.
            cx.simulate_mouse_down(
                point(px(2.0), px(2.0)),
                MouseButton::Left,
                Modifiers::default(),
            );
            advance(cx, 512);
            cx.read(|app| assert!(!view.read(app).visible));

            cx.simulate_mouse_down(
                point(px(x), px(y)),
                MouseButton::Right,
                Modifiers::default(),
            );
            advance(cx, 96);
            cx.read(|app| {
                let menu = view.read(app);
                assert!(menu.accepts_input && menu.visible);
                assert!((0.0..1.0).contains(&menu.opacity));
                assert_ne!(menu.opacity, 0.0);
            });
            advance(cx, 160);
            redraw(cx);
            let (row_id, native_origin, expected_row) = cx.read(|app| {
                let menu = view.read(app);
                let [_, _, w, h] = menu.config().menu;
                assert!((menu.origin.0 - x.min(width - w - 30.0)).abs() <= 0.1);
                assert!((menu.origin.1 - y.min(height - h - 20.0)).abs() <= 0.1);
                assert_eq!(menu.opacity, 1.0);
                let row = &menu.config().rows[1];
                assert_eq!(row.label, "Calculator");
                let canvas = menu.stage.as_ref().unwrap().texture.as_bytes(0).unwrap();
                let glass = menu.glass.as_ref().unwrap().texture.as_bytes(0).unwrap();
                let panel = menu.panel();
                let offset = 4 * ((panel.y + 100) * width as i32 + panel.x + 100) as usize;
                assert_eq!(glass[offset + 3], 255);
                assert_ne!(&glass[offset..offset + 3], &canvas[offset..offset + 3]);
                samples.push(glass[offset..offset + 3].to_vec());
                (row.id.as_str(), menu.origin, row.bounds)
            });
            let bounds = cx
                .debug_bounds(row_id)
                .expect("generated native Calculator row");
            let bounds_from_native: [f32; 4] = [
                bounds.origin.x.into(),
                bounds.origin.y.into(),
                bounds.size.width.into(),
                bounds.size.height.into(),
            ];
            let expected = [
                native_origin.0 + expected_row[0],
                native_origin.1 + expected_row[1],
                expected_row[2],
                expected_row[3],
            ];
            assert!(
                bounds_from_native
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| (actual - expected).abs() <= 1.0),
                "native hit target {bounds_from_native:?}, expected {expected:?}"
            );
            let row_center = point(
                bounds.origin.x + bounds.size.width / 2.0,
                bounds.origin.y + bounds.size.height / 2.0,
            );
            cx.simulate_mouse_move(row_center, None, Modifiers::default());
            redraw(cx);
            cx.read(|app| {
                let menu = view.read(app);
                assert_eq!(menu.hovered, Some(1));
                let (index, texture) = menu.hover_texture.as_ref().expect("hover color layer");
                assert_eq!(*index, 1);
                assert!(
                    texture
                        .as_bytes(0)
                        .unwrap()
                        .chunks_exact(4)
                        .any(|pixel| pixel[3] > 0)
                );
            });

            cx.simulate_mouse_down(row_center, MouseButton::Right, Modifiers::default());
            cx.read(|app| assert_eq!(view.read(app).origin, native_origin));
            cx.simulate_mouse_down(row_center, MouseButton::Left, Modifiers::default());
            cx.read(|app| assert_eq!(view.read(app).selected, Some(1)));
            cx.simulate_mouse_down(
                point(px(2.0), px(2.0)),
                MouseButton::Left,
                Modifiers::default(),
            );
            cx.read(|app| assert!(!view.read(app).accepts_input));
            advance(cx, 192);
            cx.read(|app| {
                let menu = view.read(app);
                assert_eq!(menu.selected, Some(1));
                assert!(menu.visible);
            });
            advance(cx, 16);
            cx.read(|app| assert!(view.read(app).selected.is_none()));
            advance(cx, 48);
            cx.read(|app| {
                let menu = view.read(app);
                assert_eq!(menu.opacity, 0.0);
                assert!(menu.visible);
            });
            advance(cx, 256);
            redraw(cx);
            cx.read(|app| assert!(!view.read(app).visible));
            cx.simulate_mouse_move(row_center, None, Modifiers::default());
            cx.simulate_mouse_down(row_center, MouseButton::Left, Modifiers::default());
            cx.read(|app| {
                let menu = view.read(app);
                assert!(menu.hovered.is_none() && menu.selected.is_none() && !menu.visible);
            });
            if shift == 0.0 {
                cx.simulate_mouse_down(
                    point(px(80.0), px(120.0)),
                    MouseButton::Right,
                    Modifiers::default(),
                );
                advance(cx, 64);
                cx.simulate_mouse_down(
                    point(px(2.0), px(2.0)),
                    MouseButton::Left,
                    Modifiers::default(),
                );
                advance(cx, 64);
                cx.simulate_mouse_down(
                    point(px(108.0), px(124.0)),
                    MouseButton::Right,
                    Modifiers::default(),
                );
                advance(cx, 512);
                redraw(cx);
                cx.read(|app| {
                    let menu = view.read(app);
                    assert!(menu.accepts_input && menu.visible);
                    assert_eq!(menu.origin, (108.0, 124.0));
                    assert_eq!(menu.opacity, 1.0);
                });
            }
        }
        assert_ne!(samples[0], samples[1]);
    }

    fn advance(cx: &mut VisualTestContext, elapsed: u64) {
        let executor = cx.executor();
        for _ in 0..elapsed {
            executor.advance_clock(Duration::from_millis(1));
            cx.run_until_parked();
        }
    }

    fn redraw(cx: &mut VisualTestContext) {
        cx.update(|window, app| window.draw(app).clear());
    }
}
