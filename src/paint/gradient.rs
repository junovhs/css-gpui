use crate::resolver::cascade::parse_color;

/// Parse a computed background value into comma-separated layers, with the
/// topmost layer first. Commas inside functions or URLs stay with their layer.
pub fn layers(value: &str) -> impl Iterator<Item = &str> {
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    value
        .char_indices()
        .chain(std::iter::once((value.len(), ',')))
        .filter_map(move |(index, ch)| {
            if escaped {
                escaped = false;
                return None;
            }
            if ch == '\\' {
                escaped = true;
                return None;
            }
            if let Some(active) = quote {
                if ch == active {
                    quote = None;
                }
                return None;
            }
            match ch {
                '\'' | '"' => quote = Some(ch),
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    let part = value[start..index].trim();
                    start = index + 1;
                    return Some(part);
                }
                _ => {}
            }
            None
        })
}

pub fn rgba(value: &str) -> Option<[u8; 4]> {
    let normalized = match value {
        "black" => "#000".to_owned(),
        "white" => "#fff".to_owned(),
        _ => parse_color(value)?,
    };
    let mut result = [0, 0, 0, 255];
    let hex = normalized.strip_prefix('#')?;
    match hex.len() {
        3 | 4 => {
            for (index, digit) in hex.bytes().enumerate() {
                result[index] = (digit as char).to_digit(16)? as u8 * 17;
            }
        }
        6 | 8 => {
            for (index, value) in result.iter_mut().take(hex.len() / 2).enumerate() {
                *value = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
            }
        }
        _ => return None,
    }
    Some(result)
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Linear(f32),
    Conic(f32),
}

/// A resolved CSS linear or conic gradient. Angles use CSS's clockwise
/// convention with zero pointing toward the top of the box.
#[derive(Clone, Debug)]
pub struct Gradient {
    shape: Shape,
    oklab: bool,
    stops: Vec<(f32, [f32; 4])>,
}

impl Gradient {
    pub fn parse(input: &str) -> Option<Self> {
        let (value, conic) = if let Some(value) = input.strip_prefix("linear-gradient(") {
            (value, false)
        } else {
            (input.strip_prefix("conic-gradient(")?, true)
        };
        let mut parts: Vec<_> = layers(&value[..value.rfind(')')?]).collect();
        let mut angle = if conic { 0.0 } else { 180.0 };
        let mut oklab = false;
        if let Some(first) = parts.first().copied()
            && split_stop(first, conic).is_none()
        {
            let (direction, interpolation) = first.rsplit_once(" in ").unwrap_or((first, "srgb"));
            let direction = direction.trim();
            oklab = matches!(interpolation.trim(), "oklab" | "oklch");
            if conic {
                let direction = direction.strip_prefix("from ").unwrap_or(direction);
                let direction = direction.strip_suffix(" at center").unwrap_or(direction);
                if !direction.is_empty() && direction != "at center" {
                    angle = degrees(direction)?;
                }
            } else {
                angle = match direction {
                    "to top" => 0.0,
                    "to right" => 90.0,
                    "to bottom" => 180.0,
                    "to left" => 270.0,
                    _ => degrees(direction)?,
                };
            }
            parts.remove(0);
        }
        let mut parsed: Vec<_> = parts
            .into_iter()
            .map(|part| split_stop(part, conic))
            .collect::<Option<_>>()?;
        parsed.first_mut()?.0.get_or_insert(0.0);
        parsed.last_mut()?.0.get_or_insert(1.0);
        let mut start = 0;
        while start + 1 < parsed.len() {
            let end = (start + 1..parsed.len()).find(|&index| parsed[index].0.is_some())?;
            let lo = parsed[start].0?;
            let hi = parsed[end].0?.max(lo);
            for (index, entry) in parsed[start + 1..=end].iter_mut().enumerate() {
                entry.0 = Some(lo + (hi - lo) * (index + 1) as f32 / (end - start) as f32);
            }
            start = end;
        }
        let stops = parsed
            .into_iter()
            .map(|(position, rgba)| {
                let [r, g, b, alpha] = rgba.map(|channel| f32::from(channel) / 255.0);
                let [r, g, b] = if oklab {
                    rgb_to_oklab([r, g, b])
                } else {
                    [r, g, b]
                };
                (position.unwrap(), [r * alpha, g * alpha, b * alpha, alpha])
            })
            .collect();
        Some(Self {
            shape: if conic {
                Shape::Conic(angle.to_radians())
            } else {
                Shape::Linear(angle.to_radians())
            },
            oklab,
            stops,
        })
    }

    /// Sample a point in the layer rectangle, in CSS pixels.
    pub fn sample(&self, x: f32, y: f32, width: f32, height: f32) -> [f32; 4] {
        let dx = x - width * 0.5;
        let dy = y - height * 0.5;
        let progress = match self.shape {
            Shape::Linear(angle) => {
                let (sin, cos) = angle.sin_cos();
                let length = sin.abs() * width + cos.abs() * height;
                if length > 0.0 {
                    0.5 + (dx * sin - dy * cos) / length
                } else {
                    0.5
                }
            }
            Shape::Conic(angle) => {
                (dx.atan2(-dy) - angle).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU
            }
        };
        let upper = self
            .stops
            .partition_point(|(position, _)| *position <= progress);
        let value = if upper == 0 {
            self.stops[0].1
        } else if upper == self.stops.len() {
            self.stops[upper - 1].1
        } else {
            let (lo, start) = self.stops[upper - 1];
            let (hi, end) = self.stops[upper];
            let ratio = (progress - lo) / (hi - lo);
            std::array::from_fn(|channel| start[channel] + ratio * (end[channel] - start[channel]))
        };
        let alpha = value[3];
        if alpha <= 0.0 {
            return [0.0; 4];
        }
        let rgb = [value[0] / alpha, value[1] / alpha, value[2] / alpha];
        let [r, g, b] = if self.oklab { oklab_to_rgb(rgb) } else { rgb };
        [
            r.clamp(0.0, 1.0),
            g.clamp(0.0, 1.0),
            b.clamp(0.0, 1.0),
            alpha,
        ]
    }
}

fn split_stop(input: &str, conic: bool) -> Option<(Option<f32>, [u8; 4])> {
    let end = if let Some(end) = input.find(')') {
        end + 1
    } else {
        input.find(char::is_whitespace).unwrap_or(input.len())
    };
    let color = rgba(&input[..end])?;
    let position = input[end..].trim();
    let position = match position {
        "" => None,
        "0" => Some(0.0),
        _ => Some(if let Some(percentage) = position.strip_suffix('%') {
            percentage.parse::<f32>().ok()? / 100.0
        } else if conic {
            degrees(position)? / 360.0
        } else {
            return None;
        }),
    };
    if position.is_some_and(|value| !value.is_finite()) {
        return None;
    }
    Some((position, color))
}

fn degrees(input: &str) -> Option<f32> {
    let degrees = if let Some(value) = input.strip_suffix("deg") {
        value.parse::<f32>().ok()?
    } else if let Some(value) = input.strip_suffix("turn") {
        value.parse::<f32>().ok()? * 360.0
    } else if let Some(value) = input.strip_suffix("rad") {
        value.parse::<f32>().ok()?.to_degrees()
    } else if input == "0" {
        0.0
    } else {
        return None;
    };
    degrees.is_finite().then_some(degrees)
}

// OKLab conversion from the reference matrices in CSS Color 4.
fn rgb_to_oklab(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|x| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn oklab_to_rgb([lightness, a, b]: [f32; 3]) -> [f32; 3] {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
    .map(|x| {
        if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        }
    })
}
