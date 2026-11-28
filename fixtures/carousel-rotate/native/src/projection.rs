use std::collections::BTreeMap;

use css_gpui::{
    carousel::{CarouselScene, Panel},
    projection::{Layer, ProjectedPlane, project_plane},
};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    pub angle: f32,
    pub z: f32,
}

pub(crate) struct Plane<'a> {
    pub panel: &'a Panel,
    pub projected: ProjectedPlane,
}

pub(crate) fn scene_origin(scene: &CarouselScene, width: f32) -> [f32; 2] {
    // The body margin and first block margin collapse in the standards-mode shell.
    [(width - scene.scene[0]) * 0.5, scene.margins_y[0].max(8.0)]
}

pub(crate) fn project(
    scene: &CarouselScene,
    scene_origin: [f32; 2],
    viewport: [f32; 2],
    pose: Pose,
) -> Vec<Plane<'_>> {
    let scene_origin = scene_origin.map(f64::from);
    let carousel_origin = scene_origin.map(|point| point + f64::from(scene.scene_border.width));
    // This layer starts the projected context. Perspective applies to its
    // child, which preserves 3D for the rotating ring.
    let camera = BTreeMap::from([
        ("perspective".into(), format!("{}px", scene.perspective)),
        ("transform-style".into(), "preserve-3d".into()),
    ]);
    let carousel = BTreeMap::from([
        (
            "transform".into(),
            format!("translateZ({}px) rotateY({}deg)", pose.z, pose.angle),
        ),
        ("transform-style".into(), "preserve-3d".into()),
    ]);
    let camera = Layer {
        origin: scene_origin,
        size: scene.scene.map(f64::from),
        font_size: 16.0,
        css: &camera,
    };
    let carousel = Layer {
        origin: carousel_origin,
        size: scene.carousel.map(f64::from),
        font_size: 16.0,
        css: &carousel,
    };
    let viewport = viewport.map(f64::from);

    scene
        .panels
        .iter()
        .map(|panel| {
            let css = BTreeMap::from([(
                "transform".into(),
                format!(
                    "rotateY({}deg) translateZ({}px)",
                    panel.rotation_degrees, panel.translate_z
                ),
            )]);
            let layers = [
                Layer { ..camera },
                Layer { ..carousel },
                Layer {
                    origin: std::array::from_fn(|axis| {
                        carousel_origin[axis] + f64::from(panel.origin[axis])
                    }),
                    size: panel.size.map(f64::from),
                    font_size: panel.font_size,
                    css: &css,
                },
            ];
            let projected = project_plane(&layers, viewport, 16.0)
                .expect("project compiled carousel transforms")
                .expect("carousel chain has a panel");
            Plane { panel, projected }
        })
        .collect()
}
