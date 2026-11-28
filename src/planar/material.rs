use std::collections::BTreeMap;

use crate::{
    paint::{Gradient, layers, rgba},
    resolver::values::{LengthContext, value_parts},
};

use super::{css_value, raster::over};

pub(super) struct Material {
    pub color: [f32; 4],
    gradients: Vec<BackgroundLayer>,
    texture: Option<Texture>,
    brightness: Option<f32>,
    opacity: f32,
}

impl Material {
    pub fn from_css(
        css: &BTreeMap<String, String>,
        effects: &BTreeMap<String, String>,
        text_color: Option<[u8; 4]>,
        loss: &mut impl FnMut(&str, &str),
    ) -> Self {
        let parse_color = |value: &str| {
            if value.eq_ignore_ascii_case("currentcolor") {
                text_color.or(Some([0, 0, 0, 255]))
            } else {
                rgba(value)
            }
        };
        let shorthand = css_value(effects, "background", "none");
        let image = effects.get("background-image");
        // A trailing shorthand color survives a later image longhand.
        let color = if let Some(raw_color) = css.get("background-color") {
            parse_color(raw_color).unwrap_or_else(|| {
                loss("background-color", raw_color);
                [0; 4]
            })
        } else {
            layers(shorthand)
                .last()
                .and_then(parse_color)
                .unwrap_or([0; 4])
        };
        let mut gradients = Vec::new();
        let mut background_layers = layers(image.map_or(shorthand, String::as_str))
            .enumerate()
            .peekable();
        while let Some((index, layer)) = background_layers.next() {
            if layer == "none"
                || (image.is_none()
                    && background_layers.peek().is_none()
                    && parse_color(layer).is_some())
            {
                continue;
            }
            if let Some(gradient) = Gradient::parse(layer).filter(|_| layer.ends_with(')')) {
                gradients.push(BackgroundLayer {
                    index,
                    gradient,
                    tile: None,
                });
            } else {
                loss("background-image", layer);
            }
        }
        let filter = css_value(effects, "filter", "none");
        let brightness = filter
            .strip_prefix("brightness(")
            .and_then(|value| value.strip_suffix(')'))
            .and_then(number)
            .filter(|&value| value >= 0.0);
        if filter != "none" && brightness.is_none() {
            loss("filter", filter);
        }
        let raw_opacity = css_value(effects, "opacity", "1");
        let opacity = number(raw_opacity)
            .unwrap_or_else(|| {
                loss("opacity", raw_opacity);
                1.0
            })
            .clamp(0.0, 1.0);
        for property in [
            "box-shadow",
            "mask",
            "backdrop-filter",
            "mix-blend-mode",
            "background-blend-mode",
        ] {
            if let Some(value) = effects
                .get(property)
                .filter(|value| !matches!(value.as_str(), "none" | "normal"))
            {
                loss(property, value);
            }
        }
        for property in [
            "clip-path",
            "outline",
            "outline-color",
            "outline-style",
            "outline-width",
        ] {
            if let Some(value) = css
                .get(property)
                .filter(|value| !matches!(value.as_str(), "none" | "0"))
            {
                loss(property, value);
            }
        }
        Self {
            color: color.map(|value| f32::from(value) / 255.0),
            gradients,
            texture: None,
            brightness,
            opacity,
        }
    }

    pub fn configure_background(
        &mut self,
        css: &BTreeMap<String, String>,
        effects: &BTreeMap<String, String>,
        area: [f32; 2],
        lengths: LengthContext,
        loss: &mut impl FnMut(&str, &str),
    ) {
        if self.gradients.is_empty() {
            return;
        }
        let lists = [
            (css, "background-attachment", "scroll"),
            (css, "background-clip", "border-box"),
            (css, "background-origin", "padding-box"),
            (effects, "background-size", "auto"),
            (effects, "background-position", "0% 0%"),
            (effects, "background-repeat", "repeat"),
        ]
        .map(|(css, property, initial)| {
            let value = css_value(css, property, initial);
            let mut list: Vec<_> = layers(value).collect();
            if list.is_empty() {
                list.push(value);
            }
            (property, initial, list)
        });
        self.gradients.retain_mut(|layer| {
            // CSS cycles the lists against the original image index, including
            // omitted or unsupported layers, and ignores excess list entries.
            let selected = lists
                .each_ref()
                .map(|(_, _, list)| list[layer.index % list.len()]);
            for ((property, initial, _), value) in lists.iter().zip(selected).take(3) {
                if value != *initial {
                    loss(property, value);
                    return false;
                }
            }
            let [_, _, _, size, position, repeat] = selected;
            let Some(size) = background_size(size, area, lengths).or_else(|| {
                loss("background-size", size);
                None
            }) else {
                return false;
            };
            if size.contains(&0.0) {
                return false;
            }
            let room = std::array::from_fn(|axis| area[axis] - size[axis]);
            let Some(origin) = background_position(position, room, lengths).or_else(|| {
                loss("background-position", position);
                None
            }) else {
                return false;
            };
            let Some(repeat) = background_repeat(repeat).or_else(|| {
                loss("background-repeat", repeat);
                None
            }) else {
                return false;
            };
            if size != area || origin != [0.0; 2] {
                layer.tile = Some(Tile {
                    size,
                    origin,
                    repeat,
                });
            }
            true
        });
        // A CSS filter makes the element an intermediate surface, including
        // a no-op brightness filter. Magnification interpolates its texels.
        if self.brightness.is_some() && self.gradients.iter().any(|layer| layer.tile.is_some()) {
            self.texture = Texture::rasterize(area, |point| self.paint(point, area));
            if self.texture.is_none() {
                loss(
                    "filter",
                    "tiled filter surface exceeds the preview raster limit",
                );
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.opacity == 0.0 || (self.color[3] == 0.0 && self.gradients.is_empty())
    }

    pub fn has_images(&self) -> bool {
        !self.gradients.is_empty()
    }

    pub fn sample(&self, point: [f32; 2], size: [f32; 2]) -> [f32; 4] {
        self.texture
            .as_ref()
            .map_or_else(|| self.paint(point, size), |texture| texture.sample(point))
    }

    fn paint(&self, [x, y]: [f32; 2], [width, height]: [f32; 2]) -> [f32; 4] {
        let mut color = self.color;
        for layer in self.gradients.iter().rev() {
            let (point, size) = if let Some(tile) = &layer.tile {
                let Some(point) = tile.point([x, y]) else {
                    continue;
                };
                (point, tile.size)
            } else {
                ([x, y], [width, height])
            };
            color = over(
                layer.gradient.sample(point[0], point[1], size[0], size[1]),
                color,
            );
        }
        if let Some(brightness) = self.brightness {
            for channel in &mut color[..3] {
                *channel = (*channel * brightness).min(1.0);
            }
        }
        color[3] *= self.opacity;
        color
    }
}

struct Texture {
    width: usize,
    height: usize,
    premultiplied: Vec<[u8; 4]>,
}

impl Texture {
    fn rasterize(area: [f32; 2], paint: impl Fn([f32; 2]) -> [f32; 4]) -> Option<Self> {
        let [width, height] = area.map(|size| size.ceil() as usize);
        let count = width.checked_mul(height)?;
        if !(1..=262_144).contains(&count) {
            return None;
        }
        let mut premultiplied = Vec::with_capacity(count);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b, alpha] =
                    paint([(x as f32 + 0.5).min(area[0]), (y as f32 + 0.5).min(area[1])]);
                premultiplied
                    .push([r * alpha, g * alpha, b * alpha, alpha].map(super::raster::byte));
            }
        }
        Some(Self {
            width,
            height,
            premultiplied,
        })
    }

    fn sample(&self, [x, y]: [f32; 2]) -> [f32; 4] {
        let x = (x - 0.5).clamp(0.0, (self.width - 1) as f32);
        let y = (y - 0.5).clamp(0.0, (self.height - 1) as f32);
        let x0 = x as usize;
        let y0 = y as usize;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let fx = x - x0 as f32;
        let fy = y - y0 as f32;
        let [r, g, b, alpha] = std::array::from_fn(|channel| {
            let at =
                |x: usize, y: usize| f32::from(self.premultiplied[y * self.width + x][channel]);
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            top * (1.0 - fy) + bottom * fy
        });
        if alpha > 0.0 {
            [r / alpha, g / alpha, b / alpha, alpha / 255.0]
        } else {
            [0.0; 4]
        }
    }
}

struct BackgroundLayer {
    index: usize,
    gradient: Gradient,
    tile: Option<Tile>,
}

struct Tile {
    size: [f32; 2],
    origin: [f32; 2],
    repeat: [bool; 2],
}

impl Tile {
    fn point(&self, mut point: [f32; 2]) -> Option<[f32; 2]> {
        for (axis, coordinate) in point.iter_mut().enumerate() {
            *coordinate -= self.origin[axis];
            let size = self.size[axis];
            if self.repeat[axis] {
                *coordinate = coordinate.rem_euclid(size);
            } else if !(0.0..size).contains(coordinate) {
                return None;
            }
        }
        Some(point)
    }
}

fn background_size(value: &str, area: [f32; 2], lengths: LengthContext) -> Option<[f32; 2]> {
    // Gradients have no intrinsic size or aspect ratio.
    if matches!(value, "cover" | "contain") {
        return Some(area);
    }
    let mut parts = value_parts(value);
    let inputs = [parts.next()?, parts.next().unwrap_or("auto")];
    if parts.next().is_some() {
        return None;
    }
    let size = |axis| match inputs[axis] {
        "auto" => Some(area[axis]),
        value => LengthContext {
            percent: Some(area[axis]),
            ..lengths
        }
        .pixels(value)
        .filter(|value| *value >= 0.0),
    };
    Some([size(0)?, size(1)?])
}

fn background_position(value: &str, room: [f32; 2], lengths: LengthContext) -> Option<[f32; 2]> {
    let mut parts = value_parts(value);
    let first = parts.next()?;
    let vertical = |part| matches!(part, "top" | "bottom");
    let horizontal = |part| matches!(part, "left" | "right");
    let (x, y) = match parts.next() {
        None if vertical(first) => ("center", first),
        None => (first, "center"),
        Some(second)
            if (vertical(first) && (horizontal(second) || second == "center"))
                || (first == "center" && horizontal(second)) =>
        {
            (second, first)
        }
        Some(second) => (first, second),
    };
    if parts.next().is_some() {
        return None;
    }
    let resolve = |axis: usize, value| match (axis, value) {
        (0, "left") | (1, "top") => Some(0.0),
        (0, "right") | (1, "bottom") => Some(room[axis]),
        (_, "center") => Some(room[axis] * 0.5),
        _ => LengthContext {
            percent: Some(room[axis]),
            ..lengths
        }
        .pixels(value),
    };
    Some([resolve(0, x)?, resolve(1, y)?])
}

fn background_repeat(value: &str) -> Option<[bool; 2]> {
    match value {
        "repeat-x" => Some([true, false]),
        "repeat-y" => Some([false, true]),
        _ => {
            let mut parts = value_parts(value);
            let first = parts.next()?;
            let second = parts.next().unwrap_or(first);
            if parts.next().is_some() {
                return None;
            }
            let axis = |value| match value {
                "repeat" => Some(true),
                "no-repeat" => Some(false),
                _ => None,
            };
            Some([axis(first)?, axis(second)?])
        }
    }
}

pub(super) fn number(value: &str) -> Option<f32> {
    let number = if let Some(percent) = value.strip_suffix('%') {
        percent.parse::<f32>().ok()? / 100.0
    } else {
        value.parse::<f32>().ok()?
    };
    number.is_finite().then_some(number)
}
