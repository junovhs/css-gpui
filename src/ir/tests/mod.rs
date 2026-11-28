mod contract;
mod roundtrip;

use super::*;

pub(super) fn minimal_layout() -> Layout {
    Layout {
        position: Position::Static,
        display: Display::Block,
        box_sizing: BoxSizing::BorderBox,
        top: None,
        left: None,
        width: None,
        height: None,
        aspect_ratio: None,
        min_width: None,
        min_height: None,
        grid: None,
        margin: EdgeInset::zero(),
        padding: EdgeInset::zero(),
        flex_direction: None,
        flex_wrap: None,
        text_align: None,
        align_items: None,
        justify_content: None,
        align_self: None,
        gap: None,
    }
}

pub(super) fn minimal_paint() -> Paint {
    Paint {
        effects: Default::default(),
        placement: Default::default(),
        decorations: Vec::new(),
        interaction: None,
        background_color: None,
        border: None,
        border_radius: None,
        cursor: None,
    }
}

pub(super) fn minimal_scene() -> SceneIr {
    SceneIr {
        canvas: None,
        schema_version: 1,
        scene_id: "test_scene".to_string(),
        corpus: "v0.1".to_string(),
        execution_mode: ExecutionMode::Static,
        nodes: vec![IrNode {
            id: "n0".to_string(),
            kind: NodeKind::Box,
            parent_id: None,
            control_kind: None,
            text_content: None,
            layout: minimal_layout(),
            paint: Paint {
                effects: Default::default(),
                placement: Default::default(),
                decorations: Vec::new(),
                interaction: None,
                background_color: Some(Color("#ffffff".to_string())),
                ..minimal_paint()
            },
            typography: None,
            letter_spacing: None,
            source: SourceRef {
                doc_id: 0,
                dom_path: "body".to_string(),
                span: None,
            },
        }],
    }
}
