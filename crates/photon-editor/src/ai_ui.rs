use crate::widgets::{build_ai_task_banner, build_apply_target_row};
use crate::*;
use iced::widget::{checkbox, column, row, rule, text_input};
use iced::{Alignment, Element, Fill};
use photon_i18n::tr;
use photon_panels::section_card;
use photon_protocol::{Command, Language};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::icons::Icon;
use photon_ui::theme::{PaletteTokens, TEXT_SIZE_CONTROL, hairline_rule_style, input_style};
use photon_ui::typography::text;

fn build_ai_model_quick_row<'a>(
    state: EditorViewState<'a>,
    lang: Language,
    model_id: &str,
) -> Element<'a, EditorMessage> {
    let status = state
        .snapshot
        .meta
        .ai_models
        .iter()
        .find(|m| m.id == model_id);
    let owner_plugin_id = state
        .snapshot
        .meta
        .plugins
        .iter()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
        .map(|p| p.id.clone());
    let installed = status.is_some_and(|m| m.installed);
    let running = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .is_some_and(|t| t.running);
    let badge = if installed {
        tr(lang, "model-status-installed")
    } else {
        tr(lang, "model-status-missing")
    };
    let mut r = row![
        text(badge).size(11).style(move |theme| text::Style {
            color: Some(if installed {
                PaletteTokens::for_theme(theme).success
            } else {
                PaletteTokens::for_theme(theme).warning
            }),
        }),
        iced::widget::space::horizontal(),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if status.is_some() && !installed {
        r = r.push(button::labeled(
            tr(lang, "action-download"),
            None,
            ButtonVariant::Secondary,
            (!running).then(|| {
                EditorMessage::Dispatch(Command::AiDownloadModel {
                    model_id: model_id.to_owned(),
                })
            }),
        ));
    }
    if let Some(plugin_id) = owner_plugin_id {
        r = r.push(button::compact_icon_tipped(
            Icon::Settings,
            ButtonVariant::Ghost,
            Some(EditorMessage::OpenPluginModels(plugin_id)),
            tr(lang, "action-plugin-settings-models"),
        ));
    }
    r.into()
}

pub(crate) fn build_ai_rembg_tool_options<'a>(
    state: EditorViewState<'a>,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = state.editor_ui;
    let rembg_models: Vec<String> = {
        let mut ids: Vec<String> = state
            .snapshot
            .meta
            .ai_models
            .iter()
            .filter(|m| {
                matches!(
                    m.id.as_str(),
                    "u2netp" | "silueta" | "rmbg-1.4" | "birefnet-general" | "birefnet-portrait"
                ) || m.tier == "fast"
                    || m.tier == "balanced"
                    || m.tier == "hq"
            })
            .map(|m| m.id.clone())
            .collect();
        if ids.is_empty() {
            ids = vec![
                "u2netp".to_owned(),
                "silueta".to_owned(),
                "rmbg-1.4".to_owned(),
                "birefnet-general".to_owned(),
                "birefnet-portrait".to_owned(),
            ];
        }
        ids
    };

    let mut model_col = column![].spacing(4);
    for id in rembg_models {
        let active = ui.ai_rembg_model == id;
        model_col = model_col.push(
            button::labeled(
                id.clone(),
                None,
                ButtonVariant::Tab { active },
                Some(EditorMessage::AiSetRembgModel(id)),
            )
            .width(Fill),
        );
    }

    let running = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .is_some_and(|t| t.running);

    let mut col = column![
        text(tr(lang, "label-ai-model")).size(11),
        model_col,
        build_ai_model_quick_row(state, lang, &ui.ai_rembg_model),
        slider_stepper_control(
            tr(lang, "label-ai-edge-feather"),
            f32::from(ui.ai_rembg_feather),
            0.0..=20.0,
            1.0_f32,
            format!("{} px", ui.ai_rembg_feather),
            |v| EditorMessage::AiSetRembgFeather(v.round() as u8),
        ),
        build_apply_target_row(ui.apply_target, lang),
        button::labeled(
            tr(lang, "action-remove-background"),
            Some(Icon::Mask),
            ButtonVariant::Primary,
            (!running).then_some(EditorMessage::AiRunRemoveBackground),
        )
        .width(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        text(tr(lang, "label-refine-alpha-brush")).size(11),
        row![
            button::labeled(
                tr(lang, "refine-mode-off"),
                None,
                ButtonVariant::Tab {
                    active: ui.ai_rembg_refine == AlphaRefineMode::Off,
                },
                Some(EditorMessage::AiSetRembgRefine(AlphaRefineMode::Off)),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "refine-mode-erase-bg"),
                None,
                ButtonVariant::Tab {
                    active: ui.ai_rembg_refine == AlphaRefineMode::EraseBg,
                },
                Some(EditorMessage::AiSetRembgRefine(AlphaRefineMode::EraseBg)),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "refine-mode-restore-fg"),
                None,
                ButtonVariant::Tab {
                    active: ui.ai_rembg_refine == AlphaRefineMode::RestoreFg,
                },
                Some(EditorMessage::AiSetRembgRefine(AlphaRefineMode::RestoreFg)),
            )
            .width(Fill),
        ]
        .spacing(3),
        slider_stepper_control(
            tr(lang, "label-brush-size"),
            ui.ai_rembg_refine_radius,
            2.0..=150.0,
            1.0_f32,
            format!("{:.0} px", ui.ai_rembg_refine_radius),
            EditorMessage::AiSetRembgRefineRadius,
        ),
        slider_stepper_control(
            tr(lang, "label-brush-softness"),
            ui.ai_rembg_refine_softness,
            0.0..=1.0,
            0.05_f32,
            format!("{:.0}%", ui.ai_rembg_refine_softness * 100.0),
            EditorMessage::AiSetRembgRefineSoftness,
        ),
    ]
    .spacing(6);

    if let Some(banner) = build_ai_task_banner(state, lang) {
        col = col.push(banner);
    }

    section_card(tr(lang, "tool-ai-rembg"), col)
}

pub(crate) fn build_ai_watermark_tool_options<'a>(
    state: EditorViewState<'a>,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = state.editor_ui;
    let running = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .is_some_and(|t| t.running);

    let tabs = row![
        button::labeled(
            tr(lang, "watermark-tab-ai-auto"),
            None,
            ButtonVariant::Tab {
                active: ui.ai_wm_tab == WatermarkTab::AiAuto,
            },
            Some(EditorMessage::AiSetWatermarkTab(WatermarkTab::AiAuto)),
        )
        .width(Fill),
        button::labeled(
            tr(lang, "watermark-tab-gemini"),
            None,
            ButtonVariant::Tab {
                active: ui.ai_wm_tab == WatermarkTab::Gemini,
            },
            Some(EditorMessage::AiSetWatermarkTab(WatermarkTab::Gemini)),
        )
        .width(Fill),
        button::labeled(
            tr(lang, "watermark-tab-region"),
            None,
            ButtonVariant::Tab {
                active: ui.ai_wm_tab == WatermarkTab::Region,
            },
            Some(EditorMessage::AiSetWatermarkTab(WatermarkTab::Region)),
        )
        .width(Fill),
    ]
    .spacing(3);

    let mut col = column![tabs, build_apply_target_row(ui.apply_target, lang)].spacing(6);

    let region_controls = || {
        column![
            text(tr(lang, "label-corner-presets")).size(11),
            row![
                button::labeled(
                    tr(lang, "corner-bottom-right"),
                    None,
                    ButtonVariant::Ghost,
                    Some(EditorMessage::AiSetWmRegion(78.0, 84.0, 20.0, 14.0)),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "corner-bottom-left"),
                    None,
                    ButtonVariant::Ghost,
                    Some(EditorMessage::AiSetWmRegion(2.0, 84.0, 20.0, 14.0)),
                )
                .width(Fill),
            ]
            .spacing(3),
            row![
                button::labeled(
                    tr(lang, "corner-top-right"),
                    None,
                    ButtonVariant::Ghost,
                    Some(EditorMessage::AiSetWmRegion(78.0, 2.0, 20.0, 14.0)),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "corner-top-left"),
                    None,
                    ButtonVariant::Ghost,
                    Some(EditorMessage::AiSetWmRegion(2.0, 2.0, 20.0, 14.0)),
                )
                .width(Fill),
            ]
            .spacing(3),
            slider_stepper_control(
                tr(lang, "label-region-x"),
                ui.ai_wm_region_x,
                0.0..=95.0,
                1.0_f32,
                format!("{:.0}%", ui.ai_wm_region_x),
                |v| {
                    EditorMessage::AiSetWmRegion(
                        v,
                        ui.ai_wm_region_y,
                        ui.ai_wm_region_w,
                        ui.ai_wm_region_h,
                    )
                },
            ),
            slider_stepper_control(
                tr(lang, "label-region-y"),
                ui.ai_wm_region_y,
                0.0..=95.0,
                1.0_f32,
                format!("{:.0}%", ui.ai_wm_region_y),
                |v| {
                    EditorMessage::AiSetWmRegion(
                        ui.ai_wm_region_x,
                        v,
                        ui.ai_wm_region_w,
                        ui.ai_wm_region_h,
                    )
                },
            ),
            slider_stepper_control(
                tr(lang, "label-region-w"),
                ui.ai_wm_region_w,
                1.0..=100.0,
                1.0_f32,
                format!("{:.0}%", ui.ai_wm_region_w),
                |v| {
                    EditorMessage::AiSetWmRegion(
                        ui.ai_wm_region_x,
                        ui.ai_wm_region_y,
                        v,
                        ui.ai_wm_region_h,
                    )
                },
            ),
            slider_stepper_control(
                tr(lang, "label-region-h"),
                ui.ai_wm_region_h,
                1.0..=100.0,
                1.0_f32,
                format!("{:.0}%", ui.ai_wm_region_h),
                |v| {
                    EditorMessage::AiSetWmRegion(
                        ui.ai_wm_region_x,
                        ui.ai_wm_region_y,
                        ui.ai_wm_region_w,
                        v,
                    )
                },
            ),
        ]
        .spacing(4)
    };

    match ui.ai_wm_tab {
        WatermarkTab::AiAuto => {
            col = col
                .push(text(tr(lang, "label-detector-model")).size(11))
                .push(
                    row![
                        button::labeled(
                            "YOLO26",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_detector == "yolo26",
                            },
                            Some(EditorMessage::AiSetWmDetector("yolo26".to_owned())),
                        )
                        .width(Fill),
                        button::labeled(
                            "YOLO11",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_detector == "yolo11",
                            },
                            Some(EditorMessage::AiSetWmDetector("yolo11".to_owned())),
                        )
                        .width(Fill),
                        button::labeled(
                            tr(lang, "label-mask-brush"),
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_detector == "brush",
                            },
                            Some(EditorMessage::AiSetWmDetector("brush".to_owned())),
                        )
                        .width(Fill),
                        button::labeled(
                            "Region",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_detector == "none",
                            },
                            Some(EditorMessage::AiSetWmDetector("none".to_owned())),
                        )
                        .width(Fill),
                    ]
                    .spacing(3),
                )
                .push(text(tr(lang, "label-inpainter-model")).size(11))
                .push(
                    row![
                        button::labeled(
                            "MI-GAN v2",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_inpainter == "migan-pipeline-v2",
                            },
                            Some(EditorMessage::AiSetWmInpainter(
                                "migan-pipeline-v2".to_owned(),
                            )),
                        )
                        .width(Fill),
                        button::labeled(
                            "LaMa FP32",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_inpainter == "lama-fp32",
                            },
                            Some(EditorMessage::AiSetWmInpainter("lama-fp32".to_owned())),
                        )
                        .width(Fill),
                    ]
                    .spacing(3),
                )
                .push(build_ai_model_quick_row(state, lang, &ui.ai_wm_inpainter))
                .push(slider_stepper_control(
                    tr(lang, "label-confidence"),
                    ui.ai_wm_confidence,
                    0.05..=0.95,
                    0.05_f32,
                    format!("{:.0}%", ui.ai_wm_confidence * 100.0),
                    EditorMessage::AiSetWmConfidence,
                ))
                .push(slider_stepper_control(
                    tr(lang, "label-mask-padding"),
                    ui.ai_wm_mask_padding as f32,
                    0.0..=64.0,
                    1.0_f32,
                    format!("{} px", ui.ai_wm_mask_padding),
                    |v| EditorMessage::AiSetWmMaskPadding(v.round() as u32),
                ));
            if ui.ai_wm_detector == "brush" {
                col = col
                    .push(slider_stepper_control(
                        format!(
                            "{} ({})",
                            tr(lang, "label-brush-size"),
                            ui.ai_wm_mask_strokes.len()
                        ),
                        ui.ai_wm_brush_radius,
                        4.0..=150.0,
                        1.0_f32,
                        format!("{:.0} px", ui.ai_wm_brush_radius),
                        EditorMessage::AiSetWmMaskBrushRadius,
                    ))
                    .push(
                        button::labeled(
                            tr(lang, "action-clear-mask"),
                            Some(Icon::Trash),
                            ButtonVariant::Secondary,
                            (!ui.ai_wm_mask_strokes.is_empty())
                                .then_some(EditorMessage::AiClearWmMaskStrokes),
                        )
                        .width(Fill),
                    );
            } else if ui.ai_wm_detector == "none" {
                col = col.push(region_controls());
            }
            col = col.push(
                button::labeled(
                    tr(lang, "action-remove-watermark"),
                    Some(Icon::Denoise),
                    ButtonVariant::Primary,
                    (!running).then_some(EditorMessage::AiRunWatermark),
                )
                .width(Fill),
            );
        }
        WatermarkTab::Gemini => {
            col = col
                .push(text(tr(lang, "label-gemini-size")).size(11))
                .push(
                    row![
                        button::labeled(
                            "Auto",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_gemini_size.is_none(),
                            },
                            Some(EditorMessage::AiSetWmGeminiSize(None)),
                        )
                        .width(Fill),
                        button::labeled(
                            "48",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_gemini_size == Some(48),
                            },
                            Some(EditorMessage::AiSetWmGeminiSize(Some(48))),
                        )
                        .width(Fill),
                        button::labeled(
                            "96",
                            None,
                            ButtonVariant::Tab {
                                active: ui.ai_wm_gemini_size == Some(96),
                            },
                            Some(EditorMessage::AiSetWmGeminiSize(Some(96))),
                        )
                        .width(Fill),
                    ]
                    .spacing(3),
                )
                .push(slider_stepper_control(
                    tr(lang, "label-gemini-brightness"),
                    ui.ai_wm_gemini_brightness,
                    0.0..=1.0,
                    0.02_f32,
                    format!("{:.2}", ui.ai_wm_gemini_brightness),
                    EditorMessage::AiSetWmGeminiBrightness,
                ))
                .push(slider_stepper_control(
                    tr(lang, "label-offset-x"),
                    ui.ai_wm_gemini_offset_x as f32,
                    -20.0..=20.0,
                    1.0_f32,
                    format!("{} px", ui.ai_wm_gemini_offset_x),
                    |v| EditorMessage::AiSetWmGeminiOffsetX(v.round() as i32),
                ))
                .push(slider_stepper_control(
                    tr(lang, "label-offset-y"),
                    ui.ai_wm_gemini_offset_y as f32,
                    -20.0..=20.0,
                    1.0_f32,
                    format!("{} px", ui.ai_wm_gemini_offset_y),
                    |v| EditorMessage::AiSetWmGeminiOffsetY(v.round() as i32),
                ))
                .push(
                    checkbox(ui.ai_wm_gemini_force)
                        .label(tr(lang, "label-force-remove"))
                        .on_toggle(|_| EditorMessage::AiToggleWmGeminiForce),
                )
                .push(
                    button::labeled(
                        tr(lang, "action-remove-watermark"),
                        Some(Icon::Denoise),
                        ButtonVariant::Primary,
                        (!running).then_some(EditorMessage::AiRunWatermark),
                    )
                    .width(Fill),
                );
        }
        WatermarkTab::Region => {
            col = col
                .push(region_controls())
                .push(slider_stepper_control(
                    tr(lang, "label-ai-edge-feather"),
                    ui.ai_wm_region_feather as f32,
                    0.0..=32.0,
                    1.0_f32,
                    format!("{} px", ui.ai_wm_region_feather),
                    |v| EditorMessage::AiSetWmRegionFeather(v.round() as u32),
                ))
                .push(
                    button::labeled(
                        tr(lang, "action-inpaint-region"),
                        Some(Icon::Denoise),
                        ButtonVariant::Primary,
                        (!running).then_some(EditorMessage::AiRunWatermark),
                    )
                    .width(Fill),
                );
        }
    }

    if let Some(banner) = build_ai_task_banner(state, lang) {
        col = col.push(banner);
    }

    section_card(tr(lang, "tool-ai-watermark"), col)
}

pub(crate) fn build_ai_caption_tool_options<'a>(
    state: EditorViewState<'a>,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = state.editor_ui;
    let running = state
        .snapshot
        .meta
        .ai_task
        .as_ref()
        .is_some_and(|t| t.running);

    let mut col = column![
        text(tr(lang, "label-ai-model")).size(11),
        row![
            button::labeled(
                "WD ViT v3",
                None,
                ButtonVariant::Tab {
                    active: ui.ai_caption_model == "wd-vit-tagger-v3",
                },
                Some(EditorMessage::AiSetCaptionModel(
                    "wd-vit-tagger-v3".to_owned(),
                )),
            )
            .width(Fill),
            button::labeled(
                "WD SwinV2 v3",
                None,
                ButtonVariant::Tab {
                    active: ui.ai_caption_model == "wd-swinv2-tagger-v3",
                },
                Some(EditorMessage::AiSetCaptionModel(
                    "wd-swinv2-tagger-v3".to_owned(),
                )),
            )
            .width(Fill),
        ]
        .spacing(3),
        build_ai_model_quick_row(state, lang, &ui.ai_caption_model),
        slider_stepper_control(
            tr(lang, "label-general-threshold"),
            ui.ai_caption_general_threshold,
            0.1..=0.9,
            0.05_f32,
            format!("{:.2}", ui.ai_caption_general_threshold),
            EditorMessage::AiSetCaptionGeneralThreshold,
        ),
        slider_stepper_control(
            tr(lang, "label-character-threshold"),
            ui.ai_caption_char_threshold,
            0.1..=0.95,
            0.05_f32,
            format!("{:.2}", ui.ai_caption_char_threshold),
            EditorMessage::AiSetCaptionCharThreshold,
        ),
        checkbox(ui.ai_caption_replace_underscores)
            .label(tr(lang, "label-replace-underscores"))
            .on_toggle(|_| EditorMessage::AiToggleCaptionUnderscores),
        text(tr(lang, "label-caption-prefix")).size(11),
        text_input("", &ui.ai_caption_prefix)
            .on_input(EditorMessage::AiSetCaptionPrefix)
            .size(TEXT_SIZE_CONTROL)
            .padding([4, 8])
            .style(input_style),
        text(tr(lang, "label-caption-suffix")).size(11),
        text_input("", &ui.ai_caption_suffix)
            .on_input(EditorMessage::AiSetCaptionSuffix)
            .size(TEXT_SIZE_CONTROL)
            .padding([4, 8])
            .style(input_style),
        text(tr(lang, "label-caption-exclude")).size(11),
        text_input("tag1, tag2", &ui.ai_caption_exclude)
            .on_input(EditorMessage::AiSetCaptionExclude)
            .size(TEXT_SIZE_CONTROL)
            .padding([4, 8])
            .style(input_style),
        button::labeled(
            tr(lang, "action-generate-caption"),
            Some(Icon::Plug),
            ButtonVariant::Primary,
            (!running).then_some(EditorMessage::AiRunCaption),
        )
        .width(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        text_input(&tr(lang, "caption-placeholder"), &ui.ai_caption_draft)
            .on_input(EditorMessage::AiSetCaptionDraft)
            .size(TEXT_SIZE_CONTROL)
            .padding([5, 8])
            .style(input_style),
        row![
            button::labeled(
                tr(lang, "action-save-caption"),
                Some(Icon::Save),
                ButtonVariant::Secondary,
                Some(EditorMessage::AiSaveCaption(ui.ai_caption_draft.clone())),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "action-copy"),
                Some(Icon::Copy),
                ButtonVariant::Ghost,
                (!ui.ai_caption_draft.trim().is_empty())
                    .then(|| EditorMessage::CopyText(ui.ai_caption_draft.clone())),
            ),
            button::labeled(
                tr(lang, "action-clear"),
                Some(Icon::Trash),
                ButtonVariant::Ghost,
                Some(EditorMessage::AiSaveCaption(String::new())),
            ),
        ]
        .spacing(4),
    ]
    .spacing(6);

    if let Some(banner) = build_ai_task_banner(state, lang) {
        col = col.push(banner);
    }

    section_card(tr(lang, "tool-ai-caption"), col)
}
