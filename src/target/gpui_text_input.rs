use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Div, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Keystroke, MouseButton, Pixels, Point, ShapedLine,
    SharedString, TextAlign, TextRun, UTF16Selection, Window, canvas, fill, point, px, size,
};
use std::ops::Range;

#[derive(gpui::IntoElement)]
struct SceneTabNavigation(Div);

impl RenderOnce for SceneTabNavigation {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // With no focused element, GPUI dispatches keys to the host view. Observe unhandled
        // Tab for this window so an unmodified host can enter the scene's tab order.
        let window_id = window.window_handle().window_id();
        window.use_state(cx, |_, cx| {
            App::observe_keystrokes(cx, move |event, window, cx| {
                if window.window_handle().window_id() == window_id
                    && window.focused(cx).is_none()
                    && event.action.is_none()
                {
                    move_scene_focus(&event.keystroke, window);
                }
            })
        });
        self.0.on_key_down(|event, window, cx| {
            if move_scene_focus(&event.keystroke, window) {
                cx.stop_propagation();
            }
        })
    }
}

fn move_scene_focus(key: &Keystroke, window: &mut Window) -> bool {
    let modifiers = key.modifiers;
    if key.key != "tab"
        || modifiers.alt
        || modifiers.control
        || modifiers.platform
        || modifiers.function
    {
        return false;
    }
    if modifiers.shift {
        window.focus_prev();
    } else {
        window.focus_next();
    }
    true
}

#[derive(gpui::IntoElement)]
struct SceneTextInput {
    element: Div,
    id: &'static str,
    value: &'static str,
    placeholder: &'static str,
    padding: [f32; 4],
}

impl RenderOnce for SceneTextInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let input = window.use_keyed_state(self.id, cx, |_, cx| TextInput {
            focus: cx.focus_handle().tab_stop(true),
            text: self.value.to_owned(),
            anchor: self.value.len(),
            cursor: self.value.len(),
            marked: None,
            line: None,
        });
        let focus = input.read(cx).focus.clone();
        let [left, right, top, bottom] = self.padding;
        let key_input = input.clone();
        let mouse_input = input.clone();
        self.element
            .id(self.id)
            .track_focus(&focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(move |event, window, cx| {
                key_input.update(cx, |input, cx| input.key_down(&event.keystroke, window, cx));
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                mouse_input.update(cx, |input, cx| {
                    input.focus.focus(window);
                    let index = input.index_at(event.position);
                    input.move_cursor(index, event.modifiers.shift);
                    cx.notify();
                });
            })
            .child(
                input_text(input, self.placeholder)
                    .absolute()
                    .left(px(left))
                    .right(px(right))
                    .top(px(top))
                    .bottom(px(bottom)),
            )
    }
}

struct TextInput {
    focus: FocusHandle,
    text: String,
    anchor: usize,
    cursor: usize,
    marked: Option<Range<usize>>,
    line: Option<(ShapedLine, Point<Pixels>)>,
}

impl TextInput {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    fn move_cursor(&mut self, index: usize, extend: bool) {
        self.cursor = index;
        if !extend {
            self.anchor = index;
        }
    }

    fn index_at(&self, point: Point<Pixels>) -> usize {
        self.line.as_ref().map_or(self.cursor, |(line, origin)| {
            line.closest_index_for_x(point.x - origin.x)
                .min(self.text.len())
        })
    }

    fn key_down(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = key.modifiers;
        let selection = self.selection();
        if modifiers.alt || modifiers.function {
            return;
        }
        if modifiers.control || modifiers.platform {
            if modifiers.shift {
                return;
            }
            match key.key.as_str() {
                "a" => {
                    self.anchor = 0;
                    self.cursor = self.text.len();
                }
                "c" | "x" if !selection.is_empty() => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.text[selection].to_owned(),
                    ));
                    if key.key == "x" {
                        self.replace_text_in_range(None, "", window, cx);
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace_text_in_range(None, &text, window, cx);
                    }
                }
                _ => return,
            }
        } else {
            let previous = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(index, _)| index);
            let next = self.cursor
                + self.text[self.cursor..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
            match key.key.as_str() {
                "left" if !modifiers.shift && !selection.is_empty() => {
                    self.move_cursor(selection.start, false)
                }
                "right" if !modifiers.shift && !selection.is_empty() => {
                    self.move_cursor(selection.end, false)
                }
                "left" => self.move_cursor(previous, modifiers.shift),
                "right" => self.move_cursor(next, modifiers.shift),
                "home" => self.move_cursor(0, modifiers.shift),
                "end" => self.move_cursor(self.text.len(), modifiers.shift),
                "backspace" | "delete" => {
                    if selection.is_empty() {
                        self.anchor = if key.key == "backspace" {
                            previous
                        } else {
                            next
                        };
                    }
                    self.replace_text_in_range(None, "", window, cx);
                }
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn replacement_range(&self, utf16: Option<Range<usize>>) -> Range<usize> {
        utf16
            .map(|range| self.from_utf16(range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection())
    }

    fn from_utf16(&self, range: Range<usize>) -> Range<usize> {
        utf8_offset(&self.text, range.start)..utf8_offset(&self.text, range.end)
    }

    fn to_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }
}

fn utf8_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, character) in text.char_indices() {
        if units >= utf16 {
            return index;
        }
        units += character.len_utf16();
    }
    text.len()
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.from_utf16(range);
        *actual = Some(self.to_utf16(range.clone()));
        Some(self.text[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.to_utf16(self.selection()),
            reversed: self.cursor < self.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.clone().map(|range| self.to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range);
        let text = text.replace(['\r', '\n'], "");
        let end = range.start + text.len();
        self.text.replace_range(range, &text);
        self.line = None;
        self.move_cursor(end, false);
        self.marked = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = self.replacement_range(range.clone()).start;
        self.replace_text_in_range(range, text, window, cx);
        let end = self.cursor;
        self.marked = (start < end).then_some(start..end);
        if let Some(selected) = selected {
            let marked = &self.text[start..end];
            self.anchor = start + utf8_offset(marked, selected.start);
            self.cursor = start + utf8_offset(marked, selected.end);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (line, origin) = self.line.as_ref()?;
        let range = self.from_utf16(range);
        let start = origin.x + line.x_for_index(range.start);
        let end = origin.x + line.x_for_index(range.end);
        Some(Bounds::from_corners(
            point(start.min(end), bounds.top()),
            point(start.max(end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.index_at(point);
        Some(self.text[..index].encode_utf16().count())
    }
}

fn input_text(input: Entity<TextInput>, placeholder: &'static str) -> impl Styled + IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let state = input.read(cx);
            let style = window.text_style();
            let color = style.color;
            let (text, text_color) = if state.text.is_empty() {
                (SharedString::from(placeholder), color.opacity(0.55))
            } else {
                (SharedString::from(state.text.clone()), color)
            };
            let run = TextRun {
                len: text.len(),
                font: style.font(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(
                text,
                style.font_size.to_pixels(window.rem_size()),
                &[run],
                None,
            );
            let line_height = window.line_height().min(bounds.size.height);
            let top = bounds.top() + (bounds.size.height - line_height) / 2.0;
            let cursor = line.x_for_index(state.cursor);
            let scroll = (cursor - bounds.size.width + px(1.0)).max(px(0.0));
            let spare_width = (bounds.size.width - line.width).max(px(0.0));
            let alignment_offset = match style.text_align {
                TextAlign::Left => px(0.0),
                TextAlign::Center => spare_width / 2.0,
                TextAlign::Right => spare_width,
            };
            let origin = point(bounds.left() + alignment_offset - scroll, top);
            let focused = state.focus.is_focused(window);
            let selection = state.selection();
            let marked = state.marked.clone();

            window.handle_input(
                &state.focus,
                ElementInputHandler::new(bounds, input.clone()),
                cx,
            );
            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                if focused && !selection.is_empty() {
                    let start = origin.x + line.x_for_index(selection.start);
                    let end = origin.x + line.x_for_index(selection.end);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(start.min(end), top),
                            size((end - start).abs(), line_height),
                        ),
                        gpui::rgba(0x3875d766),
                    ));
                }
                let _ = line.paint(origin, line_height, window, cx);
                if focused && selection.is_empty() {
                    let cursor_x = (origin.x + cursor).min(bounds.right() - px(1.0));
                    window.paint_quad(fill(
                        Bounds::new(point(cursor_x, top), size(px(1.0), line_height)),
                        color,
                    ));
                }
                if focused && let Some(marked) = marked {
                    let start = origin.x + line.x_for_index(marked.start);
                    let end = origin.x + line.x_for_index(marked.end);
                    window.paint_quad(fill(
                        Bounds::new(
                            point(start.min(end), top + line_height - px(1.0)),
                            size((end - start).abs(), px(1.0)),
                        ),
                        color,
                    ));
                }
            });
            input.update(cx, |input, _| input.line = Some((line, origin)));
        },
    )
}
