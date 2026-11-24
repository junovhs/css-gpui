use std::{env, error::Error, fs, io, path::PathBuf};

use css_gpui::{SceneSourceGraph, diagnostics, extractor, layout, resolver};

fn main() -> Result<(), Box<dyn Error>> {
    let paths: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    let [html_path, css_path] = paths.as_slice() else {
        return Err("usage: inspect <component.html> <component.css>".into());
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
