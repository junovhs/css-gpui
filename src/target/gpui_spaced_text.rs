// GPUI 0.2.2 exposes shaped glyphs but has no letter-spacing style. Adjust the
// shaped line before both layout and paint so siblings use the tracked advance.
struct SceneSpacedText {
    text: &'static str,
    spacing: f32,
}

impl IntoElement for SceneSpacedText {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for SceneSpacedText {
    type RequestLayoutState = (gpui::ShapedLine, gpui::Pixels);
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        _: &mut gpui::App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let mut run = style.to_run(self.text.len());
        // CSS nonzero tracking suppresses optional ligatures. Keep kerning.
        let features = std::sync::Arc::make_mut(&mut run.font.features.0);
        for tag in ["liga", "clig", "dlig", "hlig", "calt"] {
            features.retain(|(feature, _)| feature != tag);
            features.push((tag.into(), 0));
        }
        let mut line = window
            .text_system()
            .shape_line(self.text.into(), font_size, &[run], None);
        let spacing = gpui::px(self.spacing);
        let mut runs = line.runs.clone();
        for glyph in runs.iter_mut().flat_map(|run| &mut run.glyphs) {
            glyph.position.x += spacing * glyph.index as f32;
        }
        // Chromium includes tracking after the last character in the measured
        // advance. The last glyph's position includes only the preceding gaps.
        let width = (line.width + spacing * self.text.len() as f32).max(gpui::px(0.0));
        *line = std::sync::Arc::new(gpui::LineLayout {
            font_size,
            width,
            ascent: line.ascent,
            descent: line.descent,
            runs,
            len: line.len(),
        });
        let layout = window.request_measured_layout(Default::default(), move |_, _, _, _| {
            gpui::size(width.ceil(), line_height)
        });
        (layout, (line, line_height))
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut gpui::Window,
        _: &mut gpui::App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        (line, line_height): &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let spare = bounds.size.width - line.width;
        let offset = match window.text_style().text_align {
            gpui::TextAlign::Left => gpui::px(0.0),
            gpui::TextAlign::Center => spare / 2.0,
            gpui::TextAlign::Right => spare,
        };
        let origin = gpui::point(bounds.origin.x + offset, bounds.origin.y);
        let _ = line.paint(origin, *line_height, window, cx);
    }
}
