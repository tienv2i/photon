use std::path::Path;

use iced::widget::{column, container, image as iced_image, mouse_area, row, scrollable};
use iced::{Alignment, ContentFit, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, kv_row, sidebar_dock, sidebar_panel,
    stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{BrowserLeftPanel, BrowserRightPanel, Command, MoveDirection};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::icons::Icon;
use photon_ui::theme::{PaletteTokens, RADIUS_SM, TEXT_SIZE_CONTROL};
use photon_ui::tooltip::with_tooltip;
use photon_ui::typography::text;

use crate::model::{format_bytes, format_modified_ms, is_archive_path_str};
use crate::state::{BrowserMessage, BrowserViewState};

pub(crate) fn build_browser_left_sidebar(
    state: BrowserViewState<'_>,
    left_w: f32,
) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let show_quick_access = state.browser_quick_access_open;
    let show_explorer = state.browser_explorer_open;
    let dragging_any = state.browser_left_panels.is_dragging();
    let explorer_fills = show_explorer && !state.browser_explorer_collapsed;

    let mut stacked = column![].spacing(8).width(Fill).height(Fill);

    for &panel in state.browser_left_panels.order() {
        let (visible, collapsed, title, icon, fill_height, body) = match panel {
            BrowserLeftPanel::QuickAccess => {
                if !show_quick_access {
                    continue;
                }
                (
                    show_quick_access,
                    state.browser_quick_access_collapsed,
                    tr(lang, "panel-quick-access"),
                    Icon::Star,
                    false,
                    build_quick_access_body(state),
                )
            }
            BrowserLeftPanel::Explorer => {
                if !show_explorer {
                    continue;
                }
                (
                    show_explorer,
                    state.browser_explorer_collapsed,
                    tr(lang, "panel-explorer"),
                    Icon::Folder,
                    true,
                    build_explorer_tree(state),
                )
            }
        };
        if !visible {
            continue;
        }

        let mut header = DraggablePanelHeaderSpec::new(title)
            .icon(icon)
            .collapsed(collapsed)
            .dragging(state.browser_left_panels.is_dragging_panel(panel))
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
            .close_tooltip(tr(lang, "tooltip-panel-close"))
            .on_drag_start(BrowserMessage::BeginDragBrowserPanel(panel))
            .on_drag_hover(dragging_any.then_some(BrowserMessage::HoverBrowserPanelDrop(panel)))
            .on_drag_end(dragging_any.then_some(BrowserMessage::DropBrowserPanel(panel)))
            .on_toggle_collapse(BrowserMessage::ToggleBrowserLeftPanelCollapse(panel))
            .on_close(Some(BrowserMessage::CloseBrowserLeftPanel(panel)));

        if panel == BrowserLeftPanel::QuickAccess {
            header = header
                .options_icon(Icon::Editor)
                .options_active(state.quick_access_editing)
                .options_tooltip(if state.quick_access_editing {
                    tr(lang, "action-done-editing")
                } else {
                    tr(lang, "action-edit-quick-access")
                })
                .on_options(Some(BrowserMessage::ToggleQuickAccessEditing));
        }

        stacked = stacked.push(sidebar_panel(
            PanelFrameSpec::new(header)
                .body(body)
                .fill_height(fill_height)
                .body_padding([2, 2])
                .drop_indicator(state.browser_left_panels.drop_indicator(panel))
                .on_drop_hover(dragging_any.then_some(BrowserMessage::HoverBrowserPanelDrop(panel)))
                .on_drop_release(dragging_any.then_some(BrowserMessage::DropBrowserPanel(panel))),
        ));
    }

    if !explorer_fills {
        stacked = stacked.push(iced::widget::space::vertical());
    }

    sidebar_dock(left_w, stacked)
}

fn build_quick_access_body(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let current = state.snapshot.meta.current_folder.as_deref();
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(140.0, 220.0));

    let mut quick_col = column![].spacing(4).width(Fill);

    let fav_len = state.snapshot.meta.favorites.len();
    for (idx, fav_path) in state.snapshot.meta.favorites.iter().enumerate() {
        let label = Path::new(fav_path)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(fav_path.as_str())
            .to_owned();
        let active = current == Some(fav_path.as_str());
        let mut row_btn = row![button::list_row(
            Icon::Folder,
            label,
            None,
            active,
            Some(BrowserMessage::Dispatch(Command::OpenFolder {
                path: fav_path.clone(),
            })),
        )]
        .spacing(1)
        .width(Fill)
        .align_y(Alignment::Center);

        if state.quick_access_editing {
            row_btn = row_btn
                .push(button::compact_icon_tipped(
                    Icon::ArrowUp,
                    ButtonVariant::Ghost,
                    (idx > 0).then(|| {
                        BrowserMessage::Dispatch(Command::MoveFavorite {
                            path: fav_path.clone(),
                            direction: MoveDirection::Up,
                        })
                    }),
                    tr(lang, "action-move-up"),
                ))
                .push(button::compact_icon_tipped(
                    Icon::ArrowDown,
                    ButtonVariant::Ghost,
                    (idx + 1 < fav_len).then(|| {
                        BrowserMessage::Dispatch(Command::MoveFavorite {
                            path: fav_path.clone(),
                            direction: MoveDirection::Down,
                        })
                    }),
                    tr(lang, "action-move-down"),
                ))
                .push(button::compact_icon_tipped(
                    Icon::Trash,
                    ButtonVariant::Ghost,
                    Some(BrowserMessage::Dispatch(Command::RemoveFavorite {
                        path: fav_path.clone(),
                    })),
                    tr(lang, "action-unpin-quick-access"),
                ));
        }

        quick_col = quick_col.push(mouse_area(row_btn).on_right_press(
            BrowserMessage::OpenQuickAccessContextMenu {
                path: fav_path.clone(),
                x: cursor_pt.x,
                y: cursor_pt.y,
            },
        ));
    }

    container(scrollable(quick_col).width(Fill).height(Length::Shrink))
        .max_height(220.0)
        .width(Fill)
        .into()
}

pub(crate) fn browser_right_panel_is_open(
    state: BrowserViewState<'_>,
    panel: BrowserRightPanel,
) -> bool {
    match panel {
        BrowserRightPanel::Preview => state.browser_preview_open,
        BrowserRightPanel::Info => state.browser_info_open,
    }
}

pub(crate) fn build_browser_right_sidebar<'a>(
    state: BrowserViewState<'a>,
    right_w: f32,
    open_panels: &[BrowserRightPanel],
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let dragging_any = state.browser_right_panels.is_dragging();
    let can_swap = open_panels.len() >= 2;
    let preview_collapsed = state.browser_preview_collapsed;
    let info_collapsed = state.browser_info_collapsed;
    let (top_collapsed, bottom_collapsed) = if open_panels.len() >= 2 {
        (
            match open_panels[0] {
                BrowserRightPanel::Preview => preview_collapsed,
                BrowserRightPanel::Info => info_collapsed,
            },
            match open_panels[1] {
                BrowserRightPanel::Preview => preview_collapsed,
                BrowserRightPanel::Info => info_collapsed,
            },
        )
    } else {
        (false, false)
    };
    let (top_h, bottom_h) =
        stacked_panel_heights(top_collapsed, bottom_collapsed, 55.0, 22.0, 82.0);

    let mut stacked = column![].spacing(6).width(Fill).height(Fill);
    for (idx, &panel_kind) in open_panels.iter().enumerate() {
        let (collapsed, title, icon, body) = match panel_kind {
            BrowserRightPanel::Preview => (
                preview_collapsed,
                tr(lang, "panel-preview"),
                Icon::Image,
                build_browser_preview_body(state),
            ),
            BrowserRightPanel::Info => (
                info_collapsed,
                tr(lang, "panel-info"),
                Icon::Info,
                build_browser_info_body(state),
            ),
        };

        let header = DraggablePanelHeaderSpec::new(title)
            .icon(icon)
            .collapsed(collapsed)
            .dragging(state.browser_right_panels.is_dragging_panel(panel_kind))
            .drag_tooltip(tr(lang, "tooltip-sidebar-panel-drag"))
            .collapse_tooltip(tr(lang, "tooltip-panel-collapse"))
            .close_tooltip(tr(lang, "tooltip-panel-close"))
            .on_drag_start(BrowserMessage::BeginDragBrowserRightPanel(panel_kind))
            .on_drag_hover(
                dragging_any.then_some(BrowserMessage::HoverBrowserRightPanelDrop(panel_kind)),
            )
            .on_drag_end(dragging_any.then_some(BrowserMessage::DropBrowserRightPanel(panel_kind)))
            .on_toggle_collapse(BrowserMessage::ToggleBrowserRightPanelCollapse(panel_kind))
            .on_close(Some(BrowserMessage::CloseBrowserRightPanel(panel_kind)));

        let panel_el = sidebar_panel(
            PanelFrameSpec::new(header)
                .body(body)
                .fill_height(!collapsed)
                .body_padding(Padding::from([4, 4]))
                .drop_indicator(state.browser_right_panels.drop_indicator(panel_kind))
                .on_drop_hover(
                    dragging_any.then_some(BrowserMessage::HoverBrowserRightPanelDrop(panel_kind)),
                )
                .on_drop_release(
                    dragging_any.then_some(BrowserMessage::DropBrowserRightPanel(panel_kind)),
                ),
        );

        let slot_height = if collapsed {
            Length::Shrink
        } else if can_swap {
            if idx == 0 { top_h } else { bottom_h }
        } else {
            Length::Fill
        };

        stacked = stacked.push(container(panel_el).width(Fill).height(slot_height));

        if idx == 0 && can_swap && !top_collapsed && !bottom_collapsed {
            let height_hint = (state.window_size.height - 120.0).max(260.0);
            stacked = stacked.push(vertical_panel_splitter(
                55.0,
                height_hint,
                22.0,
                82.0,
                55.0,
                |_| BrowserMessage::Noop,
                |_| BrowserMessage::Noop,
            ));
        }
    }

    if can_swap && top_collapsed && bottom_collapsed {
        stacked = stacked.push(iced::widget::space::vertical());
    }

    sidebar_dock(right_w, stacked)
}

fn build_browser_preview_body(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let Some(sel) = state.snapshot.meta.selection_info.as_ref() else {
        return container(
            column![
                photon_ui::icons::render(Icon::Image, 24.0, None),
                text(tr(lang, "browser-detail-empty"))
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .padding([24, 10])
        .center_x(Fill)
        .center_y(Fill)
        .into();
    };

    let mut preview_col = column![].spacing(10).width(Fill);
    if sel.is_image {
        let media: Element<'_, BrowserMessage> =
            if let Some(handle) = state.thumbnails.get(&sel.path).cloned() {
                iced_image(handle)
                    .width(Fill)
                    .height(Fill)
                    .content_fit(ContentFit::Contain)
                    .into()
            } else {
                photon_ui::icons::render(Icon::Image, 32.0, None).into()
            };
        preview_col = preview_col.push(
            container(media)
                .width(Fill)
                .height(Length::Fixed(180.0))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    container::Style {
                        background: Some(t.bg_app.into()),
                        border: iced::Border {
                            radius: RADIUS_SM.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }),
        );
    } else {
        let icon = if sel.kind == "folder" {
            Icon::Folder
        } else if is_archive_path_str(&sel.path) {
            Icon::BookOpen
        } else {
            Icon::Info
        };
        preview_col = preview_col.push(
            container(photon_ui::icons::render(icon, 36.0, None))
                .width(Fill)
                .height(Length::Fixed(110.0))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    container::Style {
                        background: Some(t.bg_app.into()),
                        border: iced::Border {
                            radius: RADIUS_SM.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                }),
        );
    }

    preview_col = preview_col.push(
        text(sel.name.clone())
            .size(TEXT_SIZE_CONTROL)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_primary),
                }
            }),
    );

    if sel.is_image {
        preview_col = preview_col.push(
            row![
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-view"),
                        Some(Icon::Viewer),
                        ButtonVariant::Primary,
                        Some(BrowserMessage::OpenInViewerWindow(sel.path.clone())),
                    )
                    .width(Fill),
                    tr(lang, "action-view"),
                ),
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-edit"),
                        Some(Icon::Editor),
                        ButtonVariant::Secondary,
                        Some(BrowserMessage::EditPath(sel.path.clone())),
                    ),
                    tr(lang, "tooltip-browser-edit"),
                ),
            ]
            .spacing(6),
        );
    } else if sel.kind == "folder" {
        preview_col = preview_col.push(with_tooltip(
            button::labeled(
                tr(lang, "action-open-folder"),
                Some(Icon::FolderOpen),
                ButtonVariant::Primary,
                Some(BrowserMessage::Dispatch(Command::Navigate {
                    path: sel.path.clone(),
                })),
            )
            .width(Fill),
            tr(lang, "action-open-folder"),
        ));
    } else if is_archive_path_str(&sel.path) {
        preview_col = preview_col.push(
            row![
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-open-archive"),
                        Some(Icon::BookOpen),
                        ButtonVariant::Primary,
                        Some(BrowserMessage::Dispatch(Command::OpenArchive {
                            path: sel.path.clone(),
                        })),
                    )
                    .width(Fill),
                    tr(lang, "action-open-archive"),
                ),
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-edit"),
                        Some(Icon::Editor),
                        ButtonVariant::Secondary,
                        Some(BrowserMessage::EditPath(sel.path.clone())),
                    ),
                    tr(lang, "tooltip-browser-edit"),
                ),
            ]
            .spacing(6),
        );
        preview_col = preview_col.push(
            button::labeled(
                tr(lang, "action-open-system"),
                Some(Icon::Globe),
                ButtonVariant::Ghost,
                Some(BrowserMessage::OpenExternally(sel.path.clone())),
            )
            .width(Fill),
        );
    } else {
        preview_col = preview_col.push(
            row![
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-open-system"),
                        Some(Icon::Globe),
                        ButtonVariant::Primary,
                        Some(BrowserMessage::OpenExternally(sel.path.clone())),
                    )
                    .width(Fill),
                    tr(lang, "action-open-system"),
                ),
                with_tooltip(
                    button::labeled(
                        tr(lang, "action-edit"),
                        Some(Icon::Editor),
                        ButtonVariant::Secondary,
                        Some(BrowserMessage::EditPath(sel.path.clone())),
                    ),
                    tr(lang, "tooltip-browser-edit"),
                ),
            ]
            .spacing(6),
        );
    }

    scrollable(preview_col).height(Fill).into()
}

fn build_browser_info_body(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let Some(sel) = state.snapshot.meta.selection_info.as_ref() else {
        return container(
            column![
                photon_ui::icons::render(Icon::Info, 24.0, None),
                text(tr(lang, "browser-detail-empty"))
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
            ]
            .spacing(8)
            .align_x(Alignment::Center),
        )
        .padding([24, 10])
        .center_x(Fill)
        .center_y(Fill)
        .into();
    };

    let mut info_col = column![
        kv_row(tr(lang, "label-selected"), sel.name.clone()),
        kv_row(tr(lang, "label-format"), sel.kind.clone()),
        kv_row(tr(lang, "label-file-size"), format_bytes(sel.size)),
    ]
    .spacing(6);
    if let (Some(w), Some(h)) = (sel.width, sel.height) {
        info_col = info_col.push(kv_row(tr(lang, "label-dimensions"), format!("{w} × {h}")));
    }
    if let Some(entry) = state.snapshot.entries.iter().find(|e| e.path == sel.path)
        && entry.modified_ms > 0
    {
        info_col = info_col.push(kv_row(
            tr(lang, "col-modified"),
            format_modified_ms(entry.modified_ms),
        ));
    }
    info_col = info_col.push(kv_row(tr(lang, "label-location"), sel.path.clone()));

    if sel.kind == "folder" {
        let is_fav = state.snapshot.meta.favorites.iter().any(|f| f == &sel.path);
        info_col = info_col.push(with_tooltip(
            button::labeled(
                tr(
                    lang,
                    if is_fav {
                        "action-unpin-quick-access"
                    } else {
                        "action-pin-quick-access"
                    },
                ),
                Some(if is_fav { Icon::PinOff } else { Icon::Star }),
                ButtonVariant::Secondary,
                Some(BrowserMessage::Dispatch(if is_fav {
                    Command::RemoveFavorite {
                        path: sel.path.clone(),
                    }
                } else {
                    Command::AddFavorite {
                        path: sel.path.clone(),
                    }
                })),
            )
            .width(Fill),
            tr(
                lang,
                if is_fav {
                    "action-unpin-quick-access"
                } else {
                    "action-pin-quick-access"
                },
            ),
        ));
    }

    scrollable(container(info_col).padding(Padding {
        top: 0.0,
        right: 10.0,
        bottom: 0.0,
        left: 0.0,
    }))
    .height(Fill)
    .into()
}

fn build_explorer_tree(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let Some(root) = &state.snapshot.meta.root_folder else {
        return container(
            text(tr(lang, "label-no-folder"))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
        )
        .padding([8, 6])
        .into();
    };

    let root_name = Path::new(root)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(root.as_str())
        .to_owned();

    let mut rows: Vec<Element<'_, BrowserMessage>> = Vec::new();
    collect_tree_rows(state, root, &root_name, 0, &mut rows);
    scrollable(column(rows).spacing(2)).height(Fill).into()
}

fn collect_tree_rows<'a>(
    state: BrowserViewState<'a>,
    path: &str,
    name: &str,
    depth: usize,
    out: &mut Vec<Element<'a, BrowserMessage>>,
) {
    if depth > 32 {
        return;
    }
    let expanded = state.expanded_folders.contains(path);
    let active = state.snapshot.meta.current_folder.as_deref() == Some(path);
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(140.0, 260.0));
    let chevron = if expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };

    let lang = state.snapshot.meta.language;
    let row_el = container(
        mouse_area(
            row![
                button::compact_icon_tipped(
                    chevron,
                    ButtonVariant::Ghost,
                    Some(BrowserMessage::ToggleFolderExpand(path.to_owned())),
                    tr(lang, "tooltip-panel-collapse"),
                ),
                button::list_row(
                    if expanded {
                        Icon::FolderOpen
                    } else {
                        Icon::Folder
                    },
                    name.to_owned(),
                    None,
                    active,
                    Some(BrowserMessage::Dispatch(Command::Navigate {
                        path: path.to_owned(),
                    })),
                ),
            ]
            .spacing(2)
            .width(Fill)
            .align_y(Alignment::Center),
        )
        .on_right_press(BrowserMessage::OpenQuickAccessContextMenu {
            path: path.to_owned(),
            x: cursor_pt.x,
            y: cursor_pt.y,
        }),
    )
    .width(Fill)
    .clip(true)
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: depth as f32 * 12.0,
    });
    out.push(row_el.into());

    if expanded && let Some(children) = state.folder_children.get(path) {
        for (child_name, child_path) in children {
            collect_tree_rows(state, child_path, child_name, depth + 1, out);
        }
    }
}
