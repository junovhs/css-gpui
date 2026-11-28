mod paint;
mod projection;

use std::{sync::Arc, time::Instant};

use css_gpui::carousel::CarouselScene;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, KeyBinding, Render, RenderImage, Window,
    WindowBounds, WindowOptions, actions, canvas, div, prelude::*, px, rgb, size,
};
use image::Frame;
use paint::Label;
use projection::{Pose, project, scene_origin};
use serde_json::json;

const SOURCE_SCRIPT: &str = include_str!("../../source.js");

actions!(carousel, [TabNext, TabPrevious]);

struct ScriptBindings {
    index: i64,
    cell_count: u32,
    turn_degrees: f32,
    inline_z: f32,
}

impl ScriptBindings {
    fn compile(js: &str) -> Self {
        // Native binding for this specific upstream DOM click contract. Read
        // its operands and fail if the handlers no longer have that shape.
        fn operand<'a>(js: &'a str, prefix: &str, suffix: &str) -> &'a str {
            js.lines()
                .find_map(|line| line.trim().strip_prefix(prefix)?.strip_suffix(suffix))
                .unwrap_or_else(|| {
                    panic!("unsupported carousel script: expected {prefix}…{suffix}")
                })
        }

        for (variable, selector, operation) in [
            ("prevButton", "previous-button", "--"),
            ("nextButton", "next-button", "++"),
        ] {
            let query = format!("var {variable} = document.querySelector('.{selector}');");
            let handler = format!(
                "{variable}.addEventListener( 'click', function() {{\n  selectedIndex{operation};\n  rotateCarousel();\n}});"
            );
            assert!(
                js.contains(&query) && js.contains(&handler),
                "unsupported .{selector} binding"
            );
        }
        let compiled = Self {
            index: operand(js, "var selectedIndex = ", ";").parse().unwrap(),
            cell_count: operand(js, "var cellCount = ", ";").parse().unwrap(),
            turn_degrees: operand(js, "var angle = selectedIndex / cellCount * ", ";")
                .parse()
                .unwrap(),
            inline_z: operand(
                js,
                "carousel.style.transform = 'translateZ(",
                "px) rotateY(' + angle + 'deg)';",
            )
            .parse()
            .unwrap(),
        };
        assert!(
            compiled.cell_count > 0
                && compiled.inline_z.is_finite()
                && compiled.turn_degrees.is_finite()
        );
        compiled
    }

    fn step(&mut self, delta: i64) -> Pose {
        self.index += delta;
        Pose {
            angle: self.index as f32 / self.cell_count as f32 * self.turn_degrees,
            z: self.inline_z,
        }
    }
}

struct Motion {
    from: Pose,
    to: Pose,
    started: Instant,
    duration: f32,
}

impl Motion {
    fn sample(&self, now: Instant) -> (Pose, bool) {
        let elapsed = now.duration_since(self.started).as_secs_f32();
        if elapsed >= self.duration {
            return (self.to, true);
        }
        let progress = ease(elapsed / self.duration);
        let lerp = |from, to| from + (to - from) * progress;
        (
            Pose {
                angle: lerp(self.from.angle, self.to.angle),
                z: lerp(self.from.z, self.to.z),
            },
            false,
        )
    }
}

// CSS ease is cubic-bezier(.25, .1, .25, 1). Solve x for the parameter.
fn ease(progress: f32) -> f32 {
    let cubic = |t: f32, a: f32, b: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * a + 3.0 * inverse * t * t * b + t * t * t
    };
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..16 {
        let t = (lower + upper) * 0.5;
        if cubic(t, 0.25, 0.25) < progress {
            lower = t;
        } else {
            upper = t;
        }
    }
    cubic((lower + upper) * 0.5, 0.1, 1.0)
}

struct Carousel {
    scene: CarouselScene,
    script: ScriptBindings,
    labels: Vec<Label>,
    target: Pose,
    motion: Option<Motion>,
    frame: Option<FrameCache>,
    root: FocusHandle,
    previous: FocusHandle,
    next: FocusHandle,
    trace: bool,
    report_settled: bool,
}

struct FrameCache {
    image: Arc<RenderImage>,
    pose: Pose,
    dimensions: (u32, u32, f32),
}

impl Carousel {
    fn new(scene: CarouselScene, trace: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let labels = paint::labels(&scene.panels);
        let target = Pose {
            angle: 0.0,
            z: scene.resting_z,
        };
        let script = ScriptBindings::compile(SOURCE_SCRIPT);
        assert_eq!(
            script.cell_count as usize,
            scene.panels.len(),
            "script cellCount"
        );
        let root = cx.focus_handle();
        window.focus(&root);
        Self {
            scene,
            script,
            labels,
            target,
            motion: None,
            frame: None,
            root,
            previous: cx.focus_handle().tab_index(1).tab_stop(true),
            next: cx.focus_handle().tab_index(2).tab_stop(true),
            trace,
            report_settled: true,
        }
    }

    fn navigate(&mut self, step: i64, cx: &mut Context<Self>) {
        let now = Instant::now();
        let from = self
            .motion
            .as_ref()
            .map_or(self.target, |motion| motion.sample(now).0);
        self.target = self.script.step(step);
        self.motion = Some(Motion {
            from,
            to: self.target,
            started: now,
            duration: self.scene.transition_seconds,
        });
        self.report_settled = true;
        cx.notify();
    }

    fn button(&self, step: i64, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, label, focus) = if step < 0 {
            ("previous-button", &self.scene.previous, &self.previous)
        } else {
            ("next-button", &self.scene.next, &self.next)
        };
        // The upstream design leaves the buttons to the user agent. Keep a
        // compact desktop button style and use GPUI's mouse and key activation.
        div()
            .id(id)
            .track_focus(focus)
            .px(px(6.0))
            .h(px(21.0))
            .flex()
            .items_center()
            .border_1()
            .border_color(rgb(0x767676))
            .rounded(px(2.0))
            .bg(rgb(0xefefef))
            .hover(|style| style.bg(rgb(0xe5e5e5)))
            .active(|style| style.bg(rgb(0xf5f5f5)))
            .focus(|style| style.border_color(rgb(0x215bca)))
            .text_color(rgb(0x000000))
            .font_family("Liberation Sans")
            .text_size(px(40.0 / 3.0))
            .line_height(px(15.0))
            .child(label.clone())
            .on_click(cx.listener(move |view, _, _, cx| view.navigate(step, cx)))
    }
}

impl Render for Carousel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (pose, settled) = self
            .motion
            .as_ref()
            .map_or((self.target, true), |motion| motion.sample(Instant::now()));
        if settled {
            self.motion = None;
        } else {
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        }

        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        let scale = window.scale_factor();
        let dimensions = (
            (width * scale).round() as u32,
            (height * scale).round() as u32,
            scale,
        );
        let scene_origin = scene_origin(&self.scene, width);
        let needs_frame = self
            .frame
            .as_ref()
            .is_none_or(|frame| frame.pose != pose || frame.dimensions != dimensions);
        let trace_settled = self.trace && self.report_settled && settled;
        let planes = (needs_frame || trace_settled)
            .then(|| project(&self.scene, scene_origin, [width, height], pose));
        if needs_frame {
            let texture = paint::texture(
                &self.scene,
                scene_origin,
                planes.as_deref().unwrap(),
                &self.labels,
                dimensions.0,
                dimensions.1,
                scale,
            );
            let image = Arc::new(RenderImage::new(vec![Frame::new(texture)]));
            if let Some(previous) = self.frame.replace(FrameCache {
                image,
                pose,
                dimensions,
            }) {
                window
                    .drop_image(previous.image)
                    .expect("release previous carousel texture");
            }
        }
        let image = self.frame.as_ref().unwrap().image.clone();
        let row_top = scene_origin[1] + self.scene.scene[1] + self.scene.margins_y[1].max(16.0); // The following paragraph's UA block margin.
        let record = if trace_settled {
            self.report_settled = false;
            let cells: Vec<Vec<Option<f64>>> = planes
                .as_deref()
                .unwrap()
                .iter()
                .map(|plane| {
                    plane
                        .projected
                        .geometry
                        .corners
                        .into_iter()
                        .flat_map(|corner| corner.map_or([None; 2], |point| point.map(Some)))
                        .collect()
                })
                .collect();
            Some(json!({
                "width": width, "height": height, "index": self.script.index, "angle": pose.angle,
                "cells": cells,
            }))
        } else {
            None
        };

        div()
            .id("carousel-viewport")
            .track_focus(&self.root)
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(|_: &TabNext, window, _| window.focus_next())
            .on_action(|_: &TabPrevious, window, _| window.focus_prev())
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| {
                        window
                            .paint_image(bounds, Default::default(), image, 0, false)
                            .expect("paint carousel texture");
                        if let Some(record) = record {
                            println!("{record}");
                        }
                    },
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top(px(row_top))
                    .w_full()
                    .flex()
                    .justify_center()
                    .gap(px(4.45))
                    .child(self.button(-1, cx))
                    .child(self.button(1, cx)),
            )
    }
}

fn main() {
    let mut trace = false;
    let mut dimensions = Vec::new();
    for argument in std::env::args().skip(1) {
        if argument == "--trace" {
            trace = true;
        } else {
            dimensions.push(argument.parse::<u32>().expect("width and height in pixels"));
        }
    }
    let [width, height] = match dimensions.as_slice() {
        [] => [720, 280],
        [width, height] if *width > 0 && *height > 0 => [*width, *height],
        _ => panic!("usage: css-gpui-carousel [width height] [--trace]"),
    };
    let scene = serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/carousel.json")))
        .expect("read built carousel scene");
    Application::new().run(move |app: &mut App| {
        app.bind_keys([
            KeyBinding::new("tab", TabNext, None),
            KeyBinding::new("shift-tab", TabPrevious, None),
        ]);
        let bounds = Bounds::centered(None, size(px(width as f32), px(height as f32)), app);
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_resizable: false,
                ..Default::default()
            },
            |window, app| app.new(|cx| Carousel::new(scene, trace, window, cx)),
        )
        .expect("open native carousel");
        app.activate(true);
    });
}
