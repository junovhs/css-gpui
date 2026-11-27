use std::collections::BTreeMap;

use css_gpui::{
    ir::{IrNode, Paint, SceneIr},
    paint::{Backdrop, Background, ImagePixels, Rect, rounded_coverage, stamp},
};

type Declarations = BTreeMap<String, String>;

#[derive(Clone, Copy)]
struct BoxBounds {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl BoxBounds {
    fn inset(self, amount: f32) -> Self {
        Self {
            x: self.x + amount,
            y: self.y + amount,
            width: self.width - 2.0 * amount,
            height: self.height - 2.0 * amount,
        }
    }

    fn positioned(self, style: &Declarations) -> Self {
        let width = self.width;
        let height = self.height;
        let left = length(style, "left", width);
        let right = length(style, "right", width);
        let top = length(style, "top", height);
        let bottom = length(style, "bottom", height);
        let ratio = number(style, "aspect-ratio", 0.0);
        let w = length(style, "width", width)
            .or_else(|| {
                let (left, right) = left.zip(right)?;
                Some(width - left - right)
            })
            .unwrap_or(0.0);
        let h = length(style, "height", height)
            .or_else(|| {
                let (top, bottom) = top.zip(bottom)?;
                Some(height - top - bottom)
            })
            .unwrap_or_else(|| if ratio > 0.0 { w / ratio } else { 0.0 });
        Self {
            x: self.x
                + left
                    .or_else(|| right.map(|right| width - right - w))
                    .unwrap_or(0.0),
            y: self.y
                + top
                    .or_else(|| bottom.map(|bottom| height - bottom - h))
                    .unwrap_or(0.0),
            width: w.max(0.0),
            height: h.max(0.0),
        }
    }

    fn raster(self) -> Rect {
        let x = self.x.floor() as i32;
        let y = self.y.floor() as i32;
        Rect {
            x,
            y,
            width: ((self.x + self.width).ceil() as i32 - x).max(0) as u32,
            height: ((self.y + self.height).ceil() as i32 - y).max(0) as u32,
        }
    }
}

pub fn paint_glow(
    frame: &mut [[u8; 4]],
    dimensions: (u32, u32),
    panel: Rect,
    scene: &SceneIr,
    menu: &IrNode,
    noise: ImagePixels<'_>,
    noise_url: &str,
) -> Result<usize, String> {
    let radius = menu.paint.border_radius.unwrap_or(0.0);
    let panel = BoxBounds {
        x: panel.x as f32,
        y: panel.y as f32,
        width: panel.width as f32,
        height: panel.height as f32,
    };
    let menu_border = menu
        .paint
        .border
        .as_ref()
        .map_or(0.0, |border| border.width);
    let parent = panel.inset(menu_border);
    let mut groups: Vec<_> = scene
        .nodes
        .iter()
        .filter(|node| {
            node.parent_id.as_deref() == Some(menu.id.as_str())
                && !node.paint.placement.is_empty()
                && (node.paint.effects.contains_key("mask") || !node.paint.decorations.is_empty())
        })
        .collect();
    groups.sort_by_key(|node| number(&node.paint.placement, "z-index", 0.0) as i32);
    for node in &groups {
        paint_group(
            frame,
            dimensions,
            parent,
            radius,
            &node.paint,
            noise,
            noise_url,
        )?;
    }
    Ok(groups.len())
}

fn paint_group(
    frame: &mut [[u8; 4]],
    (width, height): (u32, u32),
    parent: BoxBounds,
    parent_radius: f32,
    paint: &Paint,
    noise: ImagePixels<'_>,
    noise_url: &str,
) -> Result<(), String> {
    let bounds = parent.positioned(&paint.placement);
    if bounds.width == 0.0 || bounds.height == 0.0 {
        return Ok(());
    }
    let border = paint.border.as_ref().map_or(0.0, |border| border.width);
    let radius = paint.border_radius.unwrap_or(parent_radius);
    let filter = paint.effects.get("filter").map_or("", String::as_str);
    let blur = filter_arg(filter, "blur")
        .and_then(|value| value.strip_suffix("px")?.parse::<f32>().ok())
        .unwrap_or(0.0);
    let brightness = filter_arg(filter, "brightness")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0);
    let saturation = filter_arg(filter, "saturate")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0);
    let opacity = number(&paint.effects, "opacity", 1.0).clamp(0.0, 1.0);
    let buffer = bounds.inset(-(3.0 * blur + parent_radius)).raster();
    let mut texture = vec![[0; 4]; buffer.width as usize * buffer.height as usize];

    paint_layer(&mut texture, buffer, bounds, paint, false, border, radius)?;
    let inner = bounds.inset(border);
    for decoration in &paint.decorations {
        let bounds = inner.positioned(&decoration.placement);
        let decor = Paint {
            background_color: None,
            border: None,
            border_radius: None,
            cursor: None,
            effects: decoration.effects.clone(),
            placement: decoration.placement.clone(),
            decorations: Vec::new(),
            interaction: None,
        };
        paint_layer(
            &mut texture,
            buffer,
            bounds,
            &decor,
            true,
            decoration.border_width,
            if decoration.radius > 0.0 {
                decoration.radius
            } else {
                radius
            },
        )?;
    }

    let blur_pixels = Backdrop::new(buffer.width, buffer.height, &texture, blur)
        .ok_or("invalid glow filter dimensions")?;
    let mask = paint.effects.get("mask").map_or("", String::as_str);
    let box_ring = is_ring(&paint.effects);
    let noise_mask = mask
        .strip_prefix("url(")
        .and_then(|value| value.strip_suffix(')'))
        .is_some_and(|url| url.trim_matches(['\'', '"']) == noise_url);
    let mask_size = length(&paint.effects, "mask-size", bounds.width)
        .unwrap_or(bounds.width)
        .max(1.0);
    let additive = paint
        .effects
        .get("mix-blend-mode")
        .is_some_and(|value| value == "plus-lighter");
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let fx = buffer.x as f32 + x as f32 + 0.5 - bounds.x;
            let fy = buffer.y as f32 + y as f32 + 0.5 - bounds.y;
            let mut color = blur_pixels.sample(x as i32, y as i32);
            let coverage = if box_ring {
                ring_coverage(fx, fy, bounds, radius, border)
            } else if noise_mask
                && (0.0..bounds.width).contains(&fx)
                && (0.0..bounds.height).contains(&fy)
            {
                let nx = (fx.rem_euclid(mask_size) / mask_size * noise.width as f32) as u32;
                let ny = (fy.rem_euclid(mask_size) / mask_size * noise.height as f32) as u32;
                let mask = noise.rgba
                    [(ny.min(noise.height - 1) * noise.width + nx.min(noise.width - 1)) as usize];
                luminance(mask) / 255.0 * (f32::from(mask[3]) / 255.0)
            } else if noise_mask {
                0.0
            } else {
                1.0
            };
            let neutral = luminance(color);
            for channel in &mut color[..3] {
                *channel = ((neutral + saturation * (f32::from(*channel) - neutral)) * brightness)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            color[3] = (f32::from(color[3]) * coverage * opacity).round() as u8;
            texture[(y * buffer.width + x) as usize] = color;
        }
    }
    if additive {
        add(frame, width, height, buffer, &texture);
    } else {
        stamp(frame, width, height, buffer, &texture);
    }
    Ok(())
}

fn paint_layer(
    texture: &mut [[u8; 4]],
    enclosing: Rect,
    bounds: BoxBounds,
    paint: &Paint,
    sample_ring: bool,
    border: f32,
    radius: f32,
) -> Result<(), String> {
    if !paint.effects.contains_key("background") && !paint.effects.contains_key("background-image")
    {
        return Ok(());
    }
    let fill = Background::new(paint, |_| None, (0.0, 0.0))?;
    let box_ring = is_ring(&paint.effects) && sample_ring;
    let opacity = number(&paint.effects, "opacity", 1.0).clamp(0.0, 1.0);
    let raster = bounds.raster();
    let mut fg = Vec::with_capacity((raster.width * raster.height) as usize);
    for y in 0..raster.height {
        for x in 0..raster.width {
            let fx = raster.x as f32 + x as f32 + 0.5 - bounds.x;
            let fy = raster.y as f32 + y as f32 + 0.5 - bounds.y;
            let mut color = fill.sample(fx, fy, bounds.width, bounds.height);
            color[3] *= opacity
                * if box_ring {
                    ring_coverage(fx, fy, bounds, radius, border)
                } else {
                    corner_coverage(fx, fy, bounds.width, bounds.height, radius)
                };
            fg.push(color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    stamp(
        texture,
        enclosing.width,
        enclosing.height,
        Rect {
            x: raster.x - enclosing.x,
            y: raster.y - enclosing.y,
            ..raster
        },
        &fg,
    );
    Ok(())
}

fn is_ring(effects: &Declarations) -> bool {
    effects
        .get("mask-composite")
        .is_some_and(|value| value == "subtract")
        && effects.get("mask").is_some_and(|value| value != "none")
}

fn ring_coverage(x: f32, y: f32, bounds: BoxBounds, radius: f32, border: f32) -> f32 {
    let outside = corner_coverage(x, y, bounds.width, bounds.height, radius);
    let inside = corner_coverage(
        x - border,
        y - border,
        bounds.width - 2.0 * border,
        bounds.height - 2.0 * border,
        (radius - border).max(0.0),
    );
    (outside - inside).max(0.0)
}

fn corner_coverage(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    if width <= 0.0 || height <= 0.0 {
        return 0.0;
    }
    let rounded = (x > width * 0.5 && y < height * 0.5) || (x < width * 0.5 && y > height * 0.5);
    let radius = if rounded {
        radius.min(width.min(height) * 0.5)
    } else {
        0.0
    };
    rounded_coverage(x, y, width, height, radius)
}

fn length(style: &Declarations, property: &str, basis: f32) -> Option<f32> {
    let value = style.get(property)?;
    let amount = if let Some(value) = value.strip_suffix('%') {
        value.parse::<f32>().ok()? * basis / 100.0
    } else {
        value.strip_suffix("px").unwrap_or(value).parse().ok()?
    };
    amount.is_finite().then_some(amount)
}

fn number(style: &Declarations, property: &str, default: f32) -> f32 {
    style
        .get(property)
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

fn filter_arg<'a>(filter: &'a str, name: &str) -> Option<&'a str> {
    let tail = filter.split_once(name)?.1.strip_prefix('(')?;
    Some(tail.split_once(')')?.0)
}

fn luminance(pixel: [u8; 4]) -> f32 {
    0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2])
}

fn add(frame: &mut [[u8; 4]], width: u32, height: u32, bounds: Rect, source: &[[u8; 4]]) {
    for y in 0..bounds.height as i32 {
        for x in 0..bounds.width as i32 {
            let dx = bounds.x + x;
            let dy = bounds.y + y;
            if (0..width as i32).contains(&dx) && (0..height as i32).contains(&dy) {
                let front = source[(y * bounds.width as i32 + x) as usize];
                let back = &mut frame[dy as usize * width as usize + dx as usize];
                for (destination, source) in back[..3].iter_mut().zip(front) {
                    *destination = destination
                        .saturating_add((u16::from(source) * u16::from(front[3]) / 255) as u8);
                }
            }
        }
    }
}
