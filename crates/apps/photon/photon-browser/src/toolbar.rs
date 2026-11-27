use fw_ui::button::{self, ButtonVariant};
use fw_ui::grid::{BreadcrumbItem, breadcrumbs_bar};
use fw_ui::icons::Icon;
use fw_ui::menu::{MenubarTrigger, menubar_row_hoverable};
use fw_ui::theme::{
    PaletteTokens, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, canvas_backdrop_style, card_style,
    hairline_rule_style, input_style, surface_panel_style,
};
use fw_ui::toolbar::toolbar;
use fw_ui::tooltip::{TooltipPosition, with_tooltip_pos};
use fw_ui::typography::text;
use iced::widget::{
    Id, column, container, container as iced_container, progress_bar, row, rule, text_input,
};
use iced::{Alignment, Element, Fill, Length};
use photon_i18n::tr;
use photon_protocol::{BrowserSortField, BrowserViewMode, Command, SortOrder};

use crate::model::{step_thumb_size, thumb_size_label};
use crate::state::{BrowserMenuKind, BrowserMessage, BrowserViewState};

fn toolbar_divider<'a, M: 'a>() -> Element<'a, M> {
    iced_container(rule::vertical(1).style(hairline_rule_style))
        .height(Length::Fixed(18.0))
        .into()
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
                | BrowserMenuKind::Edit
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
                label: tr(lang, "menu-edit"),
                active: state.active_menu == Some(BrowserMenuKind::Edit),
                on_press: BrowserMessage::ToggleMenu(BrowserMenuKind::Edit),
            },
            hover_switch(BrowserMenuKind::Edit),
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

    let selection = &state.snapshot.meta.selection;
    let single_path = (selection.len() == 1).then(|| selection[0].clone());
    let has_folder = state.snapshot.meta.current_folder.is_some();
    let quick_actions = row![
        button::toolbar_icon(
            Icon::Copy,
            false,
            (!selection.is_empty()).then(|| BrowserMessage::BrowserCopyFiles(selection.clone())),
            Some(format!("{} (Ctrl+C)", tr(lang, "action-copy"))),
        ),
        button::toolbar_icon(
            Icon::Paste,
            false,
            has_folder.then_some(BrowserMessage::BrowserPasteFiles),
            Some(format!("{} (Ctrl+V)", tr(lang, "action-paste"))),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Rename,
            false,
            single_path.map(BrowserMessage::BrowserStartRename),
            Some(format!("{} (F2)", tr(lang, "action-rename"))),
        ),
        button::toolbar_icon(
            Icon::Trash,
            false,
            (!selection.is_empty()).then(|| BrowserMessage::Dispatch(Command::Trash {
                paths: selection.clone(),
            })),
            Some(format!("{} (Del)", tr(lang, "action-trash"))),
        ),
        toolbar_divider(),
        button::toolbar_icon(
            Icon::Refresh,
            false,
            has_folder.then_some(BrowserMessage::Dispatch(Command::Refresh)),
            Some(format!("{} (F5)", tr(lang, "tooltip-browser-refresh"))),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let left = row![menubar, toolbar_divider(), quick_actions]
        .spacing(8)
        .align_y(Alignment::Center);

    toolbar(left, right_utilities)
}

pub(crate) fn build_batch_banner<'a>(
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
        let args = std::collections::BTreeMap::from([
            ("done".to_owned(), batch.done.to_string()),
            ("total".to_owned(), batch.total.to_string()),
        ]);
        photon_i18n::tr_args(lang, "batch-cancelled", &args)
    } else {
        let args = std::collections::BTreeMap::from([
            ("done".to_owned(), batch.done.to_string()),
            ("total".to_owned(), batch.total.to_string()),
        ]);
        photon_i18n::tr_args(lang, "batch-done", &args)
    };

    let mut top_row = row![
        fw_ui::icons::render(Icon::Batch, 15.0, None),
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

pub(crate) fn build_browser_header(state: BrowserViewState<'_>) -> Element<'_, BrowserMessage> {
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

    let hidden_toggle_btn = button::icon_only_tipped(
        Icon::Eye,
        ButtonVariant::Tab {
            active: layout.browser_show_hidden,
        },
        Some(BrowserMessage::ToggleShowHidden),
        tr(lang, "tooltip-browser-show-hidden"),
    );

    let open_folder_btn = button::labeled_tipped(
        tr(lang, "action-open-folder"),
        Some(Icon::FolderOpen),
        ButtonVariant::Secondary,
        Some(BrowserMessage::PickFolderDialog),
        tr(lang, "action-open-folder-dialog"),
    );

    let mut bar = row![
        open_folder_btn,
        nav_left,
        toolbar_divider(),
        filter_toggle_btn,
        hidden_toggle_btn,
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

pub(crate) fn build_browser_filter_bar(
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
            fw_ui::icons::render_muted(Icon::Search, 14.0),
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
