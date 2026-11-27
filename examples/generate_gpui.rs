use std::{env, error::Error, fs, io, path::PathBuf};

use css_gpui::{
    SceneSourceGraph, diagnostics, extractor,
    ir::SceneIr,
    layout, resolver,
    target::{TargetEmitter, gpui::GpuiTarget},
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1).peekable();
    let mut viewport = None;
    let mut allow_loss = false;
    while let Some(arg) = args.peek() {
        match arg.to_str() {
            Some("--viewport") => {
                args.next();
                let value = args
                    .next()
                    .ok_or("expected WIDTHxHEIGHT after --viewport")?;
                viewport = Some(parse_viewport(
                    value.to_str().ok_or("viewport must use ASCII digits")?,
                )?);
            }
            Some("--allow-loss") => {
                args.next();
                allow_loss = true;
            }
            _ => break,
        }
    }
    let paths: Vec<PathBuf> = args.map(PathBuf::from).collect();
    if let [mode, scene_path] = paths.as_slice()
        && mode.as_os_str() == "--scene-json"
    {
        let scene = SceneIr::from_json(&fs::read_to_string(scene_path)?)?;
        return emit_scene(&scene, scene_path, allow_loss);
    }
    let [html_path, css_path] = paths.as_slice() else {
        return Err(
            "usage: generate_gpui [--allow-loss] [--viewport WIDTHxHEIGHT] <source.html> <source.css> | --scene-json <scene.json>".into(),
        );
    };
    let html = fs::read_to_string(html_path)?;
    let css = fs::read_to_string(css_path)?;
    let graph = if let Some((width, _)) = viewport {
        SceneSourceGraph::from_strings_at_viewport("component", &html, &css, width)
    } else {
        SceneSourceGraph::from_strings("component", &html, &css)
    }
    .map_err(|error| {
        io::Error::other(format!(
            "could not parse {} and {}: {error}",
            html_path.display(),
            css_path.display()
        ))
    })?;
    let losses = diagnostics::analyze(&graph);
    for loss in &losses {
        let rule = &graph.css_rules[loss.rule_id];
        let span = loss
            .property
            .as_deref()
            .and_then(|property| {
                rule.declarations.iter().find(|declaration| {
                    declaration.property == property
                        && Some(declaration.value.as_str()) == loss.value.as_deref()
                })
            })
            .map_or(&rule.span, |declaration| &declaration.span);
        eprintln!(
            "{}:{}:{}: source loss in {}: {}",
            if rule.document_id == graph.html.id {
                html_path
            } else {
                css_path
            }
            .display(),
            span.line,
            span.column,
            loss.selector.as_deref().unwrap_or("CSS rule"),
            loss.message
        );
    }
    let mut has_source_losses = !losses.is_empty();
    for loss in diagnostics::form_control_losses(&graph) {
        eprintln!(
            "{}:{}:{}: source loss in {}: unsupported form control",
            html_path.display(),
            loss.span.line,
            loss.span.column,
            loss.element
        );
        has_source_losses = true;
    }
    if has_source_losses && !allow_loss {
        return Err("GPUI conversion has source losses".into());
    }
    let resolved = match viewport {
        Some((width, height)) => resolver::resolve_scene_at_viewport(&graph, width, height),
        None => resolver::resolve_scene(&graph),
    }?;
    let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph)?;
    emit_scene(&scene, html_path, allow_loss)
}

fn parse_viewport(value: &str) -> Result<(f32, f32), Box<dyn Error>> {
    let (width, height) = value
        .split_once('x')
        .ok_or("viewport must use WIDTHxHEIGHT")?;
    let width: f32 = width.parse()?;
    let height: f32 = height.parse()?;
    if ![width, height]
        .iter()
        .all(|length| length.is_finite() && *length > 0.0)
    {
        return Err("viewport dimensions must be positive and finite".into());
    }
    Ok((width, height))
}

fn emit_scene(
    scene: &SceneIr,
    source_path: &std::path::Path,
    allow_loss: bool,
) -> Result<(), Box<dyn Error>> {
    let emission = GpuiTarget.emit(scene);
    if !emission.declared_loss.is_empty() {
        for gap in &emission.declared_loss {
            let source = scene
                .nodes
                .iter()
                .find(|node| node.id == gap.node_id)
                .map_or(gap.node_id.as_str(), |node| node.source.dom_path.as_str());
            eprintln!(
                "{}: GPUI target loss at {source}: {:?}",
                source_path.display(),
                gap.capability
            );
        }
        if !allow_loss {
            return Err("GPUI conversion has target losses".into());
        }
    }
    print!("{}", emission.artifact);
    Ok(())
}
