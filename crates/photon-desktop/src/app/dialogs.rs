use super::*;
use iced::widget::column;
use photon_ui::theme::card_style;

pub(crate) fn default_window_state_for(id: DesktopDialogId) -> DialogWindowState {
    match id {
        DesktopDialogId::Settings => DialogWindowState::new(580.0, 520.0)
            .min_size(480.0, 380.0)
            .max_size(960.0, 860.0),
        DesktopDialogId::NewFolder | DesktopDialogId::RenameItem => {
            DialogWindowState::new(440.0, 185.0).min_size(320.0, 160.0)
        }
        DesktopDialogId::NewDocument => DialogWindowState::new(480.0, 460.0).min_size(380.0, 360.0),
        DesktopDialogId::ConfirmResetLayout | DesktopDialogId::ConfirmDeletePermanently => {
            DialogWindowState::new(440.0, 168.0).min_size(320.0, 148.0)
        }
        DesktopDialogId::ConfirmUnsavedLeave => {
            DialogWindowState::new(480.0, 170.0).min_size(340.0, 150.0)
        }
        DesktopDialogId::Resize => DialogWindowState::new(500.0, 445.0).min_size(380.0, 360.0),
        DesktopDialogId::CanvasSize => DialogWindowState::new(460.0, 435.0).min_size(360.0, 350.0),
        DesktopDialogId::Export => DialogWindowState::new(500.0, 335.0).min_size(380.0, 280.0),
        DesktopDialogId::ColorPicker => DialogWindowState::new(500.0, 390.0).min_size(440.0, 340.0),
        DesktopDialogId::BatchResize => DialogWindowState::new(580.0, 640.0)
            .min_size(460.0, 420.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::BatchCrop => DialogWindowState::new(560.0, 500.0)
            .min_size(440.0, 380.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::BatchConvert => DialogWindowState::new(520.0, 495.0)
            .min_size(420.0, 360.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::BatchTransform => DialogWindowState::new(580.0, 680.0)
            .min_size(460.0, 440.0)
            .max_size(960.0, 920.0),
        DesktopDialogId::Shortcuts => DialogWindowState::new(480.0, 225.0).min_size(340.0, 180.0),
        DesktopDialogId::About | DesktopDialogId::Alert => {
            DialogWindowState::new(440.0, 180.0).min_size(320.0, 150.0)
        }
        DesktopDialogId::AdjustColor => DialogWindowState::new(460.0, 480.0)
            .min_size(360.0, 320.0)
            .max_size(760.0, 860.0),
        DesktopDialogId::AdjustHsv => DialogWindowState::new(440.0, 305.0)
            .min_size(340.0, 240.0)
            .max_size(760.0, 720.0),
        DesktopDialogId::AdjustColorBalance => DialogWindowState::new(460.0, 365.0)
            .min_size(360.0, 280.0)
            .max_size(760.0, 760.0),
        DesktopDialogId::AdjustCurves => DialogWindowState::new(500.0, 650.0)
            .min_size(420.0, 560.0)
            .max_size(760.0, 900.0),
        DesktopDialogId::AdjustDenoise => DialogWindowState::new(440.0, 290.0)
            .min_size(340.0, 240.0)
            .max_size(720.0, 640.0),
        DesktopDialogId::SelectionModify => {
            DialogWindowState::new(400.0, 195.0).min_size(320.0, 170.0)
        }
        DesktopDialogId::ManagePlugins => DialogWindowState::new(680.0, 520.0)
            .min_size(580.0, 400.0)
            .max_size(1040.0, 820.0),
        DesktopDialogId::PluginParams => DialogWindowState::new(440.0, 420.0)
            .min_size(340.0, 280.0)
            .max_size(760.0, 800.0),
        DesktopDialogId::PluginModels => DialogWindowState::new(540.0, 440.0)
            .min_size(400.0, 320.0)
            .max_size(860.0, 800.0),
        DesktopDialogId::TranslateSettings => DialogWindowState::new(560.0, 520.0)
            .min_size(420.0, 360.0)
            .max_size(880.0, 840.0),
        DesktopDialogId::TransformEach => DialogWindowState::new(460.0, 520.0)
            .min_size(380.0, 420.0)
            .max_size(720.0, 820.0),
        DesktopDialogId::VectorBlend => DialogWindowState::new(400.0, 210.0).min_size(340.0, 180.0),
        DesktopDialogId::ExportVectorSvg => {
            DialogWindowState::new(460.0, 210.0).min_size(360.0, 180.0)
        }
        DesktopDialogId::SliceImage => DialogWindowState::new(440.0, 340.0).min_size(380.0, 290.0),
        DesktopDialogId::RecoveryPrompt => {
            DialogWindowState::new(420.0, 190.0).min_size(340.0, 170.0)
        }
        DesktopDialogId::SaveSelection => {
            DialogWindowState::new(420.0, 280.0).min_size(360.0, 240.0)
        }
        DesktopDialogId::LoadSelection => {
            DialogWindowState::new(420.0, 310.0).min_size(360.0, 260.0)
        }
        DesktopDialogId::NewGuide => DialogWindowState::new(380.0, 240.0).min_size(320.0, 200.0),
        DesktopDialogId::GridSettings => {
            DialogWindowState::new(420.0, 310.0).min_size(360.0, 260.0)
        }
    }
}

pub(super) fn dialog_spec_to_managed<'a>(
    id: DesktopDialogId,
    spec: DialogSpec<'a, Message>,
    mut defaults: DialogWindowState,
    overlay: OverlayMode,
    lang: Language,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    if spec.width > 0.0 {
        let target_w = spec.width.max(defaults.min_width);
        let h = defaults.height;
        defaults = defaults.preferred_size(target_w, h);
    }
    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(spec.on_close.clone()),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );
    ManagedDialogSpec {
        id,
        open: spec.open,
        title: spec.title,
        defaults,
        resizable: true,
        draggable: true,
        overlay,
        body: spec.body,
        footer: Some(spec.footer),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(Message::DialogEvent),
        on_close: spec.on_close,
        on_noop: spec.on_noop,
    }
}

fn selection_only_control<'a>(
    lang: Language,
    has_selection: bool,
    selection_only: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Option<Element<'a, Message>> {
    if !has_selection {
        return None;
    }
    Some(
        column![
            text(tr(lang, "label-selection-scope"))
                .size(11)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_secondary),
                }),
            row![
                button::labeled(
                    tr(lang, "scope-selection-only"),
                    None,
                    ButtonVariant::Tab {
                        active: selection_only,
                    },
                    Some(on_toggle(true)),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "scope-full-target"),
                    None,
                    ButtonVariant::Tab {
                        active: !selection_only,
                    },
                    Some(on_toggle(false)),
                )
                .width(Fill),
            ]
            .spacing(4),
        ]
        .spacing(4)
        .width(Fill)
        .into(),
    )
}

pub(super) fn build_editor_adjustment_dialog<'a>(
    state: &'a PhotonGui,
    active_tab: AdjustmentPopupTab,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let lang = state.snapshot.meta.language;
    let ui = &state.editor_ui;
    let adj = &state.pending_adj;

    let target_row = apply_target_control(lang, ui.apply_target, Message::EditorSetApplyTarget);

    let tab_body: Element<'a, Message> = match active_tab {
        AdjustmentPopupTab::ColorAndLight => column![
            slider_stepper_control(
                tr(lang, "label-exposure"),
                adj.exposure,
                -2.0..=2.0,
                0.05_f32,
                format!("{:.2}", adj.exposure),
                Message::SetExposure,
            ),
            slider_stepper_control(
                tr(lang, "label-gamma"),
                adj.gamma,
                0.2..=3.0,
                0.05_f32,
                format!("{:.2}", adj.gamma),
                Message::SetGamma,
            ),
            slider_stepper_control(
                tr(lang, "label-highlights"),
                adj.highlights,
                -1.0..=1.0,
                0.05_f32,
                format!("{:.2}", adj.highlights),
                Message::SetHighlights,
            ),
            slider_stepper_control(
                tr(lang, "label-shadows"),
                adj.shadows,
                -1.0..=1.0,
                0.05_f32,
                format!("{:.2}", adj.shadows),
                Message::SetShadows,
            ),
            slider_stepper_control(
                tr(lang, "label-brightness"),
                adj.brightness,
                -0.5..=0.5,
                0.02_f32,
                format!("{:.2}", adj.brightness),
                Message::SetBrightness,
            ),
            slider_stepper_control(
                tr(lang, "label-contrast"),
                adj.contrast,
                -0.5..=0.5,
                0.02_f32,
                format!("{:.2}", adj.contrast),
                Message::SetContrast,
            ),
            slider_stepper_control(
                tr(lang, "label-saturation"),
                adj.saturation,
                -1.0..=1.0,
                0.05_f32,
                format!("{:.2}", adj.saturation),
                Message::SetSaturation,
            ),
        ]
        .spacing(6)
        .into(),
        AdjustmentPopupTab::Hsv => column![
            slider_stepper_control(
                tr(lang, "label-hue"),
                ui.hsv.hue,
                -180.0..=180.0,
                1.0_f32,
                format!("{:.0}°", ui.hsv.hue),
                Message::EditorSetHsvHue,
            ),
            slider_stepper_control(
                tr(lang, "label-saturation"),
                ui.hsv.saturation,
                -1.0..=1.0,
                0.05_f32,
                format!("{:.2}", ui.hsv.saturation),
                Message::EditorSetHsvSat,
            ),
            slider_stepper_control(
                tr(lang, "label-value"),
                ui.hsv.value,
                -1.0..=1.0,
                0.05_f32,
                format!("{:.2}", ui.hsv.value),
                Message::EditorSetHsvVal,
            ),
        ]
        .spacing(6)
        .into(),
        AdjustmentPopupTab::ColorBalance => {
            let zone_vals = match ui.balance_zone {
                ColorBalanceZone::Shadows => ui.balance.shadows,
                ColorBalanceZone::Midtones => ui.balance.midtones,
                ColorBalanceZone::Highlights => ui.balance.highlights,
            };
            column![
                row![
                    button::labeled(
                        tr(lang, "label-shadows"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Shadows,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Shadows)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "label-midtones"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Midtones,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Midtones)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "label-highlights"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.balance_zone == ColorBalanceZone::Highlights,
                        },
                        Some(Message::EditorSetBalanceZone(ColorBalanceZone::Highlights)),
                    )
                    .width(Fill),
                ]
                .spacing(4),
                slider_stepper_control(
                    "Cyan / Red",
                    zone_vals[0],
                    -1.0..=1.0,
                    0.02_f32,
                    format!("{:.2}", zone_vals[0]),
                    |v| Message::EditorSetBalanceAxis(0, v),
                ),
                slider_stepper_control(
                    "Magenta / Green",
                    zone_vals[1],
                    -1.0..=1.0,
                    0.02_f32,
                    format!("{:.2}", zone_vals[1]),
                    |v| Message::EditorSetBalanceAxis(1, v),
                ),
                slider_stepper_control(
                    "Yellow / Blue",
                    zone_vals[2],
                    -1.0..=1.0,
                    0.02_f32,
                    format!("{:.2}", zone_vals[2]),
                    |v| Message::EditorSetBalanceAxis(2, v),
                ),
                checkbox(ui.balance.preserve_luminosity)
                    .label(tr(lang, "label-preserve-luminosity"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(Message::EditorTogglePreserveLuminosity),
            ]
            .spacing(6)
            .into()
        }
        AdjustmentPopupTab::Curves => {
            let active_pts = match ui.curves_channel {
                CurvesChannel::Rgb => ui.curves.rgb.clone(),
                CurvesChannel::Red => ui.curves.red.clone(),
                CurvesChannel::Green => ui.curves.green.clone(),
                CurvesChannel::Blue => ui.curves.blue.clone(),
            };
            let pts_count = active_pts.len();
            let channel_color = match ui.curves_channel {
                CurvesChannel::Rgb => {
                    if state.snapshot.meta.theme == Theme::Dark {
                        Color::from_rgb8(228, 228, 231)
                    } else {
                        Color::from_rgb8(39, 39, 42)
                    }
                }
                CurvesChannel::Red => Color::from_rgb8(239, 68, 68),
                CurvesChannel::Green => Color::from_rgb8(34, 197, 94),
                CurvesChannel::Blue => Color::from_rgb8(59, 130, 246),
            };
            let histogram = state.editor_histogram.as_ref().map(|(_, h)| Arc::clone(h));
            let graph = curve_graph(active_pts, channel_color, histogram, |ev| match ev {
                CurveGraphEvent::PointsChanged(pts) => Message::EditorCurvesChanged(pts),
                CurveGraphEvent::ResetChannel => Message::EditorResetActiveCurvesChannel,
            });
            column![
                row![
                    button::labeled(
                        tr(lang, "channel-rgb"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Rgb,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Rgb)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-red"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Red,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Red)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-green"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Green,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Green)),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "channel-blue"),
                        None,
                        ButtonVariant::Tab {
                            active: ui.curves_channel == CurvesChannel::Blue,
                        },
                        Some(Message::EditorSelectCurvesChannel(CurvesChannel::Blue)),
                    )
                    .width(Fill),
                ]
                .spacing(4),
                container(graph)
                    .width(Fill)
                    .align_x(iced::alignment::Horizontal::Center),
                row![
                    text(tr_ui_text(
                        lang,
                        &UiText::with("label_curve_points", [("count", pts_count.to_string())]),
                    ))
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                    iced::widget::space::horizontal(),
                    button::labeled(
                        tr(lang, "action-reset-channel"),
                        Some(Icon::Reset),
                        ButtonVariant::Ghost,
                        Some(Message::EditorResetActiveCurvesChannel),
                    )
                    .padding([2, 8]),
                ]
                .align_y(Alignment::Center),
                text(tr(lang, "hint-curve-usage"))
                    .size(11)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                {
                    #[derive(Clone, Copy, PartialEq, Eq)]
                    struct CurvePresetOpt {
                        idx: usize,
                        label: &'static str,
                    }
                    impl std::fmt::Display for CurvePresetOpt {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            f.write_str(self.label)
                        }
                    }
                    let opts = [
                        CurvePresetOpt {
                            idx: 0,
                            label: Box::leak(tr(lang, "curve-preset-linear").into_boxed_str()),
                        },
                        CurvePresetOpt {
                            idx: 1,
                            label: Box::leak(tr(lang, "curve-preset-s-curve").into_boxed_str()),
                        },
                        CurvePresetOpt {
                            idx: 2,
                            label: Box::leak(tr(lang, "curve-preset-fade").into_boxed_str()),
                        },
                        CurvePresetOpt {
                            idx: 3,
                            label: Box::leak(
                                tr(lang, "curve-preset-high-contrast").into_boxed_str(),
                            ),
                        },
                        CurvePresetOpt {
                            idx: 4,
                            label: Box::leak(tr(lang, "curve-preset-negative").into_boxed_str()),
                        },
                    ];
                    row![
                        text(tr(lang, "label-general-presets")).size(12),
                        pick_list(opts, None::<CurvePresetOpt>, |opt| {
                            let pts = match opt.idx {
                                0 => vec![[0.0, 0.0], [1.0, 1.0]],
                                1 => vec![[0.0, 0.0], [0.25, 0.18], [0.75, 0.82], [1.0, 1.0]],
                                2 => vec![[0.0, 0.08], [0.5, 0.52], [1.0, 0.96]],
                                3 => vec![[0.0, 0.0], [0.20, 0.10], [0.80, 0.90], [1.0, 1.0]],
                                _ => vec![[0.0, 1.0], [1.0, 0.0]],
                            };
                            Message::EditorCurvesChanged(pts)
                        })
                        .placeholder(tr(lang, "label-general-presets"))
                        .text_size(TEXT_SIZE_CONTROL)
                        .padding([5, 10])
                        .width(Fill)
                        .style(pick_list_style)
                        .menu_style(pick_list_menu_style),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                },
            ]
            .spacing(6)
            .into()
        }
    };

    let has_selection = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .is_some_and(|ed| ed.selection.has_selection);
    let mut body_items: Vec<Element<'a, Message>> = vec![target_row];
    if let Some(sel_row) = selection_only_control(
        lang,
        has_selection,
        ui.apply_to_selection,
        Message::EditorToggleApplyToSelection,
    ) {
        body_items.push(sel_row);
    }
    body_items.push(tab_body);
    let body = column(body_items).spacing(10);
    let live_preview_on = ui.adjust_live_preview;
    let footer = row![
        with_tooltip_pos(
            button::labeled(
                tr(lang, "label-live-preview"),
                Some(if live_preview_on {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                }),
                ButtonVariant::Tab {
                    active: live_preview_on,
                },
                Some(Message::EditorToggleAdjustLivePreview),
            ),
            tr(lang, "label-live-preview"),
            TooltipPosition::Top,
        ),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-reset"),
                Some(Icon::Reset),
                ButtonVariant::Ghost,
                Some(Message::EditorResetPendingAdjustments),
            ),
            tr(lang, "action-reset"),
            TooltipPosition::Top,
        ),
        iced::widget::space::horizontal(),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-cancel"),
                None,
                ButtonVariant::Secondary,
                Some(Message::EditorCloseAdjustPopup { apply: false }),
            ),
            tr(lang, "tooltip-dialog-close"),
            TooltipPosition::Top,
        ),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "action-apply"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                Some(Message::EditorCloseAdjustPopup { apply: true }),
            ),
            tr(lang, "action-apply"),
            TooltipPosition::Top,
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(Message::EditorCloseAdjustPopup { apply: false }),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );

    let id = adjustment_dialog_id(active_tab);
    let title = match active_tab {
        AdjustmentPopupTab::ColorAndLight => tr(lang, "dialog-adjust-color"),
        AdjustmentPopupTab::Hsv => tr(lang, "dialog-adjust-hsv"),
        AdjustmentPopupTab::ColorBalance => tr(lang, "dialog-adjust-balance"),
        AdjustmentPopupTab::Curves => tr(lang, "dialog-adjust-curves"),
    };

    ManagedDialogSpec {
        id,
        open: true,
        title,
        defaults: default_window_state_for(id),
        resizable: true,
        draggable: true,
        overlay: OverlayMode::Cutout {
            cutout: resolved_editor_canvas_bounds(state),
            dim_alpha: 0.42,
            lock_outside: true,
        },
        body: body.into(),
        footer: Some(footer.into()),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(Message::DialogEvent),
        on_close: Message::EditorCloseAdjustPopup { apply: false },
        on_noop: Message::Noop,
    }
}

pub(super) fn build_plugins_manager_body<'a>(
    state: &'a PhotonGui,
    search: &str,
    _selected_id: Option<&str>,
    on_search: impl Fn(String) -> Message + 'a,
    _on_select: impl Fn(String) -> Message + 'a,
    list_pane_h: f32,
) -> Element<'a, Message> {
    let lang = state.snapshot.meta.language;
    let query = search.trim().to_ascii_lowercase();
    let plugins: Vec<&PluginInfo> = state
        .snapshot
        .meta
        .plugins
        .iter()
        .filter(|p| {
            query.is_empty()
                || p.name.to_ascii_lowercase().contains(&query)
                || p.id.to_ascii_lowercase().contains(&query)
                || p.description.to_ascii_lowercase().contains(&query)
        })
        .collect();

    let top_bar = row![
        text_input(&tr(lang, "label-plugin-search"), search)
            .on_input(on_search)
            .size(TEXT_SIZE_CONTROL)
            .padding([6, 8])
            .style(input_style)
            .width(Fill),
        button::labeled(
            tr(lang, "action-install-plugin"),
            Some(Icon::Plus),
            ButtonVariant::Secondary,
            Some(Message::PickPluginInstallFolder),
        ),
        button::labeled(
            tr(lang, "action-open-plugins-folder"),
            Some(Icon::FolderOpen),
            ButtonVariant::Ghost,
            Some(Message::OpenPluginsFolder),
        ),
        button::compact_icon_tipped(
            Icon::Refresh,
            ButtonVariant::Ghost,
            Some(Message::Dispatch(Command::PluginRefresh)),
            tr(lang, "action-reload-plugins"),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut rows_col = column![].spacing(6).width(Fill);
    if plugins.is_empty() {
        rows_col = rows_col.push(
            text(tr(lang, "menu-plugins-no-enabled"))
                .size(12)
                .style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).text_muted),
                }),
        );
    } else {
        for p in plugins {
            let has_plugin_settings = !p.models.is_empty()
                || !p.params.is_empty()
                || p.quick_actions.iter().any(|qa| qa.slot == "quick");
            let status_label = if p.error.is_some() {
                "!"
            } else if !p.enabled {
                "Disabled"
            } else if p.available {
                "Ready"
            } else {
                "Unavailable"
            };
            let is_err = p.error.is_some();
            let is_ready = p.enabled && p.available && p.error.is_none();

            let mut info_col = column![
                row![
                    text(p.display_name(lang).to_owned()).size(13),
                    text(format!("v{}", p.version))
                        .size(11)
                        .style(|th| text::Style {
                            color: Some(PaletteTokens::for_theme(th).text_muted),
                        }),
                    text(status_label).size(11).style(move |th| text::Style {
                        color: Some(if is_err {
                            PaletteTokens::for_theme(th).danger
                        } else if is_ready {
                            PaletteTokens::for_theme(th).success
                        } else {
                            PaletteTokens::for_theme(th).warning
                        }),
                    }),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(2)
            .width(Fill);
            if let Some(err) = &p.error {
                info_col = info_col.push(text(err.clone()).size(11).style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).danger),
                }));
            }

            let actions_row = row![
                button::labeled(
                    if p.enabled {
                        tr(lang, "action-off")
                    } else {
                        tr(lang, "action-on")
                    },
                    None,
                    if p.enabled {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Primary
                    },
                    Some(Message::Dispatch(Command::PluginSetEnabled {
                        id: p.id.clone(),
                        enabled: !p.enabled,
                    })),
                ),
                button::compact_icon_tipped(
                    Icon::Settings,
                    ButtonVariant::Ghost,
                    has_plugin_settings.then(|| Message::OpenPluginModels(p.id.clone())),
                    tr(lang, "action-plugin-settings-models"),
                ),
                button::compact_icon_tipped(
                    Icon::Trash,
                    ButtonVariant::Ghost,
                    Some(Message::Dispatch(Command::PluginUninstall {
                        id: p.id.clone(),
                    })),
                    tr(lang, "action-uninstall-plugin"),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center);

            rows_col = rows_col.push(
                container(
                    row![info_col, actions_row]
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .width(Fill),
                )
                .padding([6, 8])
                .width(Fill)
                .style(card_style),
            );
        }
    }

    column![
        top_bar,
        container(scrollable(rows_col).height(Fill).width(Fill))
            .height(Length::Fixed(list_pane_h))
            .width(Fill),
    ]
    .spacing(8)
    .width(Fill)
    .into()
}

pub(super) fn build_in_app_dialog<'a>(
    state: &'a PhotonGui,
    dialog: &'a InAppDialog,
    theme: &IcedTheme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let lang = state.snapshot.meta.language;
    let id = dialog_id_for_in_app(dialog);
    let overlay = if matches!(dialog, InAppDialog::PluginParams { .. }) {
        let cutout = if state.snapshot.meta.workspace == Workspace::Editor {
            resolved_editor_canvas_bounds(state)
        } else {
            iced::Rectangle {
                x: 0.0,
                y: 44.0,
                width: state.window_size.width.max(1.0),
                height: (state.window_size.height - 68.0).max(1.0),
            }
        };
        OverlayMode::Cutout {
            cutout,
            dim_alpha: 0.42,
            lock_outside: true,
        }
    } else {
        OverlayMode::default()
    };
    match dialog {
        InAppDialog::SelectionModify { kind, value, error } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, kind.title_key()),
                    tr(lang, kind.hint_key()),
                    value.clone(),
                    tr(lang, "dialog-select-modify-apply"),
                    tr(lang, "dialog-cancel"),
                    Message::SelectionModifyValueChanged,
                    Message::ConfirmSelectionModify,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(tr(lang, kind.label_key()))
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::NewFolder {
            parent,
            name,
            error,
        } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, "dialog-new-folder-title"),
                    tr(lang, "dialog-new-folder-placeholder"),
                    name.clone(),
                    tr(lang, "dialog-create"),
                    tr(lang, "dialog-cancel"),
                    Message::NewFolderNameChanged,
                    Message::ConfirmNewFolder,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(parent.clone())
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::NewDocument {
            dims,
            background,
            custom_rgb,
            error,
        } => {
            #[derive(Clone, Copy, PartialEq, Eq)]
            struct DocPresetOpt {
                label: &'static str,
                w: u32,
                h: u32,
            }
            impl std::fmt::Display for DocPresetOpt {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str(self.label)
                }
            }
            let presets = [
                DocPresetOpt {
                    label: "1920×1080",
                    w: 1920,
                    h: 1080,
                },
                DocPresetOpt {
                    label: "1080×1080",
                    w: 1080,
                    h: 1080,
                },
                DocPresetOpt {
                    label: "1280×720",
                    w: 1280,
                    h: 720,
                },
                DocPresetOpt {
                    label: "2480×3508",
                    w: 2480,
                    h: 3508,
                },
                DocPresetOpt {
                    label: "1080×1920",
                    w: 1080,
                    h: 1920,
                },
            ];
            let selected_preset = presets
                .iter()
                .find(|p| dims.width_px == p.w && dims.height_px == p.h)
                .copied();
            let preset_row = column![
                text(tr(lang, "label-general-presets")).size(12),
                pick_list(presets, selected_preset, |p| {
                    Message::NewDocApplyPreset(p.w, p.h)
                })
                .placeholder(tr(lang, "label-general-presets"))
                .text_size(TEXT_SIZE_CONTROL)
                .padding([5, 10])
                .width(Fill)
                .style(pick_list_style)
                .menu_style(pick_list_menu_style),
            ]
            .spacing(4);

            #[derive(Clone, Copy, PartialEq, Eq)]
            struct BgOpt {
                bg: NewDocBackground,
                label: &'static str,
            }
            impl std::fmt::Display for BgOpt {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str(self.label)
                }
            }
            let bg_opts = [
                BgOpt {
                    bg: NewDocBackground::Transparent,
                    label: Box::leak(tr(lang, "label-bg-transparent").into_boxed_str()),
                },
                BgOpt {
                    bg: NewDocBackground::White,
                    label: Box::leak(tr(lang, "label-bg-white").into_boxed_str()),
                },
                BgOpt {
                    bg: NewDocBackground::Black,
                    label: Box::leak(tr(lang, "label-bg-black").into_boxed_str()),
                },
                BgOpt {
                    bg: NewDocBackground::Custom(*custom_rgb),
                    label: Box::leak(tr(lang, "label-bg-custom").into_boxed_str()),
                },
            ];
            let selected_bg = bg_opts
                .iter()
                .find(|o| match (o.bg, *background) {
                    (NewDocBackground::Custom(_), NewDocBackground::Custom(_)) => true,
                    (a, b) => a == b,
                })
                .copied();
            let custom_swatch_color = Some(rgb_to_color(*custom_rgb));
            let bg_row = row![
                pick_list(bg_opts, selected_bg, |o| Message::NewDocSetBackground(o.bg))
                    .text_size(TEXT_SIZE_CONTROL)
                    .padding([5, 10])
                    .width(Fill)
                    .style(pick_list_style)
                    .menu_style(pick_list_menu_style),
                checker_swatch(
                    custom_swatch_color,
                    36.0,
                    26.0,
                    Some(Message::OpenNewDocColorPicker),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            let clipboard_size_label = match state.new_doc_clipboard_dims {
                Some((cw, ch)) => {
                    let args = BTreeMap::from([
                        ("width".to_owned(), cw.to_string()),
                        ("height".to_owned(), ch.to_string()),
                    ]);
                    photon_i18n::tr_args(lang, "action-size-from-clipboard-dims", &args)
                }
                None => tr(lang, "action-size-from-clipboard"),
            };
            let has_cb_img =
                state.new_doc_clipboard_dims.is_some() || state.snapshot.meta.has_pixel_clipboard;
            let clipboard_row = column![
                row![
                    button::labeled(
                        clipboard_size_label,
                        Some(Icon::Paste),
                        ButtonVariant::Secondary,
                        state
                            .new_doc_clipboard_dims
                            .is_some()
                            .then_some(Message::NewDocUseClipboardSize),
                    )
                    .width(Fill),
                    button::labeled(
                        tr(lang, "action-create-from-clipboard"),
                        Some(Icon::Plus),
                        ButtonVariant::Secondary,
                        has_cb_img.then_some(Message::NewDocCreateFromClipboard),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                checkbox(state.new_doc_create_from_clipboard)
                    .label(tr(lang, "action-create-from-clipboard"))
                    .on_toggle_maybe(has_cb_img.then_some(Message::NewDocToggleCreateFromClipboard))
                    .size(15)
                    .text_size(12),
            ]
            .spacing(6);

            let mut body_col = column![
                preset_row,
                clipboard_row,
                dims.view(lang, Message::NewDocDimAction),
                column![text(tr(lang, "label-background")).size(12), bg_row].spacing(4),
            ]
            .spacing(10);

            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12).style(|theme| {
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
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "dialog-create"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmNewDocument),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-new-document-title"),
                    width: 480.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::ConfirmResetLayout => dialog_spec_to_managed(
            id,
            confirm_dialog_spec(ConfirmDialogSpec {
                title: tr(lang, "dialog-reset-title"),
                message: tr(lang, "dialog-reset-body"),
                confirm_label: tr(lang, "action-reset-layout"),
                cancel_label: tr(lang, "dialog-cancel"),
                danger: true,
                secondary_action: None,
                on_confirm: Message::ConfirmResetLayout,
                on_cancel: Message::CloseDialog,
                on_noop: Message::Noop,
            }),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ConfirmUnsavedLeave {
            tab_id,
            close_editor,
        } => {
            let doc_name = tab_id
                .and_then(|tid| {
                    state
                        .snapshot
                        .meta
                        .editor_tabs
                        .iter()
                        .find(|t| t.id == tid)
                        .map(|t| t.name.clone())
                })
                .or_else(|| state.snapshot.meta.editor.as_ref().map(|d| d.name.clone()))
                .unwrap_or_default();
            let body = tr_ui_text(
                lang,
                &UiText::with("dialog_unsaved_body", [("name", doc_name)]),
            );
            dialog_spec_to_managed(
                id,
                confirm_dialog_spec(ConfirmDialogSpec {
                    title: tr(lang, "dialog-unsaved-title"),
                    message: body,
                    confirm_label: tr(lang, "dialog-save-and-leave"),
                    cancel_label: tr(lang, "dialog-cancel"),
                    danger: true,
                    secondary_action: Some((
                        tr(lang, "dialog-discard"),
                        Message::DiscardAndLeaveEditor(*close_editor),
                    )),
                    on_confirm: Message::SaveAndLeaveEditor(*close_editor),
                    on_cancel: Message::CloseDialog,
                    on_noop: Message::Noop,
                }),
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::Resize {
            dims,
            filter,
            mode,
            error,
            ..
        } => {
            #[derive(Clone, Copy, PartialEq, Eq)]
            struct ResizeChoice<T: Copy + PartialEq> {
                value: T,
                label: &'static str,
            }
            impl<T: Copy + PartialEq> std::fmt::Display for ResizeChoice<T> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str(self.label)
                }
            }
            let mode_opts = [
                ResizeChoice {
                    value: ResizeMode::Stretch,
                    label: Box::leak(tr(lang, "mode-stretch").into_boxed_str()),
                },
                ResizeChoice {
                    value: ResizeMode::Cover,
                    label: Box::leak(tr(lang, "mode-cover").into_boxed_str()),
                },
                ResizeChoice {
                    value: ResizeMode::Pad,
                    label: Box::leak(tr(lang, "mode-pad").into_boxed_str()),
                },
            ];
            let selected_mode = mode_opts.iter().find(|o| o.value == *mode).copied();
            let filter_opts = [
                ResizeChoice {
                    value: ResizeFilter::Lanczos3,
                    label: "Lanczos3",
                },
                ResizeChoice {
                    value: ResizeFilter::CatmullRom,
                    label: "CatmullRom",
                },
                ResizeChoice {
                    value: ResizeFilter::Triangle,
                    label: "Triangle",
                },
                ResizeChoice {
                    value: ResizeFilter::Nearest,
                    label: "Nearest",
                },
            ];
            let selected_filter = filter_opts.iter().find(|o| o.value == *filter).copied();
            let mut body_col = column![
                dimension_presets_picker(dims, lang, Message::ResizeDimAction),
                dims.view(lang, Message::ResizeDimAction),
                row![
                    column![
                        text(tr(lang, "label-resize-mode")).size(12),
                        pick_list(mode_opts, selected_mode, |o| {
                            Message::ResizeModeChanged(o.value)
                        })
                        .text_size(TEXT_SIZE_CONTROL)
                        .padding([5, 10])
                        .width(Fill)
                        .style(pick_list_style)
                        .menu_style(pick_list_menu_style),
                    ]
                    .spacing(4)
                    .width(Fill),
                    column![
                        text(tr(lang, "label-filter")).size(12),
                        pick_list(filter_opts, selected_filter, |o| {
                            Message::ResizeFilterChanged(o.value)
                        })
                        .text_size(TEXT_SIZE_CONTROL)
                        .padding([5, 10])
                        .width(Fill)
                        .style(pick_list_style)
                        .menu_style(pick_list_menu_style),
                    ]
                    .spacing(4)
                    .width(Fill),
                ]
                .spacing(10),
            ]
            .spacing(12);

            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12).style(|theme| {
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
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmResize),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-resize-title"),
                    width: 460.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::CanvasSize {
            dims,
            anchor,
            fill,
            error,
        } => {
            let active_anchor_idx = photon_protocol::CanvasAnchor::ALL
                .iter()
                .position(|a| a == anchor)
                .unwrap_or(4);
            let mut body_col = column![
                dimension_presets_picker(dims, lang, Message::CanvasSizeDimAction),
                dims.view(lang, Message::CanvasSizeDimAction),
                row![
                    column![
                        text(tr(lang, "label-canvas-anchor")).size(12),
                        anchor_grid_3x3(active_anchor_idx, |idx| {
                            Message::CanvasSizeAnchorChanged(
                                photon_protocol::CanvasAnchor::ALL[idx],
                            )
                        }),
                    ]
                    .spacing(6),
                    iced::widget::space::horizontal(),
                    column![
                        text(tr(lang, "label-canvas-fill")).size(12),
                        checker_swatch(
                            fill.map(rgb_to_color),
                            44.0,
                            24.0,
                            Some(Message::OpenColorPicker(ColorPickerTarget::CanvasSizeFill)),
                        ),
                    ]
                    .spacing(6)
                    .align_x(Alignment::End),
                ]
                .align_y(Alignment::Start),
            ]
            .spacing(12);

            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12).style(|theme| {
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
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmCanvasSize),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-canvas-size-title"),
                    width: 440.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::Batch(batch_state) => {
            let defaults = default_window_state_for(id);
            let spec = match batch_state.kind {
                BatchDialogKind::Resize => {
                    build_batch_resize_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
                BatchDialogKind::Crop => {
                    build_batch_crop_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
                BatchDialogKind::Convert => {
                    build_batch_convert_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
                BatchDialogKind::Transform => {
                    build_batch_transform_dialog(&state.snapshot, batch_state, id, defaults, theme)
                }
            };
            map_batch_dialog_spec(spec)
        }
        InAppDialog::RenameItem { path, name, error } => dialog_spec_to_managed(
            id,
            prompt_dialog_spec(
                PromptDialogSpec::new(
                    tr(lang, "dialog-rename-title"),
                    tr(lang, "action-rename"),
                    name.clone(),
                    tr(lang, "action-rename"),
                    tr(lang, "dialog-cancel"),
                    Message::BrowserRenameChanged,
                    Message::BrowserConfirmRename,
                    Message::CloseDialog,
                    Message::Noop,
                )
                .label(path.clone())
                .error(error.clone()),
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ConfirmDeletePermanently { paths } => dialog_spec_to_managed(
            id,
            confirm_dialog_spec(ConfirmDialogSpec {
                title: tr(lang, "dialog-delete-title"),
                message: format!("{} ({})", tr(lang, "dialog-delete-body"), paths.len()),
                confirm_label: tr(lang, "action-delete-permanently"),
                cancel_label: tr(lang, "dialog-cancel"),
                danger: true,
                secondary_action: None,
                on_confirm: Message::BrowserExecuteDeletePermanently(paths.clone()),
                on_cancel: Message::CloseDialog,
                on_noop: Message::Noop,
            }),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::Export {
            path,
            quality,
            max_edge,
            metadata,
            error,
        } => dialog_spec_to_managed(
            id,
            map_editor_dialog_spec(build_export_dialog(
                lang,
                path,
                *quality,
                max_edge,
                *metadata,
                error.as_deref(),
            )),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ColorPicker {
            target,
            r,
            g,
            b,
            hex,
            transparent,
            ..
        } => dialog_spec_to_managed(
            id,
            map_editor_dialog_spec(build_color_picker_dialog(
                lang,
                *target,
                *r,
                *g,
                *b,
                hex,
                *transparent,
            )),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::AdjustDenoise {
            strength,
            detail_preserve,
            target,
            selection_only,
        } => {
            let has_selection = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|ed| ed.selection.has_selection);
            let mut denoise_items: Vec<Element<'a, Message>> = vec![apply_target_control(
                lang,
                *target,
                Message::DenoiseDialogTargetChanged,
            )];
            if let Some(sel_row) = selection_only_control(
                lang,
                has_selection,
                *selection_only,
                Message::DenoiseDialogSelectionOnlyChanged,
            ) {
                denoise_items.push(sel_row);
            }
            denoise_items.extend([
                slider_stepper_control(
                    tr(lang, "label-strength"),
                    *strength,
                    0.05..=1.0,
                    0.05_f32,
                    format!("{:.0}%", *strength * 100.0),
                    Message::DenoiseDialogStrengthChanged,
                ),
                slider_stepper_control(
                    tr(lang, "label-preserve-detail"),
                    *detail_preserve,
                    0.0..=1.0,
                    0.05_f32,
                    format!("{:.0}%", *detail_preserve * 100.0),
                    Message::DenoiseDialogDetailChanged,
                ),
            ]);
            let body = column(denoise_items).spacing(10);

            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "dialog-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmDenoiseDialog),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-adjust-denoise"),
                    width: 440.0,
                    body: body.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::ManagePlugins {
            search,
            selected_id,
        } => {
            let dlg_rect = state
                .dialog_manager
                .state(&DesktopDialogId::ManagePlugins)
                .cloned()
                .unwrap_or_else(|| default_window_state_for(id))
                .resolved_rect(state.window_size);
            let list_pane_h = (dlg_rect.height - 180.0).max(220.0);
            let body = build_plugins_manager_body(
                state,
                search,
                selected_id.as_deref(),
                Message::ManagePluginsSearchChanged,
                Message::ManagePluginsSelect,
                list_pane_h,
            );

            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-manage-plugins-title"),
                    width: 640.0,
                    body,
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::PluginParams {
            plugin_name,
            specs,
            values,
            target,
            show_target,
            ..
        } => {
            let mut body_col = column![text(plugin_name.clone()).size(14)].spacing(10);
            if *show_target {
                body_col = body_col.push(apply_target_control(
                    lang,
                    *target,
                    Message::PluginParamTargetChanged,
                ));
            }

            for spec in specs {
                let key = spec.key.clone();
                let label = spec.label.clone();
                let current = values
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| default_value_for_plugin_param(spec));
                match spec.kind {
                    PluginParamKind::Float => {
                        let min = spec.min.unwrap_or(0.0);
                        let max = spec.max.unwrap_or(1.0).max(min + 0.001);
                        let step = spec.step.unwrap_or(0.05).max(0.001);
                        let val = match current {
                            PluginParamValue::Float(v) => v,
                            PluginParamValue::Int(v) => v as f64,
                            _ => min,
                        }
                        .clamp(min, max);
                        body_col = body_col.push(slider_stepper_control(
                            label,
                            val as f32,
                            (min as f32)..=(max as f32),
                            step as f32,
                            format!("{val:.2}"),
                            move |v| {
                                Message::PluginParamChanged(
                                    key.clone(),
                                    PluginParamValue::Float(f64::from(v)),
                                )
                            },
                        ));
                    }
                    PluginParamKind::Int => {
                        let min = spec.min.unwrap_or(0.0).round() as i64;
                        let max = (spec.max.unwrap_or(100.0).round() as i64).max(min + 1);
                        let step = spec.step.unwrap_or(1.0).max(1.0);
                        let val = match current {
                            PluginParamValue::Int(v) => v,
                            PluginParamValue::Float(v) => v.round() as i64,
                            _ => min,
                        }
                        .clamp(min, max);
                        body_col = body_col.push(slider_stepper_control(
                            label,
                            val as f32,
                            (min as f32)..=(max as f32),
                            step as f32,
                            val.to_string(),
                            move |v| {
                                Message::PluginParamChanged(
                                    key.clone(),
                                    PluginParamValue::Int(v.round() as i64),
                                )
                            },
                        ));
                    }
                    PluginParamKind::Bool => {
                        let checked = matches!(current, PluginParamValue::Bool(true));
                        body_col = body_col.push(
                            checkbox(checked)
                                .label(label)
                                .text_size(TEXT_SIZE_CONTROL)
                                .on_toggle(move |b| {
                                    Message::PluginParamChanged(
                                        key.clone(),
                                        PluginParamValue::Bool(b),
                                    )
                                }),
                        );
                    }
                    PluginParamKind::Select => {
                        let selected = match current {
                            PluginParamValue::Select(s) => s,
                            _ => spec.options.first().cloned().unwrap_or_default(),
                        };
                        let mut opts_row = row![].spacing(4);
                        for opt in &spec.options {
                            let active = opt == &selected;
                            let k = key.clone();
                            let o = opt.clone();
                            opts_row = opts_row.push(button::labeled(
                                opt.clone(),
                                None,
                                ButtonVariant::Tab { active },
                                Some(Message::PluginParamChanged(k, PluginParamValue::Select(o))),
                            ));
                        }
                        body_col = body_col
                            .push(column![text(label).size(12), opts_row.wrap()].spacing(4));
                    }
                    PluginParamKind::Color => {
                        let rgb = match current {
                            PluginParamValue::Color(c) => c,
                            _ => [0, 0, 0],
                        };
                        let k0 = key.clone();
                        let k1 = key.clone();
                        let k2 = key;
                        body_col = body_col.push(
                            column![
                                row![
                                    text(label).size(12),
                                    iced::widget::space::horizontal(),
                                    text(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
                                        .size(11),
                                ],
                                slider_stepper_control(
                                    "R",
                                    f32::from(rgb[0]),
                                    0.0..=255.0,
                                    1.0_f32,
                                    rgb[0].to_string(),
                                    move |v| {
                                        Message::PluginParamChanged(
                                            k0.clone(),
                                            PluginParamValue::Color([
                                                v.round() as u8,
                                                rgb[1],
                                                rgb[2],
                                            ]),
                                        )
                                    },
                                ),
                                slider_stepper_control(
                                    "G",
                                    f32::from(rgb[1]),
                                    0.0..=255.0,
                                    1.0_f32,
                                    rgb[1].to_string(),
                                    move |v| {
                                        Message::PluginParamChanged(
                                            k1.clone(),
                                            PluginParamValue::Color([
                                                rgb[0],
                                                v.round() as u8,
                                                rgb[2],
                                            ]),
                                        )
                                    },
                                ),
                                slider_stepper_control(
                                    "B",
                                    f32::from(rgb[2]),
                                    0.0..=255.0,
                                    1.0_f32,
                                    rgb[2].to_string(),
                                    move |v| {
                                        Message::PluginParamChanged(
                                            k2.clone(),
                                            PluginParamValue::Color([
                                                rgb[0],
                                                rgb[1],
                                                v.round() as u8,
                                            ]),
                                        )
                                    },
                                ),
                            ]
                            .spacing(4),
                        );
                    }
                    PluginParamKind::String => {
                        let text_val = current.as_str().unwrap_or_default().to_owned();
                        body_col = body_col.push(
                            column![
                                text(label).size(12),
                                text_input("", &text_val)
                                    .on_input(move |s| {
                                        Message::PluginParamChanged(
                                            key.clone(),
                                            PluginParamValue::Select(s),
                                        )
                                    })
                                    .size(TEXT_SIZE_CONTROL)
                                    .padding([5, 8])
                                    .style(input_style),
                            ]
                            .spacing(4),
                        );
                    }
                }
            }

            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "dialog-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-run-plugin"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Message::ConfirmPluginParams),
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-plugin-params-title"),
                    width: 440.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::PluginModels { plugin_id } => {
            let plugin = state
                .snapshot
                .meta
                .plugins
                .iter()
                .find(|p| &p.id == plugin_id);
            let title = plugin
                .map(|p| {
                    format!(
                        "{} — {}",
                        p.display_name(lang),
                        tr(lang, "action-plugin-settings-models")
                    )
                })
                .unwrap_or_else(|| tr(lang, "settings-plugins-models-title"));
            let has_models = plugin.is_some_and(|p| !p.models.is_empty());
            let active_tab = if has_models {
                state.plugin_settings_tab
            } else {
                PluginSettingsTab::General
            };

            let mut tabs = vec![
                PanelTabItem::new(
                    "general",
                    tr(lang, "plugin-settings-tab-general"),
                    active_tab == PluginSettingsTab::General,
                    Message::SelectPluginSettingsTab(PluginSettingsTab::General),
                )
                .icon(Icon::Settings),
            ];
            if has_models {
                tabs.push(
                    PanelTabItem::new(
                        "models",
                        tr(lang, "plugin-settings-tab-models"),
                        active_tab == PluginSettingsTab::Models,
                        Message::SelectPluginSettingsTab(PluginSettingsTab::Models),
                    )
                    .icon(Icon::Plug),
                );
            }
            let tab_bar = panel_tab_bar(tabs, false);

            let tab_body: Element<'a, Message> = match active_tab {
                PluginSettingsTab::General => {
                    let mut col = column![].spacing(8);
                    if let Some(p) = plugin {
                        let qa_enabled = !state
                            .snapshot
                            .meta
                            .disabled_quick_action_plugins
                            .iter()
                            .any(|id| id == &p.id);
                        let has_quick = p.quick_actions.iter().any(|qa| qa.slot == "quick");
                        col = col.push(setting_row(
                            tr(lang, "label-plugin-enabled"),
                            row![
                                button::labeled(
                                    tr(lang, "action-on"),
                                    None,
                                    ButtonVariant::Tab { active: p.enabled },
                                    Some(Message::Dispatch(Command::PluginSetEnabled {
                                        id: p.id.clone(),
                                        enabled: true,
                                    })),
                                ),
                                button::labeled(
                                    tr(lang, "action-off"),
                                    None,
                                    ButtonVariant::Tab { active: !p.enabled },
                                    Some(Message::Dispatch(Command::PluginSetEnabled {
                                        id: p.id.clone(),
                                        enabled: false,
                                    })),
                                ),
                            ]
                            .spacing(6),
                        ));
                        if has_quick {
                            col = col.push(setting_row(
                                tr(lang, "plugin-settings-quick-action"),
                                row![
                                    button::labeled(
                                        tr(lang, "action-on"),
                                        None,
                                        ButtonVariant::Tab { active: qa_enabled },
                                        Some(Message::Dispatch(
                                            Command::PluginSetQuickActionEnabled {
                                                id: p.id.clone(),
                                                enabled: true,
                                            }
                                        )),
                                    ),
                                    button::labeled(
                                        tr(lang, "action-off"),
                                        None,
                                        ButtonVariant::Tab {
                                            active: !qa_enabled,
                                        },
                                        Some(Message::Dispatch(
                                            Command::PluginSetQuickActionEnabled {
                                                id: p.id.clone(),
                                                enabled: false,
                                            }
                                        )),
                                    ),
                                ]
                                .spacing(6),
                            ));
                        }
                    }
                    section_card(tr(lang, "plugin-settings-tab-general"), col)
                }
                PluginSettingsTab::Models => {
                    build_models_card_for_plugins(state, &[plugin_id.as_str()])
                }
            };

            let body: Element<'a, Message> =
                column![tab_bar, tab_body].spacing(10).width(Fill).into();
            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title,
                    width: 540.0,
                    body,
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::TranslateSettings => {
            let ocr_plugin_ids: Vec<&str> = state
                .snapshot
                .meta
                .plugins
                .iter()
                .filter(|p| p.models.iter().any(|m| m.tier == "ocr"))
                .map(|p| p.id.as_str())
                .collect();
            let mut body = column![build_settings_translate_tab(state)].spacing(10);
            if !ocr_plugin_ids.is_empty() {
                body = body.push(section_card(
                    tr(lang, "settings-plugins-models-title"),
                    build_models_card_for_plugins(state, &ocr_plugin_ids),
                ));
            }
            let footer = row![
                iced::widget::space::horizontal(),
                button::labeled(
                    tr(lang, "action-close"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::CloseDialog),
                ),
            ];
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "action-translate-settings"),
                    width: 560.0,
                    body: body.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::TransformEach { spec } => dialog_spec_to_managed(
            id,
            map_editor_dialog_spec(build_transform_each_dialog(lang, spec)),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::VectorBlend { spec } => dialog_spec_to_managed(
            id,
            map_editor_dialog_spec(build_vector_blend_dialog(lang, spec)),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::ExportVectorSvg { path, error } => dialog_spec_to_managed(
            id,
            map_editor_dialog_spec(build_export_vector_svg_dialog(lang, path, error.as_deref())),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
        InAppDialog::SliceImage {
            is_grid,
            rows,
            cols,
            strip_height,
            output_dir,
            prefix,
            format,
            error,
        } => {
            let mode_row = row![
                button::labeled(
                    tr(lang, "slice-mode-grid"),
                    None,
                    if *is_grid {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::SliceImageModeChanged(true)),
                ),
                button::labeled(
                    tr(lang, "slice-mode-strip"),
                    None,
                    if !*is_grid {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::SliceImageModeChanged(false)),
                ),
            ]
            .spacing(8);

            let dims_row: Element<'_, Message> = if *is_grid {
                row![
                    column![
                        text(tr(lang, "slice-rows")).size(12),
                        text_input("2", rows)
                            .id(iced::widget::Id::new("photon.dialog.first_input"))
                            .on_input(Message::SliceImageRowsChanged)
                            .padding([6, 8]),
                    ]
                    .spacing(4)
                    .width(Fill),
                    column![
                        text(tr(lang, "slice-cols")).size(12),
                        text_input("2", cols)
                            .on_input(Message::SliceImageColsChanged)
                            .padding([6, 8]),
                    ]
                    .spacing(4)
                    .width(Fill),
                ]
                .spacing(8)
                .into()
            } else {
                column![
                    text(tr(lang, "slice-strip-height")).size(12),
                    text_input("1024", strip_height)
                        .id(iced::widget::Id::new("photon.dialog.first_input"))
                        .on_input(Message::SliceImageStripHeightChanged)
                        .padding([6, 8]),
                ]
                .spacing(4)
                .into()
            };

            let mut body = column![
                mode_row,
                dims_row,
                column![
                    text(tr(lang, "slice-output-dir")).size(12),
                    text_input("/tmp/slices", output_dir)
                        .on_input(Message::SliceImageOutputDirChanged)
                        .padding([6, 8]),
                ]
                .spacing(4),
                row![
                    column![
                        text(tr(lang, "slice-prefix")).size(12),
                        text_input("slice", prefix)
                            .on_input(Message::SliceImagePrefixChanged)
                            .padding([6, 8]),
                    ]
                    .spacing(4)
                    .width(Fill),
                    column![
                        text(tr(lang, "slice-format")).size(12),
                        text_input("png", format)
                            .on_input(Message::SliceImageFormatChanged)
                            .padding([6, 8]),
                    ]
                    .spacing(4)
                    .width(Length::Fixed(90.0)),
                ]
                .spacing(8),
            ]
            .spacing(10);

            if let Some(err) = error {
                body = body.push(text(err.clone()).size(12).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.danger),
                    }
                }));
            }

            let footer = row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-export"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::ConfirmSliceImage),
                ),
            ]
            .spacing(8);

            let spec = DialogSpec {
                open: true,
                title: tr(lang, "action-slice-image"),
                width: 440.0,
                body: body.into(),
                footer: footer.into(),
                on_close: Message::CloseDialog,
                on_noop: Message::Noop,
            };

            dialog_spec_to_managed(id, spec, default_window_state_for(id), overlay, lang, theme)
        }
        InAppDialog::RecoveryPrompt { count } => {
            let body = column![
                text(tr_ui_text(
                    lang,
                    &UiText::with("dialog_recovery_body", [("count", count.to_string())]),
                ))
                .size(13)
            ]
            .spacing(8);
            let footer = row![
                button::labeled(
                    tr(lang, "action-recovery-discard"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::DiscardRecoverySession),
                ),
                button::labeled(
                    tr(lang, "action-recovery-restore"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::RestoreRecoverySession),
                ),
            ]
            .spacing(8);
            let spec = DialogSpec {
                open: true,
                title: tr(lang, "dialog-recovery-title"),
                width: 420.0,
                body: body.into(),
                footer: footer.into(),
                on_close: Message::CloseDialog,
                on_noop: Message::Noop,
            };
            dialog_spec_to_managed(id, spec, default_window_state_for(id), overlay, lang, theme)
        }
        InAppDialog::SaveSelection {
            name,
            target_id,
            op,
            error,
        } => {
            let saved = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|ed| ed.selection.saved_selections.as_slice())
                .unwrap_or(&[]);
            let mut body_col = column![
                text(tr(lang, "dialog-save-selection-name")).size(12),
                text_input("Selection", name)
                    .id(iced::widget::Id::new("photon.dialog.first_input"))
                    .on_input(Message::SaveSelectionNameChanged)
                    .on_submit(Message::ConfirmSaveSelection)
                    .padding(6),
            ]
            .spacing(8);

            if !saved.is_empty() {
                let mut channel_btns = row![button::labeled(
                    tr(lang, "label-new-channel"),
                    None,
                    if target_id.is_none() {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::SaveSelectionTargetChanged(None)),
                )]
                .spacing(6);
                for ch in saved {
                    channel_btns = channel_btns.push(button::labeled(
                        ch.name.clone(),
                        None,
                        if *target_id == Some(ch.id) {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        Some(Message::SaveSelectionTargetChanged(Some(ch.id))),
                    ));
                }
                body_col = body_col
                    .push(text(tr(lang, "dialog-save-selection-target")).size(12))
                    .push(channel_btns);

                if target_id.is_some() {
                    let op_row = row![
                        selection_op_btn(
                            lang,
                            "select-op-replace",
                            SelectionBooleanOp::Replace,
                            *op,
                            Message::SaveSelectionOpChanged
                        ),
                        selection_op_btn(
                            lang,
                            "select-op-add",
                            SelectionBooleanOp::Add,
                            *op,
                            Message::SaveSelectionOpChanged
                        ),
                        selection_op_btn(
                            lang,
                            "select-op-subtract",
                            SelectionBooleanOp::Subtract,
                            *op,
                            Message::SaveSelectionOpChanged
                        ),
                        selection_op_btn(
                            lang,
                            "select-op-intersect",
                            SelectionBooleanOp::Intersect,
                            *op,
                            Message::SaveSelectionOpChanged
                        ),
                    ]
                    .spacing(6);
                    body_col = body_col
                        .push(text(tr(lang, "label-operation")).size(12))
                        .push(op_row);
                }
            }
            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12));
            }
            let footer = row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::ConfirmSaveSelection),
                ),
            ]
            .spacing(8);
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-save-selection-title"),
                    width: 420.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::LoadSelection {
            selected_id,
            op,
            invert,
            error,
        } => {
            let saved = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|ed| ed.selection.saved_selections.as_slice())
                .unwrap_or(&[]);
            let mut list_col = column![].spacing(4);
            for ch in saved {
                let active = *selected_id == Some(ch.id);
                list_col = list_col.push(
                    row![
                        button::labeled(
                            ch.name.clone(),
                            None,
                            if active {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Secondary
                            },
                            Some(Message::LoadSelectionSelectedChanged(ch.id)),
                        ),
                        button::icon_only(
                            Icon::Trash,
                            ButtonVariant::Ghost,
                            Some(Message::DeleteSavedSelection(ch.id)),
                        ),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
            let op_row = row![
                selection_op_btn(
                    lang,
                    "select-op-replace",
                    SelectionBooleanOp::Replace,
                    *op,
                    Message::LoadSelectionOpChanged
                ),
                selection_op_btn(
                    lang,
                    "select-op-add",
                    SelectionBooleanOp::Add,
                    *op,
                    Message::LoadSelectionOpChanged
                ),
                selection_op_btn(
                    lang,
                    "select-op-subtract",
                    SelectionBooleanOp::Subtract,
                    *op,
                    Message::LoadSelectionOpChanged
                ),
                selection_op_btn(
                    lang,
                    "select-op-intersect",
                    SelectionBooleanOp::Intersect,
                    *op,
                    Message::LoadSelectionOpChanged
                ),
            ]
            .spacing(6);
            let invert_btn = button::labeled(
                tr(lang, "action-select-invert"),
                None,
                if *invert {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                },
                Some(Message::LoadSelectionInvertChanged(!*invert)),
            );
            let mut body_col = column![
                text(tr(lang, "dialog-load-selection-channel")).size(12),
                list_col,
                text(tr(lang, "label-operation")).size(12),
                op_row,
                invert_btn,
            ]
            .spacing(8);
            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12));
            }
            let footer = row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    None,
                    ButtonVariant::Primary,
                    selected_id.map(|_| Message::ConfirmLoadSelection),
                ),
            ]
            .spacing(8);
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-load-selection-title"),
                    width: 420.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::NewGuide {
            axis,
            value_input,
            unit_percent,
            error,
        } => {
            let axis_row = row![
                button::labeled(
                    tr(lang, "guide-axis-horizontal"),
                    None,
                    if *axis == photon_canvas::GuideAxis::Horizontal {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::NewGuideAxisChanged(
                        photon_canvas::GuideAxis::Horizontal,
                    )),
                ),
                button::labeled(
                    tr(lang, "guide-axis-vertical"),
                    None,
                    if *axis == photon_canvas::GuideAxis::Vertical {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::NewGuideAxisChanged(
                        photon_canvas::GuideAxis::Vertical,
                    )),
                ),
            ]
            .spacing(8);
            let pos_row = row![
                text_input("50", value_input)
                    .id(iced::widget::Id::new("photon.dialog.first_input"))
                    .on_input(Message::NewGuideValueChanged)
                    .on_submit(Message::ConfirmNewGuide)
                    .padding(6)
                    .width(Length::Fill),
                button::labeled(
                    "px",
                    None,
                    if !*unit_percent {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::NewGuideUnitChanged(false)),
                ),
                button::labeled(
                    "%",
                    None,
                    if *unit_percent {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                    Some(Message::NewGuideUnitChanged(true)),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            let mut body_col = column![
                text(tr(lang, "guide-orientation")).size(12),
                axis_row,
                text(tr(lang, "guide-position")).size(12),
                pos_row,
            ]
            .spacing(8);
            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12));
            }
            let footer = row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::ConfirmNewGuide),
                ),
            ]
            .spacing(8);
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-new-guide-title"),
                    width: 380.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::GridSettings {
            spacing_input,
            subdivisions_input,
            opacity_input,
            show_grid,
            snap_to_grid,
            error,
        } => {
            let mut body_col = column![
                text(tr(lang, "grid-spacing-px")).size(12),
                text_input("64", spacing_input)
                    .id(iced::widget::Id::new("photon.dialog.first_input"))
                    .on_input(Message::GridSettingsSpacingChanged)
                    .on_submit(Message::ConfirmGridSettings)
                    .padding(6),
                text(tr(lang, "grid-subdivisions")).size(12),
                text_input("4", subdivisions_input)
                    .on_input(Message::GridSettingsSubdivisionsChanged)
                    .on_submit(Message::ConfirmGridSettings)
                    .padding(6),
                text(tr(lang, "grid-opacity-pct")).size(12),
                text_input("100", opacity_input)
                    .on_input(Message::GridSettingsOpacityChanged)
                    .on_submit(Message::ConfirmGridSettings)
                    .padding(6),
                row![
                    button::labeled(
                        tr(lang, "menu-view-grid"),
                        None,
                        if *show_grid {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        Some(Message::GridSettingsShowGridChanged(!*show_grid)),
                    ),
                    button::labeled(
                        tr(lang, "grid-snap-to-grid"),
                        Some(Icon::Magnet),
                        if *snap_to_grid {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        },
                        Some(Message::GridSettingsSnapChanged(!*snap_to_grid)),
                    ),
                ]
                .spacing(8),
            ]
            .spacing(8);
            if let Some(err) = error {
                body_col = body_col.push(text(err.clone()).size(12));
            }
            let footer = row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    None,
                    ButtonVariant::Secondary,
                    Some(Message::CloseDialog),
                ),
                button::labeled(
                    tr(lang, "action-apply"),
                    None,
                    ButtonVariant::Primary,
                    Some(Message::ConfirmGridSettings),
                ),
            ]
            .spacing(8);
            dialog_spec_to_managed(
                id,
                DialogSpec {
                    open: true,
                    title: tr(lang, "dialog-grid-settings-title"),
                    width: 420.0,
                    body: body_col.into(),
                    footer: footer.into(),
                    on_close: Message::CloseDialog,
                    on_noop: Message::Noop,
                },
                default_window_state_for(id),
                overlay,
                lang,
                theme,
            )
        }
        InAppDialog::Alert { title, message, .. } => dialog_spec_to_managed(
            id,
            alert_dialog_spec(
                title.clone(),
                message.clone(),
                tr(lang, "action-close"),
                Message::CloseDialog,
                Message::Noop,
            ),
            default_window_state_for(id),
            overlay,
            lang,
            theme,
        ),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SizePresetChoice {
    w: u32,
    h: u32,
    label: &'static str,
}

impl std::fmt::Display for SizePresetChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

const COMMON_SIZE_PRESETS: &[SizePresetChoice] = &[
    SizePresetChoice {
        w: 1080,
        h: 1080,
        label: "1080×1080 (1:1 Square)",
    },
    SizePresetChoice {
        w: 1080,
        h: 1350,
        label: "1080×1350 (4:5 Social Portrait)",
    },
    SizePresetChoice {
        w: 1080,
        h: 1920,
        label: "1080×1920 (9:16 Story / Reel)",
    },
    SizePresetChoice {
        w: 1280,
        h: 720,
        label: "1280×720 (HD 720p)",
    },
    SizePresetChoice {
        w: 1920,
        h: 1080,
        label: "1920×1080 (Full HD)",
    },
    SizePresetChoice {
        w: 3840,
        h: 2160,
        label: "3840×2160 (4K UHD)",
    },
    SizePresetChoice {
        w: 800,
        h: 1280,
        label: "800×1280 (Webtoon Strip)",
    },
    SizePresetChoice {
        w: 1600,
        h: 2400,
        label: "1600×2400 (Manga Page)",
    },
    SizePresetChoice {
        w: 1240,
        h: 1754,
        label: "1240×1754 (A4 150 DPI)",
    },
    SizePresetChoice {
        w: 2480,
        h: 3508,
        label: "2480×3508 (A4 300 DPI)",
    },
];

fn dimension_presets_picker<'a>(
    dims: &DimensionInputState,
    lang: Language,
    on_action: fn(DimGridAction) -> Message,
) -> Element<'a, Message> {
    let selected = COMMON_SIZE_PRESETS
        .iter()
        .find(|p| p.w == dims.width_px && p.h == dims.height_px)
        .copied();
    row![
        text(tr(lang, "label-size-preset"))
            .size(12)
            .width(Length::Fixed(104.0)),
        pick_list(COMMON_SIZE_PRESETS, selected, move |choice| {
            on_action(DimGridAction::PresetDimensions(choice.w, choice.h))
        })
        .placeholder("Custom…")
        .text_size(TEXT_SIZE_CONTROL)
        .padding([5, 10])
        .width(Fill)
        .style(pick_list_style)
        .menu_style(pick_list_menu_style),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

pub(super) fn resolve_desktop_plugins_dir() -> PathBuf {
    if let Ok(env_dir) = std::env::var("PHOTON_PLUGINS_DIR")
        && !env_dir.trim().is_empty()
    {
        return PathBuf::from(env_dir.trim());
    }
    photon_storage::AppPaths::current().data_dir.join("plugins")
}

pub(super) fn default_value_for_plugin_param(spec: &PluginParamSpec) -> PluginParamValue {
    if let Some(def) = &spec.default {
        return def.clone();
    }
    match spec.kind {
        PluginParamKind::Float => PluginParamValue::Float(spec.min.unwrap_or(0.0)),
        PluginParamKind::Int => PluginParamValue::Int(spec.min.unwrap_or(0.0).round() as i64),
        PluginParamKind::Bool => PluginParamValue::Bool(false),
        PluginParamKind::Select => {
            PluginParamValue::Select(spec.options.first().cloned().unwrap_or_default())
        }
        PluginParamKind::String => {
            PluginParamValue::Select(spec.options.first().cloned().unwrap_or_default())
        }
        PluginParamKind::Color => PluginParamValue::Color([0, 0, 0]),
    }
}

pub(super) fn build_models_card_for_plugins<'a>(
    state: &'a PhotonGui,
    plugin_ids: &[&str],
) -> Element<'a, Message> {
    let lang = state.snapshot.meta.language;
    let mut allowed_ids = BTreeSet::new();
    for p in &state.snapshot.meta.plugins {
        if plugin_ids.contains(&p.id.as_str()) {
            for m in &p.models {
                allowed_ids.insert(m.id.clone());
            }
        }
    }
    let models: Vec<_> = state
        .snapshot
        .meta
        .ai_models
        .iter()
        .filter(|m| allowed_ids.contains(&m.id))
        .collect();
    let task = state.snapshot.meta.ai_task.as_ref();
    let running = task.is_some_and(|t| t.running);

    let mut col = column![].spacing(6);
    if let Some(t) = task.filter(|t| t.running) {
        col = col.push(
            row![
                text(format!("{}% · {}", t.percent, t.stage))
                    .size(11)
                    .width(Fill),
                button::labeled(
                    tr(lang, "action-cancel"),
                    Some(Icon::Close),
                    ButtonVariant::Danger,
                    Some(Message::Dispatch(Command::AiCancel)),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    if models.is_empty() {
        col = col.push(
            text(tr(lang, "menu-plugins-none"))
                .size(12)
                .style(|th| text::Style {
                    color: Some(PaletteTokens::for_theme(th).text_muted),
                }),
        );
    } else {
        for m in models {
            let size_label = if m.size_bytes > 0 {
                format!("{:.1} MB", m.size_bytes as f64 / (1024.0 * 1024.0))
            } else {
                String::new()
            };
            let badge = if m.installed {
                tr(lang, "model-status-installed")
            } else {
                tr(lang, "model-status-missing")
            };
            let installed = m.installed;
            let mut row_items = row![
                column![
                    text(m.name.clone()).size(12),
                    text(if size_label.is_empty() {
                        m.id.clone()
                    } else {
                        format!("{} · {size_label}", m.id)
                    })
                    .size(10)
                    .style(|th| text::Style {
                        color: Some(PaletteTokens::for_theme(th).text_muted),
                    }),
                ]
                .spacing(2)
                .width(Fill),
                text(badge).size(11).style(move |th| text::Style {
                    color: Some(if installed {
                        PaletteTokens::for_theme(th).success
                    } else {
                        PaletteTokens::for_theme(th).warning
                    }),
                }),
                button::labeled(
                    tr(lang, "action-local-file"),
                    Some(Icon::FolderOpen),
                    ButtonVariant::Ghost,
                    (!running).then(|| Message::PickCustomModelFile(m.id.clone())),
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            if m.installed {
                row_items = row_items.push(button::labeled(
                    tr(lang, "action-delete"),
                    Some(Icon::Trash),
                    ButtonVariant::Danger,
                    (!running).then(|| {
                        Message::Dispatch(Command::AiDeleteModel {
                            model_id: m.id.clone(),
                        })
                    }),
                ));
            } else {
                row_items = row_items.push(button::labeled(
                    tr(lang, "action-download"),
                    None,
                    ButtonVariant::Primary,
                    (!running).then(|| {
                        Message::Dispatch(Command::AiDownloadModel {
                            model_id: m.id.clone(),
                        })
                    }),
                ));
            }
            col = col.push(row_items);
        }
    }

    col.into()
}

fn selection_op_btn<'a>(
    lang: Language,
    label_key: &str,
    op: SelectionBooleanOp,
    active: SelectionBooleanOp,
    on_select: fn(SelectionBooleanOp) -> Message,
) -> Element<'a, Message> {
    button::labeled(
        tr(lang, label_key),
        None,
        if active == op {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        },
        Some(on_select(op)),
    )
    .into()
}
