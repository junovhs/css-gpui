use crate::projection::{Point, SamplingPlane};

use super::Material;

pub(super) struct Face {
    plane: SamplingPlane,
    paint: Material,
    size: Point,
    bounds: [Point; 2],
}

impl Face {
    pub fn new(plane: SamplingPlane, paint: Material, size: Point, polygon: &[Point]) -> Self {
        let bounds = polygon.iter().fold(
            [[f64::INFINITY; 2], [f64::NEG_INFINITY; 2]],
            |[min, max], point| {
                [
                    std::array::from_fn(|i| min[i].min(point[i])),
                    std::array::from_fn(|i| max[i].max(point[i])),
                ]
            },
        );
        Self {
            plane,
            paint,
            size,
            bounds,
        }
    }
}

struct Fragment {
    depth: f64,
    color: [f32; 4],
}

pub(super) fn render(faces: &[Face], background: [u8; 4], [width, height]: [u32; 2]) -> Vec<u8> {
    let mut output = Vec::with_capacity(width as usize * height as usize * 4);
    let mut scanline: Vec<Vec<Fragment>> = (0..width).map(|_| Vec::new()).collect();
    let background = background.map(|value| f32::from(value) / 255.0);
    for row in 0..height {
        let y = f64::from(row) + 0.5;
        for face in faces {
            let [[left, top], [right, bottom]] = face.bounds;
            if !(top..bottom).contains(&y) {
                continue;
            }
            let start = left.floor() as usize;
            let end = (right.ceil() as usize).min(scanline.len());
            let [width, height] = face.size;
            for (column, fragments) in scanline.iter_mut().enumerate().take(end).skip(start) {
                let Some(([u, v], depth)) = face.plane.sample(column as f64 + 0.5, y) else {
                    continue;
                };
                if !(0.0..width).contains(&u) || !(0.0..height).contains(&v) {
                    continue;
                }
                let color = face
                    .paint
                    .sample([u as f32, v as f32], [width as f32, height as f32]);
                if color[3] > 0.0 {
                    // Quantize below the visible depth resolution so numerically
                    // coplanar faces fall back to stable source paint order.
                    fragments.push(Fragment {
                        depth: (depth * 1e6).round(),
                        color,
                    });
                }
            }
        }
        for fragments in &mut scanline {
            fragments.sort_by(|a, b| a.depth.total_cmp(&b.depth));
            let first = fragments
                .iter()
                .rposition(|fragment| fragment.color[3] == 1.0)
                .unwrap_or(0);
            let pixel = fragments[first..]
                .iter()
                .fold(background, |color, fragment| over(fragment.color, color));
            output.extend(pixel.map(byte));
            fragments.clear();
        }
    }
    output
}

pub(super) fn byte(value: f32) -> u8 {
    (value * 255.0).round() as u8
}

pub(super) fn over(source: [f32; 4], destination: [f32; 4]) -> [f32; 4] {
    let retain = destination[3] * (1.0 - source[3]);
    let alpha = source[3] + retain;
    if alpha == 0.0 {
        return [0.0; 4];
    }
    let [r, g, b, _] =
        std::array::from_fn(|i| (source[i] * source[3] + destination[i] * retain) / alpha);
    [r, g, b, alpha]
}
