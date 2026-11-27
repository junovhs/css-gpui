use std::{env, fs, path::PathBuf};

use css_gpui::{
    HtmlNode, SceneSourceGraph, diagnostics, extractor,
    ir::{Color, Position, SceneIr},
    layout, resolver,
    target::gpui::GpuiTarget,
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
    let live = env::var_os("CARGO_FEATURE_INTERACTIVE").is_some();
    if live {
        for file in ["abstract-light.jpg", "noise-base.png"] {
            let source = reference.join(file);
            println!("cargo:rerun-if-changed={}", source.display());
            fs::copy(source, output.join(file))
                .expect("interactive build input is missing; run capture.mjs --effects");
        }
        embed_fonts(&reference, &output);
    }
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
        if live {
            build_live(&output, width, height, &scene, &graph, case, menu);
        }
    }
}

fn attr<'a>(node: &'a HtmlNode, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn build_live(
    output: &std::path::Path,
    width: u64,
    height: u64,
    scene: &SceneIr,
    graph: &SceneSourceGraph,
    case: &Value,
    menu: &HtmlNode,
) {
    let node = scene
        .nodes
        .iter()
        .find(|node| node.source.dom_path == menu.dom_path)
        .unwrap();
    let inside = |path: &str| {
        path == menu.dom_path
            || path
                .strip_prefix(&menu.dom_path)
                .is_some_and(|part| part.starts_with('/'))
    };
    let mut stage = scene.clone();
    stage.nodes.retain(|node| !inside(&node.source.dom_path));
    let app = graph
        .html_nodes
        .iter()
        .find(|node| attr(node, "id") == Some("app"))
        .unwrap();
    let app_layout = &mut stage
        .nodes
        .iter_mut()
        .find(|node| node.source.dom_path == app.dom_path)
        .unwrap()
        .layout;
    // The standalone stage is compiled for this viewport. GPUI's grid adds
    // padding to stretched auto rows when only a minimum height is supplied.
    app_layout.height = app_layout.height.or(app_layout.min_height);
    // GPUI 0.2.2's DirectWrite lookup asks for normal stretch. The stage's
    // condensed face has a condensed width class and cannot satisfy that lookup.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for node in &mut stage.nodes {
            if let Some(typography) = &mut node.typography {
                for family in &mut typography.font_family {
                    if family == "Asap Condensed" {
                        "Asap".clone_into(family);
                    }
                }
            }
        }
    }
    let mut overlay = scene.clone();
    overlay.nodes.retain(|node| inside(&node.source.dom_path));
    let panel = overlay.nodes.first_mut().unwrap();
    panel.layout.position = Position::Static;
    panel.layout.top = None;
    panel.layout.left = None;
    if let Some(border) = &mut panel.paint.border {
        border.color = Color("#0000".to_owned());
    }

    let mut hover = Vec::new();
    let mut hover_colors = Vec::new();
    let rows: Vec<_> = graph
        .html_nodes
        .iter()
        .filter(|node| node.name.as_deref() == Some("li") && inside(&node.dom_path))
        .collect();
    assert_eq!(rows.len(), case["rows"].as_array().unwrap().len());
    let initial_selection = rows.iter().position(|row| {
        attr(row, "class").is_some_and(|class| {
            class
                .split_ascii_whitespace()
                .any(|class| class == "selected")
        })
    });
    for row in rows {
        let mut hovered = graph.clone();
        for rule in &mut hovered.css_rules {
            for selector in &mut rule.selectors {
                *selector = selector.replace(":hover", ".neon-native-hover");
            }
        }
        let attributes = &mut hovered.html_nodes[row.id].attributes;
        if let Some((_, classes)) = attributes.iter_mut().find(|(name, _)| name == "class") {
            classes.push_str(" neon-native-hover");
        } else {
            attributes.push(("class".to_owned(), "neon-native-hover".to_owned()));
        }
        let resolved =
            resolver::resolve_scene_at_viewport(&hovered, width as f32, height as f32).unwrap();
        let hovered = extractor::extract_ir(&layout::compute_layout(&resolved), &hovered).unwrap();
        let hovered = hovered
            .nodes
            .iter()
            .find(|node| node.source.dom_path == row.dom_path)
            .unwrap();
        let paint = hovered.paint.clone();
        let id = scene
            .nodes
            .iter()
            .find(|node| node.source.dom_path == row.dom_path)
            .unwrap()
            .id
            .clone();
        assert!(
            paint.effects.contains_key("background"),
            "source hover must supply a row fill"
        );
        let color = hovered.typography.as_ref().unwrap().color.clone();
        hover_colors.push((id.clone(), color));
        hover.push((id, paint));
    }

    let target = GpuiTarget;
    let footer = graph
        .html_nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("footer"))
        .unwrap();
    let footer_id = &stage
        .nodes
        .iter()
        .find(|node| node.source.dom_path == footer.dom_path)
        .unwrap()
        .id;
    let main_styles = graph
        .css_rules
        .iter()
        .flat_map(|rule| rule.declarations.iter().map(move |decl| (rule, decl)));
    let property = |name| {
        main_styles
            .clone()
            .find(|(rule, declaration)| rule.selectors == ["#menu"] && declaration.property == name)
            .map(|(_, value)| value.value.as_str())
            .unwrap()
    };
    let properties: Vec<_> = property("transition-property")
        .split(',')
        .map(str::trim)
        .collect();
    let ms = |name, index| {
        let value = property(name).split(',').nth(index).unwrap().trim();
        if let Some(value) = value.strip_suffix("ms") {
            value.parse::<f64>().unwrap()
        } else {
            value.strip_suffix('s').unwrap().parse::<f64>().unwrap() * 1000.0
        }
    };
    let fade = ms(
        "transition-duration",
        properties
            .iter()
            .position(|property| *property == "opacity")
            .unwrap(),
    );
    let hide = ms(
        "transition-delay",
        properties
            .iter()
            .position(|property| *property == "visibility")
            .unwrap(),
    );
    let ease = graph
        .css_rules
        .iter()
        .flat_map(|rule| &rule.declarations)
        .find(|decl| decl.property == "--ease")
        .unwrap()
        .value
        .strip_prefix("cubic-bezier(")
        .unwrap()
        .strip_suffix(')')
        .unwrap()
        .split(',')
        .map(|part| part.trim().parse::<f32>().unwrap())
        .collect::<Vec<_>>();
    let metadata = serde_json::json!({
        "geometry": case,
        "menuId": node.id,
        "footerId": footer_id,
        "hover": hover,
        "initialSelection": initial_selection,
        "fadeMs": fade,
        "hideMs": hide,
        "ease": ease,
    });
    fs::write(
        output.join(format!("live-{width}.json")),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join(format!("stage-{width}.rs")),
        target.emit_with_debug_bounds(&stage).artifact,
    )
    .unwrap();
    fs::write(
        output.join(format!("overlay-{width}.rs")),
        target
            .emit_with_hover_colors(&overlay, &hover_colors)
            .artifact,
    )
    .unwrap();
}

fn embed_fonts(reference: &std::path::Path, output: &std::path::Path) {
    let source = reference.join("fonts");
    let destination = output.join("fonts");
    fs::create_dir_all(&destination).unwrap();
    println!("cargo:rerun-if-changed={}", source.display());
    let mut names: Vec<_> = fs::read_dir(source)
        .expect("missing embedded fonts; run capture.mjs --native")
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".ttf"))
        .collect();
    names.sort();
    assert!(names.len() >= 6, "expected the pinned subset fonts");
    let mut rust = String::from("const FONTS: &[&[u8]] = &[\n");
    for name in names {
        let file = reference.join("fonts").join(&name);
        println!("cargo:rerun-if-changed={}", file.display());
        fs::copy(file, destination.join(&name)).unwrap();
        rust.push_str(&format!(
            "    include_bytes!({:?}),\n",
            format!("fonts/{name}")
        ));
    }
    rust.push_str("];\n");
    fs::write(output.join("fonts.rs"), rust).unwrap();
}
