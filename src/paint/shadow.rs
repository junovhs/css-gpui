use super::{Backdrop, Rect, backdrop::rounded_coverage, layers, rgba, stamp};

/// An outer CSS box shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread: f32,
    pub color: [u8; 4],
}

impl BoxShadow {
    pub fn parse_list(value: &str) -> Option<Vec<Self>> {
        if value == "none" {
            return Some(Vec::new());
        }
        layers(value).map(Self::parse).collect()
    }

    fn parse(input: &str) -> Option<Self> {
        let (color, sizes) = if let Some(open) = input.find('(') {
            let start = input[..open]
                .rfind(char::is_whitespace)
                .map_or(0, |index| index + 1);
            let end = input.find(')')? + 1;
            (
                rgba(&input[start..end])?,
                format!("{} {}", &input[..start], &input[end..]),
            )
        } else {
            let mut tokens: Vec<_> = input.split_ascii_whitespace().collect();
            let index = tokens.iter().position(|token| rgba(token).is_some())?;
            (rgba(tokens.remove(index))?, tokens.join(" "))
        };
        let sizes: Vec<_> = sizes
            .split_ascii_whitespace()
            .map(|value| {
                if value == "0" {
                    Some(0.0)
                } else {
                    value.strip_suffix("px")?.parse().ok()
                }
            })
            .collect::<Option<_>>()?;
        let (x, y, blur, spread) = match sizes.as_slice() {
            [x, y] => (*x, *y, 0.0_f32, 0.0),
            [x, y, blur] => (*x, *y, *blur, 0.0),
            [x, y, blur, spread] => (*x, *y, *blur, *spread),
            _ => return None,
        };
        if ![x, y, blur, spread].iter().all(|value| value.is_finite()) || blur < 0.0 {
            return None;
        }
        Some(Self {
            offset_x: x,
            offset_y: y,
            blur_radius: blur,
            spread,
            color,
        })
    }

    /// Composite the shadow behind a box. CSS blur radius is twice the
    /// approximate standard deviation of its blur kernel.
    pub fn paint(self, frame: &mut [[u8; 4]], width: u32, height: u32, bounds: Rect, radius: f32) {
        let shape_width = bounds.width as f32 + 2.0 * self.spread;
        let shape_height = bounds.height as f32 + 2.0 * self.spread;
        if shape_width <= 0.0 || shape_height <= 0.0 {
            return;
        }
        let sigma = self.blur_radius / 2.0;
        let pad = (sigma * 3.0).ceil() + 2.0;
        let origin_x = bounds.x as f32 + self.offset_x - self.spread;
        let origin_y = bounds.y as f32 + self.offset_y - self.spread;
        let x = (origin_x - pad).floor().max(0.0) as i32;
        let y = (origin_y - pad).floor().max(0.0) as i32;
        let right = (origin_x + shape_width + pad).ceil().min(width as f32) as i32;
        let bottom = (origin_y + shape_height + pad).ceil().min(height as f32) as i32;
        if right <= x || bottom <= y {
            return;
        }
        let width_px = (right - x) as u32;
        let height_px = (bottom - y) as u32;
        let mut mask = Vec::with_capacity((width_px * height_px) as usize);
        for dy in 0..height_px {
            for dx in 0..width_px {
                let alpha = rounded_coverage(
                    x as f32 + dx as f32 + 0.5 - origin_x,
                    y as f32 + dy as f32 + 0.5 - origin_y,
                    shape_width,
                    shape_height,
                    radius + self.spread,
                );
                mask.push([0, 0, 0, (alpha * 255.0).round() as u8]);
            }
        }
        let blur = Backdrop::new(width_px, height_px, &mask, sigma).unwrap();
        for dy in 0..height_px {
            for dx in 0..width_px {
                let alpha = u16::from(blur.sample(dx as i32, dy as i32)[3])
                    * u16::from(self.color[3])
                    / 255;
                let [r, g, b, _] = self.color;
                mask[(dy * width_px + dx) as usize] = [r, g, b, alpha as u8];
            }
        }
        stamp(
            frame,
            width,
            height,
            Rect {
                x,
                y,
                width: width_px,
                height: height_px,
            },
            &mask,
        );
    }
}
