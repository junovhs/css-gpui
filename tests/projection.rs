use std::collections::BTreeMap;

use css_gpui::projection::{Layer, project_box};

#[test]
fn a_face_crossing_the_eye_is_clipped_before_perspective_division() {
    let camera = BTreeMap::from([
        ("perspective".into(), "100px".into()),
        ("perspective-origin".into(), "left top".into()),
        ("transform-style".into(), "preserve-3d".into()),
    ]);
    let wall = BTreeMap::from([
        ("transform".into(), "rotateY(-90deg)".into()),
        ("transform-origin".into(), "0 0 0".into()),
    ]);

    let viewport = [100.0, 150.0];
    let projected = project_box(
        &[
            Layer {
                origin: [0.0, 0.0],
                size: viewport,
                font_size: 16.0,
                css: &camera,
            },
            Layer {
                origin: [10.0, 10.0],
                size: [120.0, 10.0],
                font_size: 16.0,
                css: &wall,
            },
        ],
        viewport,
        16.0,
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        projected.corners,
        [Some([10.0, 10.0]), None, None, Some([10.0, 20.0])]
    );
    // The two horizontal wall edges project to y=x and y=2x.
    // The first meets the right edge, the second meets the bottom edge.
    let polygon = projected.viewport_polygon;
    assert_eq!(polygon.len(), 5);
    for expected in [
        [10.0, 10.0],
        [100.0, 100.0],
        [100.0, 150.0],
        [75.0, 150.0],
        [10.0, 20.0],
    ] {
        assert!(
            polygon.iter().any(|point| {
                (point[0] - expected[0]).abs() < 1e-8 && (point[1] - expected[1]).abs() < 1e-8
            }),
            "clipped polygon {polygon:?} omitted {expected:?}"
        );
    }
}
