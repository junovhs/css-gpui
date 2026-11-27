
struct SceneStatePaint {
    hover: fn(gpui::StyleRefinement) -> gpui::StyleRefinement,
    focus: fn(gpui::StyleRefinement) -> gpui::StyleRefinement,
    hover_focus: fn(gpui::StyleRefinement) -> gpui::StyleRefinement,
}

impl SceneStatePaint {
    fn apply(
        self,
        mut element: gpui::Stateful<gpui::Div>,
        id: &'static str,
        focused: bool,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> gpui::Stateful<gpui::Div> {
        // Text runs inherit their color during layout. Select the whole
        // CSS paint before layout, including the pre-cascaded joint state.
        let hovered = window.use_keyed_state((id, 0usize), cx, |_, _| false);
        let paint = match (*hovered.read(cx), focused) {
            (false, false) => None,
            (true, false) => Some(self.hover),
            (false, true) => Some(self.focus),
            (true, true) => Some(self.hover_focus),
        };
        if let Some(paint) = paint {
            let style = element.style();
            *style = paint(std::mem::take(style));
        }
        element.on_hover(move |hover, window, cx| {
            hovered.update(cx, |hovered, _| *hovered = *hover);
            window.refresh();
        })
    }
}
