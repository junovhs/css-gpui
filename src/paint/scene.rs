use crate::ir::Paint;

use super::{Gradient, Rect, layers, rgba};

/// Borrowed unpremultiplied sRGB pixels of a decoded raster asset.
#[derive(Clone, Copy)]
pub struct ImagePixels<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [[u8; 4]],
}

impl ImagePixels<'_> {
    fn valid(self) -> bool {
        self.width > 0
            && self.height > 0
            && (self.width as usize).checked_mul(self.height as usize) == Some(self.rgba.len())
    }

    fn sample(self, x: f32, y: f32) -> [f32; 4] {
        let x = x.clamp(0.0, (self.width - 1) as f32);
        let y = y.clamp(0.0, (self.height - 1) as f32);
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let t = x.fract();
        let u = y.fract();
        let get = |x, y| normalized(self.rgba[(y * self.width + x) as usize]);
        let [nw, ne, sw, se] = [get(x0, y0), get(x1, y0), get(x0, y1), get(x1, y1)];
        std::array::from_fn(|i| {
            (nw[i] * (1.0 - t) + ne[i] * t) * (1.0 - u) + (sw[i] * (1.0 - t) + se[i] * t) * u
        })
    }
}

enum Layer<'a> {
    Gradient(Gradient),
    Image(ImagePixels<'a>),
}

/// Paint sampled from a resolved CSS node. Images are supplied by the host so
/// the compiler and compositor need no network or filesystem access.
pub struct Background<'a> {
    base: [f32; 4],
    layers: Vec<Layer<'a>>,
    hard_light: bool,
    cover: bool,
    offset: (f32, f32),
}

pub fn background_image_urls(paint: &Paint) -> impl Iterator<Item = &str> {
    background_layers(paint).filter_map(image_url)
}

pub fn blur_radius(paint: &Paint) -> Option<f32> {
    let value = paint.effects.get("backdrop-filter")?;
    let pixels: f32 = value
        .strip_prefix("blur(")?
        .strip_suffix(')')?
        .strip_suffix("px")?
        .parse()
        .ok()?;
    (pixels.is_finite() && pixels >= 0.0).then_some(pixels)
}

impl<'a> Background<'a> {
    pub fn new(
        paint: &Paint,
        image: impl Fn(&str) -> Option<ImagePixels<'a>>,
        offset: (f32, f32),
    ) -> Result<Self, String> {
        let mut parsed = Vec::new();
        for source in background_layers(paint) {
            if let Some(url) = image_url(source) {
                let image = image(url)
                    .filter(|asset| asset.valid())
                    .ok_or_else(|| format!("missing raster asset {url}"))?;
                parsed.push(Layer::Image(image));
            } else if let Some(gradient) = Gradient::parse(source) {
                parsed.push(Layer::Gradient(gradient));
            } else if source != "none" && !source.is_empty() {
                return Err(format!("unsupported background layer {source}"));
            }
        }
        let base = paint
            .background_color
            .as_ref()
            .and_then(|color| rgba(&color.0))
            .map(normalized)
            .unwrap_or([0.0; 4]);
        Ok(Self {
            base,
            layers: parsed,
            hard_light: paint
                .effects
                .get("background-blend-mode")
                .is_some_and(|mode| mode == "hard-light"),
            cover: paint
                .effects
                .get("background-size")
                .is_some_and(|size| size == "cover"),
            offset,
        })
    }

    pub fn sample(&self, x: f32, y: f32, width: f32, height: f32) -> [f32; 4] {
        self.layers.iter().rev().fold(self.base, |behind, layer| {
            let source = match layer {
                Layer::Gradient(gradient) => gradient.sample(x, y, width, height),
                Layer::Image(image) => {
                    let scale = if self.cover {
                        (width / image.width as f32).max(height / image.height as f32)
                    } else {
                        1.0
                    };
                    let iw = image.width as f32;
                    let ih = image.height as f32;
                    let x = (x - (width - iw * scale) * 0.5 - self.offset.0) / scale - 0.5;
                    let y = (y - (height - ih * scale) * 0.5 - self.offset.1) / scale - 0.5;
                    image.sample(x, y)
                }
            };
            blend_over(source, behind, self.hard_light)
        })
    }

    pub fn rasterize(&self, width: u32, height: u32) -> Vec<[u8; 4]> {
        (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    self.sample(x as f32 + 0.5, y as f32 + 0.5, width as f32, height as f32)
                        .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
                })
            })
            .collect()
    }
}

fn background_layers(paint: &Paint) -> impl Iterator<Item = &str> {
    paint
        .effects
        .get("background-image")
        .or_else(|| paint.effects.get("background"))
        .into_iter()
        .flat_map(|source| layers(source))
}

fn image_url(value: &str) -> Option<&str> {
    let path = value.strip_prefix("url(")?.strip_suffix(')')?.trim();
    Some(path.trim_matches(['"', '\'']))
}

fn normalized(rgba: [u8; 4]) -> [f32; 4] {
    rgba.map(|channel| f32::from(channel) / 255.0)
}

fn blend_over(front: [f32; 4], back: [f32; 4], hard_light: bool) -> [f32; 4] {
    let alpha = front[3] + back[3] * (1.0 - front[3]);
    if alpha == 0.0 {
        return [0.0; 4];
    }
    std::array::from_fn(|i| {
        if i == 3 {
            return alpha;
        }
        let source = front[i];
        let destination = back[i];
        let blend = if !hard_light {
            source
        } else if source <= 0.5 {
            2.0 * destination * source
        } else {
            1.0 - 2.0 * (1.0 - destination) * (1.0 - source)
        };
        let mixed = source * (1.0 - back[3]) + blend * back[3];
        (front[3] * mixed + back[3] * (1.0 - front[3]) * destination) / alpha
    })
}

/// Composite a texture into a frame at its original scene position.
pub fn stamp(frame: &mut [[u8; 4]], width: u32, height: u32, bounds: Rect, stamp: &[[u8; 4]]) {
    assert_eq!(frame.len(), width as usize * height as usize);
    assert_eq!(stamp.len(), bounds.width as usize * bounds.height as usize);
    for y in 0..bounds.height {
        for x in 0..bounds.width {
            let destination_x = i64::from(bounds.x) + i64::from(x);
            let destination_y = i64::from(bounds.y) + i64::from(y);
            if !(0..i64::from(width)).contains(&destination_x)
                || !(0..i64::from(height)).contains(&destination_y)
            {
                continue;
            }
            let pixel = stamp[(y * bounds.width + x) as usize];
            let slot = &mut frame[destination_y as usize * width as usize + destination_x as usize];
            if pixel[3] == 255 {
                *slot = pixel;
            } else if pixel[3] != 0 {
                *slot = blend_over(normalized(pixel), normalized(*slot), false)
                    .map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
    }
}
