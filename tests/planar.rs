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
