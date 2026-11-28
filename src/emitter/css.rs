//! CSS generation from IR nodes.

use crate::ir::layout::{
    AlignItems, AlignSelf, BoxSizing, Cursor, Display, EdgeInset, FlexDirection, JustifyContent,
    Layout, Position,
};
use crate::ir::paint::{Paint, StatePaint};
use crate::ir::typography::{LineHeight, Typography};
use crate::ir::{ControlKind, IrNode, LetterSpacing, NodeKind};

/// Emit a complete `<style>` block for all non-Text nodes in `nodes`.
pub fn build_css(nodes: &[IrNode]) -> String {
    let mut rules = vec!["body {\n  margin: 0;\n}".to_owned()];
    for node in nodes.iter().filter(|node| node.kind != NodeKind::Text) {
        rules.extend(emit_rule(node));
        if let Some(ControlKind::Radio(radio)) = node.control_kind.as_ref() {
            rules.extend(emit_state_rule(node, ":checked", &radio.checked_paint));
        }
        if let Some(background) = node.paint.radio_background.as_ref() {
            rules.push(format!(
                ".{}:has(#{}:checked) {{\n  background-color: {};\n}}",
                node.id, background.radio, background.color.0
            ));
        }
        if let Some(interaction) = node.paint.interaction.as_ref() {
            rules.extend(emit_state_rule(node, ":hover", &interaction.hover));
            rules.extend(emit_state_rule(node, ":focus", &interaction.focus));
            if interaction.uses_focus() {
                let combined = interaction.combined(
                    &node.paint,
                    node.typography.as_ref().map(|typography| &typography.color),
                );
                rules.extend(emit_state_rule(node, ":hover:focus", &combined));
            }
        }
    }
    rules.join("\n\n")
}

fn emit_rule(node: &IrNode) -> Option<String> {
    let mut decls: Vec<String> = Vec::new();
    // Reset native browser chrome for interactive controls before any layout
    // or paint declarations. The supported source policy accepts only the
    // explicit `none` reset; it is not stored in IR because every control is
    // canonicalized to this value on output. This avoids OS-level gradients,
    // inset borders, and text-baseline drift on round-trip.
    if node.kind == NodeKind::Control {
        decls.push("appearance: none".to_owned());
        if matches!(node.control_kind, Some(ControlKind::Radio(_)))
            && node.layout.margin == EdgeInset::zero()
        {
            decls.push("margin: 0".to_owned());
        }
        if node.paint.border.is_none() {
            decls.push("border: 0".to_owned());
        }
        if node.layout.padding == EdgeInset::zero() {
            decls.push("padding: 0".to_owned());
        }
    }
    // Radio inputs and labels do not default to block flow in the browser.
    if node.layout.display == Display::Block
        && (matches!(node.control_kind, Some(ControlKind::Radio(_)))
            || matches!(node.kind, NodeKind::RadioLabel { .. }))
    {
        decls.push("display: block".to_owned());
    }
    decls.extend(layout_decls(&node.layout));
    decls.extend(paint_decls(&node.paint));
    if let Some(ref typo) = node.typography {
        decls.extend(typography_decls(typo));
    } else if let Some(color) = node
        .paint
        .interaction
        .as_ref()
        .and_then(|interaction| interaction.resting_text_color.as_ref())
    {
        decls.push(format!("color: {}", color.0));
    }
    match node.letter_spacing {
        Some(LetterSpacing::Normal) => decls.push("letter-spacing: normal".to_owned()),
        Some(LetterSpacing::Length { value }) => {
            decls.push(format!("letter-spacing: {}", px(value)));
        }
        None => {}
    }
    if decls.is_empty() {
        return None;
    }
    let body = decls
        .iter()
        .map(|d| format!("  {d};"))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!(".{} {{\n{body}\n}}", node.id))
}

fn emit_state_rule(node: &IrNode, selector: &str, paint: &StatePaint) -> Option<String> {
    let declarations = [
        ("background-color", &paint.background_color),
        ("border-color", &paint.border_color),
        ("color", &paint.text_color),
    ]
    .into_iter()
    .filter_map(|(property, color)| Some(format!("  {property}: {};", color.as_ref()?.0)))
    .collect::<Vec<_>>();
    (!declarations.is_empty())
        .then(|| format!(".{}{selector} {{\n{}\n}}", node.id, declarations.join("\n")))
}

fn layout_decls(l: &Layout) -> Vec<String> {
    let mut d: Vec<String> = Vec::new();
    match l.position {
        Position::Absolute => d.push("position: absolute".to_owned()),
        Position::Relative => d.push("position: relative".to_owned()),
        Position::Fixed => d.push("position: fixed".to_owned()),
        Position::Static => {}
    }
    match l.display {
        Display::Flex => d.push("display: flex".to_owned()),
        Display::InlineFlex => d.push("display: inline-flex".to_owned()),
        Display::Grid => d.push("display: grid".to_owned()),
        Display::Block => {}
    }
    if l.box_sizing == BoxSizing::BorderBox {
        d.push("box-sizing: border-box".to_owned());
    }
    for (property, value) in [
        ("top", l.top),
        ("left", l.left),
        ("right", l.right),
        ("bottom", l.bottom),
    ] {
        if let Some(value) = value {
            d.push(format!("{property}: {}", px(value)));
        }
    }
    if let Some(v) = l.width {
        d.push(format!("width: {}", px(v)));
    }
    if let Some(v) = l.height {
        d.push(format!("height: {}", px(v)));
    }
    if let Some(ratio) = l.aspect_ratio {
        d.push(format!("aspect-ratio: {ratio}"));
    }
    if let Some(v) = l.min_width {
        d.push(format!("min-width: {}", px(v)));
    }
    if let Some(v) = l.min_height {
        d.push(format!("min-height: {}", px(v)));
    }
    if let Some(grid) = &l.grid {
        let tracks = |count| vec!["1fr"; usize::from(count)].join(" ");
        match (grid.rows, grid.columns) {
            (Some(rows), Some(columns)) => d.push(format!(
                "grid-template: {} / {}",
                tracks(rows),
                tracks(columns)
            )),
            (Some(rows), None) => d.push(format!("grid-template-rows: {}", tracks(rows))),
            (None, Some(columns)) => d.push(format!("grid-template-columns: {}", tracks(columns))),
            _ => {}
        }
        match (grid.row_start, grid.column_start) {
            (Some(row), Some(column)) => d.push(format!("grid-area: {row} / {column}")),
            (Some(row), None) => d.push(format!("grid-row-start: {row}")),
            (None, Some(column)) => d.push(format!("grid-column-start: {column}")),
            _ => {}
        }
    }
    if let Some(s) = edge_shorthand("margin", &l.margin) {
        d.push(s);
    }
    if let Some(s) = edge_shorthand("padding", &l.padding) {
        d.push(s);
    }
    if let Some(ref fd) = l.flex_direction {
        d.push(format!("flex-direction: {}", flex_dir(fd)));
    }
    if let Some(wrap) = l.flex_wrap {
        d.push(format!("flex-wrap: {}", wrap.css_keyword()));
    }
    if let Some(alignment) = l.text_align {
        d.push(format!("text-align: {}", alignment.css_keyword()));
    }
    if let Some(ref ai) = l.align_items {
        d.push(format!("align-items: {}", align_items_val(ai)));
    }
    if let Some(ref jc) = l.justify_content {
        d.push(format!("justify-content: {}", justify_val(jc)));
    }
    if let Some(ref ase) = l.align_self {
        d.push(format!("align-self: {}", align_self_val(ase)));
    }
    if let Some(v) = l.gap {
        d.push(format!("gap: {}", px(v)));
    }
    d
}

fn paint_decls(p: &Paint) -> Vec<String> {
    let mut d: Vec<String> = Vec::new();
    if let Some(ref bg) = p.background_color {
        d.push(format!("background-color: {}", bg.0));
    }
    if let Some(ref b) = p.border {
        d.push(format!("border: {} solid {}", px(b.width), b.color.0));
    }
    if let Some(r) = p.border_radius {
        d.push(format!("border-radius: {}", px(r)));
    }
    if let Some(ref c) = p.cursor {
        d.push(format!("cursor: {}", cursor_val(c)));
    }
    d.extend(
        p.effects
            .iter()
            .map(|(property, value)| format!("{property}: {value}")),
    );
    d
}

fn typography_decls(t: &Typography) -> Vec<String> {
    let mut d: Vec<String> = Vec::new();
    d.push(format!("font-family: {}", t.font_family.join(", ")));
    d.push(format!("font-size: {}", px(t.font_size)));
    d.push(format!("font-weight: {}", t.font_weight));
    d.push(format!("line-height: {}", line_height_val(&t.line_height)));
    d.push(format!("color: {}", t.color.0));
    d
}

// ---------------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------------

pub fn px(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}px", v as i64)
    } else {
        format!("{v}px")
    }
}

fn edge_shorthand(name: &str, e: &EdgeInset) -> Option<String> {
    if e.top == 0.0 && e.right == 0.0 && e.bottom == 0.0 && e.left == 0.0 {
        return None;
    }
    let s = if e.top == e.right && e.right == e.bottom && e.bottom == e.left {
        format!("{name}: {}", px(e.top))
    } else if e.top == e.bottom && e.left == e.right {
        format!("{name}: {} {}", px(e.top), px(e.right))
    } else {
        format!(
            "{name}: {} {} {} {}",
            px(e.top),
            px(e.right),
            px(e.bottom),
            px(e.left)
        )
    };
    Some(s)
}

fn flex_dir(fd: &FlexDirection) -> &'static str {
    match fd {
        FlexDirection::Row => "row",
        FlexDirection::Column => "column",
    }
}

fn align_items_val(ai: &AlignItems) -> &'static str {
    match ai {
        AlignItems::Center => "center",
    }
}

fn justify_val(jc: &JustifyContent) -> &'static str {
    match jc {
        JustifyContent::Center => "center",
        JustifyContent::SpaceBetween => "space-between",
    }
}

fn align_self_val(ase: &AlignSelf) -> &'static str {
    match ase {
        AlignSelf::Auto => "auto",
        AlignSelf::FlexStart => "flex-start",
        AlignSelf::Center => "center",
        AlignSelf::End => "end",
    }
}

fn cursor_val(c: &Cursor) -> &'static str {
    match c {
        Cursor::Auto => "auto",
        Cursor::Pointer => "pointer",
    }
}

fn line_height_val(lh: &LineHeight) -> String {
    match lh {
        LineHeight::Normal => "normal".to_owned(),
        LineHeight::Length { value } => px(*value),
    }
}
