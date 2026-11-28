use css_gpui::{SceneSourceGraph, planar::preview, resolver::resolve_scene_with_radio_selection};

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
