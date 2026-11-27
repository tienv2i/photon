use fw_ui::icons::Icon;
use fw_ui::theme::{PaletteTokens, canvas_backdrop_style, surface_panel_style};
use iced::widget::{column, container, image as iced_image, mouse_area, scrollable, stack};
use iced::{Alignment, Border, ContentFit, Element, Fill, Length, Padding};
use photon_protocol::{ViewerRightPanel, WebtoonFitMode};

use crate::sidebar::{viewer_panel_is_open, viewer_right_width};
use crate::translate_overlay::render_translate_bubble_item;
use crate::types::{ViewerMessage, ViewerTranslateMode, ViewerViewState};

pub fn webtoon_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("viewer_webtoon_strip")
}

pub fn compute_webtoon_layout(state: ViewerViewState<'_>) -> (f32, Vec<(String, f32, f32)>) {
    let right_open = ViewerRightPanel::ALL
        .iter()
        .any(|&p| viewer_panel_is_open(state, p));
    let right_chrome = if right_open {
        viewer_right_width(state) + 20.0
    } else {
        24.0
    };
    let avail_w = (state.window_size.width - right_chrome).max(320.0);
    let avail_h = (state.window_size.height - 130.0).max(240.0);

    let mut widths: Vec<u32> = state
        .snapshot
        .viewer_images
        .iter()
        .take(20)
        .filter_map(|p| state.image_sizes.get(p).map(|(w, _)| *w))
        .filter(|w| *w > 0)
        .collect();
    widths.sort_unstable();
    let median_w = widths
        .get(widths.len() / 2)
        .copied()
        .map(|w| w as f32)
        .unwrap_or(0.0);

    let base_col_w = match state.viewer_webtoon_fit_mode {
        WebtoonFitMode::Width => avail_w,
        WebtoonFitMode::Original => {
            if median_w > 0.0 {
                median_w
            } else {
                avail_w
            }
        }
        WebtoonFitMode::Auto | WebtoonFitMode::Height => {
            if median_w > 0.0 {
                median_w.max(360.0).min(avail_w)
            } else {
                avail_w
            }
        }
    };
    let col_w = (base_col_w * state.webtoon_zoom).max(240.0);

    let pages = state
        .snapshot
        .viewer_images
        .iter()
        .map(|p| {
            let aspect = state
                .image_sizes
                .get(p)
                .filter(|(w, h)| *w > 0 && *h > 0)
                .map(|(w, h)| (*h as f32 / *w as f32).clamp(0.2, 12.0))
                .unwrap_or(1.4);
            let (slot_w, slot_h) = if state.viewer_webtoon_fit_mode == WebtoonFitMode::Height {
                let h = (avail_h * state.webtoon_zoom).max(200.0);
                let w = (h / aspect).max(160.0);
                (w, h)
            } else {
                (col_w, (col_w * aspect).max(64.0))
            };
            (p.clone(), slot_w, slot_h)
        })
        .collect();

    (col_w, pages)
}

pub fn webtoon_offset_for_index(state: ViewerViewState<'_>, target: usize) -> Option<f32> {
    let (_, pages) = compute_webtoon_layout(state);
    if target >= pages.len() {
        return None;
    }
    let gap = f32::from(state.viewer_manga_page_gap);
    let y: f32 = pages.iter().take(target).map(|(_, _, h)| *h + gap).sum();
    Some(y)
}

pub fn webtoon_index_at_scroll(state: ViewerViewState<'_>, center_y: f32) -> Option<usize> {
    let (_, pages) = compute_webtoon_layout(state);
    let gap = f32::from(state.viewer_manga_page_gap);
    let mut y = 0.0f32;
    for (idx, (_, _, h)) in pages.iter().enumerate() {
        let bottom = y + *h + gap;
        if center_y < bottom || idx + 1 == pages.len() {
            return Some(idx);
        }
        y = bottom;
    }
    None
}

pub(crate) fn build_viewer_webtoon_strip<'a>(
    state: ViewerViewState<'a>,
) -> Element<'a, ViewerMessage> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (_, pages) = compute_webtoon_layout(state);
    let lang = state.snapshot.meta.language;
    let tr_state = state.translate;
    let tr_active = tr_state.active;
    let show_bubbles = tr_active && tr_state.show_bubbles;
    let is_review = tr_state.mode == ViewerTranslateMode::Review;
    let marquee_enabled = tr_active
        && !tr_state.space_pan_held
        && !is_review
        && (tr_state.picking_region
            || matches!(
                tr_state.mode,
                ViewerTranslateMode::Translate | ViewerTranslateMode::OcrOnly
            ));

    let mut col = column![]
        .spacing(gap)
        .align_x(Alignment::Center)
        .width(Fill);

    for (path, w, h) in pages {
        let handle = state
            .webtoon_pages
            .get(&path)
            .cloned()
            .or_else(|| state.thumbnails.get(&path).cloned());
        let page_el: Element<'_, ViewerMessage> = if let Some(hnd) = handle {
            container(
                iced_image(hnd)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .content_fit(ContentFit::Contain),
            )
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
        } else {
            container(fw_ui::icons::render(Icon::Image, 28.0, None))
                .width(Length::Fixed(w))
                .height(Length::Fixed(h))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(surface_panel_style)
                .into()
        };

        let page_blocks = tr_state.bubble_cache.get(&path).or_else(|| {
            (tr_state.active_page_path.as_deref() == Some(path.as_str())
                || (tr_state.active_page_path.is_none()
                    && state.snapshot.meta.viewer_path.as_deref() == Some(path.as_str())))
            .then_some(&tr_state.blocks)
        });

        let mut layers: Vec<Element<'_, ViewerMessage>> = vec![page_el];

        if show_bubbles
            && let Some(blocks) = page_blocks
            && !blocks.is_empty()
            && let Some(&(img_w, img_h)) = state.image_sizes.get(&path)
            && img_w > 0
            && img_h > 0
        {
            let doc_w = img_w as f32;
            let doc_h = img_h as f32;
            let mut bubble_items: Vec<Element<'_, ViewerMessage>> =
                Vec::with_capacity(blocks.len() + 1);
            bubble_items.push(
                iced::widget::space::horizontal()
                    .width(Fill)
                    .height(Fill)
                    .into(),
            );
            for (idx, b) in blocks.iter().enumerate() {
                let (dx_px, dy_px) = match tr_state.active_drag_delta {
                    Some((drag_idx, d))
                        if drag_idx == idx
                            && tr_state.active_drag_page.as_deref() == Some(path.as_str()) =>
                    {
                        (d[0], d[1])
                    }
                    _ => (0.0, 0.0),
                };
                let sx = ((b.x as f32 / doc_w) * w + dx_px).max(0.0);
                let sy = ((b.y as f32 / doc_h) * h + dy_px).max(0.0);
                let min_w = if is_review { 48.0 } else { 96.0 };
                let min_h = if is_review { 28.0 } else { 44.0 };
                let sw = ((b.w.max(1) as f32 / doc_w) * w).max(min_w);
                let sh = ((b.h.max(1) as f32 / doc_h) * h).max(min_h);
                if let Some(el) = render_translate_bubble_item(
                    tr_state,
                    lang,
                    Some(&path),
                    idx,
                    b,
                    sx,
                    sy,
                    sw,
                    sh,
                ) {
                    bubble_items.push(el);
                }
            }
            layers.push(
                stack(bubble_items)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .into(),
            );
        }

        if let Some((m_path, [mx, my, mw, mh])) = &tr_state.webtoon_marquee
            && m_path == &path
            && *mw >= 2.0
            && *mh >= 2.0
        {
            let marquee_box = container(iced::widget::space::horizontal().width(Fill).height(Fill))
                .width(Length::Fixed(*mw))
                .height(Length::Fixed(*mh))
                .style(|theme| {
                    let pal = PaletteTokens::for_theme(theme);
                    container::Style {
                        background: Some(pal.accent.scale_alpha(0.16).into()),
                        border: Border {
                            color: pal.accent,
                            width: 1.5,
                            radius: 2.0.into(),
                        },
                        ..Default::default()
                    }
                });
            layers.push(
                container(marquee_box)
                    .padding(Padding {
                        top: (*my).max(0.0),
                        right: 0.0,
                        bottom: 0.0,
                        left: (*mx).max(0.0),
                    })
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(h))
                    .into(),
            );
        }

        let stacked_page: Element<'_, ViewerMessage> = if layers.len() == 1 {
            layers.pop().unwrap()
        } else {
            stack(layers)
                .width(Length::Fixed(w))
                .height(Length::Fixed(h))
                .into()
        };

        let mut area =
            mouse_area(stacked_page).on_right_press(ViewerMessage::OpenViewerContextMenu {
                path: Some(path.clone()),
                pos: None,
            });
        if marquee_enabled {
            let p_move = path.clone();
            let p_press = path;
            area = area
                .on_move(move |pt| ViewerMessage::WebtoonHoverPage {
                    path: p_move.clone(),
                    local_pos: [pt.x, pt.y],
                    disp_w: w,
                    disp_h: h,
                })
                .on_press(ViewerMessage::BeginWebtoonTranslateRegion {
                    path: p_press,
                    disp_w: w,
                    disp_h: h,
                });
        }
        col = col.push(area);
    }

    mouse_area(
        container(
            scrollable(container(col).padding([8, 0]).width(Fill))
                .id(webtoon_scroll_id())
                .on_scroll(ViewerMessage::WebtoonScrolled)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style),
    )
    .on_right_press(ViewerMessage::OpenViewerContextMenu {
        path: None,
        pos: None,
    })
    .into()
}
