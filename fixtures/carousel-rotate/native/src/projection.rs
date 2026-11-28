use css_gpui::carousel::{CarouselScene, Panel};

pub(crate) type Point = [f32; 2];

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Pose {
    pub angle: f32,
    pub z: f32,
}

pub(crate) struct Plane<'a> {
    pub panel: &'a Panel,
    pub center: [f32; 3],
    pub slope: Point,
    pub quad: [Point; 4],
}

pub(crate) struct Projection<'a> {
    pub scene_origin: Point,
    pub eye: Point,
    pub distance: f32,
    pub planes: Vec<Plane<'a>>,
}

pub(crate) fn project(scene: &CarouselScene, width: f32, pose: Pose) -> Projection<'_> {
    // The body margin and first block margin collapse in the standards-mode shell.
    let scene_origin = [(width - scene.scene[0]) * 0.5, scene.margins_y[0].max(8.0)];
    let eye = [
        scene_origin[0] + scene.scene[0] * 0.5,
        scene_origin[1] + scene.scene[1] * 0.5,
    ];
    let distance = scene.perspective;
    let carousel_left = scene_origin[0] + scene.scene_border.width;
    let carousel_top = scene_origin[1] + scene.scene_border.width;
    let parent_origin_x = scene.carousel[0] * 0.5;
    let (parent_sin, parent_cos) = pose.angle.to_radians().sin_cos();
    let planes = scene
        .panels
        .iter()
        .map(|panel| {
            let [half_width, half_height] = panel.size.map(|v| v * 0.5);
            let (child_sin, child_cos) = panel.rotation_degrees.to_radians().sin_cos();
            let child_center_x = panel.origin[0] + half_width + child_sin * panel.translate_z;
            let child_center_z = child_cos * panel.translate_z;
            let center_x = child_center_x - parent_origin_x;
            let center = [
                carousel_left
                    + parent_origin_x
                    + parent_cos * center_x
                    + parent_sin * child_center_z,
                carousel_top + panel.origin[1] + half_height,
                -parent_sin * center_x + parent_cos * child_center_z + pose.z,
            ];
            let (slope_z, slope_x) = (panel.rotation_degrees + pose.angle).to_radians().sin_cos();
            let slope = [slope_x, -slope_z];
            let quad = [
                [-half_width, -half_height],
                [half_width, -half_height],
                [half_width, half_height],
                [-half_width, half_height],
            ]
            .map(|[x, y]| {
                let scale = distance / (distance - (center[2] + x * slope[1]));
                [
                    eye[0] + (center[0] + x * slope[0] - eye[0]) * scale,
                    eye[1] + (center[1] + y - eye[1]) * scale,
                ]
            });
            Plane {
                panel,
                center,
                slope,
                quad,
            }
        })
        .collect();
    Projection {
        scene_origin,
        eye,
        distance,
        planes,
    }
}
