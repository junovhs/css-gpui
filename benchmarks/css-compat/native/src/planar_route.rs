use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use css_gpui::planar::{Pose, Transition, ViewportControl};
use gpui::{
    App, AppContext, Application, Bounds, Context, FocusHandle, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, RenderImage, StatefulInteractiveElement, Styled, Window,
    WindowBounds, WindowOptions, actions, canvas, div, px, rgb, rgba, size,
};
use serde::{Deserialize, Serialize};

use super::read_frame;

#[path = "planar_document.rs"]
mod document;

use document::{Activations, Document, State as DocumentState};

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
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    movement: Option<ViewportControl>,
}

struct SceneFrame {
    controls: CheckedState,
    labels: Vec<Label>,
    image: Arc<RenderImage>,
}

enum Source {
    Route(Vec<Arc<SceneFrame>>),
    Document(Box<DocumentSource>),
}

struct DocumentSource {
    document: Arc<Document>,
    activations: Activations,
    state: Arc<DocumentState>,
    settled: SettledDocument,
}

struct SettledDocument {
    activations: Activations,
    state: Arc<DocumentState>,
    frame: Arc<SceneFrame>,
}

struct MotionFrame {
    index: usize,
    image: Arc<RenderImage>,
    pose: Pose,
    elapsed: Duration,
    generated: Duration,
    started: Instant,
}

struct Viewer {
    source: Source,
    frame: Arc<SceneFrame>,
    current: usize,
    width: u32,
    height: u32,
    pending: bool,
    motion: Option<MotionFrame>,
    motion_start: Option<Pose>,
    trace_motion: bool,
    report: bool,
    focus: FocusHandle,
}

impl Viewer {
    fn destination(&self, label: &Label) -> Option<usize> {
        let controls = self.frame.controls.after(label)?;
        match &self.source {
            Source::Route(frames) => frames.iter().position(|frame| frame.controls == controls),
            Source::Document(_) => Some(self.current + 1),
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
            Source::Document(source) => {
                let document = source.document.clone();
                let mut activations = source.activations.clone();
                document.activate(&mut activations, &label);
                let started = Instant::now();
                let resolve_document = document.clone();
                let resolve = cx.background_spawn(async move {
                    resolve_document
                        .resolve(&activations)
                        .map(|state| (activations, Arc::new(state)))
                });
                self.pending = true;
                cx.spawn_in(window, async move |view, cx| {
                    let (activations, state) = match resolve.await {
                        Ok(state) => state,
                        Err(error) => {
                            let _ = view.update_in(cx, |view, _, cx| {
                                view.pending = false;
                                eprintln!("could not activate {}: {error}", label.target);
                                cx.notify();
                            });
                            return;
                        }
                    };
                    let Some((transition, render)) = view
                        .update_in(cx, |view, _, cx| {
                            let Source::Document(source) = &mut view.source else {
                                return None;
                            };
                            let transition = state.preview.transition_from(
                                &source.state.preview,
                                view.motion
                                    .as_ref()
                                    .map(|motion| &motion.pose)
                                    .or(view.motion_start.as_ref()),
                            );
                            let final_document = document.clone();
                            let final_state = state.clone();
                            let render = cx.background_spawn(async move {
                                final_document.image(&final_state, None)
                            });
                            if let Some(transition) = &transition {
                                view.motion_start = Some(transition.sample(Duration::ZERO));
                                source.state = state.clone();
                                source.activations = activations.clone();
                                view.commit(
                                    next,
                                    &label.target,
                                    state.frame(view.frame.image.clone()),
                                );
                                view.report = false;
                                view.pending = false;
                                cx.notify();
                            }
                            Some((transition, render))
                        })
                        .ok()
                        .flatten()
                    else {
                        return;
                    };

                    let moving = transition.is_some();
                    if let Some(transition) = transition {
                        let animation = Animation {
                            next,
                            started,
                            transition: Arc::new(transition),
                            document,
                            state: state.clone(),
                        };
                        if !animation.present(&view, cx).await {
                            return;
                        }
                    }

                    let result = render.await;
                    let _ = view.update_in(cx, |view, window, cx| {
                        // An interrupted move's background raster must never
                        // overwrite the successor. Its state is already stale.
                        if moving && view.current != next {
                            return;
                        }
                        match result {
                            Ok(image) => {
                                view.motion_start = None;
                                let _ = window.drop_image(view.frame.image.clone());
                                let frame = state.frame(image);
                                if !moving {
                                    view.commit(next, &label.target, frame.clone());
                                    view.pending = false;
                                } else {
                                    view.frame = frame.clone();
                                    view.report = true;
                                }
                                if let Source::Document(source) = &mut view.source {
                                    source.settled = SettledDocument {
                                        activations: activations.clone(),
                                        state: state.clone(),
                                        frame,
                                    };
                                    source.activations = activations;
                                    source.state = state;
                                }
                                if let Some(old) = view.motion.take() {
                                    let _ = window.drop_image(old.image);
                                }
                            }
                            Err(error) => {
                                eprintln!("could not activate {}: {error}", label.target);
                                if moving && let Source::Document(source) = &mut view.source {
                                    source.activations = source.settled.activations.clone();
                                    source.state = source.settled.state.clone();
                                    view.frame = source.settled.frame.clone();
                                    view.motion_start = None;
                                    if let Some(old) = view.motion.take() {
                                        let _ = window.drop_image(old.image);
                                    }
                                    view.report = true;
                                } else {
                                    view.pending = false;
                                }
                            }
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

impl DocumentState {
    fn frame(&self, image: Arc<RenderImage>) -> Arc<SceneFrame> {
        Arc::new(SceneFrame {
            controls: self.controls.clone(),
            labels: self.labels.clone(),
            image,
        })
    }
}

struct Animation {
    next: usize,
    started: Instant,
    transition: Arc<Transition>,
    document: Arc<Document>,
    state: Arc<DocumentState>,
}

impl Animation {
    async fn present(
        &self,
        view: &gpui::WeakEntity<Viewer>,
        cx: &mut gpui::AsyncWindowContext,
    ) -> bool {
        const FRAME_INTERVAL: Duration = Duration::from_micros(16_667);
        let executor = cx.background_executor().clone();
        loop {
            let elapsed = self.started.elapsed();
            let deadline = self.transition.duration();
            if elapsed >= deadline {
                return true;
            }
            let document = self.document.clone();
            let state = self.state.clone();
            let transition = self.transition.clone();
            let started = self.started;
            let index = self.next;
            let render = executor.spawn(async move {
                let rendering = Instant::now();
                let pose = transition.sample(elapsed);
                let image = document.image(&state, Some(&pose))?;
                Ok::<_, String>(MotionFrame {
                    index,
                    image,
                    pose,
                    elapsed,
                    generated: rendering.elapsed(),
                    started,
                })
            });
            let result = render.await;
            let remaining = deadline.saturating_sub(self.started.elapsed());
            let current = view
                .update_in(cx, |view, window, cx| {
                    if view.current != self.next {
                        return false;
                    }
                    if !remaining.is_zero() {
                        match result {
                            Ok(frame) => {
                                if let Some(old) = view.motion.replace(frame) {
                                    let _ = window.drop_image(old.image);
                                }
                                cx.notify();
                            }
                            Err(error) => eprintln!("could not sample planar motion: {error}"),
                        }
                    }
                    true
                })
                .unwrap_or(false);
            if !current {
                return false;
            }
            let pause = (elapsed + FRAME_INTERVAL).saturating_sub(self.started.elapsed());
            executor.timer(pause.min(remaining)).await;
        }
    }
}

impl Render for Viewer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current;
        let image = self
            .motion
            .as_ref()
            .map_or(&self.frame.image, |motion| &motion.image)
            .clone();
        let motion_report = self
            .trace_motion
            .then(|| {
                self.motion.as_ref().map(|motion| {
                    serde_json::json!({
                        "motion": motion.index, "sample_ms": motion.elapsed.as_secs_f64() * 1000.0,
                        "generated_ms": motion.generated.as_secs_f64() * 1000.0,
                        "submitted_ms": motion.started.elapsed().as_secs_f64() * 1000.0,
                        "transforms": motion.pose,
                    })
                })
            })
            .flatten();
        let report = self.report.then(|| {
            serde_json::to_string(&Frame {
                frame: current,
                controls: &self.frame.controls,
                labels: matches!(self.source, Source::Document(_)).then_some(&self.frame.labels),
            })
            .expect("serialize planar readback")
        });
        self.report = false;
        let buttons = self
            .frame
            .labels
            .iter()
            .enumerate()
            .filter(|(_, label)| label.movement.is_none())
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
        let mut movement = self
            .frame
            .labels
            .iter()
            .enumerate()
            .filter_map(|(index, label)| Some((index, label, label.movement.as_ref()?)))
            .filter(|(_, _, control)| {
                let [left, top, width, height] = control.bounds;
                width > 0.0
                    && height > 0.0
                    && left < self.width as f32
                    && top < self.height as f32
                    && left + width > 0.0
                    && top + height > 0.0
            })
            .collect::<Vec<_>>();
        movement.sort_by_key(|(index, _, control)| (control.stacking_order, *index));
        let movement = movement.into_iter().map(|(index, label, control)| {
            let [left, top, width, height] = control.bounds;
            let button = div()
                .id(("movement-label", index))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .h(px(height))
                .pt(px(control.padding_top))
                .flex()
                .justify_center()
                .items_start()
                .rounded(px(control.corner_radius))
                .bg(rgba(u32::from_be_bytes(control.background)))
                .text_color(rgba(u32::from_be_bytes(control.color)))
                .text_size(px(control.font_size))
                .child(control.glyph.to_string());
            if self.pending {
                return button;
            }
            let button = if self.destination(label).is_some() {
                button.tab_index(0).cursor_pointer()
            } else {
                button
            };
            let control = control.clone();
            button.on_click(
                cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
                    if let Some(point) = event.mouse_position()
                        && !control.contains([point.x.into(), point.y.into()])
                    {
                        return;
                    }
                    cx.stop_propagation();
                    view.activate(current, index, window, cx);
                }),
            )
        });

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
                div()
                    .relative()
                    .w_full()
                    .h(px(self.height as f32))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, (), window, _| {
                                window
                                    .paint_image(bounds, Default::default(), image, 0, false)
                                    .expect("paint the route frame");
                                if let Some(record) = motion_report {
                                    println!("{record}");
                                }
                                if let Some(record) = report {
                                    println!("{record}");
                                }
                            },
                        )
                        .size_full(),
                    )
                    .children(movement),
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
    authored_controls: bool,
) -> Result<(), Box<dyn Error>> {
    let (width, height) = viewport
        .split_once('x')
        .ok_or("expected viewport WIDTHxHEIGHT")?;
    let [width, height] = [width.parse()?, height.parse()?];
    let document = Arc::new(Document::new(
        [width, height],
        html,
        css,
        root,
        scope,
        authored_controls,
    )?);
    let activations = Activations::default();
    let state = Arc::new(document.resolve(&activations)?);
    let initial = state.frame(document.image(&state, None)?);
    let source = Source::Document(Box::new(DocumentSource {
        document,
        activations: activations.clone(),
        state: state.clone(),
        settled: SettledDocument {
            activations,
            state,
            frame: initial.clone(),
        },
    }));
    open(width, height, source, initial);
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
                        motion: None,
                        motion_start: None,
                        trace_motion: std::env::var_os("CSS_GPUI_TRACE_MOTION").is_some(),
                        width,
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
