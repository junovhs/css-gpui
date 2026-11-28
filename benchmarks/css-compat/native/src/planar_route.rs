use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
    sync::Arc,
};

use gpui::{
    App, AppContext, Application, Bounds, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, RenderImage, StatefulInteractiveElement, Styled, Window,
    WindowBounds, WindowOptions, actions, canvas, div, px, rgb, size,
};
use serde::{Deserialize, Serialize};

use super::read_frame;

#[path = "planar_document.rs"]
mod document;

use document::{Activations, Document};

const STRIP_HEIGHT: u32 = 48;
actions!(planar_route, [TabNext, TabPrevious]);

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
struct CheckedState {
    selection: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    checked: BTreeSet<String>,
}

impl CheckedState {
    fn after(&self, label: &Label) -> Option<Self> {
        let mut next = self.clone();
        if label.checkbox {
            if !next.checked.remove(&label.target) {
                next.checked.insert(label.target.clone());
            }
        } else {
            if self.selection.get(&label.group) == Some(&label.target) {
                return None;
            }
            next.selection
                .insert(label.group.clone(), label.target.clone());
        }
        Some(next)
    }
}

#[derive(Deserialize)]
struct Bundle {
    width: u32,
    height: u32,
    states: Vec<State>,
}

#[derive(Deserialize)]
struct State {
    rgba: String,
    #[serde(flatten)]
    controls: CheckedState,
    labels: Vec<Label>,
}

#[derive(Serialize)]
struct Frame<'a> {
    frame: usize,
    #[serde(flatten)]
    controls: &'a CheckedState,
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<&'a [Label]>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Label {
    group: String,
    target: String,
    caption: String,
    #[serde(default)]
    class_name: String,
    #[serde(default)]
    checkbox: bool,
}

struct SceneFrame {
    controls: CheckedState,
    labels: Vec<Label>,
    image: Arc<RenderImage>,
}

enum Source {
    Route(Vec<Arc<SceneFrame>>),
    Document {
        document: Arc<Document>,
        activations: Activations,
    },
}

struct Viewer {
    source: Source,
    frame: Arc<SceneFrame>,
    current: usize,
    height: u32,
    pending: bool,
    report: bool,
    focus: FocusHandle,
}

impl Viewer {
    fn destination(&self, label: &Label) -> Option<usize> {
        let controls = self.frame.controls.after(label)?;
        match &self.source {
            Source::Route(frames) => frames.iter().position(|frame| frame.controls == controls),
            Source::Document { .. } => Some(self.current + 1),
        }
    }

    fn commit(&mut self, next: usize, target: &str, frame: Arc<SceneFrame>) {
        println!(
            "{}",
            serde_json::json!({ "activated": target, "from": self.current, "to": next })
        );
        self.current = next;
        self.frame = frame;
        self.report = true;
    }

    fn activate(&mut self, from: usize, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if from != self.current || self.pending {
            return;
        }
        let label = self.frame.labels[index].clone();
        let Some(next) = self.destination(&label) else {
            return;
        };
        window.focus(&self.focus);
        match &self.source {
            Source::Route(frames) => {
                self.commit(next, &label.target, frames[next].clone());
            }
            Source::Document {
                document,
                activations,
            } => {
                let document = Arc::clone(document);
                let mut activations = activations.clone();
                document.activate(&mut activations, &label);
                let render = cx.background_spawn(async move {
                    let result = document.render(&activations);
                    (result, activations)
                });
                self.pending = true;
                cx.spawn_in(window, async move |view, cx| {
                    let (result, next_activations) = render.await;
                    let _ = view.update_in(cx, |view, window, cx| {
                        view.pending = false;
                        match result {
                            Ok(frame) => {
                                if let Source::Document { activations, .. } = &mut view.source {
                                    *activations = next_activations;
                                }
                                let _ = window.drop_image(view.frame.image.clone());
                                view.commit(next, &label.target, Arc::new(frame));
                            }
                            Err(error) => eprintln!("could not activate {}: {error}", label.target),
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
        }
        cx.notify();
    }
}

impl Render for Viewer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current;
        let image = self.frame.image.clone();
        let report = self.report.then(|| {
            serde_json::to_string(&Frame {
                frame: current,
                controls: &self.frame.controls,
                labels: matches!(self.source, Source::Document { .. })
                    .then_some(&self.frame.labels),
            })
            .expect("serialize planar readback")
        });
        self.report = false;
        let buttons = self
            .frame
            .labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let button = div()
                    .id(("route-label", index))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .justify_center()
                    .items_center()
                    .border_1()
                    .border_color(rgb(0x2c323a))
                    .text_size(px(19.0))
                    .child(label.caption.clone());
                if !self.pending && self.destination(label).is_some() {
                    button
                        .tab_index(0)
                        .cursor_pointer()
                        .bg(rgb(0x1e293b))
                        .text_color(rgb(0xf8fafc))
                        .focus(|style| style.border_color(rgb(0x93c5fd)))
                        .hover(|style| style.bg(rgb(0x334155)))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.activate(current, index, window, cx)
                        }))
                } else {
                    button.bg(rgb(0x171b21)).text_color(rgb(0x888e97))
                }
            })
            .collect::<Vec<_>>();

        div()
            .id("route")
            .track_focus(&self.focus)
            .tab_group()
            .on_action(|_: &TabNext, window, _| window.focus_next())
            .on_action(|_: &TabPrevious, window, _| window.focus_prev())
            .size_full()
            .flex()
            .flex_col()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| {
                        window
                            .paint_image(bounds, Default::default(), image, 0, false)
                            .expect("paint the route frame");
                        if let Some(record) = report {
                            println!("{record}");
                        }
                    },
                )
                .w_full()
                .h(px(self.height as f32))
                .flex_shrink_0(),
            )
            .child(
                div()
                    .id(("route-strip", current))
                    .flex()
                    .w_full()
                    .h(px(STRIP_HEIGHT as f32))
                    .bg(rgb(0x171b21))
                    .children(buttons),
            )
    }
}

pub(super) fn run(path: &Path) -> Result<(), Box<dyn Error>> {
    let Bundle {
        width,
        height,
        states,
    } = serde_json::from_slice(&std::fs::read(path)?)?;
    if states.is_empty() {
        return Err("the planar route requires at least one frame".into());
    }
    let directory = path.parent().unwrap_or(Path::new(""));
    let frames = states
        .into_iter()
        .map(|state| {
            Ok(Arc::new(SceneFrame {
                controls: state.controls,
                labels: state.labels,
                image: read_frame(width, height, &directory.join(&state.rgba))?,
            }))
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let initial = frames[0].clone();
    open(width, height, Source::Route(frames), initial);
    Ok(())
}

pub(super) fn run_document(
    viewport: &str,
    html: &Path,
    css: &Path,
    root: &str,
    scope: &str,
) -> Result<(), Box<dyn Error>> {
    let (width, height) = viewport
        .split_once('x')
        .ok_or("expected viewport WIDTHxHEIGHT")?;
    let [width, height] = [width.parse()?, height.parse()?];
    let document = Arc::new(Document::new([width, height], html, css, root, scope)?);
    let activations = Activations::default();
    let initial = Arc::new(document.render(&activations)?);
    open(
        width,
        height,
        Source::Document {
            document,
            activations,
        },
        initial,
    );
    Ok(())
}

fn open(width: u32, height: u32, source: Source, frame: Arc<SceneFrame>) {
    Application::new().run(move |app: &mut App| {
        app.bind_keys([
            KeyBinding::new("tab", TabNext, None),
            KeyBinding::new("shift-tab", TabPrevious, None),
        ]);
        let bounds = Bounds::centered(
            None,
            size(px(width as f32), px((height + STRIP_HEIGHT) as f32)),
            app,
        );
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                is_resizable: false,
                ..Default::default()
            },
            move |window, app| {
                app.new(|cx| {
                    let focus = cx.focus_handle();
                    window.focus(&focus);
                    Viewer {
                        source,
                        frame,
                        current: 0,
                        pending: false,
                        height,
                        report: true,
                        focus,
                    }
                })
            },
        )
        .expect("open GPUI planar route");
        app.activate(true);
    });
}
