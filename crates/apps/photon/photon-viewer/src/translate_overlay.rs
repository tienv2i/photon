use fw_ui::button::{self, ButtonVariant};
use fw_ui::icons::Icon;
use fw_ui::theme::{PaletteTokens, hairline_rule_style, popover_style};
use fw_ui::typography::text;
use iced::widget::{
    button as iced_button, column, container, mouse_area, responsive, row, rule, stack,
};
use iced::{Alignment, Border, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_protocol::{Language, TranslationBlock, ViewerViewMode};

use crate::geometry::{
    compute_bubble_font_size, transform_block_for_quarter_turns,
    transform_double_page_block_to_screen, viewer_double_fit_geometries, viewer_fit_geometry,
};
use crate::spread::dual_spread_slots;
use crate::types::{ViewerMessage, ViewerTranslateMode, ViewerTranslateState, ViewerViewState};

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_translate_bubble_item<'a>(
    tr_state: &ViewerTranslateState,
    lang: Language,
    page_path: Option<&str>,
    idx: usize,
    b: &TranslationBlock,
    sx: f32,
    sy: f32,
    sw: f32,
    sh: f32,
) -> Option<Element<'a, ViewerMessage>> {
    let ocr_only = tr_state.mode == ViewerTranslateMode::OcrOnly;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;
    let show_orig = ocr_only
        || match page_path {
            Some(p) => tr_state
                .show_original_page_blocks
                .contains(&(p.to_owned(), idx)),
            None => tr_state.show_original_indices.contains(&idx),
        };
    let has_both = !ocr_only && !b.original.trim().is_empty() && !b.translated.trim().is_empty();
    let display_text = if show_orig || b.translated.trim().is_empty() {
        b.original.clone()
    } else {
        b.translated.clone()
    };
    if display_text.trim().is_empty() {
        return None;
    }

    let header_h = if is_review { 0.0 } else { 22.0 };
    let font_size = compute_bubble_font_size(
        sw,
        sh,
        header_h,
        display_text.chars().count(),
        tr_state.font_scale,
    );
    let page_matches = match page_path {
        Some(p) => tr_state.active_page_path.as_deref() == Some(p),
        None => true,
    };
    let is_sel = page_matches && tr_state.editing_block_idx == Some(idx);
    let drag_page_matches = match page_path {
        Some(p) => tr_state.active_drag_page.as_deref() == Some(p),
        None => tr_state.active_drag_page.is_none(),
    };
    let is_dragging =
        drag_page_matches && matches!(tr_state.active_drag_delta, Some((d_idx, _)) if d_idx == idx);

    let toggle_msg = match page_path {
        Some(p) => ViewerMessage::ToggleTranslatePageBlockOriginal(p.to_owned(), idx),
        None => ViewerMessage::ToggleTranslateBlockOriginal(idx),
    };
    let select_msg = match page_path {
        Some(p) => ViewerMessage::SelectTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::SelectTranslateBlock(Some(idx)),
    };
    let drag_msg = match page_path {
        Some(p) => ViewerMessage::BeginDragTranslatePageBubble(p.to_owned(), idx),
        None => ViewerMessage::BeginDragTranslateBubble(idx),
    };
    let retry_msg = match page_path {
        Some(p) => ViewerMessage::RetryTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::RetryTranslateBlock(idx),
    };
    let remove_msg = match page_path {
        Some(p) => ViewerMessage::RemoveTranslatePageBlock(p.to_owned(), idx),
        None => ViewerMessage::RemoveTranslateBlock(idx),
    };

    let body_press = if is_review && has_both {
        toggle_msg.clone()
    } else {
        select_msg
    };

    let pass_through_pan = tr_state.space_pan_held || tr_state.mode == ViewerTranslateMode::Pan;

    let text_content = container(
        text(display_text)
            .size(font_size)
            .align_x(iced::alignment::Horizontal::Center)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_primary),
            }),
    )
    .width(Fill)
    .height(Fill)
    .padding([4, 7])
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);

    let text_body: Element<'_, ViewerMessage> = if pass_through_pan {
        text_content.into()
    } else {
        iced_button(text_content)
            .width(Fill)
            .height(Fill)
            .padding(0)
            .style(|theme, _| {
                let pal = PaletteTokens::for_theme(theme);
                iced_button::Style {
                    background: None,
                    text_color: pal.text_primary,
                    ..Default::default()
                }
            })
            .on_press(body_press)
            .into()
    };

    let idx_badge = text(format!("#{}", idx + 1))
        .size(10)
        .style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).accent),
        });

    let bubble_inner: Element<'_, ViewerMessage> = if is_review {
        text_body
    } else if pass_through_pan {
        let header_row = row![
            container(idx_badge)
                .padding([0, 6])
                .height(Length::Fixed(22.0))
                .align_y(iced::alignment::Vertical::Center),
            iced::widget::space::horizontal(),
        ]
        .spacing(3)
        .align_y(Alignment::Center)
        .height(Length::Fixed(22.0))
        .padding([0, 4]);

        let header_bar = container(header_row).width(Fill).style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_elevated.scale_alpha(0.95).into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: pal.border,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

        column![
            header_bar,
            rule::horizontal(1).style(hairline_rule_style),
            text_body
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        let drag_grip = mouse_area(
            container(
                row![
                    text("⋮⋮").size(10).style(|theme| text::Style {
                        color: Some(PaletteTokens::for_theme(theme).text_muted),
                    }),
                    idx_badge,
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .padding([0, 6])
            .height(Length::Fixed(22.0))
            .align_y(iced::alignment::Vertical::Center),
        )
        .on_press(drag_msg);

        let mut header_row = row![drag_grip, iced::widget::space::horizontal()]
            .spacing(3)
            .align_y(Alignment::Center)
            .height(Length::Fixed(22.0))
            .padding([0, 4]);

        if has_both {
            let toggle_lbl = tr(
                lang,
                if show_orig {
                    "translate-bubble-trans"
                } else {
                    "translate-bubble-orig"
                },
            );
            header_row = header_row.push(
                iced_button(text(toggle_lbl).size(10))
                    .padding([1, 5])
                    .style(button::style_for(if show_orig {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    }))
                    .on_press(toggle_msg),
            );
        }

        header_row = header_row
            .push(
                iced_button(text("↻").size(11))
                    .padding([1, 5])
                    .style(button::style_for(ButtonVariant::Ghost))
                    .on_press(retry_msg),
            )
            .push(
                iced_button(text("×").size(11))
                    .padding([1, 5])
                    .style(button::style_for(ButtonVariant::Ghost))
                    .on_press(remove_msg),
            );

        let header_bar = container(header_row).width(Fill).style(|theme| {
            let pal = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(pal.bg_elevated.scale_alpha(0.95).into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: pal.border,
                    width: 0.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });

        column![
            header_bar,
            rule::horizontal(1).style(hairline_rule_style),
            text_body
        ]
        .width(Fill)
        .height(Fill)
        .into()
    };

    let frosted = tr_state.frosted_bubbles;
    let bubble_box = container(bubble_inner)
        .width(Length::Fixed(sw))
        .height(Length::Fixed(sh))
        .style(move |theme| {
            let pal = PaletteTokens::for_theme(theme);
            let bg = if frosted {
                pal.bg_elevated.scale_alpha(0.92)
            } else {
                pal.bg_elevated.scale_alpha(0.98)
            };
            container::Style {
                background: Some(bg.into()),
                text_color: Some(pal.text_primary),
                border: Border {
                    color: if is_sel || is_dragging {
                        pal.accent
                    } else if is_review {
                        pal.border
                    } else {
                        pal.border_strong
                    },
                    width: if is_sel || is_dragging { 2.0 } else { 1.0 },
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        });

    Some(
        container(bubble_box)
            .padding(Padding {
                top: sy.max(0.0),
                right: 0.0,
                bottom: 0.0,
                left: sx.max(0.0),
            })
            .width(Fill)
            .height(Fill)
            .into(),
    )
}

pub(crate) fn build_viewer_translate_overlay<'a>(
    viewport: Element<'a, ViewerMessage>,
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let tr_state = state.translate;
    let is_single = state.viewer_mode == ViewerViewMode::Single;
    let is_double = state.viewer_mode == ViewerViewMode::Double;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;

    let mut stack_layers: Vec<Element<'a, ViewerMessage>> = vec![viewport];

    // 1. Full-box covering translated/OCR bubbles (Single & Double modes; Webtoon renders per-page inside scroll)
    if tr_state.show_bubbles
        && is_single
        && !tr_state.blocks.is_empty()
        && let Some(frame) = state.viewer_frame
    {
        let doc_w = frame.width as f32;
        let doc_h = frame.height as f32;
        let qturns = state.viewer_quarter_turns;
        let zoom = state.zoom;
        let pan_x = state.pan_x;
        let pan_y = state.pan_y;
        let tr_owned = tr_state.clone();

        let bubble_layer: Element<'a, ViewerMessage> = responsive(move |bounds| {
            let (left, top, drawn_w, drawn_h, fw, fh) = viewer_fit_geometry(
                doc_w,
                doc_h,
                qturns,
                zoom,
                pan_x,
                pan_y,
                bounds.width,
                bounds.height,
            );
            let mut items: Vec<Element<'_, ViewerMessage>> =
                Vec::with_capacity(tr_owned.blocks.len() + 1);
            items.push(
                iced::widget::space::horizontal()
                    .width(Fill)
                    .height(Fill)
                    .into(),
            );
            for (idx, b) in tr_owned.blocks.iter().enumerate() {
                let (rx, ry, rw, rh) = transform_block_for_quarter_turns(b, doc_w, doc_h, qturns);
                let (dx_px, dy_px) = match tr_owned.active_drag_delta {
                    Some((drag_idx, d)) if drag_idx == idx => (d[0], d[1]),
                    _ => (0.0, 0.0),
                };
                let sx = (left + (rx / fw) * drawn_w + dx_px).max(0.0);
                let sy = (top + (ry / fh) * drawn_h + dy_px).max(0.0);
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                let sw = ((rw / fw) * drawn_w).max(min_w);
                let sh = ((rh / fh) * drawn_h).max(min_h);
                if let Some(el) =
                    render_translate_bubble_item(&tr_owned, lang, None, idx, b, sx, sy, sw, sh)
                {
                    items.push(el);
                }
            }
            stack(items).width(Fill).height(Fill).into()
        })
        .into();
        stack_layers.push(bubble_layer);
    } else if tr_state.show_bubbles
        && is_double
        && let Some(left_frame) = state.viewer_frame
    {
        let (left_slot, right_slot) = dual_spread_slots(state);
        let left_path = left_slot.map(|(_, p)| p.to_owned());
        let right_path = right_slot.map(|(_, p)| p.to_owned());
        let left_blocks: Vec<TranslationBlock> = left_path
            .as_deref()
            .and_then(|p| tr_state.bubble_cache.get(p).cloned())
            .or_else(|| {
                (left_path.as_deref() == state.snapshot.meta.viewer_path.as_deref())
                    .then(|| tr_state.blocks.clone())
            })
            .unwrap_or_default();
        let right_blocks: Vec<TranslationBlock> = right_path
            .as_deref()
            .and_then(|p| tr_state.bubble_cache.get(p).cloned())
            .unwrap_or_default();

        if !left_blocks.is_empty() || !right_blocks.is_empty() {
            let left_size = (left_frame.width, left_frame.height);
            let right_size = state.viewer_pair_frame.map(|f| (f.width, f.height));
            let gap = f32::from(state.viewer_manga_page_gap);
            let qturns = state.viewer_quarter_turns;
            let zoom = state.zoom;
            let pan_x = state.pan_x;
            let pan_y = state.pan_y;
            let tr_owned = tr_state.clone();

            let bubble_layer: Element<'a, ViewerMessage> = responsive(move |bounds| {
                let geom = viewer_double_fit_geometries(
                    left_size,
                    right_size,
                    gap,
                    qturns,
                    zoom,
                    pan_x,
                    pan_y,
                    bounds.width,
                    bounds.height,
                );
                let mut items: Vec<Element<'_, ViewerMessage>> =
                    Vec::with_capacity(left_blocks.len() + right_blocks.len() + 1);
                items.push(
                    iced::widget::space::horizontal()
                        .width(Fill)
                        .height(Fill)
                        .into(),
                );
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                if let Some(ref lp) = left_path {
                    for (idx, b) in left_blocks.iter().enumerate() {
                        let (mut sx, mut sy, sw_raw, sh_raw) =
                            transform_double_page_block_to_screen(
                                b,
                                left_size.0 as f32,
                                left_size.1 as f32,
                                false,
                                geom,
                                qturns,
                            );
                        if let Some((drag_idx, d)) = tr_owned.active_drag_delta
                            && drag_idx == idx
                            && tr_owned.active_drag_page.as_deref() == Some(lp.as_str())
                        {
                            sx += d[0];
                            sy += d[1];
                        }
                        if let Some(el) = render_translate_bubble_item(
                            &tr_owned,
                            lang,
                            Some(lp.as_str()),
                            idx,
                            b,
                            sx,
                            sy,
                            sw_raw.max(min_w),
                            sh_raw.max(min_h),
                        ) {
                            items.push(el);
                        }
                    }
                }
                if let (Some(rp), Some(r_sz)) = (&right_path, right_size) {
                    for (idx, b) in right_blocks.iter().enumerate() {
                        let (mut sx, mut sy, sw_raw, sh_raw) =
                            transform_double_page_block_to_screen(
                                b,
                                r_sz.0 as f32,
                                r_sz.1 as f32,
                                true,
                                geom,
                                qturns,
                            );
                        if let Some((drag_idx, d)) = tr_owned.active_drag_delta
                            && drag_idx == idx
                            && tr_owned.active_drag_page.as_deref() == Some(rp.as_str())
                        {
                            sx += d[0];
                            sy += d[1];
                        }
                        if let Some(el) = render_translate_bubble_item(
                            &tr_owned,
                            lang,
                            Some(rp.as_str()),
                            idx,
                            b,
                            sx,
                            sy,
                            sw_raw.max(min_w),
                            sh_raw.max(min_h),
                        ) {
                            items.push(el);
                        }
                    }
                }
                stack(items).width(Fill).height(Fill).into()
            })
            .into();
            stack_layers.push(bubble_layer);
        }
    }

    // 2. Compact Review Pill when in Review mode (hidden while Space-panning)
    if !tr_state.space_pan_held && is_review {
        let review_pill = container(
            row![
                button::labeled(
                    "A−",
                    None,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::SetTranslateFontScale(
                        (tr_state.font_scale - 0.1).clamp(0.7, 2.0),
                    )),
                ),
                button::labeled(
                    "A+",
                    None,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::SetTranslateFontScale(
                        (tr_state.font_scale + 0.1).clamp(0.7, 2.0),
                    )),
                ),
                button::labeled(
                    tr(lang, "action-edit"),
                    None,
                    ButtonVariant::Secondary,
                    Some(ViewerMessage::SetTranslateMode(
                        ViewerTranslateMode::Translate,
                    )),
                ),
                button::compact_icon(
                    Icon::Close,
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::CloseTranslateOverlay),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .style(popover_style);

        stack_layers.push(
            container(review_pill)
                .width(Fill)
                .height(Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Top)
                .padding(12.0)
                .into(),
        );
    }

    stack(stack_layers).width(Fill).height(Fill).into()
}
