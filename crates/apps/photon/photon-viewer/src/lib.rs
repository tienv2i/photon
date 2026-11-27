//! Viewer workspace UI (`iced 0.14` + `fw-ui` + `fw-panels`):
//! Single, Double-page spread (LTR/RTL + cover split), and continuous Manhwa/Webtoon strip,
//! unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.

mod edge_nav;
mod geometry;
mod menus;
mod sidebar;
mod spread;
mod status;
mod toolbar;
mod translate_overlay;
mod translate_panel;
mod types;
mod webtoon;

pub use edge_nav::*;
pub use geometry::*;
pub use menus::*;
pub use spread::*;
pub use status::*;
pub use toolbar::*;
pub use types::*;
pub use webtoon::*;

use fw_panels::{PanelSplitterEdge, panel_splitter};
use fw_ui::button::{self, ButtonVariant};
use fw_ui::grid::{FilmstripItem, filmstrip_bar};
use fw_ui::icons::Icon;
use fw_ui::theme::{
    PaletteTokens, TEXT_SIZE_CAPTION, canvas_backdrop_style, hairline_rule_style,
    surface_panel_style,
};
use fw_ui::tooltip::{TooltipPosition, with_tooltip_pos};
use fw_ui::typography::text;
use iced::widget::{Canvas, column, container, responsive, row, rule, scrollable, stack};
use iced::{Alignment, Element, Fill, Length};
use photon_i18n::tr;
use photon_protocol::{Command, Theme, ViewerRightPanel, ViewerViewMode};
use photon_viewer_canvas::{ViewerCanvasProgram, ViewerCanvasTool};

use edge_nav::EdgeNavButtonProgram;
use sidebar::{
    VIEWER_RIGHT_DEFAULT_W, VIEWER_RIGHT_MAX_W, VIEWER_RIGHT_MIN_W, build_viewer_right_sidebar,
    viewer_panel_is_open, viewer_right_width,
};
use translate_overlay::build_viewer_translate_overlay;

/// Main Viewer workspace view: Single/Double/Manhwa viewport,
/// unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.
pub fn build_viewer_view<'a>(state: ViewerViewState<'a>) -> Element<'a, ViewerMessage> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    let viewer_mode = state.viewer_mode;

    let canvas_tool = if matches!(viewer_mode, ViewerViewMode::Single | ViewerViewMode::Double)
        && state.translate.active
        && (state.translate.picking_region
            || matches!(
                state.translate.mode,
                ViewerTranslateMode::Translate | ViewerTranslateMode::OcrOnly
            )) {
        ViewerCanvasTool::MarqueeRect
    } else if state.translate.active && state.translate.mode == ViewerTranslateMode::Pan {
        ViewerCanvasTool::Hand
    } else {
        ViewerCanvasTool::Pointer
    };

    let has_any_image = state.viewer_frame.is_some() || !state.snapshot.viewer_images.is_empty();
    let viewport: Element<'_, ViewerMessage> = if !has_any_image {
        container(
            column![
                fw_ui::icons::render(Icon::Image, 40.0, None),
                text(tr(lang, "label-no-image"))
                    .size(fw_ui::theme::TEXT_SIZE_CONTROL)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
                button::labeled(
                    tr(lang, "action-open-file"),
                    Some(Icon::FolderOpen),
                    ButtonVariant::Primary,
                    Some(ViewerMessage::PickImagesDialog),
                ),
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style)
        .into()
    } else {
        match viewer_mode {
            ViewerViewMode::Single => {
                let canvas: Element<'_, ViewerMessage> = Element::from(
                    Canvas::new(ViewerCanvasProgram {
                        frame: state.viewer_frame.cloned(),
                        pair_frame: None,
                        dual_gap_px: 0.0,
                        zoom: state.zoom,
                        pan_x: state.pan_x,
                        pan_y: state.pan_y,
                        adjustments: state.viewer_adj,
                        dark_theme: dark,
                        quarter_turns: state.viewer_quarter_turns,
                        tool: canvas_tool,
                        viewport_bg: state.viewport_background,
                        aa_mode: state.aa_mode,
                        denoise_mode: state.denoise_mode,
                    })
                    .width(Fill)
                    .height(Fill),
                )
                .map(|ev| ViewerMessage::ViewerCanvas(None, ev));

                container(canvas)
                    .width(Fill)
                    .height(Fill)
                    .style(canvas_backdrop_style)
                    .into()
            }
            ViewerViewMode::Double => build_viewer_double_panes(state, lang, dark, canvas_tool),
            ViewerViewMode::Webtoon => build_viewer_webtoon_strip(state),
        }
    };

    let viewport: Element<'_, ViewerMessage> = if state.translate.active {
        build_viewer_translate_overlay(viewport, state, lang)
    } else {
        viewport
    };

    let edge_nav_enabled = matches!(viewer_mode, ViewerViewMode::Single | ViewerViewMode::Double)
        && state.snapshot.viewer_images.len() > 1
        && !state.translate.space_pan_held
        && !(state.translate.active
            && (state.translate.picking_region
                || matches!(
                    state.translate.mode,
                    ViewerTranslateMode::Pan
                        | ViewerTranslateMode::Translate
                        | ViewerTranslateMode::OcrOnly
                )));

    let viewport: Element<'_, ViewerMessage> = if edge_nav_enabled {
        let (left_msg, right_msg) = match viewer_mode {
            ViewerViewMode::Double => {
                if state.viewer_manga_rtl {
                    (
                        ViewerMessage::DualStepSpread(1),
                        ViewerMessage::DualStepSpread(-1),
                    )
                } else {
                    (
                        ViewerMessage::DualStepSpread(-1),
                        ViewerMessage::DualStepSpread(1),
                    )
                }
            }
            _ => (
                ViewerMessage::Dispatch(Command::ViewerPrevious),
                ViewerMessage::Dispatch(Command::ViewerNext),
            ),
        };
        let (left_img_size, right_img_size) = match viewer_mode {
            ViewerViewMode::Single => {
                let sz = state.viewer_frame.map(|f| (f.width, f.height)).or_else(|| {
                    state
                        .snapshot
                        .meta
                        .viewer_path
                        .as_deref()
                        .and_then(|p| state.image_sizes.get(p).copied())
                });
                (sz, None)
            }
            ViewerViewMode::Double => {
                let (left_slot, right_slot) = dual_spread_slots(state);
                let l_sz = state
                    .viewer_frame
                    .map(|f| (f.width, f.height))
                    .or_else(|| left_slot.and_then(|(_, p)| state.image_sizes.get(p).copied()));
                let r_sz = state
                    .viewer_pair_frame
                    .map(|f| (f.width, f.height))
                    .or_else(|| right_slot.and_then(|(_, p)| state.image_sizes.get(p).copied()));
                (l_sz, r_sz)
            }
            ViewerViewMode::Webtoon => (None, None),
        };
        let dual_gap_px = f32::from(state.viewer_manga_page_gap);
        let quarter_turns = state.viewer_quarter_turns;
        let zoom = state.zoom;
        let pan_x = state.pan_x;
        let pan_y = state.pan_y;

        let edge_overlay: Element<'_, ViewerMessage> = responsive(move |bounds| {
            let (left_overlaps, right_overlaps) = viewer_edge_nav_overlap(
                viewer_mode,
                left_img_size,
                right_img_size,
                dual_gap_px,
                quarter_turns,
                zoom,
                pan_x,
                pan_y,
                bounds.width,
                bounds.height,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            );
            let left_btn = iced::widget::canvas(EdgeNavButtonProgram {
                is_next: false,
                auto_hide: left_overlaps,
                on_press: left_msg.clone(),
            })
            .width(Length::Fixed(VIEWER_EDGE_NAV_ZONE_WIDTH))
            .height(Fill);
            let right_btn = iced::widget::canvas(EdgeNavButtonProgram {
                is_next: true,
                auto_hide: right_overlaps,
                on_press: right_msg.clone(),
            })
            .width(Length::Fixed(VIEWER_EDGE_NAV_ZONE_WIDTH))
            .height(Fill);
            row![
                left_btn,
                iced::widget::space::horizontal().width(Fill),
                right_btn,
            ]
            .width(Fill)
            .height(Fill)
            .into()
        })
        .into();
        stack![viewport, edge_overlay]
            .width(Fill)
            .height(Fill)
            .into()
    } else {
        viewport
    };

    let open_panels: Vec<ViewerRightPanel> = state
        .viewer_right_panels
        .order()
        .iter()
        .copied()
        .filter(|&p| viewer_panel_is_open(state, p))
        .collect();

    let main_area: Element<'_, ViewerMessage> = if !open_panels.is_empty() {
        let right_w = viewer_right_width(state);
        let splitter = panel_splitter(
            right_w,
            PanelSplitterEdge::Right.direction(),
            VIEWER_RIGHT_MIN_W,
            VIEWER_RIGHT_MAX_W,
            VIEWER_RIGHT_DEFAULT_W,
            ViewerMessage::ViewerSplitterDrag,
            ViewerMessage::ViewerSplitterRelease,
        );

        let right_dock = build_viewer_right_sidebar(state, lang, right_w, &open_panels);

        row![
            viewport,
            splitter,
            rule::vertical(1).style(hairline_rule_style),
            right_dock,
        ]
        .height(Fill)
        .width(Fill)
        .into()
    } else {
        viewport
    };

    if viewer_mode != ViewerViewMode::Webtoon
        && state.snapshot.viewer_images.len() > 1
        && state.viewer_filmstrip_open
    {
        let active_path = state.snapshot.meta.viewer_path.as_deref();
        let items: Vec<FilmstripItem<ViewerMessage>> = state
            .snapshot
            .viewer_images
            .iter()
            .map(|path| FilmstripItem {
                thumbnail: state.thumbnails.get(path).cloned(),
                active: active_path == Some(path.as_str()),
                on_press: ViewerMessage::Dispatch(Command::ViewerOpen { path: path.clone() }),
            })
            .collect();
        let close_btn = with_tooltip_pos(
            button::compact_icon(
                Icon::Close,
                ButtonVariant::Ghost,
                Some(ViewerMessage::CloseFilmstrip),
            ),
            tr(lang, "tooltip-panel-close"),
            TooltipPosition::Left,
        );
        let archive_label = state.snapshot.meta.viewer_archive.as_ref().map(|arch| {
            let cur = state
                .snapshot
                .meta
                .viewer_index
                .unwrap_or(0)
                .min(arch.page_count.saturating_sub(1));
            let page_lbl = arch
                .page_labels
                .get(cur)
                .cloned()
                .unwrap_or_else(|| format!("Page {}", cur + 1));
            let kind = if arch.kind == "pdf" { "PDF" } else { "CBZ" };
            format!("{kind} · {} / {} · {page_lbl}", cur + 1, arch.page_count)
        });
        let mut filmstrip_col = column![].width(Fill);
        if let Some(lbl) = archive_label {
            filmstrip_col = filmstrip_col.push(
                container(
                    text(lbl)
                        .size(TEXT_SIZE_CAPTION)
                        .style(|theme| text::Style {
                            color: Some(PaletteTokens::for_theme(theme).text_muted),
                        }),
                )
                .padding([4, 10]),
            );
        }
        let on_scroll: Option<Box<dyn Fn(scrollable::Viewport) -> ViewerMessage>> =
            Some(Box::new(move |viewport| ViewerMessage::FilmstripScrolled {
                offset: viewport.absolute_offset().x,
            }));
        let on_wheel: Option<Box<dyn Fn(iced::mouse::ScrollDelta) -> ViewerMessage>> =
            Some(Box::new(ViewerMessage::FilmstripWheelScroll));
        filmstrip_col = filmstrip_col.push(
            row![
                container(filmstrip_bar(
                    items,
                    filmstrip_scroll_id(),
                    on_scroll,
                    on_wheel
                ))
                .width(Fill),
                container(close_btn).padding([4, 6]),
            ]
            .align_y(Alignment::Center)
            .width(Fill),
        );
        let filmstrip_row = container(filmstrip_col)
            .width(Fill)
            .style(surface_panel_style);
        column![
            main_area,
            rule::horizontal(1).style(hairline_rule_style),
            filmstrip_row,
        ]
        .height(Fill)
        .width(Fill)
        .into()
    } else {
        main_area
    }
}

pub fn filmstrip_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("viewer_filmstrip_strip")
}
