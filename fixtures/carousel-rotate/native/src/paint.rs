use std::{collections::HashMap, fs, process::Command};

use css_gpui::carousel::{CarouselScene, Panel};
use fontdue::{Font, FontSettings};
use image::RgbaImage;

use crate::projection::Plane;

struct Glyph {
    left: f32,
    top: f32,
    width: usize,
    height: usize,
    mask: Vec<u8>,
}

pub(crate) struct Label(Vec<Glyph>);

impl Label {
    fn new(panel: &Panel, font: &Font) -> Self {
        let metrics = font
            .horizontal_line_metrics(panel.font_size)
            .expect("panel font needs horizontal metrics");
        // Explicit CSS line-height distributes leading around ascent and descent.
        let ink_line = metrics.ascent - metrics.descent;
        let line_top = panel.border.width + (panel.line_height - ink_line) * 0.5;
        let baseline = line_top + metrics.ascent;
        let advance: f32 = panel
            .label
            .chars()
            .map(|ch| font.metrics(ch, panel.font_size).advance_width)
            .sum();
        let mut pen = (panel.size[0] - advance) * 0.5;
        let mut glyphs = Vec::new();
        for ch in panel.label.chars() {
            let (metrics, mask) = font.rasterize(ch, panel.font_size);
            glyphs.push(Glyph {
                left: (pen + metrics.xmin as f32).round(),
                top: baseline.floor() - metrics.ymin as f32 - metrics.height as f32,
                width: metrics.width,
                height: metrics.height,
                mask,
            });
            pen += metrics.advance_width;
        }
        Self(glyphs)
    }

    fn coverage(&self, x: f32, y: f32) -> f32 {
        for glyph in &self.0 {
            let x = x - glyph.left;
            let y = y - glyph.top;
            if x >= 0.0 && y >= 0.0 && x < glyph.width as f32 && y < glyph.height as f32 {
                return f32::from(glyph.mask[y as usize * glyph.width + x as usize]) / 255.0;
            }
        }
        0.0
    }
}

pub(crate) fn labels(panels: &[Panel]) -> Vec<Label> {
    let mut fonts = HashMap::new();
    panels
        .iter()
        .map(|panel| {
            let font = fonts
                .entry((&panel.font_family, panel.font_weight))
                .or_insert_with(|| {
                    // Select the Linux reference browser's generic sans-serif family.
                    let family = if panel.font_family == "sans-serif" {
                        "Liberation Sans"
                    } else {
                        panel.font_family.as_str()
                    };
                    let weight = if panel.font_weight >= 700 {
                        "bold"
                    } else {
                        "regular"
                    };
                    let result = Command::new("fc-match")
                        .args(["--format", "%{file}", &format!("{family}:weight={weight}")])
                        .output()
                        .expect("invoke Fontconfig to select the panel font");
                    assert!(
                        result.status.success(),
                        "Fontconfig could not select {family}"
                    );
                    let path = String::from_utf8(result.stdout).expect("read font path");
                    let bytes =
                        fs::read(&path).unwrap_or_else(|error| panic!("read {path}: {error}"));
                    Font::from_bytes(bytes, FontSettings::default()).expect("parse panel font")
                });
            Label::new(panel, font)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Pixel([f32; 3]);

impl Pixel {
    fn over(&mut self, rgba: [u8; 4], coverage: f32) {
        let alpha = f32::from(rgba[3]) * coverage / 255.0;
        for (channel, source) in self.0.iter_mut().zip(rgba) {
            *channel += (f32::from(source) - *channel) * alpha;
        }
    }
}

struct Surface {
    width: u32,
    height: u32,
    scale: f32,
    samples: Vec<[Pixel; 4]>,
}

impl Surface {
    fn new(width: u32, height: u32, scale: f32) -> Self {
        Self {
            width,
            height,
            scale,
            samples: vec![[Pixel([255.0; 3]); 4]; width as usize * height as usize],
        }
    }

    fn blend(&mut self, polygon: &[[f64; 2]], mut color: impl FnMut([f64; 2], &mut Pixel)) {
        if polygon.len() < 3 {
            return;
        }
        let scale = f64::from(self.scale);
        let inverse_scale = scale.recip();
        let axis = |index, limit| {
            let (min, max) = polygon
                .iter()
                .map(|point| point[index] * scale)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), value| {
                    (min.min(value), max.max(value))
                });
            min.floor().max(0.0) as u32..(max.ceil() as u32).min(limit)
        };
        let columns = axis(0, self.width);
        for y in axis(1, self.height) {
            for x in columns.clone() {
                let samples = &mut self.samples[(y * self.width + x) as usize];
                for (sample, [dx, dy]) in
                    samples
                        .iter_mut()
                        .zip([[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]])
                {
                    color(
                        [
                            (f64::from(x) + dx) * inverse_scale,
                            (f64::from(y) + dy) * inverse_scale,
                        ],
                        sample,
                    );
                }
            }
        }
    }

    fn into_image(self) -> RgbaImage {
        let mut bgra = Vec::with_capacity(self.samples.len() * 4);
        for samples in self.samples {
            for channel in [2, 1, 0] {
                let total: f32 = samples.iter().map(|pixel| pixel.0[channel]).sum();
                bgra.push((total / 4.0).round() as u8);
            }
            bgra.push(255);
        }
        RgbaImage::from_raw(self.width, self.height, bgra).unwrap()
    }
}

fn paint_plane(surface: &mut Surface, plane: &Plane<'_>, label: &Label) {
    let panel = plane.panel;
    let Some(sampling) = &plane.projected.sampling else {
        return;
    };
    let [width, height] = panel.size.map(f64::from);
    let stroke = f64::from(panel.border.width);
    let polygon = &plane.projected.geometry.viewport_polygon;
    surface.blend(polygon, |[screen_x, screen_y], pixel| {
        let Some(([x, y], _)) = sampling.sample(screen_x, screen_y) else {
            return;
        };
        if !(x > 0.0 && x < width && y > 0.0 && y < height) {
            return;
        }
        if x <= stroke || y <= stroke || x >= width - stroke || y >= height - stroke {
            pixel.over(panel.border.color, 1.0);
        } else {
            pixel.over(panel.fill, 1.0);
            let coverage = label.coverage(x as f32, y as f32);
            pixel.over(panel.text_color, coverage);
        }
    });
}

pub(crate) fn texture(
    scene: &CarouselScene,
    scene_origin: [f32; 2],
    planes: &[Plane<'_>],
    labels: &[Label],
    width: u32,
    height: u32,
    scale: f32,
) -> RgbaImage {
    let mut surface = Surface::new(width, height, scale);
    let [left, top] = scene_origin.map(f64::from);
    let [width, height] = scene.scene.map(f64::from);
    let stroke = f64::from(scene.scene_border.width);
    surface.blend(
        &[
            [left, top],
            [left + width, top],
            [left + width, top + height],
            [left, top + height],
        ],
        |[x, y], pixel| {
            let x = x - left;
            let y = y - top;
            if (0.0..width).contains(&x)
                && (0.0..height).contains(&y)
                && (x < stroke || y < stroke || x >= width - stroke || y >= height - stroke)
            {
                pixel.over(scene.scene_border.color, 1.0);
            }
        },
    );

    // The panels form a ring of disjoint vertical planes. Composite rear
    // panels first so every translucent fill reveals the planes behind it.
    let mut order: Vec<_> = planes.iter().zip(labels).collect();
    order.sort_by(|(a, _), (b, _)| {
        a.projected
            .center_depth
            .total_cmp(&b.projected.center_depth)
    });
    for (plane, label) in order {
        paint_plane(&mut surface, plane, label);
    }
    surface.into_image()
}
