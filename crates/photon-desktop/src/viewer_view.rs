//! Viewer workspace UI (`iced 0.14` + `photon-ui` + `photon-panels`):
//! Single, Double-page spread (LTR/RTL + cover split), and continuous Manhwa/Webtoon strip,
//! unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.

use std::collections::HashMap;
use std::path::Path;

use iced::widget::{
    Shader, column, container, image as iced_image, mouse_area, row, rule, scrollable, slider,
};
use iced::{Alignment, ContentFit, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, PanelSplitterEdge, kv_row, panel_splitter,
    sidebar_dock, sidebar_panel, stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{
    Command, Language, Theme, ViewerRightPanel, ViewerViewMode, WebtoonFitMode, Workspace,
};
use photon_ui::button::{self, ButtonVariant, IconButton};
use photon_ui::grid::{FilmstripItem, filmstrip_bar};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use photon_ui::theme::{
    canvas_backdrop_style, hairline_rule_style, slider_style, surface_panel_style,
};
use photon_ui::toolbar::{WorkspaceTab, toolbar, toolbar_button_group, workspace_switcher_compact};
use photon_ui::tooltip::{TooltipPosition, with_tooltip, with_tooltip_pos};

use crate::app::{
    ActiveMenu, Message, PhotonGui, WindowRole, toolbar_divider, toolbar_right_utilities,
};
use crate::shader_canvas::{CanvasEvent, CanvasTool, CpuImageFrame, ImageCanvasProgram};

pub(crate) type DualSpreadSlots<'a> = (Option<(usize, &'a str)>, Option<(usize, &'a str)>);

const VIEWER_RIGHT_MIN_W: f32 = 200.0;
const VIEWER_RIGHT_MAX_W: f32 = 600.0;
const VIEWER_RIGHT_DEFAULT_W: f32 = 290.0;
const VIEWER_SPLIT_DEFAULT_PCT: f32 = 58.0;
const VIEWER_SPLIT_MIN_PCT: f32 = 20.0;
const VIEWER_SPLIT_MAX_PCT: f32 = 80.0;

fn viewer_right_width(state: &PhotonGui) -> f32 {
    state
        .viewer_right_width_preview
        .unwrap_or(state.snapshot.meta.layout.right_width as f32)
        .clamp(VIEWER_RIGHT_MIN_W, VIEWER_RIGHT_MAX_W)
}

fn viewer_panel_is_open(state: &PhotonGui, panel: ViewerRightPanel) -> bool {
    match panel {
        ViewerRightPanel::ViewAdjustment => {
            state.viewer_adjustments_open && state.viewer_mode != ViewerViewMode::Webtoon
        }
        ViewerRightPanel::Info => state.viewer_info_open,
    }
}

fn format_viewer_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Image position counter shown in the Viewer status bar (`1 / 5` or `2·3 / 5`).
pub(crate) fn viewer_counter_summary(state: &PhotonGui) -> Option<String> {
    let meta = &state.snapshot.meta;
    let total = meta.viewer_count;
    if total == 0 {
        return None;
    }
    match state.viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total - 1);
            Some(format!("{} / {total}", active_idx + 1))
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            match (left_slot, right_slot) {
                (Some((li, _)), Some((ri, _))) => Some(format!("{}·{} / {total}", li + 1, ri + 1)),
                (Some((li, _)), None) => Some(format!("{} / {total}", li + 1)),
                _ => meta.viewer_index.map(|i| format!("{} / {total}", i + 1)),
            }
        }
        ViewerViewMode::Single => {
            let idx = meta.viewer_index.unwrap_or(0).min(total - 1);
            Some(format!("{} / {total}", idx + 1))
        }
    }
}

/// Summary text shown on the left side of the bottom status bar while in Viewer workspace.
pub(crate) fn viewer_status_summary(state: &PhotonGui, lang: Language) -> String {
    let meta = &state.snapshot.meta;
    let viewer_mode = state.viewer_mode;
    let idx = meta.viewer_index.unwrap_or(0);
    let total = meta.viewer_count;
    let name_at = |i: usize| -> Option<String> {
        state
            .snapshot
            .viewer_images
            .get(i)
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(ToOwned::to_owned)
    };
    match viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total.saturating_sub(1));
            let mode_name = tr(lang, "action-viewer-manhwa");
            if let Some(name) = name_at(active_idx) {
                if let Some((w, h)) = state
                    .snapshot
                    .viewer_images
                    .get(active_idx)
                    .and_then(|p| state.image_sizes.get(p))
                {
                    format!("{mode_name} · {name} · {w}×{h}")
                } else {
                    format!("{mode_name} · {name}")
                }
            } else {
                mode_name
            }
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            let dims_str = |path: &str, frame: &Option<CpuImageFrame>| -> Option<String> {
                if let Some(f) = frame {
                    Some(format!("{}×{}", f.width, f.height))
                } else {
                    state.image_sizes.get(path).map(|(w, h)| format!("{w}×{h}"))
                }
            };
            match (left_slot, right_slot) {
                (Some((_, lp)), Some((_, rp))) => {
                    let lname = left_slot
                        .and_then(|(li, _)| name_at(li))
                        .unwrap_or_default();
                    let rname = right_slot
                        .and_then(|(ri, _)| name_at(ri))
                        .unwrap_or_default();
                    let ldims = dims_str(lp, &state.viewer_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    let rdims = dims_str(rp, &state.viewer_pair_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    format!("{lname}{ldims}  |  {rname}{rdims}")
                }
                (Some((li, lp)), None) => {
                    let lname = name_at(li).unwrap_or_default();
                    if let Some(d) = dims_str(lp, &state.viewer_frame) {
                        format!("{lname} · {d}")
                    } else {
                        lname
                    }
                }
                _ => tr(lang, "ready"),
            }
        }
        ViewerViewMode::Single => match (name_at(idx), &state.viewer_frame) {
            (Some(name), Some(frame)) => format!("{name} · {}×{}", frame.width, frame.height),
            (Some(name), None) => name,
            _ => tr(lang, "ready"),
        },
    }
}

pub(crate) fn viewer_reader_options_right_offset(state: &PhotonGui, role: WindowRole) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels&Settings button
    if role != WindowRole::Main {
        return utilities_w + 8.0;
    }
    let compact = state.window_size.width < 1120.0;
    let switcher_w = if compact { 112.0 } else { 252.0 };
    // utilities_w + gap(8) + divider(1) + gap(8) + switcher_w + gap(8) + divider(1) + gap(8)
    utilities_w + 8.0 + 1.0 + 8.0 + switcher_w + 8.0 + 1.0 + 8.0
}

/// Top toolbar for the Viewer workspace:
/// Left: Navigation + Zoom/Fit/1:1 + Rotate + Fullscreen.
/// Right: View Mode Switcher (`Single`/`Double`/`Manhwa`) + Reader Options + Workspace Switcher + Panels & Settings.
pub(crate) fn build_viewer_toolbar(state: &PhotonGui, role: WindowRole) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let has_nav = meta.viewer_count > 1;
    let has_image = meta.viewer_path.is_some();
    let viewer_mode = state.viewer_mode;
    let is_webtoon = viewer_mode == ViewerViewMode::Webtoon;
    let is_double = viewer_mode == ViewerViewMode::Double;
    let compact_switcher = state.window_size.width < 1120.0;

    // Navigation (Prev / Next)
    let prev_msg = has_nav.then(|| {
        if is_double {
            Message::DualStepSpread(-1)
        } else {
            Message::Dispatch(Command::ViewerPrevious)
        }
    });
    let next_msg = has_nav.then(|| {
        if is_double {
            Message::DualStepSpread(1)
        } else {
            Message::Dispatch(Command::ViewerNext)
        }
    });

    let mut left = row![
        button::toolbar_icon(
            Icon::FolderOpen,
            false,
            Some(Message::PickImagesDialog),
            Some(tr(lang, "action-open-file-dialog")),
        ),
        toolbar_divider(),
        IconButton::new(Icon::Prev)
            .toolbar()
            .variant(ButtonVariant::Secondary)
            .maybe_on_press(prev_msg)
            .tooltip(tr(lang, "tooltip-viewer-prev")),
        IconButton::new(Icon::Next)
            .toolbar()
            .variant(ButtonVariant::Secondary)
            .maybe_on_press(next_msg)
            .tooltip(tr(lang, "tooltip-viewer-next")),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::ZoomOut,
            false,
            has_image.then_some(Message::ZoomStep(0.8)),
            Some(tr(lang, "tooltip-viewer-zoom-out")),
        ),
        button::toolbar_icon(
            Icon::ZoomIn,
            false,
            has_image.then_some(Message::ZoomStep(1.25)),
            Some(tr(lang, "tooltip-viewer-zoom-in")),
        ),
        IconButton::new(Icon::Fit)
            .toolbar()
            .variant(ButtonVariant::Secondary)
            .maybe_on_press(has_image.then_some(Message::Canvas(CanvasEvent::ResetFit)))
            .tooltip(tr(lang, "tooltip-viewer-fit"))
            .into_element(),
        button::toolbar_labeled(
            tr(lang, "action-actual-ratio"),
            None,
            ButtonVariant::Ghost,
            (has_image && !is_webtoon).then_some(Message::ViewerZoomActual),
            Some(tr(lang, "tooltip-viewer-actual-size")),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if !is_webtoon {
        left = left
            .push(toolbar_divider())
            .push(button::toolbar_icon(
                Icon::RotateLeft,
                false,
                has_image.then_some(Message::ViewerRotatePreview(-1)),
                Some(tr(lang, "tooltip-viewer-rotate-left")),
            ))
            .push(button::toolbar_icon(
                Icon::RotateRight,
                false,
                has_image.then_some(Message::ViewerRotatePreview(1)),
                Some(tr(lang, "tooltip-viewer-rotate-right")),
            ));
    }

    left = left.push(toolbar_divider()).push(button::toolbar_icon(
        if state.is_fullscreen {
            Icon::FullscreenExit
        } else {
            Icon::Fullscreen
        },
        state.is_fullscreen,
        Some(Message::ToggleFullscreen),
        Some(tr(lang, "tooltip-viewer-fullscreen")),
    ));

    // Right side: View Mode Switcher (Single | Double | Webtoon) + Reader Options + Workspace Switcher + Panels & Settings
    let mode_switcher = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::ViewSingle,
            viewer_mode == ViewerViewMode::Single,
            Some(Message::SetViewerViewMode(ViewerViewMode::Single)),
            Some(format!("{} (S)", tr(lang, "action-viewer-single"))),
        ),
        button::toolbar_icon(
            Icon::ViewDouble,
            viewer_mode == ViewerViewMode::Double,
            Some(Message::SetViewerViewMode(ViewerViewMode::Double)),
            Some(format!("{} (D)", tr(lang, "action-viewer-double"))),
        ),
        button::toolbar_icon(
            Icon::ViewManhwa,
            viewer_mode == ViewerViewMode::Webtoon,
            Some(Message::SetViewerViewMode(ViewerViewMode::Webtoon)),
            Some(format!("{} (M)", tr(lang, "action-viewer-manhwa"))),
        ),
    ]);
    let reader_opts_btn = button::toolbar_icon(
        Icon::Sliders,
        state.active_menu == Some(ActiveMenu::ReaderOptions),
        Some(Message::ToggleMenu(ActiveMenu::ReaderOptions)),
        Some(tr(lang, "action-reader-options")),
    );
    let view_mode_group = row![mode_switcher, reader_opts_btn]
        .spacing(4)
        .align_y(Alignment::Center);

    let right: Element<'_, Message> = if role == WindowRole::Main {
        let switcher = workspace_switcher_compact(
            vec![
                WorkspaceTab::new(
                    tr(lang, "workspace-browser"),
                    Icon::Browser,
                    false,
                    Message::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-browser")),
                WorkspaceTab::new(
                    tr(lang, "workspace-viewer"),
                    Icon::Viewer,
                    true,
                    Message::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-viewer")),
                WorkspaceTab::new(
                    tr(lang, "workspace-editor"),
                    Icon::Editor,
                    false,
                    Message::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Editor,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-editor")),
            ],
            compact_switcher,
        );
        row![
            view_mode_group,
            toolbar_divider(),
            switcher,
            toolbar_divider(),
            toolbar_right_utilities(state)
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            view_mode_group,
            toolbar_divider(),
            toolbar_right_utilities(state)
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };

    toolbar(left, right)
}

/// Main Viewer workspace view: Single/Double/Manhwa viewport,
/// unified resizable/reorderable right sidebar (`ViewAdjustment` & `Info`), and bottom Filmstrip bar.
pub(crate) fn build_viewer_view(state: &PhotonGui) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let dark = state.snapshot.meta.theme == Theme::Dark;
    let viewer_mode = state.viewer_mode;

    let viewport: Element<'_, Message> = match viewer_mode {
        ViewerViewMode::Single => {
            let canvas: Element<'_, Message> = Element::from(
                Shader::new(ImageCanvasProgram {
                    frame: state.viewer_frame.clone(),
                    pair_frame: None,
                    dual_gap_px: 0.0,
                    compare_frame: None,
                    zoom: state.zoom,
                    pan_x: state.pan_x,
                    pan_y: state.pan_y,
                    adjustments: state.viewer_adj,
                    dark_theme: dark,
                    crop_rect: None,
                    crop_aspect: None,
                    quarter_turns: state.viewer_quarter_turns,
                    tool: CanvasTool::Pointer,
                    preview_angle_rad: 0.0,
                    preview_fill: [0.0; 4],
                    canvas_bg: None,
                    viewport_bg: None,
                    compare_mode: 0,
                    compare_split_x: 0.5,
                    brush_radius_px: 0.0,
                })
                .width(Fill)
                .height(Fill),
            )
            .map(|ev| Message::ViewerCanvas(None, ev));

            container(canvas)
                .width(Fill)
                .height(Fill)
                .style(canvas_backdrop_style)
                .into()
        }
        ViewerViewMode::Double => build_viewer_double_panes(state, lang, dark),
        ViewerViewMode::Webtoon => build_viewer_webtoon_strip(state),
    };

    let open_panels: Vec<ViewerRightPanel> = state
        .viewer_right_panels
        .order()
        .iter()
        .copied()
        .filter(|&p| viewer_panel_is_open(state, p))
        .collect();

    let main_area: Element<'_, Message> = if !open_panels.is_empty() {
        let right_w = viewer_right_width(state);
        let splitter = panel_splitter(
            right_w,
            PanelSplitterEdge::Right.direction(),
            VIEWER_RIGHT_MIN_W,
            VIEWER_RIGHT_MAX_W,
            VIEWER_RIGHT_DEFAULT_W,
            Message::ViewerSplitterDrag,
            Message::ViewerSplitterRelease,
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
        let items: Vec<FilmstripItem<Message>> = state
            .snapshot
            .viewer_images
            .iter()
            .map(|path| FilmstripItem {
                thumbnail: state.thumbnails.get(path).cloned(),
                active: active_path == Some(path.as_str()),
                on_press: Message::Dispatch(Command::ViewerOpen { path: path.clone() }),
            })
            .collect();
        let close_btn = with_tooltip_pos(
            button::compact_icon(
                Icon::Close,
                ButtonVariant::Ghost,
                Some(Message::CloseFilmstrip),
            ),
            tr(lang, "tooltip-panel-close"),
            TooltipPosition::Left,
        );
        let filmstrip_row = container(
            row![
                container(filmstrip_bar(items)).width(Fill),
                container(close_btn).padding([4, 6]),
            ]
            .align_y(Alignment::Center)
            .width(Fill),
        )
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

fn build_viewer_right_sidebar<'a>(
    state: &'a PhotonGui,
    lang: Language,
    right_w: f32,
    open_panels: &[ViewerRightPanel],
) -> Element<'a, Message> {
    let dragging_any = state.viewer_right_panels.is_dragging();
    let can_swap = open_panels.len() >= 2;
    let (top_h, bottom_h) = stacked_panel_heights(
        false,
        false,
        VIEWER_SPLIT_DEFAULT_PCT,
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
        };

        let header = DraggablePanelHeaderSpec::new(title)
            .icon(icon)
            .collapsed(false)
            .dragging(state.viewer_right_panels.is_dragging_panel(panel_kind))
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .swap_tooltip(tr(lang, "tooltip-panel-drag-reorder"))
            .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
            .close_tooltip(tr(lang, "tooltip-panel-close"))
            .on_drag_start(Message::BeginDragViewerPanel(panel_kind))
            .on_drag_hover(dragging_any.then_some(Message::HoverViewerPanelDrop(panel_kind)))
            .on_drag_end(dragging_any.then_some(Message::DropViewerPanel(panel_kind)))
            .on_swap(can_swap.then_some(Message::SwapViewerRightPanels))
            .on_toggle_collapse(Message::ToggleViewerPanelCollapse(panel_kind))
            .on_close(Some(Message::CloseViewerPanel(panel_kind)));

        let panel_el = sidebar_panel(
            PanelFrameSpec::new(header)
                .body(body)
                .fill_height(true)
                .body_padding(Padding::from([2, 2]))
                .drop_indicator(state.viewer_right_panels.drop_indicator(panel_kind))
                .on_drop_hover(dragging_any.then_some(Message::HoverViewerPanelDrop(panel_kind)))
                .on_drop_release(dragging_any.then_some(Message::DropViewerPanel(panel_kind))),
        );

        let slot_height = if can_swap {
            if idx == 0 { top_h } else { bottom_h }
        } else {
            Length::Fill
        };

        stacked = stacked.push(container(panel_el).width(Fill).height(slot_height));

        if idx == 0 && can_swap {
            let height_hint = (state.window_size.height - 120.0).max(260.0);
            stacked = stacked.push(vertical_panel_splitter(
                VIEWER_SPLIT_DEFAULT_PCT,
                height_hint,
                VIEWER_SPLIT_MIN_PCT,
                VIEWER_SPLIT_MAX_PCT,
                VIEWER_SPLIT_DEFAULT_PCT,
                |_| Message::Noop,
                |_| Message::Noop,
            ));
        }
    }

    sidebar_dock(right_w, stacked)
}

fn build_viewer_info_body<'a>(state: &'a PhotonGui, lang: Language) -> Element<'a, Message> {
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
                    meta.viewer_path.as_ref().map(|_| Message::CopyViewerPath),
                )
                .width(Fill),
                tr(lang, "action-copy-path"),
                TooltipPosition::Top,
            ),
        ]
        .spacing(8)
        .width(Fill),
    )
    .height(Fill)
    .into()
}

fn adj_slider_row<'a>(
    label: String,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        kv_row(label, format!("{value:.2}")),
        slider(range, value, on_change)
            .step(step)
            .style(slider_style),
    ]
    .spacing(3)
    .into()
}

fn build_viewer_adjustments_body<'a>(
    state: &'a PhotonGui,
    lang: Language,
    right_w: f32,
) -> Element<'a, Message> {
    let adj = &state.viewer_adj;
    let has_image = state.snapshot.meta.viewer_path.is_some();

    let sliders = column![
        adj_slider_row(
            tr(lang, "label-exposure"),
            adj.exposure,
            -2.0..=2.0,
            0.05,
            Message::SetViewerAdjExposure,
        ),
        adj_slider_row(
            tr(lang, "label-gamma"),
            adj.gamma,
            0.2..=3.0,
            0.05,
            Message::SetViewerAdjGamma,
        ),
        adj_slider_row(
            tr(lang, "label-highlights"),
            adj.highlights,
            -1.0..=1.0,
            0.05,
            Message::SetViewerAdjHighlights,
        ),
        adj_slider_row(
            tr(lang, "label-shadows"),
            adj.shadows,
            -1.0..=1.0,
            0.05,
            Message::SetViewerAdjShadows,
        ),
        adj_slider_row(
            tr(lang, "label-brightness"),
            adj.brightness,
            -0.5..=0.5,
            0.02,
            Message::SetViewerAdjBrightness,
        ),
        adj_slider_row(
            tr(lang, "label-contrast"),
            adj.contrast,
            -0.5..=0.5,
            0.02,
            Message::SetViewerAdjContrast,
        ),
        adj_slider_row(
            tr(lang, "label-saturation"),
            adj.saturation,
            -1.0..=1.0,
            0.05,
            Message::SetViewerAdjSaturation,
        ),
    ]
    .spacing(6)
    .width(Fill);

    let actions: Element<'_, Message> = if right_w >= 340.0 {
        row![
            button::labeled_tipped(
                tr(lang, "action-reset-adjustments"),
                Some(Icon::Reset),
                ButtonVariant::Ghost,
                Some(Message::ResetViewerAdj),
                tr(lang, "action-reset-adjustments"),
            ),
            iced::widget::space::horizontal(),
            button::labeled_tipped(
                tr(lang, "action-open-with-adjustments"),
                Some(Icon::Editor),
                ButtonVariant::Primary,
                has_image.then_some(Message::ViewerAdjOpenInEditor),
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
                    Some(Message::ResetViewerAdj),
                )
                .width(Fill),
                tr(lang, "action-reset-adjustments"),
            ),
            with_tooltip(
                button::labeled(
                    tr(lang, "action-open-with-adjustments"),
                    Some(Icon::Editor),
                    ButtonVariant::Primary,
                    has_image.then_some(Message::ViewerAdjOpenInEditor),
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
        scrollable(sliders).height(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        actions,
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill)
    .into()
}

fn build_viewer_double_panes(
    state: &PhotonGui,
    _lang: Language,
    dark: bool,
) -> Element<'_, Message> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (left_slot, right_slot) = dual_spread_slots(state);
    let left_path = left_slot.map(|(_, p)| p.to_owned());
    let right_path = right_slot.map(|(_, p)| p.to_owned());

    let canvas: Element<'_, Message> = Element::from(
        Shader::new(ImageCanvasProgram {
            frame: state.viewer_frame.clone(),
            pair_frame: state.viewer_pair_frame.clone(),
            dual_gap_px: gap,
            compare_frame: None,
            zoom: state.zoom,
            pan_x: state.pan_x,
            pan_y: state.pan_y,
            adjustments: state.viewer_adj,
            dark_theme: dark,
            crop_rect: None,
            crop_aspect: None,
            quarter_turns: state.viewer_quarter_turns,
            tool: CanvasTool::Pointer,
            preview_angle_rad: 0.0,
            preview_fill: [0.0; 4],
            canvas_bg: None,
            viewport_bg: None,
            compare_mode: 0,
            compare_split_x: 0.5,
            brush_radius_px: 0.0,
        })
        .width(Fill)
        .height(Fill),
    )
    .map(move |ev| match ev {
        CanvasEvent::ContextMenuRequested { is_right_pair, .. } => {
            let target = if is_right_pair {
                right_path.clone().or_else(|| left_path.clone())
            } else {
                left_path.clone()
            };
            Message::ViewerCanvas(target, ev)
        }
        other => Message::ViewerCanvas(None, other),
    });

    container(canvas)
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style)
        .into()
}

pub(crate) fn webtoon_scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("viewer_webtoon_strip")
}

pub(crate) fn compute_webtoon_layout(state: &PhotonGui) -> (f32, Vec<(String, f32, f32)>) {
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

pub(crate) fn webtoon_offset_for_index(state: &PhotonGui, target: usize) -> Option<f32> {
    let (_, pages) = compute_webtoon_layout(state);
    if target >= pages.len() {
        return None;
    }
    let gap = f32::from(state.viewer_manga_page_gap);
    let y: f32 = pages.iter().take(target).map(|(_, _, h)| *h + gap).sum();
    Some(y)
}

pub(crate) fn webtoon_index_at_scroll(state: &PhotonGui, center_y: f32) -> Option<usize> {
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

fn build_viewer_webtoon_strip(state: &PhotonGui) -> Element<'_, Message> {
    let gap = f32::from(state.viewer_manga_page_gap);
    let (_, pages) = compute_webtoon_layout(state);
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
        let page_el: Element<'_, Message> = if let Some(hnd) = handle {
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
            container(photon_ui::icons::render(Icon::Image, 28.0, None))
                .width(Length::Fixed(w))
                .height(Length::Fixed(h))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(surface_panel_style)
                .into()
        };
        let wrapped_page = mouse_area(page_el).on_right_press(Message::OpenViewerContextMenu {
            path: Some(path),
            pos: None,
        });
        col = col.push(wrapped_page);
    }

    mouse_area(
        container(
            scrollable(container(col).padding([8, 0]).width(Fill))
                .id(webtoon_scroll_id())
                .on_scroll(Message::WebtoonScrolled)
                .width(Fill)
                .height(Fill),
        )
        .width(Fill)
        .height(Fill)
        .style(canvas_backdrop_style),
    )
    .on_right_press(Message::OpenViewerContextMenu {
        path: None,
        pos: None,
    })
    .into()
}

pub(crate) fn compute_dual_spreads(
    images: &[String],
    sizes: &HashMap<String, (u32, u32)>,
    separate_cover: bool,
) -> Vec<Vec<usize>> {
    let count = images.len();
    if count == 0 {
        return Vec::new();
    }
    let is_landscape = |idx: usize| -> bool {
        images
            .get(idx)
            .and_then(|p| sizes.get(p))
            .is_some_and(|(w, h)| *w > 0 && *h > 0 && *w > *h)
    };
    let mut spreads = Vec::new();
    let mut i = 0;
    if separate_cover && count > 1 {
        spreads.push(vec![0]);
        i = 1;
    }
    while i < count {
        if is_landscape(i) || i + 1 >= count || is_landscape(i + 1) {
            spreads.push(vec![i]);
            i += 1;
        } else {
            spreads.push(vec![i, i + 1]);
            i += 2;
        }
    }
    spreads
}

pub(crate) fn dual_spread_slots_for<'a>(
    images: &'a [String],
    sizes: &HashMap<String, (u32, u32)>,
    current_index: Option<usize>,
    separate_cover: bool,
    manga_rtl: bool,
) -> DualSpreadSlots<'a> {
    let spreads = compute_dual_spreads(images, sizes, separate_cover);
    if spreads.is_empty() {
        return (None, None);
    }
    let idx = current_index.unwrap_or(0);
    let spread = spreads
        .iter()
        .find(|sp| sp.contains(&idx))
        .unwrap_or(&spreads[0]);
    if spread.len() == 1 {
        let i = spread[0];
        return (images.get(i).map(|p| (i, p.as_str())), None);
    }
    let (li, ri) = if manga_rtl {
        (spread[1], spread[0])
    } else {
        (spread[0], spread[1])
    };
    (
        images.get(li).map(|p| (li, p.as_str())),
        images.get(ri).map(|p| (ri, p.as_str())),
    )
}

pub(crate) fn dual_spread_slots(state: &PhotonGui) -> DualSpreadSlots<'_> {
    dual_spread_slots_for(
        &state.snapshot.viewer_images,
        &state.image_sizes,
        state.snapshot.meta.viewer_index,
        state.viewer_manga_cover_separate,
        state.viewer_manga_rtl,
    )
}

pub(crate) fn step_dual_spread(state: &PhotonGui, delta: isize) -> Option<String> {
    let images = &state.snapshot.viewer_images;
    let spreads = compute_dual_spreads(
        images,
        &state.image_sizes,
        state.viewer_manga_cover_separate,
    );
    if spreads.is_empty() {
        return None;
    }
    let cur_img = state.snapshot.meta.viewer_index.unwrap_or(0);
    let cur_sp = spreads
        .iter()
        .position(|sp| sp.contains(&cur_img))
        .unwrap_or(0) as isize;
    let next_sp = (cur_sp + delta).rem_euclid(spreads.len() as isize) as usize;
    let target_idx = spreads[next_sp][0];
    images.get(target_idx).cloned()
}

/// Right-click context menu for Viewer canvas & pages.
pub(crate) fn build_viewer_context_menu<'a>(
    state: &'a PhotonGui,
    target_path: Option<&str>,
) -> Element<'a, Message> {
    let lang = state.snapshot.meta.language;
    let resolved_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let has_path = resolved_path.is_some();
    let vmode = state.viewer_mode;
    let is_webtoon = vmode == ViewerViewMode::Webtoon;

    let edit_msg = resolved_path
        .as_ref()
        .map(|p| Message::ContextMenuDispatch(Command::EditorOpen { path: p.clone() }))
        .unwrap_or(Message::Noop);
    let copy_img_msg = resolved_path
        .as_ref()
        .map(|p| Message::CopyImageToClipboard(Some(p.clone())))
        .unwrap_or(Message::Noop);
    let copy_path_msg = resolved_path
        .as_ref()
        .map(|_| Message::CopyViewerPath)
        .unwrap_or(Message::Noop);
    let reveal_msg = resolved_path
        .map(Message::RevealInFolder)
        .unwrap_or(Message::Noop);

    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-edit"), edit_msg)
                .icon(Icon::Editor)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-image"), copy_img_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Shift+C")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-path"), copy_path_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Alt+C")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-reveal-folder"), reveal_msg)
                .icon(Icon::FolderOpen)
                .enabled(has_path),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fit"),
                Message::Canvas(CanvasEvent::ResetFit),
            )
            .icon(Icon::Fit)
            .shortcut("F")
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-actual-size"), Message::ViewerZoomActual)
                .icon(Icon::ZoomIn)
                .shortcut("1")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-in"), Message::ZoomStep(1.25))
                .icon(Icon::ZoomIn)
                .shortcut("+")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-out"), Message::ZoomStep(0.8))
                .icon(Icon::ZoomOut)
                .shortcut("-")
                .enabled(has_path),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-left"),
                Message::ViewerRotatePreview(-1),
            )
            .icon(Icon::RotateLeft)
            .shortcut("Shift+R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-right"),
                Message::ViewerRotatePreview(1),
            )
            .icon(Icon::RotateRight)
            .shortcut("R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-single"),
                Message::SetViewerViewMode(ViewerViewMode::Single),
            )
            .icon(Icon::ViewSingle)
            .shortcut("S")
            .checked(vmode == ViewerViewMode::Single),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-double"),
                Message::SetViewerViewMode(ViewerViewMode::Double),
            )
            .icon(Icon::ViewDouble)
            .shortcut("D")
            .checked(vmode == ViewerViewMode::Double),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-manhwa"),
                Message::SetViewerViewMode(ViewerViewMode::Webtoon),
            )
            .icon(Icon::ViewManhwa)
            .shortcut("M")
            .checked(vmode == ViewerViewMode::Webtoon),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-viewer-color"),
                Message::ToggleViewerColorPopover,
            )
            .icon(Icon::Palette)
            .checked(state.viewer_adjustments_open)
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-filmstrip"), Message::ToggleFilmstrip)
                .icon(Icon::Filmstrip)
                .checked(state.viewer_filmstrip_open),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-fullscreen"), Message::ToggleFullscreen)
                .icon(if state.is_fullscreen {
                    Icon::FullscreenExit
                } else {
                    Icon::Fullscreen
                })
                .shortcut("F11")
                .checked(state.is_fullscreen),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-info"), Message::ToggleViewerInspector)
                .icon(Icon::Info)
                .shortcut("I")
                .checked(state.viewer_info_open),
        ),
    ];

    menu_panel_entries(entries, 225.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_dual_spreads_respects_separate_cover_and_landscape() {
        let imgs = vec![
            "/tmp/01.png".to_owned(),
            "/tmp/02.png".to_owned(),
            "/tmp/03.png".to_owned(),
            "/tmp/04.png".to_owned(),
            "/tmp/05.png".to_owned(),
        ];
        let mut sizes = HashMap::new();
        sizes.insert("/tmp/01.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/02.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/03.png".to_owned(), (800, 1200));
        sizes.insert("/tmp/04.png".to_owned(), (1600, 900));
        sizes.insert("/tmp/05.png".to_owned(), (800, 1200));

        let spreads = compute_dual_spreads(&imgs, &sizes, true);
        assert_eq!(spreads, vec![vec![0], vec![1, 2], vec![3], vec![4]]);

        let (left, right) = dual_spread_slots_for(&imgs, &sizes, Some(1), true, true);
        assert_eq!(left, Some((2, "/tmp/03.png")));
        assert_eq!(right, Some((1, "/tmp/02.png")));
    }
}
