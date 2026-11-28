use std::path::PathBuf;

use crate::load_scene_source_graph;
use crate::resolver::resolve_scene;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The profile_card_absolute fixture is the primary anchor scene and the most
/// demanding for the resolver: absolute positioning, compound selectors,
/// cascade between shared and specific rules, and font-family inheritance.
#[test]
fn profile_card_absolute_resolves_without_error() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let scene = resolve_scene(&graph)?;
    assert_eq!(scene.scene_id, "profile_card_absolute");
    assert!(
        !scene.nodes.is_empty(),
        "resolved scene must have element nodes"
    );
    Ok(())
}

#[test]
fn profile_card_card_has_absolute_position() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let scene = resolve_scene(&graph)?;

    let Some(card) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v.contains("profile-card"))
    }) else {
        return Err("profile-card element not found in resolved scene".into());
    };

    assert_eq!(card.style.position, "absolute");
    assert_eq!(card.style.background_color, Some("#ffffff".to_owned()));
    assert_eq!(card.style.border_width, 1.0);
    assert_eq!(card.style.border_color, Some("#e5e7eb".to_owned()));
    assert_eq!(card.style.width, Some(320.0));
    assert_eq!(card.style.height, Some(180.0));
    Ok(())
}

#[test]
fn cascade_specificity_button_primary_wins_over_button() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let scene = resolve_scene(&graph)?;

    // .button.primary has specificity (2,0); it overrides .button specificity (1,0)
    let Some(primary) = scene.nodes.iter().find(|n| {
        n.node.name.as_deref() == Some("button")
            && n.node
                .attributes
                .iter()
                .any(|(k, v)| k == "class" && v.contains("primary"))
    }) else {
        return Err("primary button not found".into());
    };
    assert_eq!(primary.style.background_color, Some("#111827".to_owned()));
    assert_eq!(primary.style.color, Some("#ffffff".to_owned()));

    let Some(secondary) = scene.nodes.iter().find(|n| {
        n.node.name.as_deref() == Some("button")
            && n.node
                .attributes
                .iter()
                .any(|(k, v)| k == "class" && v.contains("secondary"))
    }) else {
        return Err("secondary button not found".into());
    };
    assert_eq!(secondary.style.background_color, Some("#ffffff".to_owned()));
    assert_eq!(secondary.style.color, Some("#111827".to_owned()));
    Ok(())
}

#[test]
fn font_family_inherits_from_body_to_children() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let scene = resolve_scene(&graph)?;

    // body sets font-family: Inter, sans-serif
    // .button sets font-family: inherit → inherits from ancestor body
    let Some(primary) = scene.nodes.iter().find(|n| {
        n.node.name.as_deref() == Some("button")
            && n.node
                .attributes
                .iter()
                .any(|(k, v)| k == "class" && v.contains("primary"))
    }) else {
        return Err("primary button not found".into());
    };
    assert_eq!(
        primary.style.font_family.as_deref(),
        Some("Inter, sans-serif"),
        "font-family must propagate from body through inheritance"
    );
    Ok(())
}

#[test]
fn stacked_info_card_flex_properties_resolved() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "stacked_info_card")?;
    let scene = resolve_scene(&graph)?;

    let Some(card) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v == "card")
    }) else {
        return Err("card element not found".into());
    };
    assert_eq!(card.style.display, "flex");
    assert_eq!(card.style.flex_direction, Some("column".to_owned()));
    assert_eq!(card.style.gap, Some(12.0));
    assert_eq!(card.style.width, Some(320.0));
    Ok(())
}

/// Negative: unknown scene fails at the graph load level, not the resolver.
#[test]
fn unknown_scene_fails_before_resolve() {
    let result = load_scene_source_graph(repo_root(), "does_not_exist");
    assert!(result.is_err(), "unknown scene must return an error");
}

#[test]
fn radio_snapshot_cascades_anchored_selectors_across_elements() {
    let html = r#"
        <html><body>
          <section id="primary" class="group">
            <div><input type="radio" id="north" name="heading" checked></div>
            <input type="radio" id="east" name="heading" checked>
            text
            <p id="adjacent" class="readout">Adjacent</p>
            <p id="later" class="readout">Later</p>
          </section>
          <section id="secondary" class="group">
            <input type="radio" id="open" name="door" checked>
            <p id="independent" class="readout">Independent</p>
          </section>
        </body></html>
    "#;
    let css = r#"
        .group { --ink: #111111; }
        .group:has(> .missing:checked, > #east:checked):has(+ #secondary > #open:checked) {
            --ink: #222222;
        }
        #primary { --ink: #333333; }
        .group:has(> #north:checked) { --ink: #444444; }
        .group:has(#north:checked) .readout { border-color: #555555; }
        .readout { color: var(--ink); }
        #east:checked + p { background: #666666; }
        #east:checked ~ p { width: 30px; }
        #east:checked ~ p::before { content: "active"; }
        #east:checked { background: #777777; }
    "#;
    let graph = crate::SceneSourceGraph::from_strings("radio-snapshot", html, css).unwrap();
    let by_dom_id = |node: &crate::HtmlNode, id: &str| {
        node.attributes
            .iter()
            .any(|(key, value)| key == "id" && value == id)
    };
    let assert_state = |scene: &crate::resolver::ResolvedScene, east_selected: bool| {
        let style = |id| {
            &scene
                .nodes
                .iter()
                .find(|node| by_dom_id(&node.node, id))
                .unwrap()
                .style
        };
        assert_eq!(
            style("adjacent").color.as_deref(),
            Some(if east_selected { "#222222" } else { "#333333" })
        );
        assert_eq!(style("independent").color.as_deref(), Some("#111111"));
        assert_eq!(
            style("adjacent").background_color.as_deref(),
            east_selected.then_some("#666666")
        );
        assert_eq!(style("later").background_color, None);
        assert_eq!(style("later").width, east_selected.then_some(30.0));
        assert_eq!(!style("later").decorations.is_empty(), east_selected);
        assert_eq!(
            style("later").border_color.as_deref(),
            (!east_selected).then_some("#555555")
        );
        assert_eq!(
            style("east").background_color.as_deref(),
            east_selected.then_some("#777777")
        );
    };

    use crate::resolver::{ResolverError, resolve_scene_with_radio_selection};

    let defaults = resolve_scene_with_radio_selection(&graph, 800.0, 600.0, &[]).unwrap();
    assert_state(&defaults, true);

    let north = graph
        .html_nodes
        .iter()
        .find(|node| by_dom_id(node, "north"))
        .unwrap()
        .id;
    let selected = resolve_scene_with_radio_selection(&graph, 800.0, 600.0, &[north]).unwrap();
    assert_state(&selected, false);

    let invalid = resolve_scene_with_radio_selection(&graph, 800.0, 600.0, &[usize::MAX]);
    assert!(matches!(invalid, Err(ResolverError::UnsupportedRadio(id)) if id == usize::MAX));

    let ordinary = resolve_scene(&graph).unwrap();
    let east = ordinary
        .nodes
        .iter()
        .find(|node| by_dom_id(&node.node, "east"))
        .unwrap();
    assert_eq!(east.style.background_color, None);
    assert_eq!(
        east.style
            .checked_paint
            .background_color
            .as_ref()
            .map(|color| color.0.as_str()),
        Some("#777777")
    );

    let losses = crate::diagnostics::analyze(&graph);
    assert!(losses.iter().any(|loss| {
        loss.kind == crate::diagnostics::DiagnosticKind::UnsupportedSelector
            && loss.selector.as_deref() == Some("#east:checked + p")
    }));
}
