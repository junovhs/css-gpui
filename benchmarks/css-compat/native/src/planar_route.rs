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

const STRIP_HEIGHT: u32 = 48;
actions!(planar_route, [TabNext, TabPrevious]);

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
struct CheckedState {
    selection: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    checked: BTreeSet<String>,
}

impl CheckedState {
    fn activate(&mut self, label: &Label) -> bool {
        if label.checkbox {
            if !self.checked.remove(&label.target) {
                self.checked.insert(label.target.clone());
            }
        } else {
            if self.selection.get(&label.group) == Some(&label.target) {
                return false;
            }
            self.selection
                .insert(label.group.clone(), label.target.clone());
        }
        true
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
}

#[derive(Deserialize)]
struct Label {
    group: String,
    target: String,
    caption: String,
    #[serde(default)]
    checkbox: bool,
}

struct Route {
    states: Vec<State>,
    images: Vec<Arc<RenderImage>>,
    current: usize,
    height: u32,
    report: bool,
    focus: FocusHandle,
}

impl Route {
    fn destination(&self, label: &Label) -> Option<usize> {
        let mut controls = self.states[self.current].controls.clone();
        if !controls.activate(label) {
            return None;
        }
        self.states
            .iter()
            .position(|state| state.controls == controls)
    }

    fn activate(&mut self, from: usize, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if from != self.current {
            return;
        }
        let label = &self.states[from].labels[index];
        if let Some(next) = self.destination(label) {
            println!(
                "{}",
                serde_json::json!({ "activated": label.target, "from": from, "to": next })
            );
            self.current = next;
            self.report = true;
            window.focus(&self.focus);
            cx.notify();
        }
    }
}

impl Render for Route {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.current;
        let state = &self.states[current];
        let image = self.images[current].clone();
        let report = self.report.then(|| {
            serde_json::to_string(&Frame {
                frame: current,
                controls: &state.controls,
            })
            .expect("serialize route readback")
        });
        self.report = false;
        let buttons = state
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
                if self.destination(label).is_some() {
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
    let images = states
        .iter()
        .map(|state| read_frame(width, height, &directory.join(&state.rgba)))
        .collect::<Result<_, _>>()?;
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
                    Route {
                        states,
                        images,
                        current: 0,
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
    Ok(())
}
