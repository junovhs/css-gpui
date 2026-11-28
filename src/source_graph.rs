mod css;
mod html;
mod html_support;
mod model;

#[cfg(test)]
mod tests;

use crate::fixture_manifest::{FixtureSceneEntry, load_fixture_manifest};
use css::parse_css_region;
use html::parse_html_document;
pub use model::{
    CssDeclaration, CssRule, HtmlNode, HtmlNodeKind, SceneSourceGraph, SourceDocument,
    SourceDocumentKind, SourceGraphError, TraceSpan,
};
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_scene_source_graph(
    repo_root: impl AsRef<Path>,
    scene_id: &str,
) -> Result<SceneSourceGraph, SourceGraphError> {
    let manifest_path = repo_root
        .as_ref()
        .join("fixtures")
        .join("v0.1")
        .join("manifest.toml");
    let manifest = load_fixture_manifest(&manifest_path)?;
    let scene = manifest.scene(scene_id)?;
    SceneSourceGraph::load(scene)
}

impl SceneSourceGraph {
    /// Build a [`SceneSourceGraph`] directly from HTML and CSS strings without
    /// touching the file system. Intended for testing and round-trip verification.
    pub fn from_strings(
        scene_id: &str,
        html_str: &str,
        css_str: &str,
    ) -> Result<Self, SourceGraphError> {
        Self::from_documents(
            scene_id.to_owned(),
            PathBuf::from("."),
            SourceDocument::new_html(0, PathBuf::from("<html>"), html_str.to_owned()),
            SourceDocument::new_css(1, PathBuf::from("<css>"), css_str.to_owned()),
            None,
        )
    }

    /// Build a graph for a fixed CSS viewport width. Media conditions that need
    /// a height remain unevaluated. Linked stylesheet contents are supplied as
    /// CSS in cascade order before the document's inline styles.
    pub fn from_strings_at_viewport(
        scene_id: &str,
        html_str: &str,
        css_str: &str,
        viewport_width: f32,
    ) -> Result<Self, SourceGraphError> {
        Self::from_documents(
            scene_id.to_owned(),
            PathBuf::from("."),
            SourceDocument::new_html(0, PathBuf::from("<html>"), html_str.to_owned()),
            SourceDocument::new_css(1, PathBuf::from("<css>"), css_str.to_owned()),
            Some((viewport_width, None)),
        )
    }

    /// Build a fixed-size screen snapshot. This evaluates supported pixel
    /// width and height bounds, orientation, and conjunctions of those features.
    pub fn from_strings_at_viewport_size(
        scene_id: &str,
        html_str: &str,
        css_str: &str,
        width: f32,
        height: f32,
    ) -> Result<Self, SourceGraphError> {
        Self::from_documents(
            scene_id.to_owned(),
            PathBuf::from("."),
            SourceDocument::new_html(0, PathBuf::from("<html>"), html_str.to_owned()),
            SourceDocument::new_css(1, PathBuf::from("<css>"), css_str.to_owned()),
            Some((width, Some(height))),
        )
    }

    pub fn load(scene: FixtureSceneEntry) -> Result<Self, SourceGraphError> {
        let html_path = scene.manifest_root.join(scene.scene.source_html());
        let css_path = scene.manifest_root.join(scene.scene.source_css());
        let html_contents = fs::read_to_string(&html_path)?;
        let css_contents = fs::read_to_string(&css_path)?;
        let html = SourceDocument::new_html(0, html_path, html_contents);
        let css = SourceDocument::new_css(1, css_path, css_contents);
        Self::from_documents(scene.scene.id, scene.scene_root, html, css, None)
    }

    fn from_documents(
        scene_id: String,
        scene_root: PathBuf,
        html: SourceDocument,
        css: SourceDocument,
        viewport: Option<(f32, Option<f32>)>,
    ) -> Result<Self, SourceGraphError> {
        let parsed_html = parse_html_document(html.id, &html.contents)?;
        let mut css_rules =
            parse_css_region(css.id, &css.contents, 0..css.contents.len(), viewport)?;
        for region in parsed_html.styles {
            css_rules.extend(parse_css_region(html.id, &html.contents, region, viewport)?);
        }
        for (id, rule) in css_rules.iter_mut().enumerate() {
            rule.id = id;
        }
        Ok(Self {
            scene_id,
            scene_root,
            html,
            css,
            html_nodes: parsed_html.nodes,
            css_rules,
        })
    }
}
