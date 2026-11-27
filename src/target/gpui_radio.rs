struct SceneRadioOption {
    id: &'static str,
    name: &'static str,
    checked: bool,
}

struct SceneRadioState {
    options: &'static [SceneRadioOption],
    selected: std::collections::HashMap<&'static str, &'static str>,
    focus: std::collections::HashMap<&'static str, gpui::FocusHandle>,
}

impl SceneRadioState {
    fn new(options: &'static [SceneRadioOption], cx: &mut gpui::Context<Self>) -> Self {
        Self {
            options,
            selected: options
                .iter()
                .filter(|option| option.checked)
                .map(|option| (option.name, option.id))
                .collect(),
            focus: options
                .iter()
                .map(|option| (option.id, cx.focus_handle()))
                .collect(),
        }
    }

    fn target(&self, id: &str) -> &SceneRadioOption {
        self.options.iter().find(|option| option.id == id).unwrap()
    }

    fn select(
        &mut self,
        id: &'static str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let name = self.target(id).name;
        self.selected.insert(name, id);
        self.focus[id].focus(window);
        cx.notify();
        window.refresh();
    }

    fn adjacent(&self, id: &str, forward: bool) -> &'static str {
        let name = self.target(id).name;
        let group = self.options.iter().filter(|option| option.name == name);
        let next = if forward {
            group
                .clone()
                .cycle()
                .skip_while(|option| option.id != id)
                .nth(1)
        } else {
            group
                .rev()
                .cycle()
                .skip_while(|option| option.id != id)
                .nth(1)
        };
        next.unwrap().id
    }

    fn activate(
        state: gpui::Entity<Self>,
        target: &'static str,
    ) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) {
        move |_, window, cx| state.update(cx, |state, cx| state.select(target, window, cx))
    }
}

#[derive(gpui::IntoElement)]
struct SceneRadioScene<F: FnOnce(gpui::Entity<SceneRadioState>) -> SceneTabNavigation + 'static> {
    options: &'static [SceneRadioOption],
    render: F,
}

impl<F: FnOnce(gpui::Entity<SceneRadioState>) -> SceneTabNavigation + 'static> RenderOnce
    for SceneRadioScene<F>
{
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let state = window.use_state(cx, |_, cx| SceneRadioState::new(self.options, cx));
        (self.render)(state)
    }
}

#[derive(gpui::IntoElement)]
struct SceneRadio {
    element: gpui::Stateful<gpui::Div>,
    state: gpui::Entity<SceneRadioState>,
    id: &'static str,
    checked_paint: fn(gpui::StyleRefinement) -> gpui::StyleRefinement,
}

impl RenderOnce for SceneRadio {
    fn render(self, _window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let mut element = self.element;
        let state = self.state.read(cx);
        let name = state.target(self.id).name;
        let selection = state.selected.get(name).copied();
        let tab_stop = match selection {
            Some(id) => id == self.id,
            None => state
                .options
                .iter()
                .find(|option| option.name == name)
                .is_some_and(|option| option.id == self.id),
        };
        if selection == Some(self.id) {
            let style = element.style();
            *style = (self.checked_paint)(std::mem::take(style));
        }
        let id = self.id;
        let focus = state.focus[id].clone().tab_stop(tab_stop);
        element
            .track_focus(&focus)
            .on_click(SceneRadioState::activate(self.state.clone(), id))
            .on_key_down(move |event, window, cx| {
                let key = &event.keystroke;
                let modifiers = key.modifiers;
                if modifiers.alt || modifiers.control || modifiers.platform || modifiers.function {
                    return;
                }
                let state = self.state.read(cx);
                let target = match key.key.as_str() {
                    "space" => id,
                    "right" | "down" => state.adjacent(id, true),
                    "left" | "up" => state.adjacent(id, false),
                    _ => return,
                };
                self.state
                    .update(cx, |state, cx| state.select(target, window, cx));
                cx.stop_propagation();
            })
    }
}
