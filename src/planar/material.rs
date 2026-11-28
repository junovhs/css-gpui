use std::collections::BTreeMap;

use crate::paint::{Gradient, layers, rgba};

use super::{css_value, raster::over};

pub(super) struct Material {
    pub color: [f32; 4],
    gradients: Vec<Gradient>,
    brightness: f32,
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
        let size = css_value(effects, "background-size", "auto");
        let position = css_value(effects, "background-position", "0% 0%");
        let default_mapping = size == "auto" && matches!(position, "0% 0%" | "0 0");
        let mut gradients = Vec::new();
        let mut background_layers = layers(image.map_or(shorthand, String::as_str)).peekable();
        while let Some(layer) = background_layers.next() {
            if layer == "none"
                || (image.is_none()
                    && background_layers.peek().is_none()
                    && parse_color(layer).is_some())
            {
                continue;
            }
            let gradient = default_mapping
                .then(|| Gradient::parse(layer))
                .flatten()
                .filter(|_| layer.ends_with(')'));
            if let Some(gradient) = gradient {
                gradients.push(gradient);
            } else {
                loss("background-image", layer);
            }
        }
        let filter = css_value(effects, "filter", "none");
        let brightness = if filter == "none" {
            1.0
        } else {
            filter
                .strip_prefix("brightness(")
                .and_then(|value| value.strip_suffix(')'))
                .and_then(number)
                .filter(|&value| value >= 0.0)
                .unwrap_or_else(|| {
                    loss("filter", filter);
                    1.0
                })
        };
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
            brightness,
            opacity,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.opacity == 0.0 || (self.color[3] == 0.0 && self.gradients.is_empty())
    }

    pub fn has_images(&self) -> bool {
        !self.gradients.is_empty()
    }

    pub fn sample(&self, [x, y]: [f32; 2], [width, height]: [f32; 2]) -> [f32; 4] {
        let mut color = self.color;
        for gradient in self.gradients.iter().rev() {
            color = over(gradient.sample(x, y, width, height), color);
        }
        for channel in &mut color[..3] {
            *channel = (*channel * self.brightness).min(1.0);
        }
        color[3] *= self.opacity;
        color
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
