#[cfg(test)]
mod geometry {
    use gpui::prelude::*;
    use gpui::{App, AssetSource, Context, Render, TestAppContext, Window, px, size};

    struct View {
        width: u16,
    }

    mod wide {
        include!(concat!(env!("OUT_DIR"), "/viewport-1100.rs"));
        pub(super) const CASE: super::Case = (VIEWPORT, REFERENCE);
    }

    mod narrow {
        include!(concat!(env!("OUT_DIR"), "/viewport-390.rs"));
        pub(super) const CASE: super::Case = (VIEWPORT, REFERENCE);
    }

    type Case = (
        (f32, f32),
        &'static [(&'static str, &'static str, [f32; 4])],
    );

    impl Render for View {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let action = |_: &str, _: &mut Window, _: &mut App| {};
            if self.width == 390 {
                narrow::render_scene(action).into_any_element()
            } else {
                wide::render_scene(action).into_any_element()
            }
        }
    }

    #[gpui::test]
    fn original_menu_matches_chromium_at_both_widths(cx: &mut TestAppContext) {
        for assets in [
            wide::scene_assets().list("css-gpui/"),
            narrow::scene_assets().list("css-gpui/"),
        ] {
            assert_eq!(assets.unwrap().len(), 8);
        }
        let mut differences = Vec::new();
        for ((width, height), reference) in [wide::CASE, narrow::CASE] {
            let (_, cx) = cx.add_window_view(|_, _| View {
                width: width as u16,
            });
            cx.simulate_resize(size(px(width), px(height)));
            cx.update(|window, app| window.draw(app).clear());
            for (label, node_id, expected) in reference {
                let actual = cx
                    .debug_bounds(node_id)
                    .unwrap_or_else(|| panic!("missing bounds for {label} ({node_id})"));
                let actual: [f32; 4] = [
                    actual.origin.x.into(),
                    actual.origin.y.into(),
                    actual.size.width.into(),
                    actual.size.height.into(),
                ];
                if actual
                    .iter()
                    .zip(expected)
                    .any(|(actual, expected)| (actual - expected).abs() > 1.0)
                {
                    differences.push(format!(
                        "{width}px {label}: GPUI {actual:?}, Chromium {expected:?}"
                    ));
                }
            }
        }
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }
}
