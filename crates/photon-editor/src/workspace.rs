use crate::panel_splitter::{DockSplitterKind, dock_splitter_handle};
use crate::tool_options::build_editor_inspector;
use crate::viewport_chrome::{CanvasScrollbarProgram, canvas_display_dims, scrollbar_axis_metrics};
use crate::widgets::toolbar_divider;
use crate::*;
use iced::widget::{Shader, column, container, responsive, row, rule, stack};
use iced::{Alignment, Element, Fill, Length};
use photon_canvas::{CanvasTool, ImageCanvasProgram, default_viewport_bg_rgb};
use photon_i18n::tr;
use photon_panels::dock_panel_closable;
use photon_protocol::{BrushSmoothingMode, Command, FlipAxis, Language, Theme};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::editor::{checker_swatch, color_pair_widget, tool_rail_button_tipped};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenubarTrigger, menubar_row_hoverable};
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, canvas_backdrop_style, hairline_rule_style,
};
use photon_ui::toolbar::toolbar;
use photon_ui::typography::text;
use photon_ui::{TooltipPosition, with_tooltip_pos};

pub fn build_editor_toolbar<'a>(
    state: EditorViewState<'a>,
    right_utilities: Element<'a, EditorMessage>,
) -> Element<'a, EditorMessage> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let ed = meta.editor.as_ref();
    let cmp_mode = state.editor_ui.compare_mode;

    let menu_open = state.active_menu.is_some();
    let trigger = |label: String, m: EditorMenuKind| {
        (
            MenubarTrigger {
                label,
                active: state.active_menu == Some(m),
                on_press: EditorMessage::ToggleMenu(m),
            },
            menu_open.then_some(EditorMessage::HoverMenu(m)),
        )
    };

    let quick_actions = row![
        button::toolbar_icon(
            Icon::FolderOpen,
            false,
            Some(EditorMessage::OpenFileDialog),
            Some(tr(lang, "action-open-file-dialog")),
        ),
        button::toolbar_icon(
            Icon::Save,
            ed.is_some_and(|d| d.modified),
            ed.map(|_| EditorMessage::Dispatch(Command::EditorSave)),
            Some(tr(lang, "tooltip-editor-save")),
        ),
        button::toolbar_icon(
            Icon::Export,
            false,
            ed.map(|_| EditorMessage::OpenExportDialog),
            Some(tr(lang, "tooltip-editor-export")),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Undo,
            false,
            ed.filter(|d| d.can_undo)
                .map(|_| EditorMessage::Dispatch(Command::EditorUndo)),
            Some(tr(lang, "tooltip-editor-undo")),
        ),
        button::toolbar_icon(
            Icon::Redo,
            false,
            ed.filter(|d| d.can_redo)
                .map(|_| EditorMessage::Dispatch(Command::EditorRedo)),
            Some(tr(lang, "tooltip-editor-redo")),
        ),
        button::toolbar_icon(
            Icon::Reset,
            false,
            ed.filter(|d| d.modified)
                .map(|_| EditorMessage::Dispatch(Command::EditorReset)),
            Some(tr(lang, "tooltip-history-reset")),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Compare,
            ed.is_some() && cmp_mode != CompareMode::Off,
            ed.map(|_| EditorMessage::EditorCycleCompareMode),
            Some(match cmp_mode {
                CompareMode::Off => tr(lang, "tooltip-editor-compare"),
                CompareMode::Split => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-split")
                ),
                CompareMode::SideBySide => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-side-by-side")
                ),
                CompareMode::HoldOriginal => format!(
                    "{} ({})",
                    tr(lang, "action-compare-original"),
                    tr(lang, "label-compare-hold")
                ),
            }),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let left = row![
        menubar_row_hoverable(vec![
            trigger(tr(lang, "menu-file"), EditorMenuKind::File),
            trigger(tr(lang, "menu-edit"), EditorMenuKind::Edit),
            trigger(tr(lang, "menu-image"), EditorMenuKind::Image),
            trigger(tr(lang, "menu-layer"), EditorMenuKind::Layer),
            trigger(tr(lang, "menu-vector"), EditorMenuKind::Vector),
            trigger(tr(lang, "menu-select"), EditorMenuKind::Select),
            trigger(tr(lang, "menu-filter"), EditorMenuKind::Filter),
            trigger(tr(lang, "menu-plugins"), EditorMenuKind::Plugins),
            trigger(tr(lang, "menu-view"), EditorMenuKind::View),
            trigger(tr(lang, "menu-window"), EditorMenuKind::Window),
            trigger(tr(lang, "menu-help"), EditorMenuKind::Help),
        ]),
        toolbar_divider(),
        quick_actions,
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    toolbar(left, right_utilities)
}

pub fn build_editor_view<'a>(state: EditorViewState<'a>) -> Element<'a, EditorMessage> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    if let Some(info) = &state.snapshot.meta.editor_unsupported {
        let name = std::path::Path::new(&info.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(info.path.as_str())
            .to_owned();
        let ext_label = info
            .extension
            .as_deref()
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "FILE".to_owned());
        let mut actions = row![
            button::labeled(
                tr(lang, "action-new-canvas"),
                Some(Icon::Plus),
                ButtonVariant::Primary,
                Some(EditorMessage::OpenNewDocumentDialog),
            ),
            button::labeled(
                tr(lang, "action-open-system"),
                Some(Icon::Globe),
                ButtonVariant::Secondary,
                Some(EditorMessage::OpenExternally(info.path.clone())),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        actions = actions.push(button::labeled(
            tr(lang, "action-back-to-viewer"),
            Some(Icon::Viewer),
            ButtonVariant::Secondary,
            Some(EditorMessage::OpenViewerWindow),
        ));
        actions = actions.push(button::labeled(
            tr(lang, "action-back-to-browser"),
            Some(Icon::Browser),
            ButtonVariant::Ghost,
            Some(EditorMessage::OpenBrowserWindow),
        ));

        return container(
            container(
                column![
                    photon_ui::icons::render(Icon::Info, 36.0, None),
                    text(tr(lang, "editor-unsupported-title")).size(16),
                    row![
                        text(name).size(13),
                        container(text(ext_label).size(11))
                            .padding([2, 6])
                            .style(|theme| {
                                let t = PaletteTokens::for_theme(theme);
                                container::Style {
                                    background: Some(t.bg_hover.into()),
                                    border: iced::Border {
                                        radius: RADIUS_SM.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }
                            }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(tr(lang, "editor-unsupported-desc"))
                        .size(12)
                        .style(|theme| {
                            let t = PaletteTokens::for_theme(theme);
                            text::Style {
                                color: Some(t.text_muted),
                            }
                        }),
                    text(info.reason.clone()).size(12).style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
                    actions,
                ]
                .spacing(10)
                .align_x(Alignment::Center),
            )
            .padding([24, 28])
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                container::Style {
                    background: Some(t.bg_surface.into()),
                    border: iced::Border {
                        radius: RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
        )
        .center_x(Fill)
        .center_y(Fill)
        .style(canvas_backdrop_style)
        .into();
    }

    let Some(ed) = &state.snapshot.meta.editor else {
        return container(
            column![
                photon_ui::icons::render(Icon::Editor, 36.0, None),
                text(tr(lang, "label-no-document")).size(14).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
                row![
                    button::labeled(
                        tr(lang, "action-new-canvas"),
                        Some(Icon::Plus),
                        ButtonVariant::Primary,
                        Some(EditorMessage::OpenNewDocumentDialog),
                    ),
                    button::labeled(
                        tr(lang, "action-open-file-dialog"),
                        Some(Icon::FolderOpen),
                        ButtonVariant::Secondary,
                        Some(EditorMessage::OpenFileDialog),
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .style(canvas_backdrop_style)
        .into();
    };

    let adjust_open = state.editor_ui.adjust_popup_tab.is_some();
    let tool_rail = build_tool_rail(state, lang);
    let preview_fill = match state.editor_ui.rotate_fill {
        Some([r, g, b]) => [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            1.0,
        ],
        None => [0.0, 0.0, 0.0, 0.0],
    };
    let preview_angle_rad = if !adjust_open && state.editor_ui.tool == EditorTool::RotateAngle {
        state.editor_ui.rotate_angle.to_radians() as f32
    } else {
        0.0
    };
    let brush_radius_px = if adjust_open {
        0.0
    } else {
        state.editor_ui.effective_brush_radius()
    };
    // While the cutout Adjustment dialog is active, keep canvas pan/zoom
    // responsive without triggering brush/crop gestures underneath.
    let canvas_tool = if adjust_open {
        CanvasTool::Hand
    } else {
        state.editor_ui.effective_canvas_tool()
    };
    let transform_preview = if !adjust_open
        && (matches!(
            state.editor_ui.tool,
            EditorTool::Transform | EditorTool::Pointer
        ) || state.editor_ui.smart_paste.is_some())
    {
        state.transform_preview.cloned()
    } else {
        None
    };
    let gradient_preview = if !adjust_open && state.editor_ui.tool == EditorTool::Gradient {
        Some(photon_canvas::GradientPreview {
            start: state.editor_ui.gradient_draft.map(|(a, _)| a),
            end: state.editor_ui.gradient_draft.map(|(_, b)| b),
            kind: state.editor_ui.gradient_kind,
            repeat: state.editor_ui.gradient_repeat,
            stops: state.editor_ui.resolved_gradient_stops(),
            opacity: state.editor_ui.gradient_opacity,
            reverse: state.editor_ui.gradient_reverse,
            blend_mode: state.editor_ui.gradient_blend_mode,
            interactive: state.editor_ui.gradient_interactive,
        })
    } else {
        None
    };

    let canvas: Element<'_, EditorMessage> = Element::from(
        Shader::new(ImageCanvasProgram {
            frame: state.editor_frame.cloned(),
            pair_frame: None,
            dual_gap_px: 0.0,
            compare_frame: state.editor_original_frame.cloned(),
            selection_mask: state.selection_mask.cloned(),
            has_selection: ed.selection.has_selection,
            quick_mask_active: ed.selection.quick_mask_active,
            marching_ants_phase: state.marching_ants_phase,
            zoom: state.zoom,
            pan_x: state.pan_x,
            pan_y: state.pan_y,
            adjustments: *state.pending_adj,
            dark_theme: dark,
            crop_rect: state.crop_uv,
            crop_aspect: state.editor_ui.crop_aspect,
            quarter_turns: 0,
            tool: canvas_tool,
            preview_angle_rad,
            preview_fill,
            canvas_bg: ed.canvas_background,
            viewport_bg: state.viewport_background,
            compare_mode: state.editor_ui.compare_mode.as_u8(),
            compare_split_x: state.editor_ui.compare_split_x,
            brush_radius_px,
            stabilizer_radius: if matches!(
                state.editor_ui.tool,
                EditorTool::Brush | EditorTool::Eraser
            ) && state.editor_ui.brush_settings.smoothing_mode
                == BrushSmoothingMode::Stabilizer
            {
                state.editor_ui.brush_settings.stabilizer_distance
            } else {
                0.0
            },
            restrict_pan_to_hand: true,
            curve_lut: state.curve_lut,
            aa_mode: state.aa_mode,
            denoise_mode: state.denoise_mode,
            healing_source_px: state.editor_ui.healing_source_px,
            transform_preview,
            gradient_preview,
            active_vector_layer: ed.active_vector_data.clone(),
            selected_vector_node: state.editor_ui.selected_vector_node,
        })
        .width(Fill)
        .height(Fill),
    )
    .map(EditorMessage::Canvas);

    let (doc_w, doc_h) = state
        .editor_frame
        .map(|f| (f.width, f.height))
        .unwrap_or((ed.width, ed.height));
    let zoom = state.zoom;
    let pan_x = state.pan_x;
    let pan_y = state.pan_y;

    let scrollbar_overlay: Element<'_, EditorMessage> = responsive(move |bounds| {
        const BAR_THICKNESS: f32 = 10.0;
        const CORNER_PAD: f32 = 2.0;
        let (disp_w, disp_h) = canvas_display_dims(doc_w, doc_h, zoom, bounds.width, bounds.height);
        let h_metrics = scrollbar_axis_metrics(disp_w, bounds.width, pan_x);
        let v_metrics = scrollbar_axis_metrics(disp_h, bounds.height, pan_y);

        if h_metrics.is_none() && v_metrics.is_none() {
            return iced::widget::space::horizontal()
                .width(Fill)
                .height(Fill)
                .into();
        }

        let mut layers: Vec<Element<'_, EditorMessage>> = vec![
            iced::widget::space::horizontal()
                .width(Fill)
                .height(Fill)
                .into(),
        ];

        if let Some(hm) = h_metrics {
            let right_pad = if v_metrics.is_some() {
                BAR_THICKNESS + CORNER_PAD
            } else {
                CORNER_PAD
            };
            let h_bar = iced::widget::canvas(CanvasScrollbarProgram {
                horizontal: true,
                metrics: hm,
                other_axis_pan: pan_y,
            })
            .width(Fill)
            .height(Length::Fixed(BAR_THICKNESS));
            layers.push(
                container(
                    column![
                        iced::widget::space::vertical(),
                        container(h_bar).padding(iced::Padding {
                            top: 0.0,
                            right: right_pad,
                            bottom: CORNER_PAD,
                            left: CORNER_PAD,
                        }),
                    ]
                    .width(Fill)
                    .height(Fill),
                )
                .width(Fill)
                .height(Fill)
                .into(),
            );
        }

        if let Some(vm) = v_metrics {
            let bottom_pad = if h_metrics.is_some() {
                BAR_THICKNESS + CORNER_PAD
            } else {
                CORNER_PAD
            };
            let v_bar = iced::widget::canvas(CanvasScrollbarProgram {
                horizontal: false,
                metrics: vm,
                other_axis_pan: pan_x,
            })
            .width(Length::Fixed(BAR_THICKNESS))
            .height(Fill);
            layers.push(
                container(
                    row![
                        iced::widget::space::horizontal(),
                        container(v_bar).padding(iced::Padding {
                            top: CORNER_PAD,
                            right: CORNER_PAD,
                            bottom: bottom_pad,
                            left: 0.0,
                        }),
                    ]
                    .width(Fill)
                    .height(Fill),
                )
                .width(Fill)
                .height(Fill)
                .into(),
            );
        }

        stack(layers).width(Fill).height(Fill).into()
    })
    .width(Fill)
    .height(Fill)
    .into();

    let canvas_stack: Element<'_, EditorMessage> = stack![
        container(canvas)
            .width(Fill)
            .height(Fill)
            .style(canvas_backdrop_style),
        scrollbar_overlay,
    ]
    .width(Fill)
    .height(Fill)
    .into();

    let canvas_nav_bar = container(
        row![
            text(format!("{} × {}", doc_w, doc_h))
                .size(TEXT_SIZE_CAPTION)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            iced::widget::space::horizontal(),
            build_editor_zoom_controls(state),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding([3, 10])
    .width(Fill)
    .style(photon_ui::theme::surface_panel_style);

    let canvas_area: Element<'_, EditorMessage> = column![
        canvas_stack,
        rule::horizontal(1).style(hairline_rule_style),
        canvas_nav_bar,
    ]
    .width(Fill)
    .height(Fill)
    .into();

    let show_tools = state.snapshot.meta.layout.panel("editor.tools");
    let show_inspector = (state.editor_history_open || state.editor_layers_open)
        && state.snapshot.meta.layout.panel("editor.inspector");

    let mut editor_row = row![].height(Fill).width(Fill);
    if show_tools {
        editor_row = editor_row
            .push(tool_rail)
            .push(rule::vertical(1).style(hairline_rule_style));
    }
    editor_row = editor_row.push(canvas_area);
    if show_inspector {
        let right_w = state
            .dock_drag
            .effective_editor_right(&state.snapshot.meta.layout);
        let inspector = build_editor_inspector(state, ed, lang, right_w);
        let splitter = dock_splitter_handle(DockSplitterKind::EditorRight, right_w);
        editor_row = editor_row.push(splitter).push(inspector);
    }

    if state.snapshot.meta.editor_tabs.is_empty() {
        return editor_row.into();
    }

    let tab_bar = tabs::build_editor_tab_bar(state, lang);
    column![
        tab_bar,
        rule::horizontal(1).style(hairline_rule_style),
        editor_row,
    ]
    .width(Fill)
    .height(Fill)
    .into()
}

fn build_tool_rail(state: EditorViewState<'_>, lang: Language) -> Element<'_, EditorMessage> {
    let ui = &state.editor_ui;
    let active_tool = ui.tool;

    let group_btn = |group: EditorToolGroup| -> Element<'_, EditorMessage> {
        let tool = group.active_tool(ui);
        let active = group.is_active(active_tool)
            || (group == EditorToolGroup::Crop && state.crop_uv.is_some());
        let has_flyout = group.tools().len() > 1;
        let btn = tool_rail_button_tipped(
            tool.icon(),
            active,
            true,
            None,
            editor_tool_label(lang, tool),
        );
        let content: Element<'_, EditorMessage> = if has_flyout {
            let corner = container(text("▾").size(8).style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }))
            .width(Fill)
            .height(Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding([0, 2]);
            iced::widget::stack![btn, corner].into()
        } else {
            btn
        };
        iced::widget::mouse_area(content)
            .on_press(EditorMessage::ToolGroupPressed(group))
            .on_release(EditorMessage::ToolGroupReleased(group))
            .on_right_press(EditorMessage::OpenToolGroupFlyout(group))
            .into()
    };

    let tool_grid = column![
        row![
            group_btn(EditorToolGroup::Move),
            group_btn(EditorToolGroup::Marquee)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Lasso),
            group_btn(EditorToolGroup::SmartSelect)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Crop),
            group_btn(EditorToolGroup::Eyedropper)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Retouch),
            group_btn(EditorToolGroup::Brush)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Fill),
            group_btn(EditorToolGroup::Pen)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Text),
            group_btn(EditorToolGroup::Shape)
        ]
        .spacing(4),
        row![
            group_btn(EditorToolGroup::Nav),
            group_btn(EditorToolGroup::Ai)
        ]
        .spacing(4),
        row![tool_rail_button_tipped(
            Icon::QuickMask,
            state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|e| e.selection.quick_mask_active),
            true,
            Some(EditorMessage::Dispatch(
                Command::EditorSelectToggleQuickMask
            )),
            tr(lang, "tooltip-tool-quick-mask"),
        ),]
        .spacing(4),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let quick_edits = column![
        row![
            tool_rail_button_tipped(
                Icon::RotateLeft,
                false,
                true,
                Some(EditorMessage::Dispatch(Command::EditorRotate {
                    degrees: -90,
                    target: ui.apply_target,
                })),
                tr(lang, "action-rotate-left"),
            ),
            tool_rail_button_tipped(
                Icon::RotateRight,
                false,
                true,
                Some(EditorMessage::Dispatch(Command::EditorRotate {
                    degrees: 90,
                    target: ui.apply_target,
                })),
                tr(lang, "action-rotate-right"),
            ),
        ]
        .spacing(4),
        row![
            tool_rail_button_tipped(
                Icon::FlipH,
                false,
                true,
                Some(EditorMessage::Dispatch(Command::EditorFlip {
                    axis: FlipAxis::Horizontal,
                    target: ui.apply_target,
                })),
                tr(lang, "action-flip-h"),
            ),
            tool_rail_button_tipped(
                Icon::FlipV,
                false,
                true,
                Some(EditorMessage::Dispatch(Command::EditorFlip {
                    axis: FlipAxis::Vertical,
                    target: ui.apply_target,
                })),
                tr(lang, "action-flip-v"),
            ),
        ]
        .spacing(4),
    ]
    .spacing(4)
    .align_x(Alignment::Center);

    let color_pair = color_pair_widget(
        rgb_to_color(ui.fg_color),
        ui.bg_color.map(rgb_to_color),
        EditorMessage::OpenColorPicker(ColorPickerTarget::Foreground),
        EditorMessage::OpenColorPicker(ColorPickerTarget::Background),
        EditorMessage::EditorSwapColors,
        EditorMessage::EditorResetColors,
    );

    let effective_viewport_rgb = state
        .viewport_background
        .unwrap_or_else(|| default_viewport_bg_rgb(state.snapshot.meta.theme == Theme::Dark));

    let viewport_bg_swatch = with_tooltip_pos(
        checker_swatch(
            Some(rgb_to_color(effective_viewport_rgb)),
            70.0,
            20.0,
            Some(EditorMessage::OpenColorPicker(
                ColorPickerTarget::ViewportBackground,
            )),
        ),
        tr(lang, "label-viewport-bg"),
        TooltipPosition::Right,
    );

    dock_panel_closable(
        tr(lang, "label-tools"),
        Some(Icon::Sliders),
        96.0,
        Some(tr(lang, "tooltip-panel-close")),
        EditorMessage::CloseEditorToolsPanel,
        column![
            tool_grid,
            rule::horizontal(1).style(hairline_rule_style),
            quick_edits,
            rule::horizontal(1).style(hairline_rule_style),
            color_pair,
            viewport_bg_swatch,
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
}
