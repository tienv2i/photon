use super::*;
use iced::Border;
use iced::widget::{button as iced_button, column};
use photon_ui::theme::{RADIUS_SM, card_style};

#[derive(Clone, PartialEq, Eq)]
struct Choice<T: PartialEq> {
    value: T,
    label: String,
}

impl<T: PartialEq> std::fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

fn settings_dropdown<'a, T: Copy + PartialEq + 'static>(
    items: Vec<(T, String)>,
    selected_value: T,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let options: Vec<Choice<T>> = items
        .into_iter()
        .map(|(value, label)| Choice { value, label })
        .collect();
    let selected = options.iter().find(|c| c.value == selected_value).cloned();
    pick_list(options, selected, move |c: Choice<T>| on_select(c.value))
        .text_size(TEXT_SIZE_CONTROL)
        .padding([5, 10])
        .width(Length::Fixed(180.0))
        .style(pick_list_style)
        .menu_style(pick_list_menu_style)
        .into()
}

fn bool_dropdown<'a>(
    lang: Language,
    active: bool,
    on_select: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    settings_dropdown(
        vec![
            (true, tr(lang, "action-on")),
            (false, tr(lang, "action-off")),
        ],
        active,
        on_select,
    )
}

fn collapsible_card<'a>(
    state: &PhotonGui,
    key: &'static str,
    title: String,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let collapsed = state.settings_collapsed.contains(key);
    let chevron = if collapsed {
        Icon::ChevronRight
    } else {
        Icon::ChevronDown
    };
    let header = row![
        iced_button(text(title).size(13).width(Length::Fill))
            .on_press(Message::ToggleSettingsSection(key))
            .padding([4, 2])
            .width(Length::Fill)
            .style(|theme, status| {
                let t = PaletteTokens::for_theme(theme);
                let hovered = matches!(
                    status,
                    iced_button::Status::Hovered | iced_button::Status::Pressed
                );
                iced_button::Style {
                    background: hovered.then_some(t.bg_hover.into()),
                    text_color: t.text_primary,
                    border: Border {
                        radius: RADIUS_SM.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
        button::compact_icon(
            chevron,
            ButtonVariant::Ghost,
            Some(Message::ToggleSettingsSection(key)),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    let mut content = column![header].spacing(8).width(Length::Fill);
    if !collapsed {
        content = content.push(body.into());
    }

    container(content)
        .padding(10)
        .width(Length::Fill)
        .style(card_style)
        .into()
}

pub(super) fn setting_row<'a>(
    label: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        container(text(label).size(TEXT_SIZE_CONTROL)).width(Length::Fill),
        control.into(),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

pub(super) fn build_settings_header_strip(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let dlg_rect = state
        .dialog_manager
        .state(&DesktopDialogId::Settings)
        .cloned()
        .unwrap_or_else(|| default_window_state_for(DesktopDialogId::Settings))
        .resolved_rect(state.window_size);
    let compact = dlg_rect.width < 520.0;

    let tabs = vec![
        PanelTabItem::new(
            SettingsTab::General.as_str(),
            tr(lang, "settings-tab-general"),
            state.settings_tab == SettingsTab::General,
            Message::SelectSettingsTab(SettingsTab::General),
        )
        .icon(Icon::Settings)
        .tooltip(tr(lang, "settings-tab-general")),
        PanelTabItem::new(
            SettingsTab::Viewer.as_str(),
            tr(lang, "settings-tab-viewer"),
            state.settings_tab == SettingsTab::Viewer,
            Message::SelectSettingsTab(SettingsTab::Viewer),
        )
        .icon(Icon::Viewer)
        .tooltip(tr(lang, "settings-tab-viewer")),
        PanelTabItem::new(
            SettingsTab::Browser.as_str(),
            tr(lang, "settings-tab-browser"),
            state.settings_tab == SettingsTab::Browser,
            Message::SelectSettingsTab(SettingsTab::Browser),
        )
        .icon(Icon::Browser)
        .tooltip(tr(lang, "settings-tab-browser")),
        PanelTabItem::new(
            SettingsTab::Editor.as_str(),
            tr(lang, "settings-tab-editor"),
            state.settings_tab == SettingsTab::Editor,
            Message::SelectSettingsTab(SettingsTab::Editor),
        )
        .icon(Icon::Editor)
        .tooltip(tr(lang, "settings-tab-editor")),
        PanelTabItem::new(
            SettingsTab::Plugins.as_str(),
            tr(lang, "settings-tab-plugins"),
            state.settings_tab == SettingsTab::Plugins,
            Message::SelectSettingsTab(SettingsTab::Plugins),
        )
        .icon(Icon::Plug)
        .tooltip(tr(lang, "settings-tab-plugins")),
    ];

    panel_tab_bar(tabs, compact)
}

pub(super) fn build_settings_body(state: &PhotonGui) -> Element<'_, Message> {
    match state.settings_tab {
        SettingsTab::General => build_settings_general_tab(state),
        SettingsTab::Viewer => build_settings_viewer_tab(state),
        SettingsTab::Browser => build_settings_browser_tab(state),
        SettingsTab::Editor => build_settings_editor_tab(state),
        SettingsTab::Plugins => build_settings_plugins_tab(state),
    }
}

fn system_open_mode_options(lang: Language) -> Vec<(SystemOpenWindowMode, String)> {
    vec![
        (
            SystemOpenWindowMode::FitImage,
            tr(lang, "settings-open-fit"),
        ),
        (
            SystemOpenWindowMode::Maximized,
            tr(lang, "settings-open-maximized"),
        ),
        (
            SystemOpenWindowMode::FixedSize,
            tr(lang, "settings-open-fixed"),
        ),
        (
            SystemOpenWindowMode::Remembered,
            tr(lang, "settings-open-remembered"),
        ),
        (
            SystemOpenWindowMode::Fullscreen,
            tr(lang, "settings-open-fullscreen"),
        ),
    ]
}

pub(super) fn display_font_label(lang: Language, font: DisplayFont) -> String {
    match font {
        DisplayFont::System => tr(lang, "display-font-system"),
        DisplayFont::BundledSans => tr(lang, "display-font-bundled"),
        DisplayFont::Inter => "Inter".to_owned(),
        DisplayFont::Cantarell => "Cantarell".to_owned(),
        DisplayFont::NotoSans => "Noto Sans".to_owned(),
        DisplayFont::DejaVuSans => "DejaVu Sans".to_owned(),
        DisplayFont::Serif => tr(lang, "display-font-serif"),
        DisplayFont::Monospace => tr(lang, "display-font-monospace"),
    }
}

pub(super) fn build_settings_general_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let app_launch_mode = meta.layout.app_launch_window_mode;
    let sys_open_mode = meta.layout.system_open_window_mode;
    let ui_scale_pct = meta.layout.ui_scale_pct;
    let font_scale_pct = meta.layout.font_scale_pct;
    let display_font = meta.layout.display_font;

    let appearance_card = collapsible_card(
        state,
        "general.appearance",
        tr(lang, "settings-section-general"),
        column![
            setting_row(
                tr(lang, "settings-theme"),
                settings_dropdown(
                    vec![
                        (Theme::Dark, tr(lang, "settings-theme-dark")),
                        (Theme::Light, tr(lang, "settings-theme-light")),
                    ],
                    meta.theme,
                    |theme| Message::Dispatch(Command::SetTheme { theme }),
                ),
            ),
            setting_row(
                tr(lang, "settings-language"),
                settings_dropdown(
                    vec![
                        (Language::System, tr(lang, "lang-system")),
                        (Language::English, "English".to_owned()),
                        (Language::Vietnamese, "Tiếng Việt".to_owned()),
                    ],
                    lang,
                    |language| Message::Dispatch(Command::SetLanguage { language }),
                ),
            ),
            setting_row(
                tr(lang, "settings-display-font"),
                settings_dropdown(
                    DisplayFont::ALL
                        .into_iter()
                        .map(|f| (f, display_font_label(lang, f)))
                        .collect(),
                    display_font,
                    Message::SetDisplayFont,
                ),
            ),
            slider_stepper_control(
                tr(lang, "settings-ui-scale"),
                f32::from(ui_scale_pct),
                f32::from(photon_protocol::UI_SCALE_PCT_MIN)
                    ..=f32::from(photon_protocol::UI_SCALE_PCT_MAX),
                5.0_f32,
                format!("{ui_scale_pct}%"),
                |v| Message::SetUiScalePct((v / 5.0).round() as u16 * 5),
            ),
            slider_stepper_control(
                tr(lang, "settings-font-scale"),
                f32::from(font_scale_pct),
                f32::from(photon_protocol::FONT_SCALE_PCT_MIN)
                    ..=f32::from(photon_protocol::FONT_SCALE_PCT_MAX),
                5.0_f32,
                format!("{font_scale_pct}%"),
                |v| Message::SetFontScalePct((v / 5.0).round() as u16 * 5),
            ),
        ]
        .spacing(8),
    );

    let window_session_card = collapsible_card(
        state,
        "general.session",
        tr(lang, "settings-section-window-session"),
        column![
            setting_row(
                tr(lang, "settings-app-launch-window"),
                settings_dropdown(
                    vec![
                        (
                            AppLaunchWindowMode::Maximized,
                            tr(lang, "settings-launch-maximized"),
                        ),
                        (
                            AppLaunchWindowMode::Remembered,
                            tr(lang, "settings-launch-remembered"),
                        ),
                    ],
                    app_launch_mode,
                    Message::SetAppLaunchWindowMode,
                ),
            ),
            setting_row(
                tr(lang, "settings-system-open"),
                settings_dropdown(
                    system_open_mode_options(lang),
                    sys_open_mode,
                    Message::SetSystemOpenWindowMode,
                ),
            ),
            setting_row(
                tr(lang, "settings-remember-folder"),
                bool_dropdown(lang, meta.remember_last_folder, |remember_last_folder| {
                    Message::Dispatch(Command::SetSettings {
                        theme: meta.theme,
                        language: meta.language,
                        remember_last_folder,
                        hardware_acceleration: meta.hardware_acceleration,
                        save_ui_state_on_exit: meta.save_ui_state_on_exit,
                    })
                }),
            ),
            setting_row(
                tr(lang, "settings-save-ui-on-exit"),
                row![
                    bool_dropdown(lang, meta.save_ui_state_on_exit, |save_ui_state_on_exit| {
                        Message::Dispatch(Command::SetSettings {
                            theme: meta.theme,
                            language: meta.language,
                            remember_last_folder: meta.remember_last_folder,
                            hardware_acceleration: meta.hardware_acceleration,
                            save_ui_state_on_exit,
                        })
                    }),
                    button::compact_icon_tipped(
                        Icon::Save,
                        ButtonVariant::Secondary,
                        Some(Message::SaveCurrentUiLayout),
                        tr(lang, "action-save-ui-layout"),
                    ),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ),
        ]
        .spacing(8),
    );

    let mut recovery_col = column![].spacing(8);
    if meta.safe_mode {
        recovery_col = recovery_col.push(button::labeled(
            tr(lang, "settings-safe-mode-apply"),
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(Message::ApplySafeDefaultsToDisk),
        ));
    }
    recovery_col = recovery_col.push(
        row![
            button::labeled(
                tr(lang, "action-reset-layout"),
                Some(Icon::Reset),
                ButtonVariant::Secondary,
                Some(Message::RequestResetLayout),
            ),
            button::labeled(
                if state.settings_confirm_factory_reset {
                    tr(lang, "settings-factory-reset-confirm")
                } else {
                    tr(lang, "settings-factory-reset")
                },
                Some(Icon::Trash),
                if state.settings_confirm_factory_reset {
                    ButtonVariant::Danger
                } else {
                    ButtonVariant::Secondary
                },
                Some(Message::RequestFactoryReset),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    let recovery_card = collapsible_card(
        state,
        "general.recovery",
        tr(lang, "settings-section-recovery"),
        recovery_col,
    );

    column![appearance_card, window_session_card, recovery_card]
        .spacing(8)
        .into()
}

pub(super) fn build_settings_plugins_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let ai = &state.snapshot.meta.ai_settings;
    let dlg_rect = state
        .dialog_manager
        .state(&DesktopDialogId::Settings)
        .cloned()
        .unwrap_or_else(|| default_window_state_for(DesktopDialogId::Settings))
        .resolved_rect(state.window_size);
    let list_pane_h = (dlg_rect.height - 290.0).clamp(160.0, 260.0);

    let ai_card = collapsible_card(
        state,
        "plugins.ai",
        tr(lang, "settings-section-plugins-ai"),
        column![
            setting_row(
                tr(lang, "settings-plugins-ai-device"),
                settings_dropdown(
                    vec![
                        (AiDevice::Cpu, tr(lang, "ai-device-cpu")),
                        (AiDevice::Auto, tr(lang, "ai-device-auto")),
                        (AiDevice::Gpu, tr(lang, "ai-device-gpu")),
                    ],
                    ai.device,
                    |device| {
                        Message::Dispatch(Command::AiSetSettings {
                            device,
                            auto_unload_secs: ai.auto_unload_secs,
                        })
                    },
                ),
            ),
            slider_stepper_control(
                tr(lang, "settings-plugins-auto-unload"),
                ai.auto_unload_secs as f32,
                10.0..=600.0,
                10.0_f32,
                format!("{}s", ai.auto_unload_secs),
                move |v| {
                    Message::Dispatch(Command::AiSetSettings {
                        device: ai.device,
                        auto_unload_secs: v.round() as u32,
                    })
                },
            ),
        ]
        .spacing(8),
    );

    let installed_card = collapsible_card(
        state,
        "plugins.installed",
        tr(lang, "settings-section-installed-plugins"),
        build_plugins_manager_body(
            state,
            &state.plugins_search,
            state.plugins_selected_id.as_deref(),
            Message::PluginsSearchChanged,
            Message::PluginsSelect,
            list_pane_h,
        ),
    );

    column![ai_card, installed_card].spacing(8).into()
}

pub(super) fn build_settings_browser_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;

    let view_card = collapsible_card(
        state,
        "browser.view",
        tr(lang, "settings-section-browser-view"),
        column![
            setting_row(
                tr(lang, "action-view-mode"),
                settings_dropdown(
                    vec![
                        (BrowserViewMode::Grid, tr(lang, "action-view-grid")),
                        (BrowserViewMode::List, tr(lang, "action-view-list")),
                        (BrowserViewMode::Details, tr(lang, "action-view-details")),
                    ],
                    layout.browser_view_mode,
                    Message::SetBrowserViewMode,
                ),
            ),
            setting_row(
                tr(lang, "action-sort-by"),
                settings_dropdown(
                    vec![
                        (BrowserSortField::Name, tr(lang, "sort-name")),
                        (BrowserSortField::Modified, tr(lang, "sort-modified")),
                        (BrowserSortField::Size, tr(lang, "sort-size")),
                        (BrowserSortField::Kind, tr(lang, "sort-kind")),
                    ],
                    layout.browser_sort_field,
                    Message::SetBrowserSortField,
                ),
            ),
            setting_row(
                tr(lang, "action-sort-order"),
                settings_dropdown(
                    vec![
                        (photon_protocol::SortOrder::Asc, tr(lang, "sort-asc")),
                        (photon_protocol::SortOrder::Desc, tr(lang, "sort-desc")),
                    ],
                    layout.browser_sort_order,
                    Message::SetBrowserSortOrder,
                ),
            ),
            setting_row(
                tr(lang, "label-thumb-size"),
                settings_dropdown(
                    vec![
                        (BrowserThumbSize::S, "S".to_owned()),
                        (BrowserThumbSize::M, "M".to_owned()),
                        (BrowserThumbSize::L, "L".to_owned()),
                        (BrowserThumbSize::Xl, "XL".to_owned()),
                    ],
                    layout.browser_thumb_size,
                    Message::SetBrowserThumbSize,
                ),
            ),
            setting_row(
                tr(lang, "settings-browser-show-hidden"),
                bool_dropdown(lang, layout.browser_show_hidden, |want| {
                    if want != layout.browser_show_hidden {
                        Message::ToggleBrowserShowHidden
                    } else {
                        Message::Noop
                    }
                }),
            ),
        ]
        .spacing(8),
    );

    let quick_access_card = collapsible_card(
        state,
        "browser.quick_access",
        tr(lang, "panel-quick-access"),
        build_settings_quick_access_body(state),
    );

    column![view_card, quick_access_card].spacing(8).into()
}

pub(super) fn build_settings_viewer_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let vmode = meta.layout.viewer_view_mode;
    let sys_open_mode = meta.layout.system_open_window_mode;
    let aa_mode = meta.layout.canvas_anti_aliasing;
    let denoise_mode = meta.layout.viewport_denoise;
    let active_backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let renderer_is_gpu = active_backend == "wgpu";
    let renderer_restart_needed = !meta.safe_mode && meta.hardware_acceleration != renderer_is_gpu;

    let mut display_items: Vec<Element<'_, Message>> = vec![
        setting_row(
            tr(lang, "settings-default-viewer-mode"),
            settings_dropdown(
                vec![
                    (ViewerViewMode::Single, tr(lang, "action-viewer-single")),
                    (ViewerViewMode::Double, tr(lang, "action-viewer-double")),
                    (ViewerViewMode::Webtoon, tr(lang, "action-viewer-manhwa")),
                ],
                vmode,
                Message::SetDefaultViewerViewMode,
            ),
        ),
        setting_row(
            tr(lang, "settings-viewer-instance-mode"),
            settings_dropdown(
                vec![
                    (false, tr(lang, "viewer-instance-multiple")),
                    (true, tr(lang, "viewer-instance-single")),
                ],
                meta.viewer_single_instance,
                |single_instance| {
                    Message::Dispatch(Command::SetViewerSingleInstance { single_instance })
                },
            ),
        ),
        setting_row(
            tr(lang, "settings-system-open"),
            settings_dropdown(
                system_open_mode_options(lang),
                sys_open_mode,
                Message::SetSystemOpenWindowMode,
            ),
        ),
    ];
    if sys_open_mode == SystemOpenWindowMode::FixedSize {
        let fixed_w = meta.layout.viewer_fixed_width;
        let fixed_h = meta.layout.viewer_fixed_height;
        let size_inputs = row![
            iced::widget::text_input("1240", &fixed_w.to_string())
                .on_input(move |s| {
                    let parsed = s.trim().parse::<u32>().unwrap_or(fixed_w);
                    Message::SetViewerFixedWidth(parsed)
                })
                .width(74)
                .padding([5, 8]),
            text("×").size(13),
            iced::widget::text_input("800", &fixed_h.to_string())
                .on_input(move |s| {
                    let parsed = s.trim().parse::<u32>().unwrap_or(fixed_h);
                    Message::SetViewerFixedHeight(parsed)
                })
                .width(74)
                .padding([5, 8]),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        display_items.push(setting_row(
            tr(lang, "settings-viewer-fixed-size"),
            size_inputs,
        ));
    }
    display_items.extend([
        setting_row(
            tr(lang, "settings-canvas-aa"),
            settings_dropdown(
                vec![
                    (CanvasAntiAliasingMode::Crisp, tr(lang, "aa-mode-crisp")),
                    (CanvasAntiAliasingMode::Smooth, tr(lang, "aa-mode-smooth")),
                    (
                        CanvasAntiAliasingMode::HighQuality,
                        tr(lang, "aa-mode-high-quality"),
                    ),
                ],
                aa_mode,
                Message::SetAntiAliasingMode,
            ),
        ),
        setting_row(
            tr(lang, "settings-viewport-denoise"),
            settings_dropdown(
                vec![
                    (ViewportDenoiseMode::Off, tr(lang, "denoise-mode-off")),
                    (ViewportDenoiseMode::Light, tr(lang, "denoise-mode-light")),
                    (ViewportDenoiseMode::Medium, tr(lang, "denoise-mode-medium")),
                    (ViewportDenoiseMode::Strong, tr(lang, "denoise-mode-strong")),
                ],
                denoise_mode,
                Message::SetViewportDenoiseMode,
            ),
        ),
    ]);

    let display_card = collapsible_card(
        state,
        "viewer.display",
        tr(lang, "settings-section-viewer"),
        column(display_items).spacing(8),
    );

    let mut rendering_col = column![
        setting_row(
            tr(lang, "settings-hw-accel"),
            bool_dropdown(lang, meta.hardware_acceleration, |hardware_acceleration| {
                Message::Dispatch(Command::SetSettings {
                    theme: meta.theme,
                    language: meta.language,
                    remember_last_folder: meta.remember_last_folder,
                    hardware_acceleration,
                    save_ui_state_on_exit: meta.save_ui_state_on_exit,
                })
            }),
        ),
        setting_row(
            tr(lang, "settings-active-renderer"),
            text(tr(
                lang,
                if renderer_is_gpu {
                    "renderer-gpu-wgpu"
                } else {
                    "renderer-software-tiny-skia"
                },
            ))
            .size(TEXT_SIZE_CONTROL)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_secondary),
            }),
        ),
    ]
    .spacing(8);
    if renderer_restart_needed {
        rendering_col = rendering_col.push(
            text(tr(lang, "settings-restart-required-renderer"))
                .size(11)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).accent),
                }),
        );
    }
    let rendering_card = collapsible_card(
        state,
        "viewer.rendering",
        tr(lang, "settings-section-rendering"),
        rendering_col,
    );

    let translate_card = collapsible_card(
        state,
        "viewer.translate",
        tr(lang, "settings-section-translate"),
        build_settings_translate_fields(state),
    );

    column![display_card, rendering_card, translate_card]
        .spacing(8)
        .into()
}

pub(super) fn build_settings_editor_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;

    let files_card = collapsible_card(
        state,
        "editor.files",
        tr(lang, "settings-section-editor-files"),
        column![setting_row(
            tr(lang, "settings-backup-method"),
            settings_dropdown(
                vec![
                    (
                        photon_protocol::BackupMethod::Tilde,
                        tr(lang, "backup-method-tilde"),
                    ),
                    (
                        photon_protocol::BackupMethod::DotBackup,
                        tr(lang, "backup-method-dot"),
                    ),
                    (
                        photon_protocol::BackupMethod::Folder,
                        tr(lang, "backup-method-folder"),
                    ),
                ],
                meta.backup_method,
                |method| Message::Dispatch(Command::SetBackupMethod { method }),
            ),
        )]
        .spacing(8),
    );

    let canvas_card = collapsible_card(
        state,
        "editor.canvas",
        tr(lang, "settings-section-rendering"),
        column![setting_row(
            tr(lang, "settings-canvas-aa"),
            settings_dropdown(
                vec![
                    (CanvasAntiAliasingMode::Crisp, tr(lang, "aa-mode-crisp")),
                    (CanvasAntiAliasingMode::Smooth, tr(lang, "aa-mode-smooth")),
                    (
                        CanvasAntiAliasingMode::HighQuality,
                        tr(lang, "aa-mode-high-quality"),
                    ),
                ],
                meta.layout.canvas_anti_aliasing,
                Message::SetAntiAliasingMode,
            ),
        )]
        .spacing(8),
    );

    let subdivisions = state.editor_ui.grid_subdivisions;
    let opacity = state.editor_ui.grid_opacity;
    let grid_card = collapsible_card(
        state,
        "editor.grid",
        tr(lang, "settings-section-grid"),
        column![
            setting_row(
                tr(lang, "grid-spacing-px"),
                settings_dropdown(
                    vec![
                        (16u32, "16 px".to_string()),
                        (32u32, "32 px".to_string()),
                        (64u32, "64 px".to_string()),
                        (128u32, "128 px".to_string()),
                        (256u32, "256 px".to_string()),
                    ],
                    state.editor_ui.grid_spacing_px.round() as u32,
                    move |px| {
                        Message::Editor(EditorMessage::SetGridSettings {
                            spacing_px: px as f32,
                            subdivisions,
                            opacity,
                        })
                    },
                ),
            ),
            setting_row(
                tr(lang, "grid-subdivisions"),
                settings_dropdown(
                    vec![
                        (1u32, "1".to_string()),
                        (2u32, "2".to_string()),
                        (4u32, "4".to_string()),
                        (8u32, "8".to_string()),
                    ],
                    subdivisions,
                    move |divs| {
                        Message::Editor(EditorMessage::SetGridSettings {
                            spacing_px: state.editor_ui.grid_spacing_px,
                            subdivisions: divs,
                            opacity,
                        })
                    },
                ),
            ),
            setting_row(
                tr(lang, "settings-grid-customize"),
                button::labeled(
                    tr(lang, "menu-view-grid-settings"),
                    Some(Icon::Settings),
                    ButtonVariant::Secondary,
                    Some(Message::OpenGridSettingsDialog),
                ),
            ),
        ]
        .spacing(8),
    );

    column![files_card, canvas_card, grid_card]
        .spacing(8)
        .into()
}

fn build_settings_quick_access_body(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let current_folder = meta.current_folder.as_deref();
    let can_pin_current =
        current_folder.is_some_and(|cur| !meta.favorites.iter().any(|fav| fav == cur));
    let mut fav_list_col = column![].spacing(6);
    if meta.favorites.is_empty() {
        fav_list_col = fav_list_col.push(
            text(tr(lang, "settings-quick-access-empty"))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
        );
    } else {
        let fav_len = meta.favorites.len();
        for (idx, fav_path) in meta.favorites.iter().enumerate() {
            let folder_name = Path::new(fav_path)
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(fav_path.as_str())
                .to_owned();
            fav_list_col = fav_list_col.push(
                row![
                    with_tooltip_pos(
                        container(text(folder_name).size(TEXT_SIZE_CONTROL)).width(Length::Fill),
                        fav_path.clone(),
                        TooltipPosition::Top,
                    ),
                    button::compact_icon_tipped(
                        Icon::ArrowUp,
                        ButtonVariant::Ghost,
                        (idx > 0).then(|| {
                            Message::Dispatch(Command::MoveFavorite {
                                path: fav_path.clone(),
                                direction: MoveDirection::Up,
                            })
                        }),
                        tr(lang, "action-move-up"),
                    ),
                    button::compact_icon_tipped(
                        Icon::ArrowDown,
                        ButtonVariant::Ghost,
                        (idx + 1 < fav_len).then(|| {
                            Message::Dispatch(Command::MoveFavorite {
                                path: fav_path.clone(),
                                direction: MoveDirection::Down,
                            })
                        }),
                        tr(lang, "action-move-down"),
                    ),
                    button::compact_icon_tipped(
                        Icon::Trash,
                        ButtonVariant::Ghost,
                        Some(Message::Dispatch(Command::RemoveFavorite {
                            path: fav_path.clone(),
                        })),
                        tr(lang, "action-unpin-quick-access"),
                    ),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
    }
    fav_list_col = fav_list_col.push(
        row![
            button::labeled(
                tr(lang, "settings-quick-access-add"),
                Some(Icon::Plus),
                ButtonVariant::Secondary,
                Some(Message::PickFavoriteFolderDialog),
            ),
            button::labeled(
                tr(lang, "action-pin-quick-access"),
                Some(Icon::Star),
                ButtonVariant::Ghost,
                can_pin_current.then(|| Message::Dispatch(Command::AddFavorite {
                    path: current_folder.unwrap_or_default().to_owned(),
                })),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    fav_list_col.into()
}

fn build_settings_translate_fields(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let draft = &state.translate_draft;
    let api_key_val = draft.api_key.as_deref().unwrap_or("");

    column![
        setting_row(
            tr(lang, "settings-translate-provider"),
            settings_dropdown(
                vec![
                    (TranslateProvider::None, tr(lang, "translate-provider-none")),
                    (
                        TranslateProvider::GoogleWeb,
                        tr(lang, "translate-provider-google"),
                    ),
                    (
                        TranslateProvider::LibreTranslate,
                        tr(lang, "translate-provider-libre"),
                    ),
                ],
                draft.provider,
                Message::SetTranslateProvider,
            ),
        ),
        setting_row(
            tr(lang, "settings-translate-libre-url"),
            text_input("http://localhost:5000", &draft.libre_endpoint)
                .on_input(Message::TranslateEndpointChanged)
                .on_submit(Message::SaveTranslateSettings)
                .size(TEXT_SIZE_CONTROL)
                .padding([6, 8])
                .style(input_style)
                .width(Length::Fixed(180.0)),
        ),
        setting_row(
            tr(lang, "settings-translate-api-key"),
            text_input(
                &tr(lang, "settings-translate-api-key-placeholder"),
                api_key_val,
            )
            .on_input(Message::TranslateApiKeyChanged)
            .on_submit(Message::SaveTranslateSettings)
            .size(TEXT_SIZE_CONTROL)
            .padding([6, 8])
            .style(input_style)
            .width(Length::Fixed(180.0)),
        ),
        setting_row(
            tr(lang, "settings-translate-source-lang"),
            text_input("eng", &draft.default_source_lang)
                .on_input(Message::TranslateSourceLangChanged)
                .on_submit(Message::SaveTranslateSettings)
                .size(TEXT_SIZE_CONTROL)
                .padding([6, 8])
                .style(input_style)
                .width(Length::Fixed(140.0)),
        ),
        setting_row(
            tr(lang, "settings-translate-target-lang"),
            text_input("vi", &draft.default_target_lang)
                .on_input(Message::TranslateTargetLangChanged)
                .on_submit(Message::SaveTranslateSettings)
                .size(TEXT_SIZE_CONTROL)
                .padding([6, 8])
                .style(input_style)
                .width(Length::Fixed(140.0)),
        ),
        row![
            iced::widget::space::horizontal(),
            button::labeled(
                tr(lang, "settings-translate-save"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                Some(Message::SaveTranslateSettings),
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(8)
    .into()
}

pub(super) fn build_settings_translate_tab(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    section_card(
        tr(lang, "settings-section-translate"),
        build_settings_translate_fields(state),
    )
}
