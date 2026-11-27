mod glow;

use std::{error::Error, path::Path, sync::Arc};

use css_gpui::{
    ir::{Position, SceneIr},
    paint::{
        Backdrop, Background, BoxShadow, ImagePixels, Rect, background_image_urls, blur_radius,
        composite_surface, rgba, rounded_coverage, stamp,
    },
};
use gpui::RenderImage;
use image::{Frame, RgbaImage};

struct Asset {
    url: String,
    dimensions: (u32, u32),
    pixels: Vec<[u8; 4]>,
}

impl Asset {
    fn load(directory: &Path, url: &str) -> Result<Self, Box<dyn Error>> {
        let file = url.rsplit('/').next().ok_or("invalid asset URL")?;
        if file.is_empty() || file == "." || file == ".." || file.contains('\\') {
            return Err("invalid asset name".into());
        }
        let image = image::open(directory.join(file))?.into_rgba8();
        Ok(Self {
            url: url.to_owned(),
            dimensions: image.dimensions(),
            pixels: image.pixels().map(|pixel| pixel.0).collect(),
        })
    }

    fn image(&self) -> ImagePixels<'_> {
        ImagePixels {
            width: self.dimensions.0,
            height: self.dimensions.1,
            rgba: &self.pixels,
        }
    }
}

/// Composites the CSS stage and glass using the current scene and panel offsets.
pub struct GlassComposer {
    scene: SceneIr,
    panel_index: usize,
    photo: Asset,
    noise: Asset,
    blur: f32,
    shadows: Vec<BoxShadow>,
}

pub struct CompositedFrame {
    pub stage: RgbaImage,
    pub effects: RgbaImage,
    pub glow_layers: usize,
}

impl GlassComposer {
    pub fn load(scene: SceneIr, assets: &Path) -> Result<Self, Box<dyn Error>> {
        let body = scene
            .canvas
            .as_ref()
            .ok_or("scene is missing the authored backdrop")?;
        let panel_index = scene
            .nodes
            .iter()
            .position(|node| {
                node.layout.position == Position::Fixed && blur_radius(&node.paint).is_some()
            })
            .ok_or("scene is missing its glass panel")?;
        let panel = &scene.nodes[panel_index];
        let blur = blur_radius(&panel.paint).unwrap();
        let shadows = BoxShadow::parse_list(
            panel
                .paint
                .effects
                .get("box-shadow")
                .ok_or("panel needs a box shadow")?,
        )
        .ok_or("unsupported panel shadow")?;
        let photo_url = background_image_urls(body)
            .next()
            .ok_or("scene is missing its background image")?;
        let photo = Asset::load(assets, photo_url)?;
        let noise_url = scene
            .nodes
            .iter()
            .filter_map(|node| node.paint.effects.get("mask"))
            .find_map(|value| {
                Some(
                    value
                        .strip_prefix("url(")?
                        .strip_suffix(')')?
                        .trim_matches(['\'', '"']),
                )
            })
            .ok_or("scene is missing the luminance mask")?;
        let noise = Asset::load(assets, noise_url)?;
        Ok(Self {
            scene,
            panel_index,
            photo,
            noise,
            blur,
            shadows,
        })
    }

    pub fn rasterize(
        &self,
        dimensions: (u32, u32),
        panel: Rect,
        offset: (f32, f32),
    ) -> Result<CompositedFrame, String> {
        let (width, height) = dimensions;
        let body = self.scene.canvas.as_ref().unwrap();
        let menu = &self.scene.nodes[self.panel_index];
        let radius = menu.paint.border_radius.unwrap_or(0.0);
        let backdrop = Background::new(
            body,
            |url| (url == self.photo.url).then(|| self.photo.image()),
            offset,
        )?;
        let foreground = Background::new(&menu.paint, |_| None, (0.0, 0.0))?;
        let mut frame = backdrop.rasterize(width, height);
        let stage = rgba_image(width, height, &frame);
        let blur =
            Backdrop::new(width, height, &frame, self.blur).ok_or("invalid backdrop dimensions")?;
        for shadow in self.shadows.iter().rev() {
            shadow.paint(&mut frame, width, height, panel, radius);
        }
        let border = menu
            .paint
            .border
            .as_ref()
            .and_then(|border| Some((border.width, rgba(&border.color.0)?)));
        let glass = composite_surface(&blur, panel, radius, |x, y, w, h| {
            let tint = foreground.sample(x, y, w, h);
            let Some((width, border)) = border else {
                return tint;
            };
            let coverage = 1.0
                - rounded_coverage(
                    x - width,
                    y - width,
                    w - 2.0 * width,
                    h - 2.0 * width,
                    (radius - width).max(0.0),
                );
            let alpha = coverage * f32::from(border[3]) / 255.0;
            let composed_alpha = alpha + tint[3] * (1.0 - alpha);
            let mut output = [0.0, 0.0, 0.0, composed_alpha];
            if composed_alpha > 0.0 {
                for channel in 0..3 {
                    output[channel] = (alpha * f32::from(border[channel]) / 255.0
                        + tint[channel] * tint[3] * (1.0 - alpha))
                        / composed_alpha;
                }
            }
            output
        })
        .ok_or("panel dimensions overflow")?;
        stamp(&mut frame, width, height, panel, &glass);
        let glow_layers = glow::paint_glow(
            &mut frame,
            dimensions,
            panel,
            &self.scene,
            menu,
            self.noise.image(),
            &self.noise.url,
        )?;
        Ok(CompositedFrame {
            stage,
            effects: rgba_image(width, height, &frame),
            glow_layers,
        })
    }
}

pub fn gpu_texture(mut image: RgbaImage) -> Arc<RenderImage> {
    for pixel in image.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(image)]))
}

fn rgba_image(width: u32, height: u32, pixels: &[[u8; 4]]) -> RgbaImage {
    RgbaImage::from_raw(width, height, pixels.iter().flatten().copied().collect()).unwrap()
}
