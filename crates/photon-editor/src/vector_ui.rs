//! Vector layer, path selection (`A`), Transform Each, and Vector Blend UI builders.

use iced::widget::{checkbox, column, container, row, rule, text_input};
use iced::{Alignment, Element, Fill};
use photon_i18n::tr;
use photon_panels::{kv_row, section_card};
use photon_protocol::{
    CanvasAnchor, Command, DrawShapeKind, EditorState, Language, ShapeFillMode, TransformEachSpec,
    VectorBlendSpec, VectorGeometry, VectorLayerData, VectorNodeMode, VectorPathNode,
    VectorPathOperation, VectorShape, VectorStroke,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::dialog::DialogSpec;
use photon_ui::editor::{anchor_grid_3x3, checker_swatch};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuEntry, MenuItem};
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, hairline_rule_style, input_style,
};
use photon_ui::typography::text;
use photon_ui::{TooltipPosition, with_tooltip_pos};

use crate::{
    ColorPickerTarget, EditorMessage, EditorTool, EditorViewState, rgb_to_color,
    slider_stepper_control,
};

/// Build the top-level `Vector` menu entries in the Editor menubar.
pub fn build_editor_vector_menu_entries<'a>(
    state: EditorViewState<'a>,
) -> Vec<MenuEntry<'a, EditorMessage>> {
    let lang = state.snapshot.meta.language;
    let ed = state.snapshot.meta.editor.as_ref();
    let has_doc = ed.is_some();
    let next_idx = ed.map_or(1, |e| e.layers.len() + 1);
    let vec_data = ed.and_then(|e| e.active_vector_data.as_ref());
    let has_vector_layer = vec_data.is_some();
    let has_vector_shape = vec_data.is_some_and(|v| !v.shapes.is_empty());
    let has_two_shapes = vec_data.is_some_and(|v| v.shapes.len() >= 2);
    let active_non_raster = ed
        .and_then(|e| e.layers.iter().find(|l| l.id == e.active_layer_id))
        .is_some_and(|l| !l.locked && !l.kind.is_raster());
    let split_idx = state.editor_ui.selected_vector_node.unwrap_or(1);

    vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-new-layer"),
                EditorMessage::Dispatch(Command::EditorAddVectorLayer {
                    name: format!("Vector {next_idx}"),
                    data: VectorLayerData::new(),
                }),
            )
            .icon(Icon::BezierPen)
            .enabled(has_doc),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-convert-to-path"),
                EditorMessage::Dispatch(Command::EditorConvertShapeToPath {
                    layer_id: None,
                    shape_id: None,
                }),
            )
            .icon(Icon::PathSelect)
            .enabled(has_vector_shape),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-join-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::JoinPaths {
                        second_shape_id: None,
                        tolerance: 24.0,
                    },
                }),
            )
            .enabled(has_two_shapes),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-split-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::SplitAtNode { index: split_idx },
                }),
            )
            .enabled(has_vector_shape),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-close-open-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::ToggleClosed,
                }),
            )
            .enabled(has_vector_shape),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-reverse-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::ReversePath,
                }),
            )
            .enabled(has_vector_shape),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-simplify-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::Simplify {
                        tolerance: state.editor_ui.vector_simplify_tolerance,
                    },
                }),
            )
            .enabled(has_vector_shape),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-smooth-path"),
                EditorMessage::Dispatch(Command::EditorVectorPathOp {
                    layer_id: None,
                    shape_id: None,
                    op: VectorPathOperation::Smooth {
                        tension: state.editor_ui.vector_smooth_tension,
                    },
                }),
            )
            .enabled(has_vector_shape),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-transform-each"),
                EditorMessage::OpenTransformEachDialog,
            )
            .icon(Icon::Transform)
            .enabled(has_doc),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-blend"),
                EditorMessage::OpenVectorBlendDialog,
            )
            .icon(Icon::Duplicate)
            .enabled(has_two_shapes),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-rasterize"),
                EditorMessage::Dispatch(Command::EditorRasterizeLayer { id: None }),
            )
            .icon(Icon::Layers)
            .enabled(active_non_raster),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-vector-export-svg"),
                EditorMessage::OpenExportVectorSvgDialog,
            )
            .icon(Icon::Export)
            .enabled(has_vector_layer),
        ),
    ]
}

/// Extra vector controls appended to Shape (`U`) and Pen (`P`) tool options.
pub fn build_vector_shape_extra_controls<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = state.editor_ui;
    let is_vector_layer = ed.active_vector_data.is_some();
    let next_idx = ed.layers.len() + 1;

    let mut col = column![
        rule::horizontal(1).style(hairline_rule_style),
        checkbox(ui.vector_mode || is_vector_layer)
            .label(tr(lang, "label-vector-shape-mode"))
            .text_size(TEXT_SIZE_CONTROL)
            .on_toggle(EditorMessage::EditorSetVectorMode),
    ]
    .spacing(6);

    if ui.tool == EditorTool::DrawRect {
        col = col.push(slider_stepper_control(
            tr(lang, "label-corner-radius"),
            ui.shape_corner_radius,
            0.0..=120.0,
            1.0_f32,
            format!("{:.0} px", ui.shape_corner_radius),
            EditorMessage::EditorSetShapeCornerRadius,
        ));
    }

    if !is_vector_layer {
        col = col.push(
            button::labeled(
                tr(lang, "menu-vector-new-layer"),
                Some(Icon::BezierPen),
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorAddVectorLayer {
                    name: format!("Vector {next_idx}"),
                    data: VectorLayerData::new(),
                })),
            )
            .width(Fill),
        );
    }

    col.into()
}

/// Tool Options card for `EditorTool::PathSelect` (`A` — Path / Direct Node Selection).
pub fn build_path_select_tool_options<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = state.editor_ui;
    let next_idx = ed.layers.len() + 1;

    let Some(vdata) = &ed.active_vector_data else {
        let body = column![
            text(tr(lang, "hint-vector-no-active-layer"))
                .size(TEXT_SIZE_CAPTION)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            button::labeled(
                tr(lang, "menu-vector-new-layer"),
                Some(Icon::BezierPen),
                ButtonVariant::Primary,
                Some(EditorMessage::Dispatch(Command::EditorAddVectorLayer {
                    name: format!("Vector {next_idx}"),
                    data: VectorLayerData::new(),
                })),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "menu-vector-transform-each"),
                Some(Icon::Transform),
                ButtonVariant::Secondary,
                Some(EditorMessage::OpenTransformEachDialog),
            )
            .width(Fill),
        ]
        .spacing(8);
        return section_card(tr(lang, "tool-path-select"), body);
    };

    let active_shape: Option<&VectorShape> = vdata
        .selected_shape_ids
        .first()
        .and_then(|id| vdata.shape(*id))
        .or_else(|| vdata.shapes.last());

    let mut body = column![kv_row(
        tr(lang, "label-vector-shapes"),
        vdata.shapes.len().to_string(),
    )]
    .spacing(8);

    if let Some(shape) = active_shape {
        let shape_id = shape.id;
        let is_parametric = !matches!(shape.geometry, VectorGeometry::Path { .. });

        let has_fill = shape.fill.is_some();
        let fill_rgb = shape.fill.unwrap_or(ui.fg_color);
        let stroke_rgb = shape.stroke.color;

        let mut shape_toggle_fill = shape.clone();
        shape_toggle_fill.fill = if has_fill { None } else { Some(ui.fg_color) };

        body = body.push(
            row![
                checkbox(has_fill)
                    .label(tr(lang, "shape-fill-fill"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(move |_| {
                        EditorMessage::EditorUpdateActiveVectorShape(shape_toggle_fill.clone())
                    }),
                iced::widget::space::horizontal(),
                checker_swatch(
                    has_fill.then_some(rgb_to_color(fill_rgb)),
                    28.0,
                    20.0,
                    Some(EditorMessage::OpenColorPicker(
                        ColorPickerTarget::VectorShapeFill,
                    )),
                ),
                text(tr(lang, "shape-fill-stroke")).size(TEXT_SIZE_CAPTION),
                checker_swatch(
                    (shape.stroke.width > 0.0).then_some(rgb_to_color(stroke_rgb)),
                    28.0,
                    20.0,
                    Some(EditorMessage::OpenColorPicker(
                        ColorPickerTarget::VectorShapeStroke,
                    )),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );

        let shape_for_width = shape.clone();
        body = body.push(slider_stepper_control(
            tr(lang, "label-stroke-width"),
            shape.stroke.width,
            0.0..=64.0,
            1.0_f32,
            format!("{:.0} px", shape.stroke.width),
            move |w| {
                let mut next = shape_for_width.clone();
                next.stroke.width = w;
                EditorMessage::EditorUpdateActiveVectorShape(next)
            },
        ));

        if let VectorGeometry::Rect { corner_radius, .. } = shape.geometry {
            let shape_for_radius = shape.clone();
            body = body.push(slider_stepper_control(
                tr(lang, "label-corner-radius"),
                corner_radius,
                0.0..=120.0,
                1.0_f32,
                format!("{corner_radius:.0} px"),
                move |r| {
                    let mut next = shape_for_radius.clone();
                    if let VectorGeometry::Rect {
                        corner_radius: ref mut cr,
                        ..
                    } = next.geometry
                    {
                        *cr = r;
                    }
                    EditorMessage::EditorUpdateActiveVectorShape(next)
                },
            ));
        }

        if is_parametric {
            body = body.push(
                button::labeled(
                    tr(lang, "menu-vector-convert-to-path"),
                    Some(Icon::PathSelect),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::Dispatch(Command::EditorConvertShapeToPath {
                        layer_id: None,
                        shape_id: Some(shape_id),
                    })),
                )
                .width(Fill),
            );
        }

        if let Some(node_idx) = ui.selected_vector_node {
            let active_mode = match &shape.geometry {
                VectorGeometry::Path { nodes, .. } => {
                    nodes.get(node_idx).map(|n| n.inferred_mode())
                }
                _ => None,
            };
            let mode_btn = |label: String, mode: VectorNodeMode| {
                button::labeled(
                    label,
                    None,
                    ButtonVariant::Tab {
                        active: active_mode == Some(mode),
                    },
                    Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                        layer_id: None,
                        shape_id: Some(shape_id),
                        op: VectorPathOperation::SetNodeMode {
                            index: node_idx,
                            mode,
                        },
                    })),
                )
                .width(Fill)
            };

            body = body
                .push(rule::horizontal(1).style(hairline_rule_style))
                .push(text(format!("{} #{node_idx}", tr(lang, "label-vector-node"))).size(11))
                .push(
                    row![
                        mode_btn(tr(lang, "node-mode-corner"), VectorNodeMode::Corner),
                        mode_btn(tr(lang, "node-mode-smooth"), VectorNodeMode::Smooth),
                        mode_btn(tr(lang, "node-mode-symmetric"), VectorNodeMode::Symmetric),
                    ]
                    .spacing(4),
                )
                .push(
                    row![
                        button::labeled(
                            tr(lang, "menu-vector-split-path"),
                            None,
                            ButtonVariant::Secondary,
                            Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                                layer_id: None,
                                shape_id: Some(shape_id),
                                op: VectorPathOperation::SplitAtNode { index: node_idx },
                            })),
                        )
                        .width(Fill),
                        button::labeled(
                            tr(lang, "action-delete"),
                            Some(Icon::Trash),
                            ButtonVariant::Danger,
                            Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                                layer_id: None,
                                shape_id: Some(shape_id),
                                op: VectorPathOperation::DeleteNode { index: node_idx },
                            })),
                        )
                        .width(Fill),
                    ]
                    .spacing(4),
                );
        }

        body = body
            .push(rule::horizontal(1).style(hairline_rule_style))
            .push(
                row![
                    button::labeled(
                        tr(lang, "menu-vector-close-open-path"),
                        None,
                        ButtonVariant::Secondary,
                        Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::ToggleClosed,
                        })),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "menu-vector-reverse-path"),
                        Some(Icon::Swap),
                        ButtonVariant::Secondary,
                        Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::ReversePath,
                        })),
                    )
                    .width(Fill),
                ]
                .spacing(4),
            )
            .push(
                row![
                    button::labeled(
                        tr(lang, "menu-vector-join-path"),
                        None,
                        ButtonVariant::Secondary,
                        (vdata.shapes.len() >= 2).then_some(EditorMessage::Dispatch(
                            Command::EditorVectorPathOp {
                                layer_id: None,
                                shape_id: Some(shape_id),
                                op: VectorPathOperation::JoinPaths {
                                    second_shape_id: None,
                                    tolerance: 24.0,
                                },
                            },
                        )),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "action-delete-shape"),
                        Some(Icon::Trash),
                        ButtonVariant::Secondary,
                        Some(EditorMessage::Dispatch(Command::EditorDeleteVectorShape {
                            layer_id: None,
                            shape_id,
                        })),
                    )
                    .width(Fill),
                ]
                .spacing(4),
            )
            .push(slider_stepper_control(
                tr(lang, "label-simplify-tolerance"),
                ui.vector_simplify_tolerance,
                0.5..=20.0,
                0.5_f32,
                format!("{:.1} px", ui.vector_simplify_tolerance),
                EditorMessage::EditorSetVectorSimplifyTolerance,
            ))
            .push(slider_stepper_control(
                tr(lang, "label-smooth-tension"),
                ui.vector_smooth_tension,
                0.05..=1.0,
                0.05_f32,
                format!("{:.0}%", ui.vector_smooth_tension * 100.0),
                EditorMessage::EditorSetVectorSmoothTension,
            ))
            .push(
                row![
                    button::labeled(
                        tr(lang, "menu-vector-simplify-path"),
                        None,
                        ButtonVariant::Secondary,
                        Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::Simplify {
                                tolerance: ui.vector_simplify_tolerance,
                            },
                        })),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "menu-vector-smooth-path"),
                        None,
                        ButtonVariant::Secondary,
                        Some(EditorMessage::Dispatch(Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::Smooth {
                                tension: ui.vector_smooth_tension,
                            },
                        })),
                    )
                    .width(Fill),
                ]
                .spacing(4),
            );
    }

    body = body
        .push(rule::horizontal(1).style(hairline_rule_style))
        .push(
            row![
                button::labeled(
                    tr(lang, "menu-vector-transform-each"),
                    Some(Icon::Transform),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::OpenTransformEachDialog),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "menu-vector-blend"),
                    Some(Icon::Duplicate),
                    ButtonVariant::Secondary,
                    (vdata.shapes.len() >= 2).then_some(EditorMessage::OpenVectorBlendDialog),
                )
                .width(Fill),
            ]
            .spacing(4),
        )
        .push(
            row![
                button::labeled(
                    tr(lang, "action-layer-rasterize"),
                    Some(Icon::Layers),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::Dispatch(Command::EditorRasterizeLayer {
                        id: None,
                    })),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "menu-vector-export-svg"),
                    Some(Icon::Export),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::OpenExportVectorSvgDialog),
                )
                .width(Fill),
            ]
            .spacing(4),
        );

    section_card(tr(lang, "tool-path-select"), body)
}

/// Build the `Transform Each` dialog (`Scale W/H %`, `Move X/Y`, `Rotate°`, `Reflect X/Y`,
/// 9-point anchor, `Repeat Step`, `Apply to Duplicate`, right-aligned `Cancel`/`Apply`).
pub fn build_transform_each_dialog<'a>(
    lang: Language,
    spec: &'a TransformEachSpec,
) -> DialogSpec<'a, EditorMessage> {
    let s = *spec;
    let active_anchor_idx = CanvasAnchor::ALL
        .iter()
        .position(|a| *a == spec.anchor)
        .unwrap_or(4);

    let body = column![
        slider_stepper_control(
            tr(lang, "label-transform-width"),
            (spec.scale_x * 100.0).clamp(10.0, 300.0),
            10.0..=300.0,
            1.0_f32,
            format!("{:.0}%", spec.scale_x * 100.0),
            move |v| {
                let mut next = s;
                next.scale_x = v / 100.0;
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-transform-height"),
            (spec.scale_y * 100.0).clamp(10.0, 300.0),
            10.0..=300.0,
            1.0_f32,
            format!("{:.0}%", spec.scale_y * 100.0),
            move |v| {
                let mut next = s;
                next.scale_y = v / 100.0;
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-transform-x"),
            spec.translate_x.clamp(-500.0, 500.0),
            -500.0..=500.0,
            1.0_f32,
            format!("{:.0} px", spec.translate_x),
            move |v| {
                let mut next = s;
                next.translate_x = v;
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-transform-y"),
            spec.translate_y.clamp(-500.0, 500.0),
            -500.0..=500.0,
            1.0_f32,
            format!("{:.0} px", spec.translate_y),
            move |v| {
                let mut next = s;
                next.translate_y = v;
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-transform-angle"),
            spec.rotation_deg.clamp(-180.0, 180.0),
            -180.0..=180.0,
            1.0_f32,
            format!("{:.0}°", spec.rotation_deg),
            move |v| {
                let mut next = s;
                next.rotation_deg = v;
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-repeat-steps"),
            (spec.repeat_steps.clamp(1, 32)) as f32,
            1.0..=32.0,
            1.0_f32,
            format!("{}×", spec.repeat_steps.max(1)),
            move |v| {
                let mut next = s;
                next.repeat_steps = (v.round() as u32).clamp(1, 32);
                EditorMessage::TransformEachSetSpec(next)
            },
        ),
        row![
            column![
                checkbox(spec.reflect_x)
                    .label(tr(lang, "label-reflect-x"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(move |b| {
                        let mut next = s;
                        next.reflect_x = b;
                        EditorMessage::TransformEachSetSpec(next)
                    }),
                checkbox(spec.reflect_y)
                    .label(tr(lang, "label-reflect-y"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(move |b| {
                        let mut next = s;
                        next.reflect_y = b;
                        EditorMessage::TransformEachSetSpec(next)
                    }),
                checkbox(spec.apply_to_duplicate)
                    .label(tr(lang, "label-apply-to-duplicate"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(move |b| {
                        let mut next = s;
                        next.apply_to_duplicate = b;
                        EditorMessage::TransformEachSetSpec(next)
                    }),
            ]
            .spacing(6)
            .width(Fill),
            column![
                text(tr(lang, "label-canvas-anchor")).size(12),
                anchor_grid_3x3(active_anchor_idx, move |idx| {
                    let mut next = s;
                    next.anchor = CanvasAnchor::ALL[idx];
                    EditorMessage::TransformEachSetSpec(next)
                }),
            ]
            .spacing(4)
            .align_x(Alignment::End),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    let footer = row![
        button::labeled(
            tr(lang, "action-reset"),
            Some(Icon::Reset),
            ButtonVariant::Ghost,
            Some(EditorMessage::TransformEachSetSpec(
                TransformEachSpec::default(),
            )),
        ),
        iced::widget::space::horizontal(),
        button::labeled(
            tr(lang, "dialog-cancel"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::CloseDialog),
        ),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-apply"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                Some(EditorMessage::ConfirmTransformEach),
            ),
            tr(lang, "action-apply"),
            TooltipPosition::Top,
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: tr(lang, "dialog-transform-each-title"),
        width: 460.0,
        body: body.into(),
        footer: footer.into(),
        on_close: EditorMessage::CloseDialog,
        on_noop: EditorMessage::Noop,
    }
}

/// Build the `Vector Blend` dialog (`Steps` 2-line slider + right-aligned `Cancel`/`Apply`).
pub fn build_vector_blend_dialog<'a>(
    lang: Language,
    spec: &'a VectorBlendSpec,
) -> DialogSpec<'a, EditorMessage> {
    let s = *spec;
    let body = column![
        text(tr(lang, "hint-vector-blend"))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
        slider_stepper_control(
            tr(lang, "label-blend-steps"),
            (spec.steps.clamp(1, 64)) as f32,
            1.0..=64.0,
            1.0_f32,
            spec.steps.clamp(1, 64).to_string(),
            move |v| {
                let mut next = s;
                next.steps = (v.round() as u32).clamp(1, 64);
                EditorMessage::VectorBlendSetSpec(next)
            },
        ),
    ]
    .spacing(10);

    let footer = row![
        iced::widget::space::horizontal(),
        button::labeled(
            tr(lang, "dialog-cancel"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::CloseDialog),
        ),
        button::labeled(
            tr(lang, "action-apply"),
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(EditorMessage::ConfirmVectorBlend),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: tr(lang, "dialog-vector-blend-title"),
        width: 400.0,
        body: body.into(),
        footer: footer.into(),
        on_close: EditorMessage::CloseDialog,
        on_noop: EditorMessage::Noop,
    }
}

/// Build the `Export SVG` dialog for exporting the active Vector Layer to an `.svg` file.
pub fn build_export_vector_svg_dialog<'a>(
    lang: Language,
    path: &'a str,
    error: Option<&'a str>,
) -> DialogSpec<'a, EditorMessage> {
    let mut body = column![
        column![
            text(tr(lang, "label-export-file")).size(TEXT_SIZE_CONTROL),
            row![
                text_input("/path/to/vector.svg", path)
                    .on_input(EditorMessage::ExportVectorSvgPathChanged)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([7, 10])
                    .style(input_style)
                    .width(Fill),
                button::labeled(
                    "…",
                    Some(Icon::FolderOpen),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::PickExportVectorSvgDialog),
                ),
            ]
            .spacing(6),
        ]
        .spacing(4),
    ]
    .spacing(10);

    if let Some(err) = error {
        body = body.push(text(err.to_owned()).size(12).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.danger),
            }
        }));
    }

    let footer = row![
        iced::widget::space::horizontal(),
        button::labeled(
            tr(lang, "dialog-cancel"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::CloseDialog),
        ),
        button::labeled(
            tr(lang, "action-export"),
            Some(Icon::Export),
            ButtonVariant::Primary,
            Some(EditorMessage::ConfirmExportVectorSvg),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: tr(lang, "menu-vector-export-svg"),
        width: 460.0,
        body: container(body).width(Fill).into(),
        footer: footer.into(),
        on_close: EditorMessage::CloseDialog,
        on_noop: EditorMessage::Noop,
    }
}

/// Convert a shape tool commit (`DrawShapeKind`) into a non-destructive [`VectorShape`].
#[allow(clippy::too_many_arguments)]
pub fn build_vector_shape_from_draw_commit(
    kind: DrawShapeKind,
    points: &[[f32; 2]],
    nodes: &[VectorPathNode],
    closed: bool,
    fill_mode: ShapeFillMode,
    fg_color: [u8; 3],
    bg_color: Option<[u8; 3]>,
    stroke_width: f32,
    corner_radius: f32,
    opacity: f32,
) -> Option<VectorShape> {
    let fill_color = match fill_mode {
        ShapeFillMode::Stroke => None,
        ShapeFillMode::Fill => Some(fg_color),
        ShapeFillMode::StrokeAndFill => Some(bg_color.unwrap_or(fg_color)),
    };
    let stroke = match fill_mode {
        ShapeFillMode::Fill => VectorStroke::none(),
        ShapeFillMode::Stroke | ShapeFillMode::StrokeAndFill => {
            VectorStroke::new(fg_color, stroke_width.max(1.0))
        }
    };

    let geometry = match kind {
        DrawShapeKind::Line => {
            if points.len() < 2 {
                return None;
            }
            VectorGeometry::Polygon {
                points: points[..2].to_vec(),
                closed: false,
            }
        }
        DrawShapeKind::Rect => {
            if points.len() < 2 {
                return None;
            }
            let x = points[0][0].min(points[1][0]);
            let y = points[0][1].min(points[1][1]);
            let w = (points[1][0] - points[0][0]).abs().max(1.0);
            let h = (points[1][1] - points[0][1]).abs().max(1.0);
            VectorGeometry::Rect {
                x,
                y,
                w,
                h,
                corner_radius: corner_radius.max(0.0),
            }
        }
        DrawShapeKind::Ellipse => {
            if points.len() < 2 {
                return None;
            }
            let cx = (points[0][0] + points[1][0]) * 0.5;
            let cy = (points[0][1] + points[1][1]) * 0.5;
            let rx = ((points[1][0] - points[0][0]).abs() * 0.5).max(0.5);
            let ry = ((points[1][1] - points[0][1]).abs() * 0.5).max(0.5);
            VectorGeometry::Ellipse { cx, cy, rx, ry }
        }
        DrawShapeKind::Polygon | DrawShapeKind::Polyline => {
            if points.len() < 2 {
                return None;
            }
            VectorGeometry::Polygon {
                points: points.to_vec(),
                closed: kind == DrawShapeKind::Polygon || closed,
            }
        }
        DrawShapeKind::BezierPath => {
            if nodes.is_empty() {
                return None;
            }
            VectorGeometry::Path {
                nodes: nodes.to_vec(),
                closed,
            }
        }
    };

    Some(VectorShape {
        id: 0,
        name: String::new(),
        visible: true,
        opacity: opacity.clamp(0.0, 1.0),
        geometry,
        fill: fill_color,
        stroke,
    })
}
