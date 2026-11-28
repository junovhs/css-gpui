use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use css_gpui::{SceneSourceGraph, diagnostics, extractor, layout, resolver};
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if let [mode, viewport, html, css, selection @ ..] = paths.as_slice()
        && mode.as_os_str() == "--radio-snapshot"
    {
        return inspect_radio_snapshot(viewport, html, css, selection);
    }
    let [html_path, css_path] = paths.as_slice() else {
        return Err("usage: inspect [--radio-snapshot WIDTHxHEIGHT] <component.html> <component.css> [radio ID ...]".into());
    };

    let html = fs::read_to_string(html_path)?;
    let css = fs::read_to_string(css_path)?;
    let graph = SceneSourceGraph::from_strings("component", &html, &css)?;
    for diagnostic in diagnostics::analyze(&graph) {
        eprintln!(
            "{}: rule {}: {}",
            css_path.display(),
            diagnostic.rule_id,
            diagnostic.message
        );
    }

    let resolved = resolver::resolve_scene(&graph)?;
    let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph)?;
    serde_json::to_writer_pretty(io::stdout().lock(), &scene)?;
    println!();
    Ok(())
}

fn inspect_radio_snapshot(
    viewport: &Path,
    html: &Path,
    css: &Path,
    selection: &[PathBuf],
) -> Result<(), Box<dyn Error>> {
    let (width, height) = viewport
        .to_str()
        .and_then(|size| size.split_once('x'))
        .ok_or("expected snapshot viewport WIDTHxHEIGHT")?;
    let (width, height): (u32, u32) = (width.parse()?, height.parse()?);
    if width == 0 || height == 0 {
        return Err("snapshot viewport dimensions must be positive".into());
    }
    let graph = SceneSourceGraph::from_strings_at_viewport(
        "component",
        &fs::read_to_string(html)?,
        &fs::read_to_string(css)?,
        width as f32,
    )?;
    let overrides = selection
        .iter()
        .map(|id| {
            let id = id.to_str().ok_or("radio IDs must be UTF-8")?;
            graph
                .html_nodes
                .iter()
                .find(|node| {
                    node.attributes
                        .iter()
                        .any(|(name, value)| name == "id" && value == id)
                })
                .map(|node| node.id)
                .ok_or_else(|| format!("unknown radio ID {id:?}").into())
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    let scene = resolver::resolve_scene_with_radio_selection(
        &graph,
        width as f32,
        height as f32,
        &overrides,
    )?;
    let output: Vec<_> = scene
        .nodes
        .iter()
        .map(|resolved| {
            let node = &resolved.node;
            let style = &resolved.style;
            let attributes: BTreeMap<_, _> = node
                .attributes
                .iter()
                .map(|(name, value)| (name, value))
                .collect();
            json!({
                "id": node.id,
                "parent_id": node.parent_id,
                "tag": node.name,
                "attributes": attributes,
                "display": style.display,
                "position": style.position,
                "font_size": style.font_size,
                "background_color": style.background_color,
                "projection": style.projection_properties,
                "decorations": style.decorations,
            })
        })
        .collect();
    serde_json::to_writer(io::stdout().lock(), &output)?;
    println!();
    Ok(())
}
