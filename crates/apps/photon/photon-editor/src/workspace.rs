use crate::panel_splitter::{DockSplitterKind, dock_splitter_handle};
use crate::tool_options::build_editor_inspector;
use crate::viewport_chrome::{CanvasScrollbarProgram, canvas_display_dims, scrollbar_axis_metrics};
use crate::widgets::toolbar_divider;
use crate::*;
use fw_panels::dock_panel_closable;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::editor::{checker_swatch, color_pair_widget, tool_rail_button_tipped};
use fw_ui::icons::Icon;
use fw_ui::menu::{MenubarTrigger, menubar_row_hoverable};
use fw_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, canvas_backdrop_style, hairline_rule_style,
};
use fw_ui::toolbar::toolbar;
use fw_ui::typography::text;
use fw_ui::{TooltipPosition, with_tooltip_pos};
use iced::widget::{Shader, column, container, responsive, row, rule, stack};
use iced::{Alignment, Element, Fill, Length};
use photon_canvas::{CanvasTool, ImageCanvasProgram, default_viewport_bg_rgb};
use photon_i18n::tr;
use photon_protocol::{BrushSmoothingMode, Command, FlipAxis, Language, Theme};

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
                    fw_ui::icons::render(Icon::Info, 36.0, None),
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
        return iced::widget::mouse_area(
            container(
                column![
                    fw_ui::icons::render(Icon::Editor, 36.0, None),
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
            .style(canvas_backdrop_style),
        )
        .on_double_click(EditorMessage::OpenFileDialog)
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
    let adj_active_layer_only = adjust_open
        && state.editor_ui.apply_target == photon_protocol::ApplyTarget::ActiveLayer
        && state.transform_preview.is_some();
    let transform_preview = if adj_active_layer_only
        || (!adjust_open
            && (matches!(
                state.editor_ui.tool,
                EditorTool::Transform | EditorTool::Pointer
            ) || state.editor_ui.smart_paste.is_some()))
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

    let preview_enabled =
        state.editor_ui.adjust_popup_tab.is_none() || state.editor_ui.adjust_live_preview;
    let preview_adjustments = if preview_enabled {
        *state.pending_adj
    } else {
        ed.adjustments
    };
    let preview_hsv = if preview_enabled && adjust_open {
        state.editor_ui.hsv
    } else {
        photon_protocol::HsvAdjustment::default()
    };
    let preview_balance = if preview_enabled && adjust_open {
        state.editor_ui.balance
    } else {
        photon_protocol::ColorBalanceAdjustment::default()
    };
    let preview_curve_lut = if preview_enabled {
        state.curve_lut
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
            adjustments: preview_adjustments,
            hsv: preview_hsv,
            balance: preview_balance,
            adj_selection_only: state.editor_ui.adjust_popup_tab.is_some()
                && state.editor_ui.apply_to_selection,
            adj_active_layer_only,
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
            shape_stroke_width: state.editor_ui.shape_stroke_width,
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
            curve_lut: preview_curve_lut,
            aa_mode: state.aa_mode,
            denoise_mode: state.denoise_mode,
            healing_source_px: state.editor_ui.healing_source_px,
            transform_preview,
            gradient_preview,
            active_vector_layer: ed.active_vector_data.clone(),
            selected_vector_node: state.editor_ui.selected_vector_node,
            show_grid: state.editor_ui.show_grid,
            grid_spacing_px: state.editor_ui.grid_spacing_px,
            grid_subdivisions: state.editor_ui.grid_subdivisions,
            grid_opacity: state.editor_ui.grid_opacity,
            show_guides: state.editor_ui.show_guides,
            lock_guides: state.editor_ui.lock_guides,
            guides: state.editor_ui.guides.clone(),
            pending_guide_preview: state.editor_ui.pending_guide_preview,
            snap: state.editor_ui.snap,
            layer_snap_bounds: ed
                .layers
                .iter()
                .filter(|l| l.visible && l.id != ed.active_layer_id)
                .map(|l| {
                    [
                        l.offset_x as f32,
                        l.offset_y as f32,
                        (l.offset_x + l.width as i32) as f32,
                        (l.offset_y + l.height as i32) as f32,
                    ]
                })
                .collect(),
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

    let canvas_with_rulers: Element<'_, EditorMessage> = if state.editor_ui.show_rulers {
        let dark_theme = state.snapshot.meta.theme == Theme::Dark;
        let doc_wf = doc_w.max(1) as f32;
        let doc_hf = doc_h.max(1) as f32;
        let [vp_w, vp_h] = state
            .editor_ui
            .canvas_viewport_size
            .unwrap_or([doc_wf, doc_hf]);
        let corner = container(text("px").size(9).style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).text_muted),
        }))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(20.0))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(fw_ui::theme::surface_panel_style);

        let top_ruler = iced::widget::canvas::Canvas::new(EditorRulerProgram {
            axis: photon_canvas::GuideAxis::Horizontal,
            doc_w: doc_wf,
            doc_h: doc_hf,
            vp_w,
            vp_h,
            zoom,
            pan_x,
            pan_y,
            dark_theme,
        })
        .width(Fill)
        .height(Length::Fixed(20.0));

        let left_ruler = iced::widget::canvas::Canvas::new(EditorRulerProgram {
            axis: photon_canvas::GuideAxis::Vertical,
            doc_w: doc_wf,
            doc_h: doc_hf,
            vp_w,
            vp_h,
            zoom,
            pan_x,
            pan_y,
            dark_theme,
        })
        .width(Length::Fixed(20.0))
        .height(Fill);

        column![
            row![corner, top_ruler]
                .width(Fill)
                .height(Length::Fixed(20.0)),
            row![left_ruler, canvas_stack].width(Fill).height(Fill),
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        canvas_stack
    };

    let ui = &state.editor_ui;
    let quick_toggle =
        |label: String, icon: Icon, active: bool, msg: EditorMessage, tip: String| {
            with_tooltip_pos(
                button::labeled(
                    label,
                    Some(icon),
                    if active {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Ghost
                    },
                    Some(msg),
                ),
                tip,
                TooltipPosition::Top,
            )
        };
    let mut quick_bar = row![
        quick_toggle(
            tr(lang, "quick-rulers"),
            Icon::Ruler,
            ui.show_rulers,
            EditorMessage::ToggleRulers,
            format!("{} (Ctrl+R)", tr(lang, "menu-view-rulers")),
        ),
        quick_toggle(
            tr(lang, "quick-grid"),
            Icon::Grid,
            ui.show_grid,
            EditorMessage::ToggleGrid,
            format!("{} (Ctrl+')", tr(lang, "menu-view-grid")),
        ),
        quick_toggle(
            tr(lang, "quick-guides"),
            Icon::Guide,
            ui.show_guides,
            EditorMessage::ToggleGuides,
            format!("{} (Ctrl+;)", tr(lang, "menu-view-guides")),
        ),
        quick_toggle(
            tr(lang, "quick-snap"),
            Icon::Magnet,
            ui.snap.enabled,
            EditorMessage::ToggleSnap(SnapTarget::Master),
            format!("{} (Ctrl+Shift+;)", tr(lang, "menu-view-snap")),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if ui.tool == EditorTool::Crop || state.crop_uv.is_some() {
        let has_crop = state.crop_uv.is_some();
        quick_bar = quick_bar
            .push(toolbar_divider())
            .push(button::labeled(
                tr(lang, "action-apply-crop"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                has_crop.then_some(EditorMessage::ApplyCrop),
            ))
            .push(button::labeled(
                tr(lang, "dialog-cancel"),
                None,
                ButtonVariant::Secondary,
                Some(EditorMessage::CancelCrop),
            ));
    } else if ed.selection.has_selection {
        quick_bar = quick_bar
            .push(toolbar_divider())
            .push(button::labeled(
                tr(lang, "action-deselect"),
                None,
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorDeselect)),
            ))
            .push(button::labeled(
                tr(lang, "action-select-invert"),
                None,
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorSelectInvert)),
            ))
            .push(button::labeled(
                tr(lang, "action-layer-via-copy"),
                None,
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorLayerFromSelection {
                    cut: false,
                })),
            ))
            .push(button::labeled(
                tr(lang, "action-select-to-mask"),
                None,
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorSelectionToMask)),
            ));
    } else {
        quick_bar = quick_bar
            .push(toolbar_divider())
            .push(button::labeled(
                tr(lang, "action-image-size"),
                Some(Icon::Resize),
                ButtonVariant::Ghost,
                Some(EditorMessage::OpenResizeDialog),
            ))
            .push(button::labeled(
                tr(lang, "action-canvas-size-dialog"),
                Some(Icon::CanvasSize),
                ButtonVariant::Ghost,
                Some(EditorMessage::OpenCanvasSizeDialog),
            ))
            .push(button::labeled(
                tr(lang, "action-crop"),
                Some(Icon::Crop),
                ButtonVariant::Ghost,
                Some(EditorMessage::ToggleCropMode),
            ));
    }

    let canvas_nav_bar = container(
        row![
            text(format!("{} × {}", doc_w, doc_h))
                .size(TEXT_SIZE_CAPTION)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            iced::widget::space::horizontal(),
            quick_bar,
            iced::widget::space::horizontal(),
            build_editor_zoom_controls(state),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding([3, 10])
    .width(Fill)
    .style(fw_ui::theme::surface_panel_style);

    let canvas_area: Element<'_, EditorMessage> = column![
        canvas_with_rulers,
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
        row![
            group_btn(EditorToolGroup::Transform),
            tool_rail_button_tipped(
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
            ),
        ]
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

struct EditorRulerProgram {
    axis: photon_canvas::GuideAxis,
    doc_w: f32,
    doc_h: f32,
    vp_w: f32,
    vp_h: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    dark_theme: bool,
}

impl EditorRulerProgram {
    fn fit_geometry(&self) -> (f32, f32, f32, f32) {
        let (drawn_w, drawn_h) = canvas_display_dims(
            self.doc_w.max(1.0) as u32,
            self.doc_h.max(1.0) as u32,
            self.zoom,
            self.vp_w,
            self.vp_h,
        );
        let left = self.vp_w * 0.5 + self.pan_x - drawn_w * 0.5;
        let top = self.vp_h * 0.5 + self.pan_y - drawn_h * 0.5;
        (left, top, drawn_w.max(1.0), drawn_h.max(1.0))
    }

    fn screen_to_doc(&self, coord: f32, _span: f32) -> f32 {
        let (left, top, drawn_w, drawn_h) = self.fit_geometry();
        match self.axis {
            photon_canvas::GuideAxis::Horizontal => ((coord - left) / drawn_w) * self.doc_w,
            photon_canvas::GuideAxis::Vertical => ((coord - top) / drawn_h) * self.doc_h,
        }
    }

    fn doc_to_screen(&self, doc_coord: f32, _span: f32) -> f32 {
        let (left, top, drawn_w, drawn_h) = self.fit_geometry();
        match self.axis {
            photon_canvas::GuideAxis::Horizontal => {
                left + (doc_coord / self.doc_w.max(1.0)) * drawn_w
            }
            photon_canvas::GuideAxis::Vertical => top + (doc_coord / self.doc_h.max(1.0)) * drawn_h,
        }
    }

    fn cursor_guide_doc_pos(&self, pos: iced::Point, bounds: iced::Rectangle) -> Option<f32> {
        let (left, top, drawn_w, drawn_h) = self.fit_geometry();
        match self.axis {
            // Dragging down from top ruler creates a Horizontal guide along document Y.
            photon_canvas::GuideAxis::Horizontal => {
                let rel_y = pos.y - (bounds.y + bounds.height);
                if rel_y < 0.0 {
                    return None;
                }
                let doc_y = ((rel_y - top) / drawn_h) * self.doc_h;
                Some(doc_y.clamp(0.0, self.doc_h).round())
            }
            // Dragging right from left ruler creates a Vertical guide along document X.
            photon_canvas::GuideAxis::Vertical => {
                let rel_x = pos.x - (bounds.x + bounds.width);
                if rel_x < 0.0 {
                    return None;
                }
                let doc_x = ((rel_x - left) / drawn_w) * self.doc_w;
                Some(doc_x.clamp(0.0, self.doc_w).round())
            }
        }
    }
}

impl iced::widget::canvas::Program<EditorMessage> for EditorRulerProgram {
    type State = bool;

    fn update(
        &self,
        dragging: &mut Self::State,
        event: &iced::Event,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<iced::widget::canvas::Action<EditorMessage>> {
        use iced::mouse;
        use iced::widget::canvas::Action;
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                *dragging = true;
                return Some(Action::capture());
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if *dragging => {
                let preview = cursor
                    .position()
                    .and_then(|pos| self.cursor_guide_doc_pos(pos, bounds))
                    .map(|p| (self.axis, p));
                return Some(Action::publish(EditorMessage::SetPendingGuidePreview(
                    preview,
                )));
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if *dragging => {
                *dragging = false;
                if let Some(pos) = cursor.position()
                    && let Some(pos_px) = self.cursor_guide_doc_pos(pos, bounds)
                {
                    return Some(
                        Action::publish(EditorMessage::AddGuide(self.axis, pos_px)).and_capture(),
                    );
                }
                return Some(
                    Action::publish(EditorMessage::SetPendingGuidePreview(None)).and_capture(),
                );
            }
            _ => {}
        }
        None
    }

    fn mouse_interaction(
        &self,
        dragging: &Self::State,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> iced::mouse::Interaction {
        if *dragging || cursor.is_over(bounds) {
            match self.axis {
                photon_canvas::GuideAxis::Horizontal => {
                    iced::mouse::Interaction::ResizingVertically
                }
                photon_canvas::GuideAxis::Vertical => {
                    iced::mouse::Interaction::ResizingHorizontally
                }
            }
        } else {
            iced::mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        use iced::widget::canvas::{Frame, Path, Text};
        let mut frame = Frame::new(renderer, bounds.size());
        let bg = if self.dark_theme {
            iced::Color::from_rgb8(0x1e, 0x20, 0x26)
        } else {
            iced::Color::from_rgb8(0xe8, 0xea, 0xef)
        };
        let tick_col = if self.dark_theme {
            iced::Color::from_rgba(1.0, 1.0, 1.0, 0.35)
        } else {
            iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35)
        };
        let text_col = if self.dark_theme {
            iced::Color::from_rgba(1.0, 1.0, 1.0, 0.65)
        } else {
            iced::Color::from_rgba(0.0, 0.0, 0.0, 0.65)
        };
        frame.fill_rectangle(iced::Point::ORIGIN, bounds.size(), bg);

        let is_h = self.axis == photon_canvas::GuideAxis::Horizontal;
        let span = if is_h { bounds.width } else { bounds.height };
        let start_doc = self.screen_to_doc(0.0, span).floor() as i32;
        let end_doc = self.screen_to_doc(span, span).ceil() as i32;
        let raw_step = ((end_doc - start_doc).max(1) as f32 / 12.0).max(10.0);
        let step = [10, 20, 50, 100, 200, 500, 1000]
            .into_iter()
            .find(|&s| s as f32 >= raw_step)
            .unwrap_or(1000);
        let first = (start_doc / step) * step;
        let mut d = first;
        while d <= end_doc {
            let s = self.doc_to_screen(d as f32, span);
            if s >= 0.0 && s <= span {
                if is_h {
                    frame.stroke(
                        &Path::line(iced::Point::new(s, 10.0), iced::Point::new(s, 20.0)),
                        iced::widget::canvas::Stroke::default()
                            .with_color(tick_col)
                            .with_width(1.0),
                    );
                    frame.fill_text(Text {
                        content: d.to_string(),
                        position: iced::Point::new(s + 3.0, 2.0),
                        color: text_col,
                        size: iced::Pixels(9.0),
                        ..Text::default()
                    });
                } else {
                    frame.stroke(
                        &Path::line(iced::Point::new(10.0, s), iced::Point::new(20.0, s)),
                        iced::widget::canvas::Stroke::default()
                            .with_color(tick_col)
                            .with_width(1.0),
                    );
                    frame.fill_text(Text {
                        content: d.to_string(),
                        position: iced::Point::new(2.0, s + 2.0),
                        color: text_col,
                        size: iced::Pixels(8.0),
                        ..Text::default()
                    });
                }
            }
            d += step;
        }

        if let Some(pos) = cursor.position() {
            let marker_col = iced::Color::from_rgb(0.2, 0.78, 1.0);
            if is_h {
                let x = (pos.x - bounds.x).clamp(0.0, bounds.width);
                frame.stroke(
                    &Path::line(iced::Point::new(x, 0.0), iced::Point::new(x, 20.0)),
                    iced::widget::canvas::Stroke::default()
                        .with_color(marker_col)
                        .with_width(1.5),
                );
            } else {
                let y = (pos.y - bounds.y).clamp(0.0, bounds.height);
                frame.stroke(
                    &Path::line(iced::Point::new(0.0, y), iced::Point::new(20.0, y)),
                    iced::widget::canvas::Stroke::default()
                        .with_color(marker_col)
                        .with_width(1.5),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}
