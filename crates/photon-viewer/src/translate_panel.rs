use iced::widget::overlay::menu as iced_menu;
use iced::widget::{
    button as iced_button, column, container, pick_list, progress_bar, row, rule, scrollable,
    text_input,
};
use iced::{Alignment, Border, Color, Element, Fill, Length, Padding, Shadow};
use photon_i18n::tr;
use photon_protocol::{Language, TranslateProvider};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::icons::Icon;
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, card_style,
    hairline_rule_style, input_style,
};
use photon_ui::typography::text;

use crate::types::{ViewerMessage, ViewerOcrEngine, ViewerTranslateMode, ViewerViewState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TranslateProviderOption(TranslateProvider);

impl TranslateProviderOption {
    const ALL: [Self; 3] = [
        Self(TranslateProvider::None),
        Self(TranslateProvider::GoogleWeb),
        Self(TranslateProvider::LibreTranslate),
    ];
}

impl std::fmt::Display for TranslateProviderOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            TranslateProvider::None => f.write_str("Off (OCR only)"),
            TranslateProvider::GoogleWeb => f.write_str("Google Web"),
            TranslateProvider::LibreTranslate => f.write_str("LibreTranslate"),
        }
    }
}

const COMMON_OCR_LANGS: [&str; 8] = [
    "eng", "jpn", "jpn_vert", "vie", "chi_sim", "chi_tra", "kor", "fra",
];
const COMMON_TARGET_LANGS: [&str; 8] = ["vi", "en", "ja", "ko", "zh-CN", "zh-TW", "fr", "de"];

fn styled_pick_list<'a, T>(
    options: &'a [T],
    selected: Option<T>,
    on_selected: impl Fn(T) -> ViewerMessage + 'a,
) -> pick_list::PickList<'a, T, &'a [T], T, ViewerMessage>
where
    T: ToString + PartialEq + Clone + 'a,
{
    pick_list(options, selected, on_selected)
        .text_size(TEXT_SIZE_CONTROL)
        .padding([5, 8])
        .style(|theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let active = matches!(
                status,
                pick_list::Status::Hovered | pick_list::Status::Opened { .. }
            );
            pick_list::Style {
                text_color: t.text_primary,
                placeholder_color: t.text_muted,
                handle_color: t.text_secondary,
                background: if active { t.bg_hover } else { t.bg_surface }.into(),
                border: Border {
                    color: if active { t.accent } else { t.border_strong },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
            }
        })
        .menu_style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            iced_menu::Style {
                background: t.bg_elevated.into(),
                border: Border {
                    color: t.border_strong,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                text_color: t.text_primary,
                selected_text_color: Color::WHITE,
                selected_background: t.accent.into(),
                shadow: Shadow::default(),
            }
        })
}

fn lang_combo_field<'a>(
    placeholder: &str,
    value: &str,
    presets: &'static [&'static str],
    enabled: bool,
    on_input: impl Fn(String) -> ViewerMessage + Clone + 'a,
) -> Element<'a, ViewerMessage> {
    if !enabled {
        return text_input(placeholder, value)
            .size(TEXT_SIZE_CAPTION)
            .padding([5, 8])
            .width(Fill)
            .style(|theme, _status| {
                let pal = PaletteTokens::for_theme(theme);
                iced::widget::text_input::Style {
                    background: pal.bg_app.into(),
                    border: Border {
                        color: pal.border,
                        width: 1.0,
                        radius: RADIUS_SM.into(),
                    },
                    icon: pal.text_muted,
                    placeholder: pal.text_muted,
                    value: pal.text_muted,
                    selection: pal.bg_selected,
                }
            })
            .into();
    }

    let on_pick = on_input.clone();
    let input = text_input(placeholder, value)
        .on_input(on_input)
        .size(TEXT_SIZE_CAPTION)
        .padding([5, 8])
        .style(input_style)
        .width(Fill);

    let quick_pick = styled_pick_list(presets, None::<&'static str>, move |chosen| {
        on_pick(chosen.to_owned())
    })
    .placeholder("")
    .width(Length::Fixed(28.0))
    .padding([5, 8]);

    row![input, quick_pick]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Fill)
        .into()
}

fn translate_option_label<'a>(label: String) -> Element<'a, ViewerMessage> {
    text(label)
        .size(TEXT_SIZE_CAPTION)
        .style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).text_secondary),
        })
        .into()
}

pub(crate) fn build_viewer_translate_panel_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let tr_state = state.translate;
    let has_path = state.snapshot.meta.viewer_path.is_some();
    let has_blocks = !tr_state.blocks.is_empty();
    let running_stage = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .filter(|t| t.running)
        .map(|t| (t.percent, t.stage.clone()));

    let is_ocr_only = tr_state.mode == ViewerTranslateMode::OcrOnly;
    let is_pan_mode = tr_state.mode == ViewerTranslateMode::Pan;

    // 1. Mode row: Translate / OCR
    let mode_row = container(
        row![
            container(
                button::labeled(
                    tr(lang, "translate-mode-translate"),
                    None,
                    ButtonVariant::Tab {
                        active: !is_ocr_only,
                    },
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::Translate,
                    )),
                )
                .width(Fill),
            )
            .width(Length::FillPortion(1)),
            container(
                button::labeled(
                    tr(lang, "translate-mode-ocr"),
                    None,
                    ButtonVariant::Tab {
                        active: is_ocr_only,
                    },
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::OcrOnly,
                    )),
                )
                .width(Fill),
            )
            .width(Length::FillPortion(1)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding(3)
    .width(Fill)
    .style(|theme| {
        let pal = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(pal.bg_app.into()),
            border: Border {
                color: pal.border,
                width: 1.0,
                radius: RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    // 2. OCR Model dropdown + Translate Provider dropdown (dimmed when OCR Only)
    let current_provider = TranslateProviderOption(state.snapshot.meta.translate_settings.provider);
    let provider_picker: Element<'a, ViewerMessage> = if is_ocr_only {
        container(
            row![
                text(current_provider.to_string())
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    })
                    .width(Fill),
                text("▾")
                    .size(TEXT_SIZE_CAPTION)
                    .style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    }),
            ]
            .align_y(Alignment::Center),
        )
        .padding([5, 8])
        .width(Fill)
        .style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_app.into()),
                border: Border {
                    color: pal.border,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        })
        .into()
    } else {
        styled_pick_list(
            &TranslateProviderOption::ALL[..],
            Some(current_provider),
            |opt| ViewerMessage::SetTranslateProvider(opt.0),
        )
        .width(Fill)
        .into()
    };

    let engine_picker = styled_pick_list(
        &ViewerOcrEngine::ALL[..],
        Some(tr_state.ocr_engine),
        ViewerMessage::SetTranslateOcrEngine,
    )
    .width(Fill);

    let model_provider_row = row![
        column![
            translate_option_label(tr(lang, "translate-engine-label")),
            engine_picker,
        ]
        .spacing(3)
        .width(Length::FillPortion(1)),
        column![
            translate_option_label(tr(lang, "translate-provider-short")),
            provider_picker,
        ]
        .spacing(3)
        .width(Length::FillPortion(1)),
    ]
    .spacing(6)
    .align_y(Alignment::End)
    .width(Fill);

    // 3. Languages row (combo-style dropdown + free input, smartly dimmed)
    let src_enabled = match (is_ocr_only, tr_state.ocr_engine) {
        (_, ViewerOcrEngine::MangaOcr) => false,
        (true, ViewerOcrEngine::RapidOcr) => false,
        _ => true,
    };
    let dst_enabled = !is_ocr_only;
    let src_display = if tr_state.ocr_engine == ViewerOcrEngine::MangaOcr {
        "jpn"
    } else {
        tr_state.source_lang.as_str()
    };

    let lang_row = row![
        column![
            translate_option_label(tr(lang, "translate-source-short")),
            lang_combo_field(
                "eng",
                src_display,
                &COMMON_OCR_LANGS,
                src_enabled,
                ViewerMessage::SetTranslateSourceLang,
            ),
        ]
        .spacing(3)
        .width(Length::FillPortion(1)),
        button::compact_icon(
            Icon::Swap,
            ButtonVariant::Ghost,
            (src_enabled && dst_enabled).then_some(ViewerMessage::SwapTranslateLangs),
        ),
        column![
            translate_option_label(tr(lang, "translate-target-short")),
            lang_combo_field(
                "vi",
                &tr_state.target_lang,
                &COMMON_TARGET_LANGS,
                dst_enabled,
                ViewerMessage::SetTranslateTargetLang,
            ),
        ]
        .spacing(3)
        .width(Length::FillPortion(1)),
    ]
    .spacing(4)
    .align_y(Alignment::End)
    .width(Fill);

    // 4. Icon tool strip: Scan Area | Full Page | Hand | Bubbles | Clear | Save | Reload | Settings
    let tools_row = container(
        row![
            button::compact_icon_tipped(
                Icon::Crop,
                ButtonVariant::Tab {
                    active: !is_pan_mode,
                },
                Some(ViewerMessage::StartTranslateRegionPick),
                tr(lang, "translate-drag-region"),
            ),
            button::compact_icon_tipped(
                Icon::Fit,
                ButtonVariant::Ghost,
                has_path.then_some(ViewerMessage::ScanTranslatePage),
                tr(lang, "translate-select-full"),
            ),
            button::compact_icon_tipped(
                Icon::Hand,
                ButtonVariant::Tab {
                    active: is_pan_mode,
                },
                Some(ViewerMessage::SetTranslateMode(ViewerTranslateMode::Pan)),
                tr(lang, "translate-mode-pan"),
            ),
            button::compact_icon_tipped(
                if tr_state.show_bubbles {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                },
                ButtonVariant::Tab {
                    active: tr_state.show_bubbles,
                },
                Some(ViewerMessage::ToggleTranslateBubbles),
                tr(lang, "translate-show-bubbles"),
            ),
            iced::widget::space::horizontal(),
            button::compact_icon_tipped(
                Icon::Trash,
                ButtonVariant::Ghost,
                has_blocks.then_some(ViewerMessage::ClearTranslateBlocks),
                tr(lang, "action-clear-translation"),
            ),
            button::compact_icon_tipped(
                Icon::Save,
                if tr_state.saved_sidecar {
                    ButtonVariant::Tab { active: true }
                } else {
                    ButtonVariant::Ghost
                },
                (has_path && has_blocks).then_some(ViewerMessage::SaveTranslationSidecar),
                tr(lang, "translate-save-sidecar"),
            ),
            button::compact_icon_tipped(
                Icon::Refresh,
                ButtonVariant::Ghost,
                has_path.then_some(ViewerMessage::ScanTranslatePage),
                tr(lang, "translate-scan-page"),
            ),
            button::compact_icon_tipped(
                Icon::Settings,
                ButtonVariant::Ghost,
                Some(ViewerMessage::OpenTranslateSettings),
                tr(lang, "action-translate-settings"),
            ),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .width(Fill),
    )
    .padding([4, 6])
    .width(Fill)
    .style(card_style);

    // 5. Process / Progress bar
    let (pct_val, status_label) = if let Some((pct, stage)) = running_stage {
        (pct as f32, format!("{pct}% · {stage}"))
    } else if has_blocks {
        (
            100.0,
            format!(
                "{} ({})",
                tr(lang, "translate-result-title"),
                tr_state.blocks.len()
            ),
        )
    } else {
        (0.0, tr(lang, "translate-drag-region"))
    };
    let process_bar = column![
        progress_bar(0.0..=100.0, pct_val)
            .length(Fill)
            .girth(Length::Fixed(5.0))
            .style(|theme| {
                let pal = PaletteTokens::for_theme(theme);
                iced::widget::progress_bar::Style {
                    background: pal.bg_app.into(),
                    bar: pal.accent.into(),
                    border: Border {
                        color: pal.border,
                        width: 0.5,
                        radius: RADIUS_SM.into(),
                    },
                }
            }),
        text(status_label)
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
    ]
    .spacing(4)
    .width(Fill);

    let controls_tile = container(
        column![
            mode_row,
            model_provider_row,
            lang_row,
            tools_row,
            process_bar
        ]
        .spacing(7)
        .width(Fill),
    )
    .padding(10)
    .width(Fill)
    .style(card_style);

    // 6. Result tile (2 separate frames: Original + Translated)
    let sel_idx = tr_state
        .editing_block_idx
        .filter(|&i| i < tr_state.blocks.len())
        .or_else(|| has_blocks.then_some(0));

    let result_title = match sel_idx {
        Some(i) => format!(
            "{} · #{} / {}",
            tr(lang, "translate-result-title"),
            i + 1,
            tr_state.blocks.len()
        ),
        None => tr(lang, "translate-result-title"),
    };

    let mut result_header = row![
        text(result_title).size(12).style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).text_muted),
        }),
        iced::widget::space::horizontal(),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .width(Fill);

    if let Some(idx) = sel_idx {
        result_header = result_header
            .push(button::compact_icon_tipped(
                Icon::Refresh,
                ButtonVariant::Ghost,
                Some(ViewerMessage::RetryTranslateBlock(idx)),
                tr(lang, "translate-retry-bubble"),
            ))
            .push(button::compact_icon_tipped(
                Icon::Trash,
                ButtonVariant::Ghost,
                Some(ViewerMessage::RemoveTranslateBlock(idx)),
                tr(lang, "translate-remove-bubble"),
            ))
            .push(button::compact_icon_tipped(
                Icon::Copy,
                ButtonVariant::Ghost,
                Some(ViewerMessage::CopyTranslatedText),
                tr(lang, "action-copy"),
            ))
            .push(button::labeled(
                tr(lang, "tooltip-viewer-next"),
                None,
                ButtonVariant::Secondary,
                Some(ViewerMessage::NextTranslateBubble),
            ));
    }

    let result_body: Element<'a, ViewerMessage> = if let Some(idx) = sel_idx {
        let b = &tr_state.blocks[idx];
        let orig_box = column![
            text(tr(lang, "translate-bubble-orig"))
                .size(10)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            text_input("…", &b.original)
                .on_input(move |s| ViewerMessage::UpdateTranslateBlockOriginal(idx, s))
                .size(TEXT_SIZE_CAPTION)
                .padding([5, 8])
                .style(input_style)
                .width(Fill),
        ]
        .spacing(3)
        .width(Fill);

        let trans_box = column![
            text(tr(lang, "translate-bubble-trans"))
                .size(10)
                .style(|theme| text::Style {
                    color: Some(PaletteTokens::for_theme(theme).text_muted),
                }),
            if is_ocr_only {
                text_input("—", &b.translated)
                    .size(TEXT_SIZE_CAPTION)
                    .padding([5, 8])
                    .width(Fill)
                    .style(|theme, _status| {
                        let pal = PaletteTokens::for_theme(theme);
                        iced::widget::text_input::Style {
                            background: pal.bg_app.into(),
                            border: Border {
                                color: pal.border,
                                width: 1.0,
                                radius: RADIUS_SM.into(),
                            },
                            icon: pal.text_muted,
                            placeholder: pal.text_muted,
                            value: pal.text_muted,
                            selection: pal.bg_selected,
                        }
                    })
            } else {
                text_input(&tr(lang, "translate-text-placeholder"), &b.translated)
                    .on_input(move |s| ViewerMessage::UpdateTranslateBlockText(idx, s))
                    .size(TEXT_SIZE_CAPTION)
                    .padding([5, 8])
                    .style(input_style)
                    .width(Fill)
            },
        ]
        .spacing(3)
        .width(Fill);

        column![orig_box, trans_box].spacing(6).width(Fill).into()
    } else {
        text(tr(lang, "translate-empty-hint"))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            })
            .into()
    };

    let result_tile = container(
        column![
            result_header,
            rule::horizontal(1).style(hairline_rule_style),
            result_body,
        ]
        .spacing(6)
        .width(Fill),
    )
    .padding(10)
    .width(Fill)
    .style(card_style);

    // 7. History tile (separated scrollable list of blocks)
    let history_title = if has_blocks {
        format!(
            "{} ({})",
            tr(lang, "translate-history-title"),
            tr_state.blocks.len()
        )
    } else {
        tr(lang, "translate-history-title")
    };

    let history_body: Element<'a, ViewerMessage> = if tr_state.blocks.is_empty() {
        text("—")
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            })
            .into()
    } else {
        let mut list_col = column![].spacing(4).width(Fill);
        for (idx, b) in tr_state.blocks.iter().enumerate() {
            let is_sel = sel_idx == Some(idx);
            let display_text = if is_ocr_only || b.translated.trim().is_empty() {
                b.original.trim()
            } else {
                b.translated.trim()
            };
            let snippet = if display_text.is_empty() {
                "…".to_owned()
            } else {
                display_text.to_owned()
            };
            let item_btn = iced_button(
                row![
                    text(format!("#{}", idx + 1))
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| text::Style {
                            color: Some(PaletteTokens::for_theme(theme).accent),
                        }),
                    text(snippet).size(TEXT_SIZE_CAPTION),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .width(Fill),
            )
            .width(Fill)
            .padding([6, 8])
            .style(move |theme, status| {
                let pal = PaletteTokens::for_theme(theme);
                let bg = if is_sel {
                    pal.bg_selected
                } else if matches!(status, iced_button::Status::Hovered) {
                    pal.bg_hover
                } else {
                    pal.bg_app
                };
                iced_button::Style {
                    background: Some(bg.into()),
                    text_color: pal.text_primary,
                    border: Border {
                        color: if is_sel { pal.accent } else { pal.border },
                        width: 1.0,
                        radius: RADIUS_SM.into(),
                    },
                    ..Default::default()
                }
            })
            .on_press(ViewerMessage::SelectTranslateBlock(Some(idx)));
            list_col = list_col.push(item_btn);
        }
        scrollable(container(list_col).padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }))
        .height(Fill)
        .into()
    };

    let history_tile = container(
        column![
            text(history_title).size(12).style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
            rule::horizontal(1).style(hairline_rule_style),
            container(history_body).width(Fill).height(Fill),
        ]
        .spacing(6)
        .width(Fill)
        .height(Fill),
    )
    .padding(10)
    .width(Fill)
    .height(Fill)
    .style(card_style);

    container(
        column![controls_tile, result_tile, history_tile]
            .spacing(8)
            .width(Fill)
            .height(Fill),
    )
    .padding([6, 6])
    .width(Fill)
    .height(Fill)
    .into()
}
