use crate::ai_ui::{
    build_ai_caption_tool_options, build_ai_rembg_tool_options, build_ai_watermark_tool_options,
};
use crate::gradient_ui::build_gradient_tool_options;
use crate::layers_history::{build_history_panel, build_layers_panel};
use crate::panel_splitter::editor_vertical_splitter;
use crate::widgets::subtool_icon_row;
use crate::*;
use iced::widget::{checkbox, column, container, row, scrollable};
use iced::{Alignment, Color, Element, Fill, Length};
use photon_i18n::{tr, tr_args};
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, PanelTabItem, kv_row, section_card, sidebar_dock,
    sidebar_panel, stacked_panel_heights,
};
use photon_protocol::{
    BrushSmoothingMode, BrushTip, EditorRightPanel, EditorState, EditorToolTab, HealingMode,
    Language, LayerTransformSpec, LiquifyMode, ShapeFillMode,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::editor::checker_swatch;
use photon_ui::icons::Icon;
use photon_ui::theme::{PaletteTokens, TEXT_SIZE_CAPTION};
use photon_ui::typography::text;
use photon_ui::{TooltipPosition, with_tooltip_pos};
use std::collections::BTreeMap;

pub(crate) fn build_editor_inspector<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
    width: f32,
) -> Element<'a, EditorMessage> {
    let split_pct = state
        .dock_drag
        .effective_editor_split_pct(&state.snapshot.meta.layout);

    let visible_panels: Vec<EditorRightPanel> = state
        .editor_panels
        .order()
        .iter()
        .copied()
        .filter(|panel| match panel {
            EditorRightPanel::ToolOptionsHistory => state.editor_history_open,
            EditorRightPanel::Layers => state.editor_layers_open,
        })
        .collect();

    let is_collapsed = |panel: EditorRightPanel| match panel {
        EditorRightPanel::ToolOptionsHistory => state.editor_history_collapsed,
        EditorRightPanel::Layers => state.editor_layers_collapsed,
    };

    let mut stacked = column![].spacing(6).height(Fill).width(Fill);
    if visible_panels.len() == 2 {
        let p0 = visible_panels[0];
        let p1 = visible_panels[1];
        let c0 = is_collapsed(p0);
        let c1 = is_collapsed(p1);
        let (h0, h1) = stacked_panel_heights(c0, c1, split_pct, 20.0, 80.0);

        let s0 = build_right_panel_section(state, ed, lang, width, p0, c0, true);
        let s1 = build_right_panel_section(state, ed, lang, width, p1, c1, true);

        stacked = stacked.push(container(s0).width(Fill).height(h0));
        if !c0 && !c1 {
            stacked = stacked.push(editor_vertical_splitter(
                split_pct,
                state.window_size.height,
            ));
        }
        stacked = stacked.push(container(s1).width(Fill).height(h1));
        if c0 && c1 {
            stacked = stacked.push(iced::widget::space::vertical());
        }
    } else if let Some(&p0) = visible_panels.first() {
        let c0 = is_collapsed(p0);
        let h0 = if c0 { Length::Shrink } else { Length::Fill };
        let s0 = build_right_panel_section(state, ed, lang, width, p0, c0, false);
        stacked = stacked.push(container(s0).width(Fill).height(h0));
        if c0 {
            stacked = stacked.push(iced::widget::space::vertical());
        }
    }

    sidebar_dock(width, stacked)
}

pub(crate) fn build_right_panel_section<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
    sidebar_width: f32,
    panel_kind: EditorRightPanel,
    collapsed: bool,
    can_reorder: bool,
) -> Element<'a, EditorMessage> {
    let dragging_any = state.editor_panels.is_dragging();
    let is_dragging_self = state.editor_panels.is_dragging_panel(panel_kind);
    let drop_ind = state.editor_panels.drop_indicator(panel_kind);

    let mut header = match panel_kind {
        EditorRightPanel::ToolOptionsHistory => DraggablePanelHeaderSpec::with_tabs(vec![
            PanelTabItem::new(
                "options",
                tr(lang, "tab-tool-options"),
                state.editor_tool_tab == EditorToolTab::Options,
                EditorMessage::SelectEditorToolTab(EditorToolTab::Options),
            )
            .icon(Icon::Sliders)
            .tooltip(tr(lang, "tab-tool-options")),
            PanelTabItem::new(
                "history",
                tr(lang, "tab-history"),
                state.editor_tool_tab == EditorToolTab::History,
                EditorMessage::SelectEditorToolTab(EditorToolTab::History),
            )
            .icon(Icon::History)
            .tooltip(tr(lang, "tab-history")),
        ])
        .compact_tabs(sidebar_width < 245.0),
        EditorRightPanel::Layers => DraggablePanelHeaderSpec::new(tr(lang, "label-layers"))
            .icon(Icon::Layers)
            .badge(ed.layers.len().to_string()),
    };

    header = header
        .collapsed(collapsed)
        .dragging(is_dragging_self)
        .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
        .close_tooltip(tr(lang, "tooltip-panel-close"))
        .on_toggle_collapse(EditorMessage::ToggleEditorPanelCollapse(panel_kind))
        .on_close(Some(EditorMessage::CloseEditorPanel(panel_kind)));

    if can_reorder {
        header = header
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .on_drag_start(EditorMessage::BeginDragEditorPanel(panel_kind))
            .on_drag_hover(dragging_any.then_some(EditorMessage::HoverEditorPanelDrop(panel_kind)))
            .on_drag_end(dragging_any.then_some(EditorMessage::DropEditorPanel(panel_kind)));
    }

    let body = (!collapsed).then(|| match panel_kind {
        EditorRightPanel::ToolOptionsHistory => build_tool_options_history_panel(state, ed, lang),
        EditorRightPanel::Layers => build_layers_panel(state, ed, lang),
    });

    sidebar_panel(
        PanelFrameSpec::new(header)
            .maybe_body(body)
            .fill_height(!collapsed)
            .drop_indicator(drop_ind)
            .on_drop_hover(dragging_any.then_some(EditorMessage::HoverEditorPanelDrop(panel_kind)))
            .on_drop_release(dragging_any.then_some(EditorMessage::DropEditorPanel(panel_kind))),
    )
}

pub(crate) fn build_tool_options_history_panel<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let tab_body: Element<'a, EditorMessage> = match state.editor_tool_tab {
        EditorToolTab::Options => {
            let smart_paste_card = state
                .editor_ui
                .smart_paste
                .as_ref()
                .map(|sp| clipboard_paste::build_smart_paste_card(sp, lang));
            let base_card = if let Some(tool_card) = build_tool_options_card(state, ed, lang) {
                tool_card
            } else {
                let tool_title = match state.editor_ui.tool {
                    EditorTool::Hand => tr(lang, "tool-hand"),
                    EditorTool::Zoom => tr(lang, "tool-zoom"),
                    _ => tr(lang, "tool-pointer"),
                };
                section_card(
                    tool_title,
                    text("No extra options for this tool.")
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| {
                            let t = PaletteTokens::for_theme(theme);
                            text::Style {
                                color: Some(t.text_muted),
                            }
                        }),
                )
            };
            if let Some(sp_card) = smart_paste_card {
                column![sp_card, base_card].spacing(8).into()
            } else {
                base_card
            }
        }
        EditorToolTab::History => build_history_panel(ed, lang),
    };

    scrollable(container(tab_body).padding(iced::Padding {
        top: 0.0,
        right: 10.0,
        bottom: 0.0,
        left: 0.0,
    }))
    .height(Fill)
    .width(Fill)
    .into()
}

fn build_tool_options_card<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Option<Element<'a, EditorMessage>> {
    let ui = &state.editor_ui;

    if let Some(uv) = state.crop_uv {
        let (cx, cy, cw, ch) = crop_uv_to_signed(uv, ed.width, ed.height);
        return Some(section_card(
            tr(lang, "action-crop"),
            column![
                kv_row(
                    tr(lang, "label-dimensions"),
                    format!("{cw} × {ch} @ ({cx}, {cy})")
                ),
                row![
                    button::labeled(
                        "Free",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect.is_none(),
                        },
                        Some(EditorMessage::SetCropPreset(None)),
                    ),
                    button::labeled(
                        "1:1",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect == Some(1.0),
                        },
                        Some(EditorMessage::SetCropPreset(Some(1.0))),
                    ),
                    button::labeled(
                        "4:3",
                        None,
                        ButtonVariant::Tab {
                            active: ui.crop_aspect.is_some_and(|a| (a - 4.0 / 3.0).abs() < 0.01),
                        },
                        Some(EditorMessage::SetCropPreset(Some(4.0 / 3.0))),
                    ),
                    button::labeled(
                        "16:9",
                        None,
                        ButtonVariant::Tab {
                            active: ui
                                .crop_aspect
                                .is_some_and(|a| (a - 16.0 / 9.0).abs() < 0.01),
                        },
                        Some(EditorMessage::SetCropPreset(Some(16.0 / 9.0))),
                    ),
                ]
                .spacing(4),
                row![
                    text(tr(lang, "label-canvas-bg")).size(12).width(Fill),
                    checker_swatch(
                        ui.crop_fill.map(rgb_to_color),
                        24.0,
                        18.0,
                        Some(EditorMessage::OpenColorPicker(ColorPickerTarget::CropFill)),
                    ),
                ]
                .align_y(Alignment::Center),
                row![
                    button::labeled(
                        tr(lang, "dialog-cancel"),
                        None,
                        ButtonVariant::Secondary,
                        Some(EditorMessage::CancelCrop),
                    ),
                    with_tooltip_pos(
                        button::labeled(
                            tr(lang, "action-apply-crop"),
                            Some(Icon::Check),
                            ButtonVariant::Primary,
                            Some(EditorMessage::ApplyCrop),
                        )
                        .width(Fill),
                        tr(lang, "action-apply-crop"),
                        TooltipPosition::Top,
                    ),
                ]
                .spacing(6),
            ]
            .spacing(8),
        ));
    }

    if ui.tool.is_selection() {
        let group_tools: Vec<EditorTool> = if ui.tool.is_marquee_group() {
            vec![
                EditorTool::MarqueeRect,
                EditorTool::MarqueeEllipse,
                EditorTool::SingleRow,
                EditorTool::SingleCol,
            ]
        } else if ui.tool.is_lasso_group() {
            vec![
                EditorTool::Lasso,
                EditorTool::PolygonLasso,
                EditorTool::MagneticLasso,
                EditorTool::BezierSelect,
            ]
        } else {
            vec![EditorTool::QuickSelect, EditorTool::MagicWand]
        };

        let sub_row = subtool_icon_row(lang, ui.tool, &group_tools);

        let op_tabs = row![
            button::labeled(
                tr(lang, "label-select-replace"),
                None,
                ButtonVariant::Tab {
                    active: ui.select_op == photon_protocol::SelectionBooleanOp::Replace,
                },
                Some(EditorMessage::EditorSetSelectOp(
                    photon_protocol::SelectionBooleanOp::Replace
                )),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "label-select-add"),
                None,
                ButtonVariant::Tab {
                    active: ui.select_op == photon_protocol::SelectionBooleanOp::Add,
                },
                Some(EditorMessage::EditorSetSelectOp(
                    photon_protocol::SelectionBooleanOp::Add
                )),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "label-select-subtract"),
                None,
                ButtonVariant::Tab {
                    active: ui.select_op == photon_protocol::SelectionBooleanOp::Subtract,
                },
                Some(EditorMessage::EditorSetSelectOp(
                    photon_protocol::SelectionBooleanOp::Subtract
                )),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "label-select-intersect"),
                None,
                ButtonVariant::Tab {
                    active: ui.select_op == photon_protocol::SelectionBooleanOp::Intersect,
                },
                Some(EditorMessage::EditorSetSelectOp(
                    photon_protocol::SelectionBooleanOp::Intersect
                )),
            )
            .width(Fill),
        ]
        .spacing(4);

        let mut body = column![
            sub_row,
            kv_row(tr(lang, "label-select-mode"), String::new()),
            op_tabs,
        ]
        .spacing(8);

        if ui.tool.is_smart_group() {
            if ui.tool == EditorTool::QuickSelect {
                body = body.push(slider_stepper_control(
                    tr(lang, "label-brush-size"),
                    ui.quick_select_radius,
                    4.0..=80.0,
                    1.0_f32,
                    format!("{:.0} px", ui.quick_select_radius),
                    EditorMessage::EditorSetQuickSelectRadius,
                ));
            } else {
                body = body
                    .push(slider_stepper_control(
                        tr(lang, "label-tolerance"),
                        f32::from(ui.wand_tolerance),
                        0.0..=100.0,
                        1.0_f32,
                        format!("{}", ui.wand_tolerance),
                        |v| EditorMessage::EditorSetWandTolerance(v.round() as u8),
                    ))
                    .push(
                        checkbox(ui.wand_contiguous)
                            .label(tr(lang, "label-contiguous"))
                            .on_toggle(|_| EditorMessage::EditorToggleWandContiguous),
                    );
            }
            body = body.push(
                checkbox(ui.select_sample_all)
                    .label(tr(lang, "label-sample-all-layers"))
                    .on_toggle(|_| EditorMessage::EditorToggleSelectSampleAll),
            );
        } else if !matches!(ui.tool, EditorTool::SingleRow | EditorTool::SingleCol) {
            body = body
                .push(slider_stepper_control(
                    tr(lang, "label-feather"),
                    ui.select_feather,
                    0.0..=25.0,
                    1.0_f32,
                    format!("{:.0} px", ui.select_feather),
                    EditorMessage::EditorSetSelectFeather,
                ))
                .push(
                    checkbox(ui.select_anti_alias)
                        .label(tr(lang, "label-anti-alias"))
                        .on_toggle(|_| EditorMessage::EditorToggleSelectAntiAlias),
                );
        }

        return Some(section_card(tr(lang, ui.tool.label_key()), body));
    }

    match ui.tool {
        EditorTool::Brush | EditorTool::Eraser => {
            let title = if ui.tool == EditorTool::Eraser {
                tr(lang, "tool-eraser")
            } else {
                tr(lang, "tool-brush")
            };
            let tip_label = |tip: BrushTip| match tip {
                BrushTip::Round => tr(lang, "tip-round"),
                BrushTip::Flat => tr(lang, "tip-flat"),
                BrushTip::Chalk => tr(lang, "tip-chalk"),
                BrushTip::Spray => tr(lang, "tip-spray"),
                BrushTip::Pixel => tr(lang, "tip-pixel"),
            };
            let mut tip_buttons: Vec<Element<'_, EditorMessage>> = BrushTip::ALL
                .into_iter()
                .map(|tip| {
                    button::labeled(
                        tip_label(tip),
                        None,
                        ButtonVariant::Tab {
                            active: ui.brush_settings.tip == tip,
                        },
                        Some(EditorMessage::EditorSetBrushTip(tip)),
                    )
                    .width(Fill)
                    .into()
                })
                .collect();
            let second_tips = tip_buttons.split_off(3);
            let tips_grid =
                column![row(tip_buttons).spacing(4), row(second_tips).spacing(4),].spacing(4);
            let smoothing_label = |m: BrushSmoothingMode| match m {
                BrushSmoothingMode::None => tr(lang, "smoothing-none"),
                BrushSmoothingMode::Basic => tr(lang, "smoothing-basic"),
                BrushSmoothingMode::Weighted => tr(lang, "smoothing-weighted"),
                BrushSmoothingMode::Stabilizer => tr(lang, "smoothing-stabilizer"),
            };
            let mut smoothing_btns: Vec<Element<'_, EditorMessage>> = [
                BrushSmoothingMode::None,
                BrushSmoothingMode::Basic,
                BrushSmoothingMode::Weighted,
                BrushSmoothingMode::Stabilizer,
            ]
            .into_iter()
            .map(|mode| {
                button::labeled(
                    smoothing_label(mode),
                    None,
                    ButtonVariant::Tab {
                        active: ui.brush_settings.smoothing_mode == mode,
                    },
                    Some(EditorMessage::EditorSetBrushSmoothingMode(mode)),
                )
                .width(Fill)
                .into()
            })
            .collect();
            let smoothing_row2 = smoothing_btns.split_off(2);
            let smoothing_grid = column![
                row(smoothing_btns).spacing(4),
                row(smoothing_row2).spacing(4)
            ]
            .spacing(4);
            let mut col = column![
                slider_stepper_control(
                    tr(lang, "label-brush-size"),
                    ui.brush_radius,
                    1.0..=120.0,
                    1.0_f32,
                    format!("{:.0} px", ui.brush_radius),
                    EditorMessage::EditorSetBrushRadius,
                ),
                slider_stepper_control(
                    tr(lang, "label-brush-opacity"),
                    ui.brush_opacity,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.brush_opacity * 100.0),
                    EditorMessage::EditorSetBrushOpacity,
                ),
                slider_stepper_control(
                    tr(lang, "label-brush-hardness"),
                    ui.brush_settings.hardness,
                    0.0..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.brush_settings.hardness * 100.0),
                    EditorMessage::EditorSetBrushHardness,
                ),
                slider_stepper_control(
                    tr(lang, "label-brush-flow"),
                    ui.brush_settings.flow,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.brush_settings.flow * 100.0),
                    EditorMessage::EditorSetBrushFlow,
                ),
                slider_stepper_control(
                    tr(lang, "label-brush-spacing"),
                    ui.brush_settings.spacing,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.brush_settings.spacing * 100.0),
                    EditorMessage::EditorSetBrushSpacing,
                ),
                text(tr(lang, "label-brush-tip")).size(11),
                tips_grid,
                text(tr(lang, "label-brush-smoothing")).size(11),
                smoothing_grid,
            ]
            .spacing(6);
            if ui.brush_settings.smoothing_mode == BrushSmoothingMode::Stabilizer {
                col = col.push(slider_stepper_control(
                    tr(lang, "label-stabilizer-distance"),
                    ui.brush_settings.stabilizer_distance,
                    4.0..=100.0,
                    1.0_f32,
                    format!("{:.0} px", ui.brush_settings.stabilizer_distance),
                    EditorMessage::EditorSetStabilizerDistance,
                ));
            }
            Some(section_card(title, col))
        }
        EditorTool::DrawLine
        | EditorTool::DrawRect
        | EditorTool::DrawEllipse
        | EditorTool::DrawPolygon
        | EditorTool::DrawPolyline
        | EditorTool::BezierPen => {
            let mut body = column![].spacing(8);
            if ui.tool.is_shape_group() {
                body = body.push(subtool_icon_row(
                    lang,
                    ui.tool,
                    &[
                        EditorTool::DrawRect,
                        EditorTool::DrawEllipse,
                        EditorTool::DrawLine,
                        EditorTool::DrawPolygon,
                        EditorTool::DrawPolyline,
                    ],
                ));
            }
            let fill_label = |m: ShapeFillMode| match m {
                ShapeFillMode::Stroke => tr(lang, "shape-fill-stroke"),
                ShapeFillMode::Fill => tr(lang, "shape-fill-fill"),
                ShapeFillMode::StrokeAndFill => tr(lang, "shape-fill-stroke-and-fill"),
            };
            let fill_row = row([
                ShapeFillMode::Stroke,
                ShapeFillMode::Fill,
                ShapeFillMode::StrokeAndFill,
            ]
            .into_iter()
            .map(|m| {
                button::labeled(
                    fill_label(m),
                    None,
                    ButtonVariant::Tab {
                        active: ui.shape_fill_mode == m,
                    },
                    Some(EditorMessage::EditorSetShapeFillMode(m)),
                )
                .width(Fill)
                .into()
            }))
            .spacing(4);
            body = body
                .push(text(tr(lang, "label-shape-fill-mode")).size(11))
                .push(fill_row)
                .push(slider_stepper_control(
                    tr(lang, "label-stroke-width"),
                    ui.shape_stroke_width,
                    1.0..=64.0,
                    1.0_f32,
                    format!("{:.0} px", ui.shape_stroke_width),
                    EditorMessage::EditorSetShapeStrokeWidth,
                ))
                .push(vector_ui::build_vector_shape_extra_controls(
                    state, ed, lang,
                ));
            Some(section_card(tr(lang, ui.tool.label_key()), body))
        }
        EditorTool::PathSelect => Some(vector_ui::build_path_select_tool_options(state, ed, lang)),
        EditorTool::Text => Some(text_ui::build_text_tool_options(state, ed, lang)),
        EditorTool::PaintBucket => {
            let body = column![
                slider_stepper_control(
                    tr(lang, "label-flood-fill-tolerance"),
                    f32::from(ui.flood_fill_tolerance),
                    0.0..=255.0,
                    1.0_f32,
                    format!("{}", ui.flood_fill_tolerance),
                    |v| EditorMessage::EditorSetFloodFillTolerance(v.round() as u8),
                ),
                slider_stepper_control(
                    tr(lang, "label-opacity"),
                    ui.flood_fill_opacity,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.flood_fill_opacity * 100.0),
                    EditorMessage::EditorSetFloodFillOpacity,
                ),
                checkbox(ui.flood_fill_contiguous)
                    .label(tr(lang, "label-flood-fill-contiguous"))
                    .on_toggle(|_| EditorMessage::EditorToggleFloodFillContiguous),
                checkbox(ui.flood_fill_sample_all)
                    .label(tr(lang, "label-sample-all-layers"))
                    .on_toggle(|_| EditorMessage::EditorToggleFloodFillSampleAll),
            ]
            .spacing(8);
            Some(section_card(tr(lang, ui.tool.label_key()), body))
        }
        EditorTool::Gradient => Some(build_gradient_tool_options(ui, lang)),
        EditorTool::HealingBrush => {
            let mode_row = row(HealingMode::ALL.into_iter().map(|m| {
                let key = match m {
                    HealingMode::Spot => "healing-mode-spot",
                    HealingMode::Sampled => "healing-mode-sampled",
                };
                button::labeled(
                    tr(lang, key),
                    None,
                    ButtonVariant::Tab {
                        active: ui.healing_mode == m,
                    },
                    Some(EditorMessage::EditorSetHealingMode(m)),
                )
                .width(Fill)
                .into()
            }))
            .spacing(4);

            let source_label = if let Some([sx, sy]) = ui.healing_source_px {
                let mut args = BTreeMap::new();
                args.insert("x".to_owned(), format!("{sx:.0}"));
                args.insert("y".to_owned(), format!("{sy:.0}"));
                tr_args(lang, "label-healing-source-set", &args)
            } else {
                tr(lang, "label-healing-source-unset")
            };

            let mut body = column![
                text(tr(lang, "label-healing-mode")).size(11),
                mode_row,
                slider_stepper_control(
                    tr(lang, "label-brush-size"),
                    ui.healing_radius,
                    2.0..=120.0,
                    1.0_f32,
                    format!("{:.0} px", ui.healing_radius),
                    EditorMessage::EditorSetHealingRadius,
                ),
                slider_stepper_control(
                    tr(lang, "label-brush-hardness"),
                    ui.healing_hardness,
                    0.0..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.healing_hardness * 100.0),
                    EditorMessage::EditorSetHealingHardness,
                ),
                slider_stepper_control(
                    tr(lang, "label-strength"),
                    ui.healing_strength,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", ui.healing_strength * 100.0),
                    EditorMessage::EditorSetHealingStrength,
                ),
                checkbox(ui.healing_sample_all)
                    .label(tr(lang, "label-sample-all-layers"))
                    .on_toggle(|_| EditorMessage::EditorToggleHealingSampleAll),
            ]
            .spacing(6);

            if ui.healing_mode == HealingMode::Sampled {
                body =
                    body.push(text(source_label).size(TEXT_SIZE_CAPTION).style(|theme| {
                        text::Style {
                            color: Some(PaletteTokens::for_theme(theme).text_secondary),
                        }
                    }))
                    .push(
                        text(tr(lang, "hint-healing-source"))
                            .size(TEXT_SIZE_CAPTION)
                            .style(|theme| text::Style {
                                color: Some(PaletteTokens::for_theme(theme).text_muted),
                            }),
                    );
            }

            Some(section_card(tr(lang, "tool-healing-brush"), body))
        }
        EditorTool::Transform => {
            let spec = ui.transform_spec;
            let max_tx = (ed.width as f32).max(200.0);
            let max_ty = (ed.height as f32).max(200.0);

            let mut flipped_h = spec;
            flipped_h.flip_h = !flipped_h.flip_h;
            let mut flipped_v = spec;
            flipped_v.flip_v = !flipped_v.flip_v;

            let body = column![
                slider_stepper_control(
                    tr(lang, "label-transform-x"),
                    spec.translate_x,
                    -max_tx..=max_tx,
                    1.0_f32,
                    format!("{:.1} px", spec.translate_x),
                    move |v| {
                        let mut next = spec;
                        next.translate_x = v;
                        EditorMessage::EditorSetTransformSpec(next)
                    },
                ),
                slider_stepper_control(
                    tr(lang, "label-transform-y"),
                    spec.translate_y,
                    -max_ty..=max_ty,
                    1.0_f32,
                    format!("{:.1} px", spec.translate_y),
                    move |v| {
                        let mut next = spec;
                        next.translate_y = v;
                        EditorMessage::EditorSetTransformSpec(next)
                    },
                ),
                slider_stepper_control(
                    tr(lang, "label-transform-width"),
                    (spec.scale_x * 100.0).clamp(10.0, 400.0),
                    10.0..=400.0,
                    1.0_f32,
                    format!("{:.0}%", spec.scale_x * 100.0),
                    move |v| {
                        let mut next = spec;
                        next.scale_x = v / 100.0;
                        EditorMessage::EditorSetTransformSpec(next)
                    },
                ),
                slider_stepper_control(
                    tr(lang, "label-transform-height"),
                    (spec.scale_y * 100.0).clamp(10.0, 400.0),
                    10.0..=400.0,
                    1.0_f32,
                    format!("{:.0}%", spec.scale_y * 100.0),
                    move |v| {
                        let mut next = spec;
                        next.scale_y = v / 100.0;
                        EditorMessage::EditorSetTransformSpec(next)
                    },
                ),
                slider_stepper_control(
                    tr(lang, "label-transform-angle"),
                    spec.rotation_deg.clamp(-180.0, 180.0),
                    -180.0..=180.0,
                    0.5_f32,
                    format!("{:.1}°", spec.rotation_deg),
                    move |v| {
                        let mut next = spec;
                        next.rotation_deg = v;
                        EditorMessage::EditorSetTransformSpec(next)
                    },
                ),
                row![
                    button::labeled(
                        tr(lang, "action-flip-h"),
                        Some(Icon::FlipH),
                        ButtonVariant::Tab {
                            active: spec.flip_h,
                        },
                        Some(EditorMessage::EditorSetTransformSpec(flipped_h)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "action-flip-v"),
                        Some(Icon::FlipV),
                        ButtonVariant::Tab {
                            active: spec.flip_v,
                        },
                        Some(EditorMessage::EditorSetTransformSpec(flipped_v)),
                    )
                    .width(Fill),
                ]
                .spacing(4),
                row![
                    button::labeled(
                        tr(lang, "action-reset"),
                        Some(Icon::Reset),
                        ButtonVariant::Ghost,
                        (!spec.is_identity()).then_some(EditorMessage::EditorSetTransformSpec(
                            LayerTransformSpec::default()
                        )),
                    ),
                    button::labeled(
                        tr(lang, "dialog-cancel"),
                        None,
                        ButtonVariant::Secondary,
                        Some(EditorMessage::EditorTransformCancel),
                    ),
                    with_tooltip_pos(
                        button::labeled(
                            tr(lang, "action-apply-transform"),
                            Some(Icon::Check),
                            ButtonVariant::Primary,
                            Some(EditorMessage::EditorTransformCommit),
                        )
                        .width(Fill),
                        format!("{} (Enter)", tr(lang, "action-apply-transform")),
                        TooltipPosition::Top,
                    ),
                ]
                .spacing(4),
            ]
            .spacing(6);

            Some(section_card(tr(lang, "tool-transform"), body))
        }
        EditorTool::Liquify => {
            let mode_label = |m: LiquifyMode| match m {
                LiquifyMode::Push => tr(lang, "liquify-push"),
                LiquifyMode::Bloat => tr(lang, "liquify-bloat"),
                LiquifyMode::Pucker => tr(lang, "liquify-pucker"),
                LiquifyMode::TwirlCw => tr(lang, "liquify-twirl-cw"),
                LiquifyMode::TwirlCcw => tr(lang, "liquify-twirl-ccw"),
                LiquifyMode::Reconstruct => tr(lang, "liquify-reconstruct"),
            };
            let mut mode_btns: Vec<Element<'_, EditorMessage>> = LiquifyMode::ALL
                .into_iter()
                .map(|m| {
                    button::labeled(
                        mode_label(m),
                        None,
                        ButtonVariant::Tab {
                            active: ui.liquify_mode == m,
                        },
                        Some(EditorMessage::EditorSetLiquifyMode(m)),
                    )
                    .width(Fill)
                    .into()
                })
                .collect();
            let row3 = mode_btns.split_off(4);
            let row2 = mode_btns.split_off(2);
            let mode_grid = column![
                row(mode_btns).spacing(4),
                row(row2).spacing(4),
                row(row3).spacing(4),
            ]
            .spacing(4);
            Some(section_card(
                tr(lang, "tool-liquify"),
                column![
                    mode_grid,
                    slider_stepper_control(
                        tr(lang, "label-brush-size"),
                        ui.liquify_radius,
                        5.0..=150.0,
                        1.0_f32,
                        format!("{:.0} px", ui.liquify_radius),
                        EditorMessage::EditorSetLiquifyRadius,
                    ),
                    slider_stepper_control(
                        tr(lang, "label-strength"),
                        ui.liquify_strength,
                        0.05..=1.0,
                        0.05_f32,
                        format!("{:.0}%", ui.liquify_strength * 100.0),
                        EditorMessage::EditorSetLiquifyStrength,
                    ),
                ]
                .spacing(6),
            ))
        }
        EditorTool::Eyedropper => {
            let swatch = ui.sampled_color.map(|c| Color::from_rgb8(c[0], c[1], c[2]));
            let hex = ui
                .sampled_color
                .map(|c| format!("#{:02X}{:02X}{:02X} (A:{})", c[0], c[1], c[2], c[3]))
                .unwrap_or_else(|| "-".to_owned());
            Some(section_card(
                tr(lang, "tool-eyedropper"),
                row![
                    checker_swatch(swatch, 26.0, 26.0, None),
                    column![
                        text(tr(lang, "label-sampled-color")).size(11),
                        text(hex).size(12),
                    ]
                    .spacing(2),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
        }
        EditorTool::RotateAngle => Some(section_card(
            tr(lang, "tool-rotate-angle"),
            column![
                slider_stepper_control(
                    tr(lang, "label-angle"),
                    ui.rotate_angle as f32,
                    -180.0..=180.0,
                    0.5_f32,
                    format!("{:.1}°", ui.rotate_angle),
                    |v| EditorMessage::EditorSetRotateAngle(f64::from(v)),
                ),
                row![
                    button::labeled(
                        "-45°",
                        None,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorSetRotateAngle(-45.0)),
                    ),
                    button::labeled(
                        "-15°",
                        None,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorSetRotateAngle(-15.0)),
                    ),
                    button::labeled(
                        "0°",
                        None,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorSetRotateAngle(0.0)),
                    ),
                    button::labeled(
                        "+15°",
                        None,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorSetRotateAngle(15.0)),
                    ),
                    button::labeled(
                        "+45°",
                        None,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorSetRotateAngle(45.0)),
                    ),
                ]
                .spacing(3),
                row![
                    text(tr(lang, "label-canvas-bg")).size(12).width(Fill),
                    checker_swatch(
                        ui.rotate_fill.map(rgb_to_color),
                        24.0,
                        18.0,
                        Some(EditorMessage::OpenColorPicker(
                            ColorPickerTarget::RotateFill
                        )),
                    ),
                ]
                .align_y(Alignment::Center),
                with_tooltip_pos(
                    button::labeled(
                        tr(lang, "action-apply-rotate"),
                        Some(Icon::Check),
                        ButtonVariant::Primary,
                        (ui.rotate_angle.abs() > 0.01)
                            .then_some(EditorMessage::EditorApplyRotateAngle),
                    )
                    .width(Fill),
                    tr(lang, "action-apply-rotate"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(8),
        )),
        EditorTool::RemoveBg => Some(build_ai_rembg_tool_options(state, lang)),
        EditorTool::Watermark => Some(build_ai_watermark_tool_options(state, lang)),
        EditorTool::Caption => Some(build_ai_caption_tool_options(state, lang)),
        EditorTool::Pointer if ed.active_vector_data.is_some() => {
            Some(vector_ui::build_path_select_tool_options(state, ed, lang))
        }
        EditorTool::Pointer if ed.active_text_data.is_some() => {
            Some(text_ui::build_text_tool_options(state, ed, lang))
        }
        _ => None,
    }
}
