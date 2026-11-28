use std::sync::{Arc, Mutex};

use crate::{
    layout::BorderBox,
    projection::{Point, SamplingPlane},
};

use super::Material;

pub(super) struct Face {
    plane: SamplingPlane,
    paint: Arc<Material>,
    size: Point,
    bounds: [Point; 2],
}

impl Face {
    pub fn new(plane: SamplingPlane, paint: Arc<Material>, size: Point, polygon: &[Point]) -> Self {
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

struct Fragment<'a> {
    depth: f64,
    point: [f32; 2],
    face: &'a Face,
}

pub(super) struct Overlay {
    pub bounds: BorderBox,
    pub paint: Material,
}

impl Overlay {
    fn composite(&self, [x, y]: Point, backdrop: [f32; 4]) -> [f32; 4] {
        let [left, top] = self.bounds.origin;
        let mut source = self.paint.sample(
            [(x - left) as f32, (y - top) as f32],
            self.bounds.size.map(|v| v as f32),
        );
        // The planar canvas is opaque. Apply the separable overlay blend to
        // the unpremultiplied channels, then composite the layer's alpha.
        for (source, backdrop) in source[..3].iter_mut().zip(backdrop) {
            *source = if backdrop <= 0.5 {
                2.0 * backdrop * *source
            } else {
                1.0 - 2.0 * (1.0 - backdrop) * (1.0 - *source)
            };
        }
        over(source, backdrop)
    }
}

pub(super) fn render(
    faces: &[Face],
    overlays: &[Overlay],
    background: [u8; 4],
    viewport: [u32; 2],
    [width, height]: [u32; 2],
) -> Vec<u8> {
    let scale = [
        f64::from(viewport[0]) / f64::from(width),
        f64::from(viewport[1]) / f64::from(height),
    ];
    let background = background.map(|value| f32::from(value) / 255.0);
    let row_bytes = width as usize * 4;
    let mut rgba = vec![0; row_bytes * height as usize];
    let workers = if width * height >= 100_000 {
        std::thread::available_parallelism().map_or(1, |count| count.get().min(8))
    } else {
        1
    };
    let raster = Raster {
        faces,
        overlays,
        background,
        scale,
        row_bytes,
    };
    if workers == 1 {
        raster.render_rows(&mut rgba, 0);
    } else {
        // Closer rows can contain far more translucent planes than distant
        // ones. Distribute short bands as workers finish their current band.
        const BAND: usize = 16;
        let bands = Mutex::new(rgba.chunks_mut(row_bytes * BAND).enumerate());
        std::thread::scope(|scope| {
            for _ in 0..workers.min((height as usize).div_ceil(BAND)) {
                let (raster, bands) = (&raster, &bands);
                scope.spawn(move || {
                    loop {
                        let band = bands.lock().unwrap().next();
                        let Some((index, rows)) = band else { break };
                        raster.render_rows(rows, index * BAND);
                    }
                });
            }
        });
    }
    rgba
}

struct Raster<'a> {
    faces: &'a [Face],
    overlays: &'a [Overlay],
    background: [f32; 4],
    scale: Point,
    row_bytes: usize,
}

impl Raster<'_> {
    fn render_rows(&self, output: &mut [u8], first_row: usize) {
        let mut scanline: Vec<Vec<Fragment>> =
            (0..self.row_bytes / 4).map(|_| Vec::new()).collect();
        let [scale_x, scale_y] = self.scale;
        let mut colors = Vec::new();
        for (row, pixels) in output.chunks_exact_mut(self.row_bytes).enumerate() {
            let y = ((first_row + row) as f64 + 0.5) * scale_y;
            for face in self.faces {
                let [[left, top], [right, bottom]] = face.bounds;
                if !(top..bottom).contains(&y) {
                    continue;
                }
                let start = (left / scale_x).floor() as usize;
                let end = ((right / scale_x).ceil() as usize).min(scanline.len());
                let [width, height] = face.size;
                for (column, fragments) in scanline.iter_mut().enumerate().take(end).skip(start) {
                    let x = (column as f64 + 0.5) * scale_x;
                    let Some(([u, v], depth)) = face.plane.sample(x, y) else {
                        continue;
                    };
                    if !(0.0..width).contains(&u) || !(0.0..height).contains(&v) {
                        continue;
                    }
                    // Numerically coplanar faces fall back to source paint order.
                    fragments.push(Fragment {
                        depth: (depth * 1e6).round(),
                        point: [u as f32, v as f32],
                        face,
                    });
                }
            }
            for (column, (fragments, output)) in scanline
                .iter_mut()
                .zip(pixels.chunks_exact_mut(4))
                .enumerate()
            {
                fragments.sort_by(|a, b| a.depth.total_cmp(&b.depth));
                // Traverse from the front to avoid sampling material behind
                // an opaque face, then blend the visible layers in paint order.
                for fragment in fragments.iter().rev() {
                    let face = fragment.face;
                    let color = face
                        .paint
                        .sample(fragment.point, face.size.map(|value| value as f32));
                    if color[3] > 0.0 {
                        colors.push(color);
                        if color[3] == 1.0 {
                            break;
                        }
                    }
                }
                let pixel = colors
                    .drain(..)
                    .rev()
                    .fold(self.background, |color, fragment| over(fragment, color));
                let x = (column as f64 + 0.5) * scale_x;
                let composed = self.overlays.iter().fold(pixel, |backdrop, overlay| {
                    overlay.composite([x, y], backdrop)
                });
                output.copy_from_slice(&composed.map(byte));
                fragments.clear();
            }
        }
    }
}

pub(super) fn byte(value: f32) -> u8 {
    (value * 255.0).round() as u8
}

pub(super) fn over(source: [f32; 4], destination: [f32; 4]) -> [f32; 4] {
    if source[3] == 1.0 {
        return source;
    }
    let retain = destination[3] * (1.0 - source[3]);
    let alpha = source[3] + retain;
    if alpha == 0.0 {
        return [0.0; 4];
    }
    let [r, g, b, _] =
        std::array::from_fn(|i| (source[i] * source[3] + destination[i] * retain) / alpha);
    [r, g, b, alpha]
}
