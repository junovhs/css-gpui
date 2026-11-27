/// A rectangle in source-image pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A blurred scene snapshot. Reuse it when a panel moves across an unchanged
/// scene and rebuild it when content behind the panel changes.
pub struct Backdrop {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
}

impl Backdrop {
    /// Prepare a blur in premultiplied sRGB, sampling the whole scene before
    /// applying the panel clip.
    pub fn new(width: u32, height: u32, rgba: &[[u8; 4]], sigma: f32) -> Option<Self> {
        let (width, height) = (width as usize, height as usize);
        if width == 0
            || height == 0
            || width.checked_mul(height)? != rgba.len()
            || !sigma.is_finite()
            || sigma < 0.0
        {
            return None;
        }

        let mut pixels: Vec<_> = rgba.iter().map(|&pixel| premultiply(pixel)).collect();
        if sigma > 0.0 {
            let mut scratch = vec![[0.0; 4]; pixels.len()];
            for radius in box_radii(sigma) {
                blur_lines(&pixels, &mut scratch, width, height, 1, width, radius);
                blur_lines(&scratch, &mut pixels, height, width, width, 1, radius);
            }
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn sample(&self, x: i32, y: i32) -> [u8; 4] {
        quantize(self.sample_premultiplied(x, y))
    }

    fn sample_premultiplied(&self, x: i32, y: i32) -> [f32; 4] {
        let x = (x.max(0) as usize).min(self.width - 1);
        let y = (y.max(0) as usize).min(self.height - 1);
        self.pixels[y * self.width + x]
    }
}

/// Make a transparent panel texture containing the sampled backdrop and a
/// translucent fill. The texture can be painted over the scene in GPUI.
pub fn composite_panel(
    backdrop: &Backdrop,
    bounds: Rect,
    corner_radius: f32,
    fill: [u8; 4],
) -> Option<Vec<[u8; 4]>> {
    composite_surface(backdrop, bounds, corner_radius, |_, _, _, _| {
        fill.map(|value| f32::from(value) / 255.0)
    })
}

/// Composite a sampled foreground gradient or fill over the live backdrop.
pub fn composite_surface(
    backdrop: &Backdrop,
    bounds: Rect,
    corner_radius: f32,
    foreground: impl Fn(f32, f32, f32, f32) -> [f32; 4],
) -> Option<Vec<[u8; 4]>> {
    let (width, height) = (bounds.width as usize, bounds.height as usize);
    let mut result = Vec::with_capacity(width.checked_mul(height)?);
    for y in 0..height {
        for x in 0..width {
            let coverage = rounded_coverage(
                x as f32 + 0.5,
                y as f32 + 0.5,
                width as f32,
                height as f32,
                corner_radius,
            );
            let mut fill = foreground(x as f32 + 0.5, y as f32 + 0.5, width as f32, height as f32);
            let alpha = fill[3];
            for channel in &mut fill[..3] {
                *channel *= alpha;
            }
            let behind = backdrop.sample_premultiplied(
                bounds.x.saturating_add(x as i32),
                bounds.y.saturating_add(y as i32),
            );
            result.push(quantize(std::array::from_fn(|channel| {
                coverage * (fill[channel] + behind[channel] * (1.0 - fill[3]))
            })));
        }
    }
    Some(result)
}

/// Fractional coverage of a pixel centered at (x, y) in a rounded rectangle.
pub fn rounded_coverage(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    let radius = radius.max(0.0).min(width.min(height) * 0.5);
    let dx = (x - width * 0.5).abs() - (width * 0.5 - radius);
    let dy = (y - height * 0.5).abs() - (height * 0.5 - radius);
    let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn premultiply(pixel: [u8; 4]) -> [f32; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    std::array::from_fn(|channel| {
        if channel == 3 {
            alpha
        } else {
            f32::from(pixel[channel]) * (alpha / 255.0)
        }
    })
}

fn quantize(pixel: [f32; 4]) -> [u8; 4] {
    let alpha = pixel[3].clamp(0.0, 1.0);
    if alpha <= 0.5 / 255.0 {
        return [0; 4];
    }
    std::array::from_fn(|channel| {
        let value = if channel == 3 {
            alpha
        } else {
            pixel[channel] / alpha
        };
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    })
}

// Three box convolutions approximate the Gaussian with the requested variance.
// Their sliding sums make each pass linear in frame size, regardless of radius.
fn box_radii(sigma: f32) -> [usize; 3] {
    let variance = f64::from(sigma.min(1_000_000.0)).powi(2);
    let ideal = (4.0 * variance + 1.0).sqrt();
    let low = (ideal.floor() as usize).saturating_sub(1) | 1;
    let low = if low as f64 > ideal {
        low.saturating_sub(2).max(1)
    } else {
        low
    };
    let width = low as f64;
    let low_count = ((12.0 * variance - 3.0 * width * width - 12.0 * width - 9.0)
        / (-4.0 * width - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    std::array::from_fn(|index| low / 2 + usize::from(index >= low_count))
}

fn blur_lines(
    input: &[[f32; 4]],
    output: &mut [[f32; 4]],
    length: usize,
    lines: usize,
    stride: usize,
    line_stride: usize,
    radius: usize,
) {
    let scale = 1.0 / (2 * radius + 1) as f32;
    for line in 0..lines {
        let start = line * line_stride;
        let pixel = |position: usize| input[start + position * stride];
        let mut sum = pixel(0).map(|channel| channel * (radius + 1) as f32);
        let end = radius.min(length - 1);
        for index in 1..=end {
            let next = pixel(index);
            for channel in 0..4 {
                sum[channel] += next[channel];
            }
        }
        let last = pixel(length - 1);
        for channel in 0..4 {
            sum[channel] += last[channel] * (radius - end) as f32;
        }

        for index in 0..length {
            output[start + index * stride] = sum.map(|channel| channel * scale);
            let leaving = pixel(index.saturating_sub(radius));
            let entering = pixel((index + radius + 1).min(length - 1));
            for channel in 0..4 {
                sum[channel] += entering[channel] - leaving[channel];
            }
        }
    }
}
