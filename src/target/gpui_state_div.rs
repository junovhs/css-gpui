
#[derive(gpui::IntoElement)]
struct SceneStateDiv {
    element: gpui::Stateful<gpui::Div>,
    id: &'static str,
    focusable: bool,
    state_paint: Option<SceneStatePaint>,
}

impl RenderOnce for SceneStateDiv {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let mut element = self.element;
        let mut focused = false;
        if self.focusable {
            let focus =
                window.use_keyed_state(self.id, cx, |_, cx| cx.focus_handle().tab_stop(true));
            let focus = focus.read(cx);
            focused = focus.is_focused(window);
            element = element.track_focus(focus);
        }
        element.when_some(self.state_paint, |element, paint| {
            paint.apply(element, self.id, focused, window, cx)
        })
    }
}
