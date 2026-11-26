use std::path::Path;

use iced::widget::{column, container, row, rule, scrollable};
use iced::{Alignment, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, kv_row, sidebar_dock, sidebar_panel,
    stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{Language, ViewerRightPanel, ViewerViewMode};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::icons::Icon;
use photon_ui::theme::hairline_rule_style;
use photon_ui::tooltip::{TooltipPosition, with_tooltip, with_tooltip_pos};

use crate::status::format_viewer_bytes;
use crate::translate_panel::build_viewer_translate_panel_body;
use crate::types::{ViewerMessage, ViewerViewState};

pub(crate) const VIEWER_RIGHT_MIN_W: f32 = 200.0;
pub(crate) const VIEWER_RIGHT_MAX_W: f32 = 600.0;
pub(crate) const VIEWER_RIGHT_DEFAULT_W: f32 = 290.0;
const VIEWER_SPLIT_DEFAULT_PCT: f32 = 58.0;
const VIEWER_SPLIT_MIN_PCT: f32 = 20.0;
const VIEWER_SPLIT_MAX_PCT: f32 = 80.0;

pub(crate) fn viewer_right_width(state: ViewerViewState<'_>) -> f32 {
    state
        .viewer_right_width_preview
        .unwrap_or(state.snapshot.meta.layout.right_width as f32)
        .clamp(VIEWER_RIGHT_MIN_W, VIEWER_RIGHT_MAX_W)
}

pub(crate) fn viewer_panel_is_open(state: ViewerViewState<'_>, panel: ViewerRightPanel) -> bool {
    match panel {
        ViewerRightPanel::ViewAdjustment => {
            state.viewer_adjustments_open && state.viewer_mode != ViewerViewMode::Webtoon
        }
        ViewerRightPanel::Info => state.viewer_info_open,
        ViewerRightPanel::Translate => state.translate.active,
    }
}

pub(crate) fn build_viewer_right_sidebar<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
    right_w: f32,
    open_panels: &[ViewerRightPanel],
) -> Element<'a, ViewerMessage> {
    let dragging_any = state.viewer_right_panels.is_dragging();
    let is_two = open_panels.len() == 2;
    let is_multi = open_panels.len() >= 2;
    let split_pct = state
        .viewer_right_split_preview
        .unwrap_or(state.snapshot.meta.layout.viewer_right_split_pct as f32);
    let (top_h, bottom_h) = stacked_panel_heights(
        false,
        false,
        split_pct,
        VIEWER_SPLIT_MIN_PCT,
        VIEWER_SPLIT_MAX_PCT,
    );

    let mut stacked = column![].spacing(6).height(Fill).width(Fill);
    for (idx, &panel_kind) in open_panels.iter().enumerate() {
        let (title, icon, body) = match panel_kind {
            ViewerRightPanel::ViewAdjustment => (
                tr(lang, "panel-view-adjustments"),
                Icon::Palette,
                build_viewer_adjustments_body(state, lang, right_w),
            ),
            ViewerRightPanel::Info => (
                tr(lang, "panel-info"),
                Icon::Info,
                build_viewer_info_body(state, lang),
            ),
            ViewerRightPanel::Translate => (
                tr(lang, "panel-translate"),
                Icon::Translate,
                build_viewer_translate_panel_body(state, lang),
            ),
        };

        let header = DraggablePanelHeaderSpec::new(title)
            .icon(icon)
            .collapsed(false)
            .dragging(state.viewer_right_panels.is_dragging_panel(panel_kind))
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
            .close_tooltip(tr(lang, "tooltip-panel-close"))
            .on_drag_start(ViewerMessage::BeginDragViewerPanel(panel_kind))
            .on_drag_hover(dragging_any.then_some(ViewerMessage::HoverViewerPanelDrop(panel_kind)))
            .on_drag_end(dragging_any.then_some(ViewerMessage::DropViewerPanel(panel_kind)))
            .on_toggle_collapse(ViewerMessage::ToggleViewerPanelCollapse(panel_kind))
            .on_close(Some(ViewerMessage::CloseViewerPanel(panel_kind)));

        let panel_el = sidebar_panel(
            PanelFrameSpec::new(header)
                .body(body)
                .fill_height(true)
                .body_padding(Padding::from([2, 2]))
                .drop_indicator(state.viewer_right_panels.drop_indicator(panel_kind))
                .on_drop_hover(
                    dragging_any.then_some(ViewerMessage::HoverViewerPanelDrop(panel_kind)),
                )
                .on_drop_release(
                    dragging_any.then_some(ViewerMessage::DropViewerPanel(panel_kind)),
                ),
        );

        let slot_height = if is_two {
            if idx == 0 { top_h } else { bottom_h }
        } else if is_multi {
            Length::FillPortion(1)
        } else {
            Length::Fill
        };

        stacked = stacked.push(
            container(panel_el)
                .width(Fill)
                .height(slot_height)
                .clip(true),
        );

        if idx == 0 && is_two {
            let height_hint = (state.window_size.height - 120.0).max(260.0);
            stacked = stacked.push(vertical_panel_splitter(
                split_pct,
                height_hint,
                VIEWER_SPLIT_MIN_PCT,
                VIEWER_SPLIT_MAX_PCT,
                VIEWER_SPLIT_DEFAULT_PCT,
                ViewerMessage::ViewerRightSplitDrag,
                ViewerMessage::ViewerRightSplitRelease,
            ));
        }
    }

    sidebar_dock(right_w, stacked)
}

fn build_viewer_info_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
) -> Element<'a, ViewerMessage> {
    let meta = &state.snapshot.meta;
    let viewer_file = meta.viewer_path.as_deref().map(Path::new);
    let format_str = viewer_file
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "-".to_owned());
    let folder_name = viewer_file
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("-")
        .to_owned();
    let dims = state
        .viewer_frame
        .as_ref()
        .map(|f| format!("{} × {}", f.width, f.height))
        .unwrap_or_else(|| "-".to_owned());
    let file_size = meta
        .viewer_path
        .as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| format_viewer_bytes(m.len()))
        .unwrap_or_else(|| "-".to_owned());

    scrollable(
        container(
            column![
                kv_row(tr(lang, "label-format"), format_str),
                kv_row(tr(lang, "label-dimensions"), dims),
                kv_row(tr(lang, "label-file-size"), file_size),
                kv_row(tr(lang, "label-path"), folder_name),
                with_tooltip_pos(
                    button::labeled(
                        tr(lang, "action-copy-path"),
                        Some(Icon::Copy),
                        ButtonVariant::Secondary,
                        meta.viewer_path
                            .as_ref()
                            .map(|_| ViewerMessage::CopyViewerPath),
                    )
                    .width(Fill),
                    tr(lang, "action-copy-path"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(8)
            .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }),
    )
    .height(Fill)
    .into()
}

fn adj_slider_row<'a>(
    label: String,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    on_change: impl Fn(f32) -> ViewerMessage + 'a,
) -> Element<'a, ViewerMessage> {
    photon_ui::panel::slider_control_row(
        label,
        value,
        range,
        step,
        format!("{value:.2}"),
        on_change,
    )
}

fn build_viewer_adjustments_body<'a>(
    state: ViewerViewState<'a>,
    lang: Language,
    right_w: f32,
) -> Element<'a, ViewerMessage> {
    let adj = &state.viewer_adj;
    let has_image = state.snapshot.meta.viewer_path.is_some();

    let sliders = column![
        adj_slider_row(
            tr(lang, "label-exposure"),
            adj.exposure,
            -2.0..=2.0,
            0.05,
            ViewerMessage::SetViewerAdjExposure,
        ),
        adj_slider_row(
            tr(lang, "label-gamma"),
            adj.gamma,
            0.2..=3.0,
            0.05,
            ViewerMessage::SetViewerAdjGamma,
        ),
        adj_slider_row(
            tr(lang, "label-highlights"),
            adj.highlights,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjHighlights,
        ),
        adj_slider_row(
            tr(lang, "label-shadows"),
            adj.shadows,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjShadows,
        ),
        adj_slider_row(
            tr(lang, "label-brightness"),
            adj.brightness,
            -0.5..=0.5,
            0.02,
            ViewerMessage::SetViewerAdjBrightness,
        ),
        adj_slider_row(
            tr(lang, "label-contrast"),
            adj.contrast,
            -0.5..=0.5,
            0.02,
            ViewerMessage::SetViewerAdjContrast,
        ),
        adj_slider_row(
            tr(lang, "label-saturation"),
            adj.saturation,
            -1.0..=1.0,
            0.05,
            ViewerMessage::SetViewerAdjSaturation,
        ),
    ]
    .spacing(6)
    .width(Fill);

    let actions: Element<'_, ViewerMessage> = if right_w >= 340.0 {
        row![
            button::labeled_tipped(
                tr(lang, "action-reset-adjustments"),
                Some(Icon::Reset),
                ButtonVariant::Ghost,
                Some(ViewerMessage::ResetViewerAdj),
                tr(lang, "action-reset-adjustments"),
            ),
            iced::widget::space::horizontal(),
            button::labeled_tipped(
                tr(lang, "action-open-with-adjustments"),
                Some(Icon::Editor),
                ButtonVariant::Primary,
                has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor),
                tr(lang, "tooltip-viewer-edit"),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        column![
            with_tooltip(
                button::labeled(
                    tr(lang, "action-reset-adjustments"),
                    Some(Icon::Reset),
                    ButtonVariant::Ghost,
                    Some(ViewerMessage::ResetViewerAdj),
                )
                .width(Fill),
                tr(lang, "action-reset-adjustments"),
            ),
            with_tooltip(
                button::labeled(
                    tr(lang, "action-open-with-adjustments"),
                    Some(Icon::Editor),
                    ButtonVariant::Primary,
                    has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor),
                )
                .width(Fill),
                tr(lang, "tooltip-viewer-edit"),
            ),
        ]
        .spacing(6)
        .width(Fill)
        .into()
    };

    column![
        scrollable(container(sliders).padding(Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }),)
        .height(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        actions,
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill)
    .into()
}
