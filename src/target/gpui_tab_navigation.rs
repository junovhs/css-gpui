
#[derive(gpui::IntoElement)]
struct SceneTabNavigation(gpui::Div);

impl RenderOnce for SceneTabNavigation {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        // With no focused element, GPUI dispatches keys to the host view. Observe unhandled
        // Tab for this window so an unmodified host can enter the scene's tab order.
        let window_id = window.window_handle().window_id();
        window.use_state(cx, |_, cx| {
            gpui::App::observe_keystrokes(cx, move |event, window, cx| {
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

fn move_scene_focus(key: &gpui::Keystroke, window: &mut gpui::Window) -> bool {
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
