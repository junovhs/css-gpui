use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use css_gpui::{SceneSourceGraph, diagnostics, extractor, layout, planar, projection, resolver};
use serde_json::json;

fn main() -> Result<(), Box<dyn Error>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if let [mode, viewport, html, css, selection @ ..] = paths.as_slice() {
        match mode.to_str() {
            Some("--radio-snapshot") => {
                let snapshot = resolve_snapshot(viewport, html, css, selection, false)?;
                return inspect_radio_snapshot(&snapshot.scene);
            }
            Some("--planar-preview") => {
                let snapshot = resolve_snapshot(viewport, html, css, selection, true)?;
                return inspect_planar_preview(&snapshot);
            }
            Some("--projection-snapshot") => {
                let snapshot = resolve_snapshot(viewport, html, css, selection, true)?;
                return inspect_projection_snapshot(&snapshot.scene, snapshot.viewport);
            }
            _ => {}
        }
    }
    let [html_path, css_path] = paths.as_slice() else {
        return Err("usage: inspect [--radio-snapshot|--projection-snapshot|--planar-preview WIDTHxHEIGHT] <component.html> <component.css> [radio ID ...]".into());
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

struct Snapshot {
    scene: resolver::ResolvedScene,
    viewport: [f64; 2],
    graph: SceneSourceGraph,
    overrides: Vec<usize>,
}

fn resolve_snapshot(
    viewport: &Path,
    html: &Path,
    css: &Path,
    selection: &[PathBuf],
    include_orientation: bool,
) -> Result<Snapshot, Box<dyn Error>> {
    let (width, height) = viewport
        .to_str()
        .and_then(|size| size.split_once('x'))
        .ok_or("expected snapshot viewport WIDTHxHEIGHT")?;
    let (width, height): (u32, u32) = (width.parse()?, height.parse()?);
    if width == 0 || height == 0 {
        return Err("snapshot viewport dimensions must be positive".into());
    }
    let html = fs::read_to_string(html)?;
    let css = fs::read_to_string(css)?;
    let graph = if include_orientation {
        SceneSourceGraph::from_strings_at_viewport_size(
            "component",
            &html,
            &css,
            width as f32,
            height as f32,
        )?
    } else {
        SceneSourceGraph::from_strings_at_viewport("component", &html, &css, width as f32)?
    };
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
    Ok(Snapshot {
        scene,
        viewport: [f64::from(width), f64::from(height)],
        graph,
        overrides,
    })
}

#[derive(serde::Deserialize)]
struct PlanarInput {
    root: String,
    rgba: PathBuf,
    label_scope: Option<String>,
}

fn unique_node(scene: &resolver::ResolvedScene, selector: &str) -> Result<usize, Box<dyn Error>> {
    let mut matches = scene.nodes.iter().filter(|resolved| {
        if let Some(id) = selector.strip_prefix('#') {
            resolved
                .node
                .attributes
                .iter()
                .any(|(name, value)| name == "id" && value == id)
        } else {
            resolved.node.name.as_deref() == Some(selector)
        }
    });
    let root = matches
        .next()
        .ok_or_else(|| format!("{selector:?} does not match an element"))?;
    if matches.next().is_some() {
        return Err(format!("{selector:?} must match exactly one element").into());
    }
    Ok(root.node.id)
}

fn inspect_planar_preview(snapshot: &Snapshot) -> Result<(), Box<dyn Error>> {
    let input: PlanarInput = serde_json::from_reader(io::stdin().lock())?;
    let Snapshot {
        scene,
        viewport,
        graph,
        overrides,
    } = snapshot;
    let controls = input
        .label_scope
        .as_deref()
        .map(|scope| -> Result<_, Box<dyn Error>> {
            let root = unique_node(scene, scope)?;
            Ok(resolver::radio_snapshot_controls(
                graph, scene, root, overrides,
            )?)
        })
        .transpose()?;
    let frame = planar::preview(
        graph,
        scene,
        unique_node(scene, &input.root)?,
        viewport.map(|length| length as u32),
    )?;
    fs::write(input.rgba, frame.rgba)?;
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({
            "planes": frame.planes, "losses": frame.losses, "controls": controls,
        }),
    )?;
    println!();
    Ok(())
}

fn inspect_radio_snapshot(scene: &resolver::ResolvedScene) -> Result<(), Box<dyn Error>> {
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionInput {
    chains: Vec<Vec<String>>,
}

fn inspect_projection_snapshot(
    scene: &resolver::ResolvedScene,
    viewport: [f64; 2],
) -> Result<(), Box<dyn Error>> {
    let input: ProjectionInput = serde_json::from_reader(io::stdin().lock())?;
    let mut layout = layout::UsedBoxLayout::new(scene, viewport);

    let mut paths = BTreeMap::new();
    let mut counts = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    for node in &scene.nodes {
        let tag = node.node.name.as_deref().unwrap_or_default();
        let count = counts.entry((node.node.parent_id, tag)).or_insert(0);
        *count += 1;
        let parent = node
            .node
            .parent_id
            .and_then(|id| paths.get(&id))
            .map_or("", String::as_str);
        let path = format!("{parent}/{tag}:{count}");
        paths.insert(node.node.id, path.clone());
        nodes.insert(path, node);
    }

    let mut bounds = BTreeMap::new();
    let mut output = BTreeMap::new();
    for chain in &input.chains {
        let target = chain.last().ok_or("empty projection chain")?;
        let layers = chain
            .iter()
            .map(|key| {
                let (path, pseudo) = key
                    .split_once("::")
                    .map_or((key.as_str(), None), |(path, pseudo)| (path, Some(pseudo)));
                let node = nodes
                    .get(path)
                    .ok_or_else(|| format!("unknown projection path {path}"))?;
                let css = match pseudo {
                    Some(pseudo) => node
                        .style
                        .decoration_projection_properties
                        .get(pseudo)
                        .ok_or_else(|| format!("unknown pseudo-element {key}"))?,
                    None => &node.style.projection_properties,
                };
                let border = layout
                    .border_box(node.node.id, pseudo)
                    .map_err(|error| format!("{key}: {error}"))?
                    .ok_or_else(|| format!("no used box for {key}"))?;
                bounds.insert(key, border);
                let font_size = match pseudo {
                    Some(pseudo) => node.style.decoration_box_styles[pseudo].font_size(),
                    None => node.style.used_box_style.font_size(),
                };
                Ok::<_, String>(projection::Layer {
                    origin: border.origin,
                    size: border.size,
                    font_size,
                    css,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let projected = projection::project_box(&layers, viewport, layout.root_font_size())
            .map_err(|error| format!("{target}: {error}"))?;
        output.insert(target, projected);
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({"bounds": bounds, "projected": output}),
    )?;
    println!();
    Ok(())
}
