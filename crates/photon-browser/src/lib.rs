//! Advanced Browser workspace UI (`iced 0.14` + `photon-ui`):
//! - Resizable Quick Access (`favorites`) + Explorer sidebar (`panel_splitter`)
//! - Sortable header (`Name` / `Modified` / `Size` / `Type` + `Asc`/`Desc`),
//!   Thumbnail Size stepper (`XS`..`XL`), `Grid` | `List` | `Details` views
//! - Live Batch progress banner & Batch launcher menu (`Resize…` / `Crop…` / `Convert…`)
//! - Right-click context menus (`BrowserEntry` & `BrowserQuickAccess`)

use std::cmp::Ordering;
use std::path::Path;

use iced::widget::{
    Id, column, container, image as iced_image, mouse_area, progress_bar, responsive, row, rule,
    scrollable, text, text_input,
};
use iced::{Alignment, ContentFit, Element, Fill, Length, Padding};
use photon_i18n::tr;
use photon_panels::{
    DraggablePanelHeaderSpec, PanelFrameSpec, kv_row, sidebar_dock, sidebar_panel,
    stacked_panel_heights, vertical_panel_splitter,
};
use photon_protocol::{
    BrowserLeftPanel, BrowserRightPanel, BrowserSortField, BrowserThumbSize, BrowserViewMode,
    Command, Entry, MoveDirection, PluginCapability, SortOrder, Workspace,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::grid::{BreadcrumbItem, GridCardSpec, breadcrumbs_bar, thumbnail_card};
use photon_ui::icons::Icon;
use photon_ui::menu::{
    MenuEntry, MenuItem, MenubarTrigger, menu_panel_entries, menubar_row_hoverable,
};
use photon_ui::splitter::panel_splitter;
use photon_ui::table::{TableColumn, details_table_header, details_table_row};
use photon_ui::theme::{
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, canvas_backdrop_style,
    card_style, hairline_rule_style, input_style, surface_panel_style,
};
use photon_ui::toolbar::{WorkspaceTab, toolbar, workspace_switcher_responsive};
use photon_ui::tooltip::{TooltipPosition, with_tooltip, with_tooltip_pos};
use regex_automata::meta::Regex;
use regex_automata::util::syntax::Config as RegexSyntaxConfig;

pub mod batch_dialog;

use iced::widget::container as iced_container;
use photon_panels::SidebarOrderState;
use photon_protocol::UiStateSnapshot;
use photon_ui::toolbar::workspace_switcher_mode;
use std::collections::{BTreeSet, HashMap};

use batch_dialog::BatchDialogKind;

fn toolbar_divider<'a, M: 'a>() -> Element<'a, M> {
    iced_container(rule::vertical(1).style(hairline_rule_style))
        .height(Length::Fixed(18.0))
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserMenuKind {
    File,
    View,
    Tools,
    Plugins,
    Window,
    Help,
    Sort,
    WorkspaceSwitcher,
}

#[derive(Clone, Copy)]
pub struct BrowserViewState<'a> {
    pub snapshot: &'a UiStateSnapshot,
    pub thumbnails: &'a HashMap<String, iced_image::Handle>,
    pub expanded_folders: &'a BTreeSet<String>,
    pub folder_children: &'a HashMap<String, Vec<(String, String)>>,
    pub folder_input: &'a str,
    pub path_editing: bool,
    pub browser_filter: &'a str,
    pub browser_filter_bar_open: bool,
    pub browser_filter_case_sensitive: bool,
    pub browser_filter_regex: bool,
    pub browser_quick_access_open: bool,
    pub browser_quick_access_collapsed: bool,
    pub quick_access_options_open: bool,
    pub quick_access_editing: bool,
    pub browser_explorer_open: bool,
    pub browser_explorer_collapsed: bool,
    pub browser_preview_open: bool,
    pub browser_preview_collapsed: bool,
    pub browser_info_open: bool,
    pub browser_info_collapsed: bool,
    pub browser_left_panels: &'a SidebarOrderState<BrowserLeftPanel>,
    pub browser_right_panels: &'a SidebarOrderState<BrowserRightPanel>,
    pub browser_left_width_drag: Option<f32>,
    pub browser_right_width_drag: Option<f32>,
    pub is_fullscreen: bool,
    pub last_cursor: Option<iced::Point>,
    pub window_size: iced::Size,
    pub active_menu: Option<BrowserMenuKind>,
}

#[derive(Debug, Clone)]
pub enum BrowserMessage {
    Noop,
    Dispatch(Command),
    ContextMenuDispatch(Command),
    ToggleMenu(BrowserMenuKind),
    ToggleWorkspacePanels,
    ToggleFullscreen,
    OpenSettings,
    OpenShortcutsDialog,
    OpenAboutDialog,
    RequestResetLayout,
    OpenBatchDialog(BatchDialogKind),
    TogglePathEdit(bool),
    FolderInputChanged(String),
    SubmitFolderInput,
    ToggleBrowserFilterBar,
    BrowserFilterChanged(String),
    ClearBrowserFilter,
    ToggleBrowserFilterCaseSensitive,
    ToggleBrowserFilterRegex,
    SetBrowserSortField(BrowserSortField),
    SetBrowserSortOrder(SortOrder),
    SetBrowserThumbSize(BrowserThumbSize),
    SetBrowserViewMode(BrowserViewMode),
    BrowserEntryActivated {
        path: String,
        is_dir: bool,
        is_image: bool,
    },
    ToggleFolderExpand(String),
    OpenBrowserContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    OpenQuickAccessContextMenu {
        path: String,
        x: f32,
        y: f32,
    },
    BrowserCopyPath(String),
    BrowserStartRename(String),
    BrowserCopyTo(Vec<String>),
    BrowserMoveTo(Vec<String>),
    BrowserConfirmDeletePermanently(Vec<String>),
    BrowserCopyFiles(Vec<String>),
    BrowserPasteFiles,
    CopyImageToClipboard(Option<String>),
    RevealInFolder(String),
    OpenManagePlugins,
    ReloadPlugins,
    RunPlugin(String),
    BeginDragBrowserPanel(BrowserLeftPanel),
    HoverBrowserPanelDrop(BrowserLeftPanel),
    DropBrowserPanel(BrowserLeftPanel),
    SwapBrowserLeftPanels,
    ToggleBrowserLeftPanelCollapse(BrowserLeftPanel),
    CloseBrowserLeftPanel(BrowserLeftPanel),
    ToggleBrowserPanelVisible(BrowserLeftPanel),
    BrowserSplitterDrag(f32),
    BrowserSplitterRelease(f32),
    BeginDragBrowserRightPanel(BrowserRightPanel),
    HoverBrowserRightPanelDrop(BrowserRightPanel),
    DropBrowserRightPanel(BrowserRightPanel),
    SwapBrowserRightPanels,
    ToggleBrowserRightPanelCollapse(BrowserRightPanel),
    CloseBrowserRightPanel(BrowserRightPanel),
    ToggleBrowserRightPanelVisible(BrowserRightPanel),
    BrowserRightSplitterDrag(f32),
    BrowserRightSplitterRelease(f32),
    ToggleQuickAccessOptions,
    ToggleQuickAccessEditing,
    OpenInViewerWindow(String),
}

pub fn build_browser_toolbar<'a>(
    state: BrowserViewState<'a>,
    right_utilities: Element<'a, BrowserMessage>,
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let menu_open = matches!(
        state.active_menu,
        Some(
            BrowserMenuKind::File
                | BrowserMenuKind::View
                | BrowserMenuKind::Tools
                | BrowserMenuKind::Plugins
                | BrowserMenuKind::Window
                | BrowserMenuKind::Help
        )
    );
    let hover_switch = |target: BrowserMenuKind| -> Option<BrowserMessage> {
        (menu_open && state.active_menu != Some(target))
            .then_some(BrowserMessage::ToggleMenu(target))
    };

    let menubar = menubar_row_hoverable(vec![
        (
            MenubarTrigger {
                label: tr(lang, "menu-file"),
                active: state.active_menu == Some(BrowserMenuKind::File),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::File),
            },
            hover_switch(BrowserMenuKind::File),
        ),
        (
            MenubarTrigger {
                label: tr(lang, "menu-view"),
                active: state.active_menu == Some(BrowserMenuKind::View),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::View),
            },
            hover_switch(BrowserMenuKind::View),
        ),
        (
            MenubarTrigger {
                label: tr(lang, "menu-tools"),
                active: state.active_menu == Some(BrowserMenuKind::Tools),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::Tools),
            },
            hover_switch(BrowserMenuKind::Tools),
        ),
        (
            MenubarTrigger {
                label: tr(lang, "menu-plugins"),
                active: state.active_menu == Some(BrowserMenuKind::Plugins),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::Plugins),
            },
            hover_switch(BrowserMenuKind::Plugins),
        ),
        (
            MenubarTrigger {
                label: tr(lang, "menu-window"),
                active: state.active_menu == Some(BrowserMenuKind::Window),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::Window),
            },
            hover_switch(BrowserMenuKind::Window),
        ),
        (
            MenubarTrigger {
                label: tr(lang, "menu-help"),
                active: state.active_menu == Some(BrowserMenuKind::Help),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::Help),
            },
            hover_switch(BrowserMenuKind::Help),
        ),
    ]);

    let left = menubar;

    let switcher = workspace_switcher_responsive(
        vec![
            WorkspaceTab::new(
                tr(lang, "workspace-browser"),
                Icon::Browser,
                true,
                BrowserMessage::Dispatch(Command::SetWorkspace {
                    workspace: Workspace::Browser,
                }),
            )
            .tooltip(tr(lang, "tooltip-workspace-browser")),
            WorkspaceTab::new(
                tr(lang, "workspace-viewer"),
                Icon::Viewer,
                false,
                BrowserMessage::Dispatch(Command::SetWorkspace {
                    workspace: Workspace::Viewer,
                }),
            )
            .tooltip(tr(lang, "tooltip-workspace-viewer")),
            WorkspaceTab::new(
                tr(lang, "workspace-editor"),
                Icon::Editor,
                false,
                BrowserMessage::Dispatch(Command::SetWorkspace {
                    workspace: Workspace::Editor,
                }),
            )
            .tooltip(tr(lang, "tooltip-workspace-editor")),
        ],
        workspace_switcher_mode(state.window_size.width),
        state.active_menu == Some(BrowserMenuKind::WorkspaceSwitcher),
        Some(BrowserMessage::ToggleMenu(
            BrowserMenuKind::WorkspaceSwitcher,
        )),
    );

    let right = row![switcher, toolbar_divider(), right_utilities]
        .spacing(8)
        .align_y(Alignment::Center);

    toolbar(left, right)
}

pub fn build_browser_view<'a>(state: BrowserViewState<'a>) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;

    let left_w = state
        .browser_left_width_drag
        .unwrap_or(state.snapshot.meta.layout.left_width as f32)
        .clamp(160.0, 520.0);

    let header_bar = build_browser_header(state);
    let filtered = sorted_filtered_entries(state);
    let regex_err = is_browser_filter_regex_invalid(state);

    let content: Element<'_, BrowserMessage> = if state.snapshot.entries.is_empty() {
        container(
            column![
                photon_ui::icons::render(Icon::FolderOpen, 36.0, None),
                text(tr(lang, "label-no-folder"))
                    .size(TEXT_SIZE_CONTROL)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else if filtered.is_empty() {
        container(
            column![
                photon_ui::icons::render(Icon::Search, 30.0, None),
                text(tr(
                    lang,
                    if regex_err {
                        "browser-filter-invalid-regex"
                    } else {
                        "label-empty-filter"
                    }
                ))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Fill)
        .center_y(Fill)
        .into()
    } else {
        match state.snapshot.meta.layout.browser_view_mode {
            BrowserViewMode::Grid => build_browser_grid(state, &filtered),
            BrowserViewMode::List => build_browser_list(state, &filtered),
            BrowserViewMode::Details => build_browser_details(state, &filtered),
        }
    };

    let mut center_col = column![header_bar, rule::horizontal(1).style(hairline_rule_style)]
        .width(Fill)
        .height(Fill);

    if state.browser_filter_bar_open {
        center_col = center_col
            .push(build_browser_filter_bar(
                state,
                filtered.len(),
                state.snapshot.entries.len(),
                regex_err,
            ))
            .push(rule::horizontal(1).style(hairline_rule_style));
    }

    if let Some(batch) = &state.snapshot.meta.batch {
        center_col = center_col.push(build_batch_banner(lang, batch));
    }

    center_col = center_col.push(
        container(content)
            .width(Fill)
            .height(Fill)
            .style(canvas_backdrop_style),
    );

    let show_sidebar = state.snapshot.meta.layout.panel("browser.sidebar")
        && (state.browser_quick_access_open || state.browser_explorer_open);
    let open_right_panels: Vec<BrowserRightPanel> = state
        .browser_right_panels
        .order()
        .iter()
        .copied()
        .filter(|&p| browser_right_panel_is_open(state, p))
        .collect();

    let mut layout_row = row![].height(Fill).width(Fill);
    if show_sidebar {
        let left_sidebar = build_browser_left_sidebar(state, left_w);
        layout_row = layout_row.push(left_sidebar).push(panel_splitter(
            left_w,
            1.0,
            160.0,
            520.0,
            240.0,
            BrowserMessage::BrowserSplitterDrag,
            BrowserMessage::BrowserSplitterRelease,
        ));
    }
    layout_row = layout_row.push(center_col);
    if !open_right_panels.is_empty() {
        let right_w = state
            .browser_right_width_drag
            .unwrap_or(state.snapshot.meta.layout.right_width as f32)
            .clamp(200.0, 600.0);
        let right_dock = build_browser_right_sidebar(state, right_w, &open_right_panels);
        layout_row = layout_row
            .push(panel_splitter(
                right_w,
                -1.0,
                200.0,
                600.0,
                290.0,
                BrowserMessage::BrowserRightSplitterDrag,
                BrowserMessage::BrowserRightSplitterRelease,
            ))
            .push(right_dock);
    }
    layout_row.into()
}

fn build_browser_left_sidebar(
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
                .options_active(state.quick_access_options_open)
                .options_tooltip(tr(lang, "tooltip-panel-options"))
                .on_options(Some(BrowserMessage::ToggleQuickAccessOptions));
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
    if state.quick_access_options_open {
        let edit_label = if state.quick_access_editing {
            tr(lang, "action-done-editing")
        } else {
            tr(lang, "action-edit-quick-access")
        };
        let options_strip = container(
            row![
                button::labeled(
                    edit_label,
                    Some(Icon::Editor),
                    ButtonVariant::Tab {
                        active: state.quick_access_editing,
                    },
                    Some(BrowserMessage::ToggleQuickAccessEditing),
                )
                .width(Fill),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([2, 2])
        .width(Fill);
        quick_col = quick_col.push(options_strip);
    }

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

fn build_batch_banner<'a>(
    lang: photon_protocol::Language,
    batch: &'a photon_protocol::BatchState,
) -> Element<'a, BrowserMessage> {
    let pct = if batch.total > 0 {
        ((batch.done + batch.failed) as f32 / batch.total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let status_text = if batch.running {
        format!(
            "{} ({}) — {}/{}",
            tr(lang, "batch-progress"),
            batch.operation,
            batch.done,
            batch.total
        )
    } else if batch.cancelled {
        format!(
            "{} ({}/{})",
            tr(lang, "batch-cancelled"),
            batch.done,
            batch.total
        )
    } else {
        format!(
            "{} ({}/{})",
            tr(lang, "batch-done"),
            batch.done,
            batch.total
        )
    };

    let mut top_row = row![
        photon_ui::icons::render(Icon::Batch, 15.0, None),
        text(status_text).size(TEXT_SIZE_CONTROL),
        iced::widget::space::horizontal(),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    if !batch.current.is_empty() && batch.running {
        top_row = top_row.push(text(batch.current.clone()).size(TEXT_SIZE_CAPTION).style(
            |theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            },
        ));
    }
    if batch.running {
        top_row = top_row.push(button::labeled(
            tr(lang, "batch-cancel"),
            Some(Icon::Close),
            ButtonVariant::Danger,
            Some(BrowserMessage::Dispatch(Command::BatchCancel)),
        ));
    }

    container(
        container(column![top_row, progress_bar(0.0..=1.0, pct)].spacing(6))
            .padding([8, 12])
            .width(Fill)
            .style(card_style),
    )
    .padding([6, 12])
    .width(Fill)
    .style(canvas_backdrop_style)
    .into()
}

fn build_browser_header(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let crumbs = &state.snapshot.meta.breadcrumbs;
    let crumb_len = crumbs.len();

    let view_mode = layout.browser_view_mode;
    let view_toggle = row![
        button::icon_only_tipped(
            Icon::Grid,
            ButtonVariant::Tab {
                active: view_mode == BrowserViewMode::Grid,
            },
            Some(BrowserMessage::SetBrowserViewMode(BrowserViewMode::Grid)),
            tr(lang, "tooltip-browser-view-grid"),
        ),
        button::icon_only_tipped(
            Icon::List,
            ButtonVariant::Tab {
                active: view_mode == BrowserViewMode::List,
            },
            Some(BrowserMessage::SetBrowserViewMode(BrowserViewMode::List)),
            tr(lang, "tooltip-browser-view-list"),
        ),
        button::icon_only_tipped(
            Icon::Table,
            ButtonVariant::Tab {
                active: view_mode == BrowserViewMode::Details,
            },
            Some(BrowserMessage::SetBrowserViewMode(BrowserViewMode::Details)),
            tr(lang, "tooltip-browser-view-details"),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let sort_field_label = match layout.browser_sort_field {
        BrowserSortField::Name => tr(lang, "col-name"),
        BrowserSortField::Modified => tr(lang, "col-modified"),
        BrowserSortField::Size => tr(lang, "col-size"),
        BrowserSortField::Kind => tr(lang, "col-type"),
    };
    let next_order = match layout.browser_sort_order {
        SortOrder::Asc => SortOrder::Desc,
        SortOrder::Desc => SortOrder::Asc,
    };
    let sort_controls = row![
        button::labeled_tipped(
            format!("{}: {sort_field_label}", tr(lang, "label-sort")),
            None,
            if state.active_menu == Some(BrowserMenuKind::Sort) {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            },
            Some(BrowserMessage::ToggleMenu(BrowserMenuKind::Sort)),
            tr(lang, "tooltip-browser-sort"),
        ),
        button::icon_only_tipped(
            if layout.browser_sort_order == SortOrder::Asc {
                Icon::SortAsc
            } else {
                Icon::SortDesc
            },
            ButtonVariant::Secondary,
            Some(BrowserMessage::SetBrowserSortOrder(next_order)),
            tr(lang, "tooltip-browser-sort"),
        ),
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    let thumb_size = layout.browser_thumb_size;
    let thumb_stepper = row![
        button::compact_icon_tipped(
            Icon::ZoomOut,
            ButtonVariant::Ghost,
            step_thumb_size(thumb_size, -1).map(BrowserMessage::SetBrowserThumbSize),
            tr(lang, "tooltip-viewer-zoom-out"),
        ),
        text(thumb_size_label(thumb_size))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_secondary),
                }
            }),
        button::compact_icon_tipped(
            Icon::ZoomIn,
            ButtonVariant::Ghost,
            step_thumb_size(thumb_size, 1).map(BrowserMessage::SetBrowserThumbSize),
            tr(lang, "tooltip-viewer-zoom-in"),
        ),
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    let nav_left: Element<'_, BrowserMessage> = if state.path_editing || crumbs.is_empty() {
        row![
            text_input("/path/to/folder", state.folder_input)
                .on_input(BrowserMessage::FolderInputChanged)
                .on_submit(BrowserMessage::SubmitFolderInput)
                .padding([6, 10])
                .size(TEXT_SIZE_CONTROL)
                .style(input_style)
                .width(Fill),
            button::icon_only_tipped(
                Icon::Check,
                ButtonVariant::Primary,
                Some(BrowserMessage::SubmitFolderInput),
                tr(lang, "action-open-folder"),
            ),
            button::icon_only_tipped(
                Icon::Close,
                ButtonVariant::Ghost,
                (!crumbs.is_empty()).then_some(BrowserMessage::TogglePathEdit(false)),
                tr(lang, "action-cancel"),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .width(Fill)
        .into()
    } else {
        let crumb_items: Vec<BreadcrumbItem<BrowserMessage>> = crumbs
            .iter()
            .enumerate()
            .map(|(i, c)| BreadcrumbItem {
                label: c.name.clone(),
                active: i + 1 == crumb_len,
                on_press: BrowserMessage::Dispatch(Command::Navigate {
                    path: c.path.clone(),
                }),
            })
            .collect();
        row![
            breadcrumbs_bar(crumb_items),
            button::icon_only_tipped(
                Icon::FolderOpen,
                ButtonVariant::Ghost,
                Some(BrowserMessage::TogglePathEdit(true)),
                tr(lang, "action-edit-path"),
            ),
            button::icon_only_tipped(
                Icon::Refresh,
                ButtonVariant::Ghost,
                Some(BrowserMessage::Dispatch(Command::Refresh)),
                tr(lang, "tooltip-browser-refresh"),
            ),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Fill)
        .into()
    };

    let filter_active = state.browser_filter_bar_open || !state.browser_filter.is_empty();
    let filter_toggle_btn = button::icon_only_tipped(
        Icon::Search,
        ButtonVariant::Tab {
            active: filter_active,
        },
        Some(BrowserMessage::ToggleBrowserFilterBar),
        tr(lang, "tooltip-browser-filter-toggle"),
    );

    let clipboard_actions = row![
        button::icon_only_tipped(
            Icon::Copy,
            ButtonVariant::Ghost,
            (!state.snapshot.meta.selection.is_empty())
                .then(|| BrowserMessage::BrowserCopyFiles(state.snapshot.meta.selection.clone())),
            format!("{} (Ctrl+C)", tr(lang, "action-copy")),
        ),
        button::icon_only_tipped(
            Icon::Paste,
            ButtonVariant::Ghost,
            state
                .snapshot
                .meta
                .current_folder
                .as_ref()
                .map(|_| BrowserMessage::BrowserPasteFiles),
            format!("{} (Ctrl+V)", tr(lang, "action-paste")),
        ),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    let mut bar = row![
        nav_left,
        toolbar_divider(),
        clipboard_actions,
        toolbar_divider(),
        filter_toggle_btn,
        sort_controls
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    if view_mode == BrowserViewMode::Grid {
        bar = bar.push(thumb_stepper);
    }
    bar = bar.push(view_toggle);

    container(bar)
        .padding([8, 12])
        .width(Fill)
        .style(surface_panel_style)
        .into()
}

pub fn browser_filter_input_id() -> Id {
    Id::new("browser-filter-input")
}

fn build_browser_filter_bar(
    state: BrowserViewState<'_>,
    shown_count: usize,
    total_count: usize,
    regex_err: bool,
) -> Element<'_, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let input = text_input(&tr(lang, "label-search-placeholder"), state.browser_filter)
        .id(browser_filter_input_id())
        .on_input(BrowserMessage::BrowserFilterChanged)
        .padding([5, 10])
        .size(TEXT_SIZE_CONTROL)
        .style(input_style)
        .width(Fill);

    let case_btn = with_tooltip_pos(
        button::labeled(
            "Aa".to_owned(),
            None,
            ButtonVariant::Tab {
                active: state.browser_filter_case_sensitive,
            },
            Some(BrowserMessage::ToggleBrowserFilterCaseSensitive),
        )
        .padding([4, 8]),
        tr(lang, "tooltip-filter-case-sensitive"),
        TooltipPosition::Bottom,
    );

    let regex_btn = with_tooltip_pos(
        button::labeled(
            ".*".to_owned(),
            None,
            ButtonVariant::Tab {
                active: state.browser_filter_regex,
            },
            Some(BrowserMessage::ToggleBrowserFilterRegex),
        )
        .padding([4, 8]),
        tr(lang, "tooltip-filter-regex"),
        TooltipPosition::Bottom,
    );

    let count_text = if regex_err {
        tr(lang, "browser-filter-invalid-regex")
    } else {
        format!("{shown_count} / {total_count}")
    };
    let count_label = text(count_text)
        .size(TEXT_SIZE_CAPTION)
        .style(move |theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(if regex_err { t.danger } else { t.text_muted }),
            }
        });

    let clear_or_close = button::compact_icon_tipped(
        Icon::Close,
        ButtonVariant::Ghost,
        Some(BrowserMessage::ClearBrowserFilter),
        tr(
            lang,
            if state.browser_filter.is_empty() {
                "action-close"
            } else {
                "action-clear-filter"
            },
        ),
    );

    container(
        row![
            photon_ui::icons::render_muted(Icon::Search, 14.0),
            input,
            case_btn,
            regex_btn,
            count_label,
            clear_or_close,
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .width(Fill)
    .style(surface_panel_style)
    .into()
}

fn build_browser_grid<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));
    let cards: Vec<(String, GridCardSpec<BrowserMessage>)> = entries
        .iter()
        .map(|entry| {
            let ic = entry_icon(entry);
            let subtitle = if entry.is_dir {
                None
            } else {
                Some(format_bytes(entry.size))
            };
            (
                entry.path.clone(),
                GridCardSpec {
                    label: entry.name.clone(),
                    subtitle,
                    thumbnail: state.thumbnails.get(&entry.path).cloned(),
                    fallback_icon: ic,
                    selected: state.snapshot.meta.selection.contains(&entry.path),
                    on_press: BrowserMessage::BrowserEntryActivated {
                        path: entry.path.clone(),
                        is_dir: entry.is_dir,
                        is_image: entry.is_image,
                    },
                },
            )
        })
        .collect();
    let min_w = thumb_card_min_width(state.snapshot.meta.layout.browser_thumb_size);
    let gap = 10.0_f32;

    responsive(move |bounds| {
        let usable = (bounds.width - 24.0).max(min_w);
        let cols = ((usable + gap) / (min_w + gap)).floor().max(1.0) as usize;
        let card_width =
            ((usable - gap * (cols.saturating_sub(1) as f32)) / cols as f32).clamp(min_w, 300.0);

        let mut grid_col = column![].spacing(gap).padding([12, 12]);
        for chunk in cards.chunks(cols) {
            let mut r = row![].spacing(gap).align_y(Alignment::Start);
            for (path, spec) in chunk {
                let card = thumbnail_card(spec.clone(), card_width);
                r = r.push(mouse_area(card).on_right_press(
                    BrowserMessage::OpenBrowserContextMenu {
                        path: path.clone(),
                        x: cursor_pt.x,
                        y: cursor_pt.y,
                    },
                ));
            }
            grid_col = grid_col.push(r);
        }
        scrollable(grid_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn build_browser_list<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));
    let mut col = column![].spacing(3).padding([8, 12]);
    for entry in entries {
        let ic = entry_icon(entry);
        let trailing = if entry.is_dir {
            None
        } else {
            Some(format_bytes(entry.size))
        };
        let active = state.snapshot.meta.selection.contains(&entry.path);
        let row_btn = button::list_row(
            ic,
            &entry.name,
            trailing,
            active,
            Some(BrowserMessage::BrowserEntryActivated {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                is_image: entry.is_image,
            }),
        );
        col = col.push(mouse_area(row_btn).on_right_press(
            BrowserMessage::OpenBrowserContextMenu {
                path: entry.path.clone(),
                x: cursor_pt.x,
                y: cursor_pt.y,
            },
        ));
    }
    scrollable(col).height(Fill).into()
}

fn build_browser_details<'a>(
    state: BrowserViewState<'_>,
    entries: &[&Entry],
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let active_field = layout.browser_sort_field;
    let is_asc = layout.browser_sort_order == SortOrder::Asc;
    let cursor_pt = state.last_cursor.unwrap_or(iced::Point::new(360.0, 260.0));

    let sort_msg = |field: BrowserSortField| -> BrowserMessage {
        if active_field == field {
            BrowserMessage::SetBrowserSortOrder(if is_asc {
                SortOrder::Desc
            } else {
                SortOrder::Asc
            })
        } else {
            BrowserMessage::SetBrowserSortField(field)
        }
    };

    let header = details_table_header(vec![
        TableColumn {
            label: tr(lang, "col-name"),
            width: None,
            align_right: false,
            sorted: (active_field == BrowserSortField::Name).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Name),
        },
        TableColumn {
            label: tr(lang, "col-type"),
            width: Some(90.0),
            align_right: false,
            sorted: (active_field == BrowserSortField::Kind).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Kind),
        },
        TableColumn {
            label: tr(lang, "col-size"),
            width: Some(90.0),
            align_right: true,
            sorted: (active_field == BrowserSortField::Size).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Size),
        },
        TableColumn {
            label: tr(lang, "col-modified"),
            width: Some(140.0),
            align_right: false,
            sorted: (active_field == BrowserSortField::Modified).then_some(is_asc),
            on_sort: sort_msg(BrowserSortField::Modified),
        },
    ]);

    let mut rows_col = column![].spacing(2).padding([4, 8]);
    for entry in entries {
        let ic = entry_icon(entry);
        let active = state.snapshot.meta.selection.contains(&entry.path);
        let size_str = if entry.is_dir {
            "—".to_owned()
        } else {
            format_bytes(entry.size)
        };
        let row_el = details_table_row(
            ic,
            vec![
                (entry.name.clone(), None, false),
                (entry_kind_label(entry), Some(90.0), false),
                (size_str, Some(90.0), true),
                (format_modified_ms(entry.modified_ms), Some(140.0), false),
            ],
            32.0,
            active,
            BrowserMessage::BrowserEntryActivated {
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                is_image: entry.is_image,
            },
        );
        rows_col = rows_col.push(mouse_area(row_el).on_right_press(
            BrowserMessage::OpenBrowserContextMenu {
                path: entry.path.clone(),
                x: cursor_pt.x,
                y: cursor_pt.y,
            },
        ));
    }

    column![header, scrollable(rows_col).height(Fill).width(Fill)]
        .width(Fill)
        .height(Fill)
        .into()
}

fn browser_right_panel_is_open(state: BrowserViewState<'_>, panel: BrowserRightPanel) -> bool {
    match panel {
        BrowserRightPanel::Preview => state.browser_preview_open,
        BrowserRightPanel::Info => state.browser_info_open,
    }
}

fn build_browser_right_sidebar<'a>(
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
        preview_col = preview_col.push(
            container(photon_ui::icons::render(Icon::Folder, 36.0, None))
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
                        Some(BrowserMessage::Dispatch(Command::EditorOpen {
                            path: sel.path.clone(),
                        })),
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

    scrollable(info_col).height(Fill).into()
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
            .align_y(Alignment::Center),
        )
        .on_right_press(BrowserMessage::OpenQuickAccessContextMenu {
            path: path.to_owned(),
            x: cursor_pt.x,
            y: cursor_pt.y,
        }),
    )
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

pub fn build_browser_sort_menu<'a>(state: BrowserViewState<'a>) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "col-name"),
                BrowserMessage::SetBrowserSortField(BrowserSortField::Name),
            )
            .checked(layout.browser_sort_field == BrowserSortField::Name),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "col-modified"),
                BrowserMessage::SetBrowserSortField(BrowserSortField::Modified),
            )
            .checked(layout.browser_sort_field == BrowserSortField::Modified),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "col-size"),
                BrowserMessage::SetBrowserSortField(BrowserSortField::Size),
            )
            .checked(layout.browser_sort_field == BrowserSortField::Size),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "col-type"),
                BrowserMessage::SetBrowserSortField(BrowserSortField::Kind),
            )
            .checked(layout.browser_sort_field == BrowserSortField::Kind),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new("A → Z", BrowserMessage::SetBrowserSortOrder(SortOrder::Asc))
                .icon(Icon::SortAsc)
                .checked(layout.browser_sort_order == SortOrder::Asc),
        ),
        MenuEntry::Item(
            MenuItem::new(
                "Z → A",
                BrowserMessage::SetBrowserSortOrder(SortOrder::Desc),
            )
            .icon(Icon::SortDesc)
            .checked(layout.browser_sort_order == SortOrder::Desc),
        ),
    ];
    menu_panel_entries(entries, 195.0)
}

pub fn build_browser_context_menu_entries<'a>(
    state: BrowserViewState<'a>,
    entry_path: &str,
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let entry = state.snapshot.entries.iter().find(|e| e.path == entry_path);
    let is_dir = entry
        .map(|e| e.is_dir)
        .unwrap_or_else(|| Path::new(entry_path).is_dir());
    let is_image = entry.map(|e| e.is_image).unwrap_or(false);
    let is_fav = state
        .snapshot
        .meta
        .favorites
        .iter()
        .any(|f| f == entry_path);

    let paths: Vec<String> = if state
        .snapshot
        .meta
        .selection
        .iter()
        .any(|p| p == entry_path)
        && state.snapshot.meta.selection.len() > 1
    {
        state.snapshot.meta.selection.clone()
    } else {
        vec![entry_path.to_owned()]
    };

    let mut entries = Vec::new();
    if is_dir {
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-open-folder"),
                BrowserMessage::ContextMenuDispatch(Command::Navigate {
                    path: entry_path.to_owned(),
                }),
            )
            .icon(Icon::FolderOpen),
        ));
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if is_fav {
                        "action-unpin-quick-access"
                    } else {
                        "action-pin-quick-access"
                    },
                ),
                BrowserMessage::ContextMenuDispatch(if is_fav {
                    Command::RemoveFavorite {
                        path: entry_path.to_owned(),
                    }
                } else {
                    Command::AddFavorite {
                        path: entry_path.to_owned(),
                    }
                }),
            )
            .icon(if is_fav { Icon::PinOff } else { Icon::Star }),
        ));
    } else {
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-view"),
                BrowserMessage::OpenInViewerWindow(entry_path.to_owned()),
            )
            .icon(Icon::Viewer)
            .enabled(is_image),
        ));
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-edit"),
                BrowserMessage::ContextMenuDispatch(Command::EditorOpen {
                    path: entry_path.to_owned(),
                }),
            )
            .icon(Icon::Editor)
            .enabled(is_image),
        ));
    }

    entries.extend([
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-batch"),
                BrowserMessage::OpenBatchDialog(BatchDialogKind::Resize),
            )
            .icon(Icon::Batch),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy"),
                BrowserMessage::BrowserCopyFiles(paths.clone()),
            )
            .icon(Icon::Copy)
            .shortcut("Ctrl+C"),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-paste"), BrowserMessage::BrowserPasteFiles)
                .icon(Icon::Paste)
                .shortcut("Ctrl+V")
                .enabled(state.snapshot.meta.current_folder.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-image"),
                BrowserMessage::CopyImageToClipboard(Some(entry_path.to_owned())),
            )
            .icon(Icon::Copy)
            .enabled(is_image),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-path"),
                BrowserMessage::BrowserCopyPath(entry_path.to_owned()),
            )
            .icon(Icon::Copy),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-reveal-folder"),
                BrowserMessage::RevealInFolder(entry_path.to_owned()),
            )
            .icon(Icon::FolderOpen),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rename"),
                BrowserMessage::BrowserStartRename(entry_path.to_owned()),
            )
            .icon(Icon::Rename)
            .enabled(paths.len() == 1),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-to"),
                BrowserMessage::BrowserCopyTo(paths.clone()),
            )
            .icon(Icon::Copy),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-move-to"),
                BrowserMessage::BrowserMoveTo(paths.clone()),
            )
            .icon(Icon::FolderPlus),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-trash"),
                BrowserMessage::ContextMenuDispatch(Command::Trash {
                    paths: paths.clone(),
                }),
            )
            .icon(Icon::Trash),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-delete-permanently"),
                BrowserMessage::BrowserConfirmDeletePermanently(paths.clone()),
            )
            .icon(Icon::Trash),
        ),
    ]);

    let image_paths: Vec<String> = paths
        .into_iter()
        .filter(|p| {
            state
                .snapshot
                .entries
                .iter()
                .any(|e| e.path == *p && e.is_image)
        })
        .collect();
    if !image_paths.is_empty() {
        let qas = workspace_quick_actions(&state.snapshot.meta, "browser");
        if !qas.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Header(tr(lang, "menu-quick-actions")));
            for qa in qas {
                entries.push(MenuEntry::Item(
                    MenuItem::new(
                        qa.label.clone(),
                        BrowserMessage::ContextMenuDispatch(Command::QuickActionRun {
                            plugin_id: qa.plugin_id.clone(),
                            action_id: qa.id.clone(),
                            paths: image_paths.clone(),
                        }),
                    )
                    .icon(Icon::Plug),
                ));
            }
        }
    }

    menu_panel_entries(entries, 225.0)
}

pub fn build_quick_access_context_menu<'a>(
    state: BrowserViewState<'a>,
    path: &str,
) -> Element<'a, BrowserMessage> {
    let lang = state.snapshot.meta.language;
    let fav_idx = state.snapshot.meta.favorites.iter().position(|p| p == path);

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-open-folder"),
                BrowserMessage::ContextMenuDispatch(Command::OpenFolder {
                    path: path.to_owned(),
                }),
            )
            .icon(Icon::FolderOpen),
        ),
        MenuEntry::Separator,
    ];

    if let Some(idx) = fav_idx {
        entries.extend([
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-move-up"),
                    BrowserMessage::ContextMenuDispatch(Command::MoveFavorite {
                        path: path.to_owned(),
                        direction: MoveDirection::Up,
                    }),
                )
                .icon(Icon::ArrowUp)
                .enabled(idx > 0),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-move-down"),
                    BrowserMessage::ContextMenuDispatch(Command::MoveFavorite {
                        path: path.to_owned(),
                        direction: MoveDirection::Down,
                    }),
                )
                .icon(Icon::ArrowDown)
                .enabled(idx + 1 < state.snapshot.meta.favorites.len()),
            ),
            MenuEntry::Separator,
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-unpin-quick-access"),
                    BrowserMessage::ContextMenuDispatch(Command::RemoveFavorite {
                        path: path.to_owned(),
                    }),
                )
                .icon(Icon::PinOff),
            ),
        ]);
    } else {
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-pin-quick-access"),
                BrowserMessage::ContextMenuDispatch(Command::AddFavorite {
                    path: path.to_owned(),
                }),
            )
            .icon(Icon::Star),
        ));
    }

    menu_panel_entries(entries, 220.0)
}

pub fn build_browser_plugins_menu_entries<'a>(
    state: BrowserViewState<'a>,
) -> Vec<MenuEntry<'a, BrowserMessage>> {
    let lang = state.snapshot.meta.language;
    let enabled_plugins: Vec<_> = state
        .snapshot
        .meta
        .plugins
        .iter()
        .filter(|p| p.enabled && p.capabilities.contains(&PluginCapability::BrowserAction))
        .collect();

    let mut entries = Vec::new();
    if enabled_plugins.is_empty() {
        entries.push(MenuEntry::Item(
            MenuItem::new(tr(lang, "menu-plugins-no-enabled"), BrowserMessage::Noop).enabled(false),
        ));
    } else {
        for plugin in enabled_plugins {
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    plugin.name.clone(),
                    BrowserMessage::RunPlugin(plugin.id.clone()),
                )
                .icon(Icon::Plug),
            ));
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-manage-plugins"),
            BrowserMessage::OpenManagePlugins,
        )
        .icon(Icon::Plug),
    ));
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-reload-plugins"),
            BrowserMessage::ReloadPlugins,
        )
        .icon(Icon::Refresh),
    ));
    entries
}

/// Filter `meta.quick_actions` to available Quick Actions targeting `workspace` (e.g. `"browser"`).
pub fn workspace_quick_actions<'a>(
    meta: &'a photon_protocol::UiStateMeta,
    workspace: &str,
) -> Vec<&'a photon_protocol::AiQuickAction> {
    meta.quick_actions
        .iter()
        .filter(|qa| {
            qa.available
                && (qa.workspaces.is_empty() || qa.workspaces.iter().any(|w| w == workspace))
        })
        .collect()
}

/// Reusable browser Window / Help menu entry builders for `app.rs::build_active_menu`.
pub fn build_browser_window_menu_entries<'a>(
    state: BrowserViewState<'a>,
) -> Vec<MenuEntry<'a, BrowserMessage>> {
    let lang = state.snapshot.meta.language;
    vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "panel-quick-access"),
                BrowserMessage::ToggleBrowserPanelVisible(BrowserLeftPanel::QuickAccess),
            )
            .icon(Icon::Star)
            .checked(state.browser_quick_access_open),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "panel-explorer"),
                BrowserMessage::ToggleBrowserPanelVisible(BrowserLeftPanel::Explorer),
            )
            .icon(Icon::Folder)
            .checked(state.browser_explorer_open),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "panel-preview"),
                BrowserMessage::ToggleBrowserRightPanelVisible(BrowserRightPanel::Preview),
            )
            .icon(Icon::Image)
            .checked(state.browser_preview_open),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "panel-info"),
                BrowserMessage::ToggleBrowserRightPanelVisible(BrowserRightPanel::Info),
            )
            .icon(Icon::Info)
            .checked(state.browser_info_open),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fullscreen"),
                BrowserMessage::ToggleFullscreen,
            )
            .icon(if state.is_fullscreen {
                Icon::FullscreenExit
            } else {
                Icon::Fullscreen
            })
            .shortcut("F11")
            .checked(state.is_fullscreen),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-reset-layout"),
                BrowserMessage::RequestResetLayout,
            )
            .icon(Icon::Reset),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-settings"), BrowserMessage::OpenSettings)
                .icon(Icon::Settings)
                .shortcut("Ctrl+,"),
        ),
    ]
}

pub fn build_browser_help_menu_entries(
    lang: photon_protocol::Language,
) -> Vec<MenuEntry<'static, BrowserMessage>> {
    vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-shortcuts"),
                BrowserMessage::OpenShortcutsDialog,
            )
            .icon(Icon::Info)
            .shortcut("?"),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-about"), BrowserMessage::OpenAboutDialog)
                .icon(Icon::Browser),
        ),
    ]
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn format_modified_ms(ms: u64) -> String {
    if ms == 0 {
        return "—".to_owned();
    }
    let secs = (ms / 1000) as i64;
    let days_since_epoch = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let (year, month, day) = civil_from_days(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Howard Hinnant's civil-from-days conversion (no external date crate required).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

pub fn entry_kind_label(entry: &Entry) -> String {
    if !entry.kind.is_empty() {
        return entry.kind.clone();
    }
    if entry.is_dir {
        return "Folder".to_owned();
    }
    Path::new(&entry.name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "File".to_owned())
}

fn entry_icon(entry: &Entry) -> Icon {
    if entry.is_dir {
        Icon::Folder
    } else if entry.is_image {
        Icon::Image
    } else {
        Icon::Info
    }
}

pub fn thumb_card_min_width(size: BrowserThumbSize) -> f32 {
    match size {
        BrowserThumbSize::Xs => 96.0,
        BrowserThumbSize::S => 125.0,
        BrowserThumbSize::M => 155.0,
        BrowserThumbSize::L => 200.0,
        BrowserThumbSize::Xl => 250.0,
    }
}

fn thumb_size_label(size: BrowserThumbSize) -> &'static str {
    match size {
        BrowserThumbSize::Xs => "XS",
        BrowserThumbSize::S => "S",
        BrowserThumbSize::M => "M",
        BrowserThumbSize::L => "L",
        BrowserThumbSize::Xl => "XL",
    }
}

fn step_thumb_size(size: BrowserThumbSize, delta: isize) -> Option<BrowserThumbSize> {
    const ORDER: [BrowserThumbSize; 5] = [
        BrowserThumbSize::Xs,
        BrowserThumbSize::S,
        BrowserThumbSize::M,
        BrowserThumbSize::L,
        BrowserThumbSize::Xl,
    ];
    let idx = ORDER.iter().position(|s| *s == size)? as isize + delta;
    if (0..ORDER.len() as isize).contains(&idx) {
        Some(ORDER[idx as usize])
    } else {
        None
    }
}

fn compile_browser_filter_regex(query: &str, case_sensitive: bool) -> Result<Option<Regex>, ()> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    Regex::builder()
        .syntax(RegexSyntaxConfig::new().case_insensitive(!case_sensitive))
        .build(q)
        .map(Some)
        .map_err(|_| ())
}

fn is_browser_filter_regex_invalid(state: BrowserViewState<'_>) -> bool {
    state.browser_filter_regex
        && !state.browser_filter.trim().is_empty()
        && compile_browser_filter_regex(state.browser_filter, state.browser_filter_case_sensitive)
            .is_err()
}

pub fn entry_name_matches_filter(
    name: &str,
    query: &str,
    case_sensitive: bool,
    use_regex: bool,
    compiled_regex: Option<&Regex>,
) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    if use_regex {
        if let Some(re) = compiled_regex {
            return re.is_match(name);
        }
        return false;
    }
    if case_sensitive {
        name.contains(q)
    } else {
        name.to_lowercase().contains(&q.to_lowercase())
    }
}

fn sorted_filtered_entries(state: BrowserViewState<'_>) -> Vec<&Entry> {
    let compiled_regex = if state.browser_filter_regex {
        match compile_browser_filter_regex(
            state.browser_filter,
            state.browser_filter_case_sensitive,
        ) {
            Ok(opt) => opt,
            Err(()) => return Vec::new(),
        }
    } else {
        None
    };
    let mut items: Vec<&Entry> = state
        .snapshot
        .entries
        .iter()
        .filter(|e| {
            entry_name_matches_filter(
                &e.name,
                state.browser_filter,
                state.browser_filter_case_sensitive,
                state.browser_filter_regex,
                compiled_regex.as_ref(),
            )
        })
        .collect();

    let field = state.snapshot.meta.layout.browser_sort_field;
    let order = state.snapshot.meta.layout.browser_sort_order;
    items.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir);
        }
        let cmp = match field {
            BrowserSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            BrowserSortField::Modified => a.modified_ms.cmp(&b.modified_ms),
            BrowserSortField::Size => a.size.cmp(&b.size),
            BrowserSortField::Kind => entry_kind_label(a).cmp(&entry_kind_label(b)),
        };
        let ordered = match order {
            SortOrder::Asc => cmp,
            SortOrder::Desc => cmp.reverse(),
        };
        if ordered == Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            ordered
        }
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_and_modified_produce_expected_strings() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
        assert_eq!(format_modified_ms(0), "—");
        assert_eq!(format_modified_ms(1_700_000_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn thumb_stepper_respects_bounds() {
        assert_eq!(step_thumb_size(BrowserThumbSize::Xs, -1), None);
        assert_eq!(
            step_thumb_size(BrowserThumbSize::Xs, 1),
            Some(BrowserThumbSize::S)
        );
        assert_eq!(step_thumb_size(BrowserThumbSize::Xl, 1), None);
    }

    #[test]
    fn browser_filter_supports_case_sensitivity_and_regex() {
        assert!(entry_name_matches_filter(
            "Photo_01.PNG",
            "photo",
            false,
            false,
            None
        ));
        assert!(!entry_name_matches_filter(
            "Photo_01.PNG",
            "photo",
            true,
            false,
            None
        ));
        assert!(entry_name_matches_filter(
            "Photo_01.PNG",
            "Photo",
            true,
            false,
            None
        ));

        let re_ci = compile_browser_filter_regex(r"^img_\d+\.png$", false)
            .unwrap()
            .unwrap();
        assert!(entry_name_matches_filter(
            "IMG_042.png",
            r"^img_\d+\.png$",
            false,
            true,
            Some(&re_ci)
        ));

        let re_cs = compile_browser_filter_regex(r"^IMG_\d+\.png$", true)
            .unwrap()
            .unwrap();
        assert!(entry_name_matches_filter(
            "IMG_042.png",
            r"^IMG_\d+\.png$",
            true,
            true,
            Some(&re_cs)
        ));
        assert!(!entry_name_matches_filter(
            "img_042.png",
            r"^IMG_\d+\.png$",
            true,
            true,
            Some(&re_cs)
        ));

        assert!(compile_browser_filter_regex("[unclosed", false).is_err());
    }
}
