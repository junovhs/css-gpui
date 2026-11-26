use std::{env, error::Error, fs, io, path::PathBuf};

use css_gpui::{
    SceneSourceGraph, diagnostics, extractor, layout, resolver,
    target::{TargetEmitter, gpui::GpuiTarget},
};

fn main() -> Result<(), Box<dyn Error>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let [html_path, css_path] = paths.as_slice() else {
        return Err("usage: generate_gpui <source.html> <source.css>".into());
    };
    let html = fs::read_to_string(html_path)?;
    let css = fs::read_to_string(css_path)?;
    let graph = SceneSourceGraph::from_strings("component", &html, &css).map_err(|error| {
        io::Error::other(format!(
            "could not parse {} and {}: {error}",
            html_path.display(),
            css_path.display()
        ))
    })?;
    let losses = diagnostics::analyze(&graph);
    if !losses.is_empty() {
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
                css_path.display(),
                span.line,
                span.column,
                loss.selector.as_deref().unwrap_or("CSS rule"),
                loss.message
            );
        }
        return Err("GPUI conversion has source losses".into());
    }
    let resolved = resolver::resolve_scene(&graph)?;
    let scene = extractor::extract_ir(&layout::compute_layout(&resolved), &graph)?;
    let emission = GpuiTarget.emit(&scene);
    if !emission.declared_loss.is_empty() {
        for gap in &emission.declared_loss {
            let source = scene
                .nodes
                .iter()
                .find(|node| node.id == gap.node_id)
                .map_or(gap.node_id.as_str(), |node| node.source.dom_path.as_str());
            eprintln!(
                "{}: GPUI target loss at {source}: {:?}",
                html_path.display(),
                gap.capability
            );
        }
        return Err("GPUI conversion has target losses".into());
    }
    print!("{}", emission.artifact);
    Ok(())
}
