use std::{env, fs, path::PathBuf};

use css_gpui::{
    HtmlNode, SceneSourceGraph, diagnostics, extractor, layout, resolver, target::gpui::GpuiTarget,
};
use serde_json::Value;

const PEN: &str = include_str!("../source.html");

fn main() {
    println!("cargo:rerun-if-env-changed=NEON_GLASS_REFERENCE");
    println!("cargo:rerun-if-changed=../source.html");
    let reference = PathBuf::from(
        env::var_os("NEON_GLASS_REFERENCE")
            .expect("set NEON_GLASS_REFERENCE to the capture.mjs output directory"),
    );
    let css_path = reference.join("linked.css");
    let geometry_path = reference.join("geometry.json");
    for path in [&css_path, &geometry_path] {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let linked = fs::read_to_string(css_path).expect("read browser's linked.css");
    let geometry: Value =
        serde_json::from_str(&fs::read_to_string(geometry_path).unwrap()).unwrap();
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let cases = geometry.as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let width = case["width"].as_u64().unwrap();
        let height = case["height"].as_u64().unwrap();
        assert!(matches!(width, 390 | 1100));
        let compile = || {
            let graph = SceneSourceGraph::from_strings_at_viewport(
                "neon-glass",
                PEN,
                &linked,
                width as f32,
            )
            .unwrap();
            let losses = diagnostics::analyze(&graph);
            assert!(
                losses
                    .iter()
                    .any(|loss| loss.property.as_deref() == Some("backdrop-filter")
                        && graph.css_rules[loss.rule_id].document_id == graph.html.id)
            );
            let resolved =
                resolver::resolve_scene_at_viewport(&graph, width as f32, height as f32).unwrap();
            let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph).unwrap();
            (scene, graph)
        };

        let (scene, graph) = compile();
        fs::write(
            output.join(format!("scene-{width}.json")),
            scene.to_json().unwrap(),
        )
        .unwrap();
        let target = GpuiTarget;
        let mut rust = target.emit_with_debug_bounds(&scene).artifact;
        assert_eq!(rust, target.emit_with_debug_bounds(&compile().0).artifact);
        assert_eq!(
            scene
                .nodes
                .iter()
                .filter(|node| matches!(node.kind, css_gpui::ir::NodeKind::Svg(_)))
                .count(),
            8
        );
        let menu = graph
            .html_nodes
            .iter()
            .find(|node| attr(node, "id") == Some("menu"))
            .unwrap();
        let descendants: Vec<_> = graph
            .html_nodes
            .iter()
            .filter(|node| {
                node.dom_path
                    .strip_prefix(&menu.dom_path)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            })
            .collect();
        let by_class = |name| {
            descendants
                .iter()
                .copied()
                .find(|node| {
                    attr(node, "class").is_some_and(|classes| {
                        classes.split_ascii_whitespace().any(|class| class == name)
                    })
                })
                .unwrap()
        };
        let by_tag = |tag| {
            descendants
                .iter()
                .copied()
                .filter(move |node| node.name.as_deref() == Some(tag))
        };
        let boxes = [
            ("menu", menu),
            ("content", by_class("inner")),
            ("search", by_class("search")),
            ("searchInput", by_tag("input").next().unwrap()),
            ("firstRow", by_tag("li").next().unwrap()),
            ("lastRow", by_tag("li").last().unwrap()),
            ("separator", by_tag("hr").next().unwrap()),
        ];
        rust.push_str(&format!(
            "
const VIEWPORT: (f32, f32) = ({width}.0, {height}.0);
"
        ));
        rust.push_str(
            "const REFERENCE: &[(&str, &str, [f32; 4])] = &[
",
        );
        for (name, node) in boxes {
            let id = &scene
                .nodes
                .iter()
                .find(|ir| ir.source.dom_path == node.dom_path)
                .unwrap()
                .id;
            let bounds: [f32; 4] = serde_json::from_value(case["boxes"][name].clone()).unwrap();
            rust.push_str(&format!(
                "    ({name:?}, {id:?}, {bounds:?}),
"
            ));
        }
        rust.push_str(
            "];
",
        );
        fs::write(output.join(format!("viewport-{width}.rs")), rust).unwrap();
    }
}

fn attr<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}
