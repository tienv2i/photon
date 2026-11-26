use std::path::Path;

use iced::Element;
use photon_i18n::tr;
use photon_protocol::{
    BrowserLeftPanel, BrowserRightPanel, BrowserSortField, Command, MoveDirection,
    PluginCapability, SortOrder,
};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};

use crate::batch_dialog::BatchDialogKind;
use crate::model::{is_archive_path_str, sorted_filtered_entries};
use crate::plugins::{normalize_plugin_display_name, plugin_display_name, workspace_quick_actions};
use crate::state::{BrowserMessage, BrowserViewState};

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
) -> (f32, Element<'a, BrowserMessage>) {
    let (est_h, el, _) = build_browser_context_menu_entries_with_offset(state, entry_path);
    (est_h, el)
}

pub fn build_browser_context_menu_entries_with_offset<'a>(
    state: BrowserViewState<'a>,
    entry_path: &str,
) -> (f32, Element<'a, BrowserMessage>, Option<f32>) {
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
        let is_arch = is_archive_path_str(entry_path);
        let open_msg = if is_image {
            BrowserMessage::OpenInViewerWindow(entry_path.to_owned())
        } else if is_arch {
            BrowserMessage::ContextMenuDispatch(Command::OpenArchive {
                path: entry_path.to_owned(),
            })
        } else {
            BrowserMessage::OpenExternally(entry_path.to_owned())
        };
        let edit_msg = BrowserMessage::EditPath(entry_path.to_owned());
        entries.push(MenuEntry::Item(
            MenuItem::new(tr(lang, "action-view"), open_msg).icon(if is_arch {
                Icon::BookOpen
            } else {
                Icon::Viewer
            }),
        ));
        entries.push(MenuEntry::Item(
            MenuItem::new(tr(lang, "action-edit"), edit_msg).icon(Icon::Editor),
        ));
        entries.push(MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-open-with"),
                BrowserMessage::OpenWith(entry_path.to_owned()),
            )
            .icon(Icon::Globe),
        ));
    }

    let image_paths: Vec<String> = paths
        .iter()
        .filter(|p| {
            state
                .snapshot
                .entries
                .iter()
                .any(|e| e.path == **p && e.is_image)
        })
        .cloned()
        .collect();

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
    ]);

    if !image_paths.is_empty() {
        let quick_zone_actions: Vec<_> = workspace_quick_actions(&state.snapshot.meta, "browser")
            .into_iter()
            .filter(|qa| {
                qa.slot == "quick"
                    && !state
                        .snapshot
                        .meta
                        .disabled_quick_action_plugins
                        .iter()
                        .any(|id| id == &qa.plugin_id)
            })
            .collect();
        if !quick_zone_actions.is_empty() {
            entries.push(MenuEntry::Separator);
            for qa in quick_zone_actions {
                let pname = plugin_display_name(&state.snapshot.meta, &qa.plugin_id);
                let canonical_name = state
                    .snapshot
                    .meta
                    .plugins
                    .iter()
                    .find(|p| p.id == qa.plugin_id)
                    .map(|p| p.name.as_str());
                let label = if qa.label.trim().is_empty()
                    || qa.label.eq_ignore_ascii_case(&pname)
                    || canonical_name.is_some_and(|n| qa.label.eq_ignore_ascii_case(n))
                {
                    pname
                } else {
                    format!("{pname} ({})", qa.label.trim())
                };
                entries.push(MenuEntry::Item(
                    MenuItem::new(
                        label,
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

    entries.extend([
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

    let mut plugins_offset_y = None;
    if !image_paths.is_empty() {
        let qas = workspace_quick_actions(&state.snapshot.meta, "browser");
        if !qas.is_empty() {
            entries.push(MenuEntry::Separator);
            let plugins_idx = entries.len();
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "menu-plugins"),
                    BrowserMessage::ToggleContextPluginsSubmenu,
                )
                .icon(Icon::Plug)
                .submenu(true)
                .on_hover(BrowserMessage::OpenContextPluginsSubmenu),
            ));
            plugins_offset_y = Some(photon_ui::menu::estimate_entry_offset_y(
                &entries,
                plugins_idx,
            ));
        }
    }

    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, menu_panel_entries(entries, 225.0), plugins_offset_y)
}

pub(crate) fn browser_context_image_paths(
    state: BrowserViewState<'_>,
    entry_path: &str,
) -> Vec<String> {
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
    paths
        .into_iter()
        .filter(|p| {
            state
                .snapshot
                .entries
                .iter()
                .any(|e| e.path == *p && e.is_image)
        })
        .collect()
}

pub fn build_quick_access_context_menu<'a>(
    state: BrowserViewState<'a>,
    path: &str,
) -> (f32, Element<'a, BrowserMessage>) {
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

    let est_h = photon_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, menu_panel_entries(entries, 220.0))
}

pub fn build_browser_plugins_menu_entries<'a>(
    state: BrowserViewState<'a>,
) -> Vec<MenuEntry<'a, BrowserMessage>> {
    let lang = state.snapshot.meta.language;
    let mut enabled_plugins: Vec<_> = state
        .snapshot
        .meta
        .plugins
        .iter()
        .filter(|p| {
            p.enabled
                && (p.has_hook(photon_protocol::plugin_hooks::BROWSER_CONTEXT_MENU)
                    || p.capabilities.contains(&PluginCapability::BrowserAction))
        })
        .collect();
    enabled_plugins.sort_by(|a, b| {
        let name_a = normalize_plugin_display_name(a.display_name(lang)).to_ascii_lowercase();
        let name_b = normalize_plugin_display_name(b.display_name(lang)).to_ascii_lowercase();
        a.order
            .cmp(&b.order)
            .then_with(|| name_a.cmp(&name_b))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut entries = Vec::new();
    if enabled_plugins.is_empty() {
        entries.push(MenuEntry::Item(
            MenuItem::new(tr(lang, "menu-plugins-no-enabled"), BrowserMessage::Noop).enabled(false),
        ));
    } else {
        let browser_qas = workspace_quick_actions(&state.snapshot.meta, "browser");
        for plugin in enabled_plugins {
            let has_submenu =
                !plugin.params.is_empty() || browser_qas.iter().any(|qa| qa.plugin_id == plugin.id);
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    normalize_plugin_display_name(plugin.display_name(lang)),
                    BrowserMessage::RunPlugin(plugin.id.clone()),
                )
                .icon(Icon::Plug)
                .submenu(has_submenu),
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

fn tr_or_fallback(lang: photon_protocol::Language, key: &str, fallback: String) -> String {
    let translated = tr(lang, key);
    if translated.contains("Unknown localization") || translated == key {
        fallback
    } else {
        translated
    }
}

/// Reusable browser `Edit` menu entry builder for `app.rs::build_active_menu`.
///
/// Mirrors Browser selection & file operations: Edit in Editor, Copy, Paste, Copy to…,
/// Move to…, Rename, Duplicate, Delete (Trash & Permanent), Copy Path, Select All,
/// and Clear Selection.
pub fn build_browser_edit_menu_entries<'a>(
    state: BrowserViewState<'a>,
) -> Vec<MenuEntry<'a, BrowserMessage>> {
    let lang = state.snapshot.meta.language;
    let selection = &state.snapshot.meta.selection;
    let has_selection = !selection.is_empty();
    let single_path = (selection.len() == 1).then(|| selection[0].clone());
    let first_selected_entry = selection
        .first()
        .and_then(|p| state.snapshot.entries.iter().find(|e| &e.path == p));
    let edit_in_editor_msg = match first_selected_entry {
        Some(entry) if !entry.is_dir => Some(BrowserMessage::EditPath(entry.path.clone())),
        _ => None,
    };
    let current_folder = state.snapshot.meta.current_folder.clone();
    let duplicate_msg = if has_selection {
        current_folder.as_ref().map(|dest| {
            BrowserMessage::Dispatch(Command::CopyTo {
                paths: selection.clone(),
                destination: dest.clone(),
            })
        })
    } else {
        None
    };
    let all_visible_paths: Vec<String> = sorted_filtered_entries(state)
        .into_iter()
        .map(|e| e.path.clone())
        .collect();
    let has_visible_entries = !all_visible_paths.is_empty();

    let duplicate_label = tr_or_fallback(
        lang,
        "action-duplicate",
        match lang {
            photon_protocol::Language::Vietnamese => "Nhân bản".to_owned(),
            _ => "Duplicate".to_owned(),
        },
    );
    let clear_selection_label =
        tr_or_fallback(lang, "action-clear-selection", tr(lang, "action-deselect"));

    vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-file-edit"),
                edit_in_editor_msg.clone().unwrap_or(BrowserMessage::Noop),
            )
            .icon(Icon::Editor)
            .enabled(edit_in_editor_msg.is_some()),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy"),
                BrowserMessage::BrowserCopyFiles(selection.clone()),
            )
            .icon(Icon::Copy)
            .shortcut("Ctrl+C")
            .enabled(has_selection),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-paste"), BrowserMessage::BrowserPasteFiles)
                .icon(Icon::Paste)
                .shortcut("Ctrl+V")
                .enabled(current_folder.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-to"),
                BrowserMessage::BrowserCopyTo(selection.clone()),
            )
            .icon(Icon::Copy)
            .enabled(has_selection),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-move-to"),
                BrowserMessage::BrowserMoveTo(selection.clone()),
            )
            .icon(Icon::FolderPlus)
            .enabled(has_selection),
        ),
        MenuEntry::Item(
            MenuItem::new(
                duplicate_label,
                duplicate_msg.clone().unwrap_or(BrowserMessage::Noop),
            )
            .icon(Icon::Duplicate)
            .enabled(duplicate_msg.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rename"),
                single_path
                    .clone()
                    .map(BrowserMessage::BrowserStartRename)
                    .unwrap_or(BrowserMessage::Noop),
            )
            .icon(Icon::Rename)
            .shortcut("F2")
            .enabled(single_path.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-path"),
                single_path
                    .clone()
                    .map(BrowserMessage::BrowserCopyPath)
                    .unwrap_or(BrowserMessage::Noop),
            )
            .icon(Icon::Copy)
            .enabled(single_path.is_some()),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-trash"),
                BrowserMessage::Dispatch(Command::Trash {
                    paths: selection.clone(),
                }),
            )
            .icon(Icon::Trash)
            .shortcut("Del")
            .enabled(has_selection),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-delete-permanently"),
                BrowserMessage::BrowserConfirmDeletePermanently(selection.clone()),
            )
            .icon(Icon::Trash)
            .shortcut("Shift+Del")
            .enabled(has_selection),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-select-all"),
                BrowserMessage::Dispatch(Command::Select {
                    paths: all_visible_paths,
                }),
            )
            .shortcut("Ctrl+A")
            .enabled(has_visible_entries),
        ),
        MenuEntry::Item(
            MenuItem::new(
                clear_selection_label,
                BrowserMessage::Dispatch(Command::Select { paths: Vec::new() }),
            )
            .shortcut("Esc")
            .enabled(has_selection),
        ),
    ]
}

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
