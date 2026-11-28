use std::path::PathBuf;

use crate::layout::{BorderBox, UsedBoxLayout, compute_layout};
use crate::resolver::{resolve_scene, resolve_scene_with_radio_selection};
use crate::{SceneSourceGraph, load_scene_source_graph};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// profile_card_absolute is the primary geometry fixture: every element has
/// explicit top/left/width/height from CSS. Geometry extraction must be lossless.
#[test]
fn profile_card_absolute_geometry_extracted() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let scene = compute_layout(&resolved);

    assert_eq!(scene.scene_id, "profile_card_absolute");
    assert!(!scene.nodes.is_empty());
    Ok(())
}

#[test]
fn profile_card_has_correct_explicit_coords_and_content_box()
-> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let scene = compute_layout(&resolved);

    let Some(card) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v.contains("profile-card"))
    }) else {
        return Err("profile-card not found".into());
    };

    let g = &card.geometry;
    // .profile-card: position:absolute left:24 top:20 width:320 height:180 border:1px box-sizing:border-box
    assert_eq!(g.explicit_x, Some(24.0));
    assert_eq!(g.explicit_y, Some(20.0));
    assert_eq!(g.width, Some(320.0));
    assert_eq!(g.height, Some(180.0));
    // content-box: 320 - 0padding - 2border = 318; 180 - 0padding - 2border = 178
    assert_eq!(g.content_width, Some(318.0));
    assert_eq!(g.content_height, Some(178.0));
    Ok(())
}

#[test]
fn avatar_circle_has_border_box_content_size() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let scene = compute_layout(&resolved);

    let Some(avatar) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v.contains("avatar"))
    }) else {
        return Err("avatar not found".into());
    };

    let g = &avatar.geometry;
    // .avatar: position:absolute left:20 top:20 width:48 height:48 no-border box-sizing:border-box
    assert_eq!(g.explicit_x, Some(20.0));
    assert_eq!(g.explicit_y, Some(20.0));
    assert_eq!(g.width, Some(48.0));
    // no padding, no border → content size = declared size
    assert_eq!(g.content_width, Some(48.0));
    assert_eq!(g.content_height, Some(48.0));
    Ok(())
}

#[test]
fn stacked_card_has_explicit_container_width() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "stacked_info_card")?;
    let resolved = resolve_scene(&graph)?;
    let scene = compute_layout(&resolved);

    let Some(card) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v == "card")
    }) else {
        return Err("card not found".into());
    };

    let g = &card.geometry;
    // .card: display:flex width:320 padding:20 border:1 box-sizing:border-box
    assert_eq!(g.width, Some(320.0));
    // content_width = 320 - (20+20) - (1+1) = 278
    assert_eq!(g.content_width, Some(278.0));
    // Not absolutely positioned → no explicit coords
    assert_eq!(g.explicit_x, None);
    assert_eq!(g.explicit_y, None);
    Ok(())
}

/// Negative: border_radius and border_width pass through correctly.
#[test]
fn profile_card_border_radius_and_width_in_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let graph = load_scene_source_graph(repo_root(), "profile_card_absolute")?;
    let resolved = resolve_scene(&graph)?;
    let scene = compute_layout(&resolved);

    let Some(card) = scene.nodes.iter().find(|n| {
        n.node
            .attributes
            .iter()
            .any(|(k, v)| k == "class" && v.contains("profile-card"))
    }) else {
        return Err("profile-card not found".into());
    };

    assert_eq!(card.geometry.border_width, 1.0);
    assert_eq!(card.geometry.border_radius, 16.0);
    Ok(())
}

#[test]
fn source_boxes_follow_containing_blocks_and_visible_flex_items()
-> Result<(), Box<dyn std::error::Error>> {
    let html = r#"<!doctype html><html><body><aside><section>
        <i id="first"></i><i id="skip"></i><i id="second"></i><i id="third"></i>
        </section></aside></body></html>"#;
    let css = r#"
        html { font-size: 10px; }
        aside { position: absolute; inset: 4px 8px; margin: auto; width: 10rem; height: 50dvh; }
        section { position: absolute; inset: 0; padding: 5px 10px; display: flex;
                  flex-wrap: wrap; align-content: flex-end; }
        i { position: relative; width: 50%; height: 15px; }
        #skip { display: none; }
        #second { top: 2px; }
        #third::before { content: ""; position: absolute; right: 0; bottom: 0;
                         width: 200%; height: 100%; }
        #third::after { content: ""; display: none; }
    "#;
    let graph = SceneSourceGraph::from_strings_at_viewport_size("boxes", html, css, 240.0, 200.0)?;
    let resolved = resolve_scene_with_radio_selection(&graph, 240.0, 200.0, &[])?;
    let ids = |tag: &str| {
        resolved
            .nodes
            .iter()
            .find(|node| node.node.name.as_deref() == Some(tag))
            .map(|node| node.node.id)
            .unwrap()
    };
    let id = |id: &str| {
        resolved
            .nodes
            .iter()
            .find(|node| {
                node.node
                    .attributes
                    .iter()
                    .any(|(key, value)| key == "id" && value == id)
            })
            .map(|node| node.node.id)
            .unwrap()
    };
    let mut layout = UsedBoxLayout::new(&resolved, [240.0, 200.0]);
    for (node, pseudo, origin, size) in [
        (ids("aside"), None, [70.0, 50.0], [100.0, 100.0]),
        (ids("section"), None, [70.0, 50.0], [100.0, 100.0]),
        (id("first"), None, [80.0, 115.0], [40.0, 15.0]),
        (id("second"), None, [120.0, 117.0], [40.0, 15.0]),
        (id("third"), Some("before"), [40.0, 130.0], [80.0, 15.0]),
    ] {
        let actual = layout.border_box(node, pseudo)?.expect("expected a box");
        let expected = BorderBox { origin, size };
        for (actual_value, expected_value) in actual
            .origin
            .into_iter()
            .chain(actual.size)
            .zip(expected.origin.into_iter().chain(expected.size))
        {
            assert!(
                (actual_value - expected_value).abs() <= 1.0 / 64.0,
                "node {node} {pseudo:?}: {actual:?} vs {expected:?}"
            );
        }
    }
    assert_eq!(layout.border_box(id("skip"), None)?, None);
    assert_eq!(layout.border_box(id("third"), Some("after"))?, None);
    Ok(())
}
