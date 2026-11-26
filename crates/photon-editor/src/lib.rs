//! KogaView-inspired Editor workspace UI (`iced 0.14` + `photon-ui`):
//! Left tool rail + quick edits + FG/BG ColorPair, live GPU shader canvas
//! with Before/After Original compare & live strokes, floating Adjustments popup
//! (UI-locked, canvas-interactive), and 2-stack resizable right Inspector
//! (Top: Tool Options + History, Bottom: Layers + Layer Masks).

pub mod clipboard_paste;
pub mod panel_splitter;
pub mod shortcuts;
pub mod tabs;
pub mod text_ui;
pub mod vector_ui;

mod ai_ui;
mod dialogs;
mod gradient_ui;
mod layers_history;
mod menus;
mod messages;
mod state;
mod tool_options;
mod tools;
mod viewport_chrome;
mod widgets;
mod workspace;

pub use clipboard_paste::{SmartPasteFitMode, SmartPasteState};
pub use dialogs::{build_color_picker_dialog, build_export_dialog};
pub use menus::{
    build_editor_add_layer_menu, build_editor_canvas_context_menu, build_editor_layer_context_menu,
    build_editor_plugins_menu_entries,
};
pub use messages::{EditorMenuKind, EditorMessage, EditorViewState};
pub use state::{
    DimGridAction, DimensionInputState, EditorUiState, crop_uv_to_signed, default_curves,
    format_hex, format_ratio_string, parse_hex_rgb, parse_ratio_str, rgb_to_color,
};
pub use tabs::{TabCloseScope, build_editor_tab_context_menu};
pub use tools::{
    AdjustmentPopupTab, AlphaRefineMode, ColorBalanceZone, ColorPickerTarget, CompareMode,
    CurvesChannel, EditorTool, EditorToolGroup, WatermarkTab, build_tool_group_flyout_menu,
    editor_tool_label,
};
pub use vector_ui::{
    build_editor_vector_menu_entries, build_export_vector_svg_dialog, build_transform_each_dialog,
    build_vector_blend_dialog, build_vector_shape_from_draw_commit,
};
pub use viewport_chrome::build_editor_zoom_controls;
pub use widgets::{apply_target_control, slider_stepper_control};
pub use workspace::{build_editor_toolbar, build_editor_view};

#[cfg(test)]
mod tests {
    use super::*;
    use photon_canvas::CanvasTool;
    use photon_protocol::{HealingMode, ShapeFillMode};
    use photon_ui::editor::LengthUnit;
    use photon_ui::icons::Icon;

    #[test]
    fn hex_round_trip_parses_with_or_without_hash() {
        assert_eq!(parse_hex_rgb("#3b82f6"), Some([0x3b, 0x82, 0xf6]));
        assert_eq!(parse_hex_rgb("ff00aa"), Some([0xff, 0x00, 0xaa]));
        assert_eq!(parse_hex_rgb("bad"), None);
        assert_eq!(format_hex([0x10, 0x20, 0x30]), "#102030");
    }

    #[test]
    fn crop_uv_to_signed_bounds_correctly() {
        let (x, y, w, h) = crop_uv_to_signed([0.1, 0.2, 0.9, 0.8], 100, 200);
        assert_eq!((x, y, w, h), (10, 40, 80, 120));
    }

    #[test]
    fn dimension_input_state_preserves_ratio_and_print_units() {
        let mut dims = DimensionInputState::from_dims(1920, 1080);
        assert_eq!(dims.ratio_str, "16:9");
        dims.apply(DimGridAction::WidthChanged("3840".to_owned()));
        assert_eq!(dims.width_px, 3840);
        assert_eq!(dims.height_px, 2160);

        dims.apply(DimGridAction::DpiChanged(300.0));
        dims.apply(DimGridAction::UnitChanged(LengthUnit::Inch));
        assert_eq!(dims.width_str, "12.80");
        dims.apply(DimGridAction::PresetRatio(1, 1));
        assert_eq!(dims.width_px, dims.height_px);
    }

    #[test]
    fn healing_and_transform_tools_map_to_canvas_and_ui_defaults() {
        assert_eq!(
            EditorTool::HealingBrush.as_canvas_tool(),
            CanvasTool::HealingBrush
        );
        assert_eq!(
            EditorTool::Transform.as_canvas_tool(),
            CanvasTool::Transform
        );
        assert_eq!(EditorTool::HealingBrush.icon(), Icon::HealingBrush);
        assert_eq!(EditorTool::Transform.icon(), Icon::Transform);

        let ui = EditorUiState::default();
        assert_eq!(ui.healing_mode, HealingMode::Spot);
        assert!(ui.transform_spec.is_identity());
        assert_eq!(ui.flood_fill_opacity, 1.0);
        assert_eq!(ui.gradient_opacity, 1.0);
        assert!(ui.gradient_dither);
        assert!(!ui.gradient_reverse);
    }

    #[test]
    fn phase3_vector_and_text_tools_and_shape_builder() {
        assert_eq!(
            EditorTool::PathSelect.as_canvas_tool(),
            CanvasTool::PathSelect
        );
        assert_eq!(EditorTool::Text.as_canvas_tool(), CanvasTool::Text);
        assert_eq!(EditorTool::PathSelect.icon(), Icon::PathSelect);
        assert_eq!(EditorTool::Text.icon(), Icon::Text);

        let shape = build_vector_shape_from_draw_commit(
            photon_protocol::DrawShapeKind::Rect,
            &[[10.0, 20.0], [110.0, 70.0]],
            &[],
            true,
            ShapeFillMode::StrokeAndFill,
            [255, 0, 0],
            Some([0, 255, 0]),
            4.0,
            12.0,
            0.8,
        )
        .expect("vector rect shape");
        assert_eq!(shape.fill, Some([0, 255, 0]));
        assert_eq!(shape.stroke.color, [255, 0, 0]);
        assert!((shape.stroke.width - 4.0).abs() < 1e-4);
        match shape.geometry {
            photon_protocol::VectorGeometry::Rect {
                x,
                y,
                w,
                h,
                corner_radius,
            } => {
                assert_eq!((x, y, w, h, corner_radius), (10.0, 20.0, 100.0, 50.0, 12.0));
            }
            other => panic!("expected rect geometry, got {other:?}"),
        }
    }
}
