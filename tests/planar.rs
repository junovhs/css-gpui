use css_gpui::{SceneSourceGraph, planar::preview, resolver::resolve_scene_with_radio_selection};

#[test]
fn viewport_controls_follow_checked_state_and_css_geometry() {
    use css_gpui::{
        planar::viewport_controls,
        resolver::{checked_snapshot_controls, resolve_scene_with_checked_activations},
    };

    let html = r#"
        <html><body>
            <input type="radio" name="step" id="first" checked>
            <input type="radio" name="step" id="second">
            <main></main>
            <aside><label for="second"></label><label for="first"></label></aside>
        </body></html>
    "#;
    let css = r#"
        body, main { position: absolute; inset: 0; }
        body { overflow: hidden; }
        main { perspective: 500px; }
        input { display: none; }
        aside { position: absolute; right: 2px; bottom: 4px; z-index: 2; transform: translateX(3px); }
        label {
            position: absolute; right: 5px; bottom: 10px; width: 20px; aspect-ratio: 2 / 1;
            box-sizing: border-box; padding: 2px; border-radius: 4px;
            background: #111; color: #fff;
        }
        label::after { content: "+"; }
        label[for=first]::after { content: "−"; }
        #first:checked ~ aside [for=first], #second:checked ~ aside [for=second] { pointer-events: none; }
        @media (orientation: portrait) { label { right: 8px; } }
    "#;
    for (viewport, bounds, activated, target, glyph) in [
        ([120, 90], [96.0, 66.0, 20.0, 10.0], false, "second", '+'),
        ([90, 120], [63.0, 96.0, 20.0, 10.0], true, "first", '−'),
    ] {
        let [width, height] = viewport.map(|size| size as f32);
        let graph = SceneSourceGraph::from_strings_at_viewport_size(
            "viewport controls",
            html,
            css,
            width,
            height,
        )
        .unwrap();
        let element = |name: &str| {
            graph
                .html_nodes
                .iter()
                .find(|node| node.name.as_deref() == Some(name))
                .unwrap()
                .id
        };
        let activated: Vec<_> = graph
            .html_nodes
            .iter()
            .filter(|node| {
                activated
                    && node
                        .attributes
                        .iter()
                        .any(|(key, value)| key == "id" && value == "second")
            })
            .map(|node| node.id)
            .collect();
        let scene =
            resolve_scene_with_checked_activations(&graph, width, height, &activated).unwrap();
        let candidates =
            checked_snapshot_controls(&graph, &scene, element("body"), &activated).unwrap();
        assert_eq!(candidates.labels.len(), 1);
        let label = &candidates.labels[0];
        assert_eq!(label.target, target);
        let controls = viewport_controls(
            &graph,
            &scene,
            element("main"),
            viewport,
            &candidates.labels,
        );
        let control = &controls[&label.node_id];
        assert_eq!(control.bounds, bounds);
        assert_eq!(control.glyph, glyph);
        let [x, y, ..] = bounds;
        assert!(!control.contains([x, y]));
        assert!(control.contains([x + 10.0, y + 5.0]));
        assert!(!control.contains([x + 20.0, y + 5.0]));
    }
}

#[test]
fn crossing_faces_use_pixel_depth_and_preserve_reflected_front_faces() {
    let html = "<main><i></i><b></b><em></em><strong></strong></main>";
    let css = r#"
        main, main > * {
            position: absolute;
            transform-style: preserve-3d;
        }
        main { inset: 0; perspective: 800px; }
        main > * { left: 0; top: 0; width: 80px; height: 60px; }
        i { background: #f00; transform: rotateY(35deg); }
        b { background: #00f; transform: rotateY(-35deg); }
        em, strong {
            top: 20px; height: 20px; backface-visibility: hidden;
        }
        em { background: #0f0; transform: translateZ(90px) rotateY(180deg); }
        strong {
            left: 30px; width: 20px;
            background: rgba(255,255,0,0.5);
            transform: translateZ(90px) scaleX(-1);
            box-shadow: 1px 1px #000;
        }
    "#;
    let graph = SceneSourceGraph::from_strings("crossing", html, css).unwrap();
    let scene = resolve_scene_with_radio_selection(&graph, 80.0, 60.0, &[]).unwrap();
    let frame = preview(&graph, &scene, scene.nodes[0].node.id, [80, 60]).unwrap();
    let pixel = |x: usize, y: usize| {
        let index = (y * 80 + x) * 4;
        &frame.rgba[index..index + 4]
    };
    assert_eq!(pixel(20, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(60, 30), [0, 0, 255, 255]);
    assert_eq!(pixel(35, 30), [255, 128, 0, 255]);
    assert_eq!(frame.planes.len(), 3);
    assert_eq!(frame.losses.len(), 1);
    assert_eq!(frame.losses[0].property, "box-shadow");
}

#[test]
fn viewport_gradient_blends_only_above_the_3d_context() {
    let html = "<html><body><section><main><i></i></main></section></body></html>";
    let css = r#"
        html { background: #204060; }
        body, section, main { position: absolute; inset: 0; }
        body, section { overflow: hidden; }
        main { perspective: 100px; transform-style: preserve-3d; }
        i {
            position: absolute; inset: 0 auto 0 0; width: 5px;
            background: #c04020; transform: translateZ(1px);
        }
        section::before {
            content: ""; position: absolute; inset: 0; z-index: 2;
            background: radial-gradient(rgba(255, 0, 128, 0.5) 20%, #000 75%);
            mix-blend-mode: overlay;
        }
    "#;
    for (edit, expected, unsupported) in [
        ("", [[224, 32, 32, 255], [0, 0, 0, 255]], false),
        (
            "section::before { visibility: hidden; }",
            [[192, 64, 32, 255], [32, 64, 96, 255]],
            false,
        ),
        (
            "main { z-index: 3; }",
            [[192, 64, 32, 255], [32, 64, 96, 255]],
            true,
        ),
        (
            "body { isolation: isolate; }",
            [[192, 64, 32, 255], [32, 64, 96, 255]],
            true,
        ),
    ] {
        let graph =
            SceneSourceGraph::from_strings("overlay", html, &format!("{css}{edit}")).unwrap();
        let scene = resolve_scene_with_radio_selection(&graph, 9.0, 5.0, &[]).unwrap();
        let root = scene
            .nodes
            .iter()
            .find(|node| node.node.name.as_deref() == Some("main"))
            .unwrap();
        let frame = preview(&graph, &scene, root.node.id, [9, 5]).unwrap();
        let pixel = |x: usize, y: usize| {
            let index = (y * 9 + x) * 4;
            &frame.rgba[index..index + 4]
        };
        assert_eq!(pixel(4, 2), expected[0], "{edit}");
        assert_eq!(pixel(8, 4), expected[1], "{edit}");
        assert_eq!(
            frame
                .losses
                .iter()
                .any(|loss| loss.property == "mix-blend-mode"),
            unsupported,
            "{edit}"
        );
    }
}

#[test]
fn planar_background_layers_repeat_in_local_coordinates_and_keep_their_list_slots() {
    let html = "<html><body><main><i></i></main></body></html>";
    let css = r#"
        html { font-size: 10px; }
        body, main, i { position: absolute; inset: 0; }
        main { perspective: 100px; transform-style: preserve-3d; }
        i {
            inset: 0; transform: translateX(1px);
            background-color: #fff;
            background-image: radial-gradient(#f00 35%, transparent 0), none,
                repeating-linear-gradient(to right, #00f 0% 25%, #0f0 25% 50%);
            background-size: 0.8rem 50%, 75% 0.8rem;
            background-position: 0.1rem 50%, 0 0, -2px 0;
            background-repeat: no-repeat repeat, no-repeat, repeat;
        }
    "#;
    let graph = SceneSourceGraph::from_strings("wall tile", html, css).unwrap();
    let scene = resolve_scene_with_radio_selection(&graph, 18.0, 16.0, &[]).unwrap();
    let root = scene
        .nodes
        .iter()
        .find(|node| node.node.name.as_deref() == Some("main"))
        .unwrap();
    let frame = preview(&graph, &scene, root.node.id, [18, 16]).unwrap();
    let pixel = |x: usize, y: usize| {
        let index = (y * 18 + x) * 4;
        &frame.rgba[index..index + 4]
    };
    assert_eq!(pixel(5, 7), [255, 0, 0, 255]);
    assert_eq!(pixel(5, 15), [255, 0, 0, 255]);
    assert_eq!(pixel(13, 7), [0, 255, 0, 255]);
    assert_eq!(pixel(3, 7), [0, 0, 255, 255]);
    assert_eq!(pixel(15, 7), [0, 0, 255, 255]);
    assert!(frame.losses.is_empty(), "{:?}", frame.losses);
}

#[test]
fn a_brightness_filtered_tile_interpolates_its_local_raster_when_magnified() {
    let html = "<main><i></i></main>";
    let css = r#"
        main, i { position: absolute; inset: 0; }
        main { perspective: 100px; transform-style: preserve-3d; }
        i {
            width: 4px; height: 4px;
            transform-origin: 0 0;
            transform: scale(2);
            background-image: linear-gradient(to right, #fff 50%, #000 50%);
            background-size: 2px 2px;
        }
    "#;
    for (filter, expected) in [("none", [255, 0]), ("brightness(1)", [191, 64])] {
        let graph = SceneSourceGraph::from_strings(
            "magnified tiles",
            html,
            &format!("{css} i {{ filter: {filter}; }}"),
        )
        .unwrap();
        let scene = resolve_scene_with_radio_selection(&graph, 8.0, 8.0, &[]).unwrap();
        let frame = preview(&graph, &scene, scene.nodes[0].node.id, [8, 8]).unwrap();
        for (x, level) in [1, 2].into_iter().zip(expected) {
            let offset = (8 + x) * 4;
            assert_eq!(&frame.rgba[offset..offset + 4], &[level, level, level, 255]);
        }
        assert!(frame.losses.is_empty(), "{:?}", frame.losses);
    }
}

#[test]
fn a_checked_translation_projects_intermediate_geometry_and_can_reverse() {
    use std::{sync::Arc, time::Duration};

    use css_gpui::{planar::Preview, resolver::resolve_scene_with_checked_activations};

    let html = r#"<html><body>
        <input type="radio" id="rest" name="move" checked>
        <input type="radio" id="move" name="move">
        <main><i></i></main>
    </body></html>"#;
    let css = r#"
        body, main { position: absolute; inset: 0; }
        main { transform-style: preserve-3d; }
        i {
            position: absolute; left: 8px; top: 8px;
            width: 16px; height: 16px; background: #f00;
            transition: transform 800ms linear;
        }
        #move:checked ~ main > i { transform: translateX(48px); }
    "#;
    let graph = Arc::new(SceneSourceGraph::from_strings("motion", html, css).unwrap());
    let root = graph
        .html_nodes
        .iter()
        .find(|node| node.name.as_deref() == Some("main"))
        .unwrap()
        .id;
    let checked = graph
        .html_nodes
        .iter()
        .find(|node| {
            node.attributes
                .iter()
                .any(|(name, value)| name == "id" && value == "move")
        })
        .unwrap()
        .id;
    let viewport = [96, 48];
    let make = |activations: &[usize]| {
        let scene =
            resolve_scene_with_checked_activations(&graph, 96.0, 48.0, activations).unwrap();
        Preview::new(graph.clone(), scene, root, viewport)
    };
    let rest = make(&[]);
    let target = make(&[checked]);
    let forward = target.transition_from(&rest, None).unwrap();
    assert_eq!(forward.duration(), Duration::from_millis(800));
    let pose = forward.sample(Duration::from_millis(400));
    let sample = target.render([48, 24], Some(&pose)).unwrap();
    let pixel = |css_x: usize| &sample.rgba[(5 * 48 + css_x / 2) * 4..][..4];
    assert_eq!(pixel(10), [255, 255, 255, 255]);
    assert_eq!(pixel(40), [255, 0, 0, 255]);
    assert_eq!(pixel(64), [255, 255, 255, 255]);

    let reverse = rest.transition_from(&target, Some(&pose)).unwrap();
    let resumed = rest
        .render([48, 24], Some(&reverse.sample(Duration::ZERO)))
        .unwrap();
    assert_eq!(resumed.rgba, sample.rgba);
}
