use super::*;

pub(super) fn build_toolbar(
    state: &PhotonGui,
    _role: WindowRole,
    ws: Workspace,
) -> Element<'_, Message> {
    match ws {
        Workspace::Browser => build_browser_toolbar(
            state.browser_view_state(),
            Element::from(toolbar_right_utilities(state))
                .map(|_| BrowserMessage::ToggleWorkspacePanels),
        )
        .map(Message::Browser),
        Workspace::Viewer => build_viewer_toolbar(
            state.viewer_view_state(true),
            Element::from(toolbar_right_utilities(state))
                .map(|_| ViewerMessage::ToggleWorkspacePanels),
        )
        .map(Message::Viewer),
        Workspace::Editor => build_editor_toolbar(
            state.editor_view_state(),
            Element::from(toolbar_right_utilities(state))
                .map(|_| EditorMessage::ToggleWorkspacePanels),
        )
        .map(Message::Editor),
    }
}

pub(crate) fn toolbar_right_utilities(state: &PhotonGui) -> row::Row<'_, Message> {
    let lang = state.snapshot.meta.language;
    let panels_open = state.active_menu == Some(ActiveMenu::WorkspacePanels);

    row![with_tooltip_pos(
        button::icon_only(
            Icon::PanelsTopLeft,
            ButtonVariant::Tab {
                active: panels_open,
            },
            Some(Message::ToggleMenu(ActiveMenu::WorkspacePanels)),
        ),
        tr(lang, "tooltip-panels-appearance-menu"),
        TooltipPosition::Bottom,
    ),]
    .spacing(4)
    .align_y(Alignment::Center)
}

pub(super) fn build_workspace_panels_menu(
    state: &PhotonGui,
    ws: Workspace,
) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let is_dark = state.snapshot.meta.theme == Theme::Dark;

    let mut entries = vec![MenuEntry::Item(
        MenuItem::new(
            tr(
                lang,
                if is_dark {
                    "action-switch-light-theme"
                } else {
                    "action-switch-dark-theme"
                },
            ),
            Message::Dispatch(Command::ToggleTheme),
        )
        .icon(if is_dark { Icon::Sun } else { Icon::Moon }),
    )];

    entries.push(MenuEntry::Separator);
    match ws {
        Workspace::Browser => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-browser")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-quick-access"),
                    Message::PanelPopupToggleBrowserLeft(BrowserLeftPanel::QuickAccess),
                )
                .icon(Icon::Star)
                .checked(state.browser_quick_access_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-explorer"),
                    Message::PanelPopupToggleBrowserLeft(BrowserLeftPanel::Explorer),
                )
                .icon(Icon::Folder)
                .checked(state.browser_explorer_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-preview"),
                    Message::PanelPopupToggleBrowserRight(BrowserRightPanel::Preview),
                )
                .icon(Icon::Image)
                .checked(state.browser_preview_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-info"),
                    Message::PanelPopupToggleBrowserRight(BrowserRightPanel::Info),
                )
                .icon(Icon::Info)
                .checked(state.browser_info_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(tr(lang, "panel-translate"), Message::OpenTranslatePanel)
                    .icon(Icon::Translate)
                    .shortcut("L"),
            ));
        }
        Workspace::Viewer => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-viewer")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-view-adjustments"),
                    Message::ToggleViewerColorPopover,
                )
                .icon(Icon::Palette)
                .checked(state.viewer_adjustments_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(tr(lang, "action-info"), Message::ToggleViewerInspector)
                    .icon(Icon::Info)
                    .shortcut("I")
                    .checked(state.viewer_info_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-translate"),
                    Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
                )
                .icon(Icon::Translate)
                .shortcut("L")
                .checked(state.viewer_translate.active),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(tr(lang, "action-filmstrip"), Message::ToggleFilmstrip)
                    .icon(Icon::Filmstrip)
                    .checked(state.viewer_filmstrip_open),
            ));
            entries.push(MenuEntry::Separator);
            entries.push(MenuEntry::Header(tr(lang, "menu-viewer-backdrop")));
            let dark_bg = default_viewport_bg_rgb(true);
            let light_bg = default_viewport_bg_rgb(false);
            let cur_bg = state.viewer_viewport_background;
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-theme-default"),
                    Message::SetViewerViewportBackground(None),
                )
                .checked(cur_bg.is_none()),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-dark"),
                    Message::SetViewerViewportBackground(Some(dark_bg)),
                )
                .checked(cur_bg == Some(dark_bg)),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-light"),
                    Message::SetViewerViewportBackground(Some(light_bg)),
                )
                .checked(cur_bg == Some(light_bg)),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "backdrop-custom"),
                    Message::OpenColorPicker(ColorPickerTarget::ViewerViewportBg),
                )
                .icon(Icon::Palette)
                .checked(cur_bg.is_some_and(|c| c != dark_bg && c != light_bg)),
            ));
        }
        Workspace::Editor => {
            entries.push(MenuEntry::Header(tr(lang, "panels-group-editor")));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-tools"),
                    Message::PanelPopupToggleEditorTools,
                )
                .icon(Icon::Pointer)
                .checked(layout.panel("editor.tools")),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-tool-options-history"),
                    Message::PanelPopupToggleEditorPanel(EditorRightPanel::ToolOptionsHistory),
                )
                .icon(Icon::History)
                .checked(state.editor_history_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "panel-layers"),
                    Message::PanelPopupToggleEditorPanel(EditorRightPanel::Layers),
                )
                .icon(Icon::Layers)
                .checked(state.editor_layers_open),
            ));
            entries.push(MenuEntry::Item(
                MenuItem::new(tr(lang, "panel-translate"), Message::OpenTranslatePanel)
                    .icon(Icon::Translate)
                    .shortcut("Ctrl+Shift+L"),
            ));
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-save-ui-layout"),
            Message::SaveCurrentUiLayout,
        )
        .icon(Icon::Save),
    ));
    entries.push(MenuEntry::Item(
        MenuItem::new(tr(lang, "action-reset-layout"), Message::RequestResetLayout)
            .icon(Icon::Reset),
    ));
    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-plugin-settings"),
            Message::OpenManagePlugins,
        )
        .icon(Icon::Plug),
    ));
    entries.push(MenuEntry::Item(
        MenuItem::new(tr(lang, "action-settings"), Message::OpenSettings)
            .icon(Icon::Settings)
            .shortcut("Ctrl+,"),
    ));

    menu_panel_entries(entries, 256.0)
}

pub(super) fn build_workspace_switcher_menu(
    state: &PhotonGui,
    ws: Workspace,
) -> Element<'_, Message> {
    let lang = state.snapshot.meta.language;
    let browser_msg = if ws == Workspace::Editor {
        Message::RequestEditorLeaveToBrowser
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Browser,
        })
    };
    let viewer_msg = if ws == Workspace::Editor {
        Message::RequestEditorLeave {
            close_editor: false,
        }
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Viewer,
        })
    };
    let editor_msg = if ws == Workspace::Editor {
        Message::Noop
    } else {
        Message::Dispatch(Command::SetWorkspace {
            workspace: Workspace::Editor,
        })
    };

    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-browser"), browser_msg)
                .icon(Icon::Browser)
                .checked(ws == Workspace::Browser),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-viewer"), viewer_msg)
                .icon(Icon::Viewer)
                .checked(ws == Workspace::Viewer),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "workspace-editor"), editor_msg)
                .icon(Icon::Editor)
                .checked(ws == Workspace::Editor),
        ),
    ];

    menu_panel_entries(entries, 180.0)
}

pub(super) fn build_active_menu(
    state: &PhotonGui,
    _role: WindowRole,
    ws: Workspace,
) -> Option<(Option<f32>, Element<'_, Message>)> {
    let menu = state.active_menu?;
    if menu == ActiveMenu::WorkspaceSwitcher {
        return Some((None, build_workspace_switcher_menu(state, ws)));
    }
    if menu == ActiveMenu::WorkspacePanels {
        return Some((None, build_workspace_panels_menu(state, ws)));
    }
    if ws == Workspace::Editor && menu == ActiveMenu::EditorAddLayer {
        return Some((
            None,
            build_editor_add_layer_menu(state.editor_view_state())
                .1
                .map(Message::Editor),
        ));
    }
    if ws == Workspace::Editor
        && let ActiveMenu::EditorToolFlyout(group) = menu
    {
        return Some((
            None,
            build_tool_group_flyout_menu(&state.editor_ui, state.snapshot.meta.language, group)
                .1
                .map(Message::Editor),
        ));
    }
    let lang = state.snapshot.meta.language;
    let layout = &state.snapshot.meta.layout;
    let ed = state.snapshot.meta.editor.as_ref();

    let has_selected_image = state.snapshot.meta.selection.iter().any(|path| {
        state
            .snapshot
            .entries
            .iter()
            .any(|e| &e.path == path && e.is_image)
    });

    let (offset, entries) = match (ws, menu) {
        (Workspace::Browser, ActiveMenu::BrowserFile) => (
            Some(12.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-open-folder-dialog"),
                        Message::PickFolderDialog,
                    )
                    .icon(Icon::FolderOpen)
                    .shortcut("Ctrl+O"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-open-file-dialog"),
                        Message::PickImagesDialog,
                    )
                    .icon(Icon::Image)
                    .shortcut("Ctrl+Shift+O"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-new-folder"), Message::OpenNewFolderDialog)
                        .icon(Icon::Folder)
                        .shortcut("Ctrl+N"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-edit-path"), Message::TogglePathEdit(true))
                        .shortcut("Ctrl+L"),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-open-viewer"),
                        Message::BrowserOpenSelectedInViewer,
                    )
                    .icon(Icon::Viewer)
                    .enabled(has_selected_image),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-refresh"),
                        Message::Dispatch(Command::Refresh),
                    )
                    .icon(Icon::Refresh)
                    .shortcut("F5"),
                ),
            ],
        ),
        (Workspace::Browser, ActiveMenu::BrowserEdit) => {
            let selection = state.snapshot.meta.selection.clone();
            let has_selection = !selection.is_empty();
            let single_selected = (selection.len() == 1)
                .then(|| selection.first().cloned())
                .flatten();
            let all_paths: Vec<String> = state
                .snapshot
                .entries
                .iter()
                .map(|e| e.path.clone())
                .collect();
            (
                Some(52.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-file-edit"),
                            Message::BrowserOpenSelectedInEditor,
                        )
                        .icon(Icon::Editor)
                        .enabled(has_selected_image),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy-to"),
                            Message::BrowserCopyTo(selection.clone()),
                        )
                        .icon(Icon::Copy)
                        .enabled(has_selection),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-move-to"),
                            Message::BrowserMoveTo(selection.clone()),
                        )
                        .icon(Icon::FolderOpen)
                        .enabled(has_selection),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-rename"),
                            single_selected
                                .clone()
                                .map(Message::BrowserStartRename)
                                .unwrap_or(Message::Noop),
                        )
                        .icon(Icon::Rename)
                        .shortcut("F2")
                        .enabled(single_selected.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-duplicate"),
                            state
                                .snapshot
                                .meta
                                .current_folder
                                .clone()
                                .map(|destination| {
                                    Message::Dispatch(Command::CopyTo {
                                        paths: selection.clone(),
                                        destination,
                                    })
                                })
                                .unwrap_or(Message::Noop),
                        )
                        .icon(Icon::Duplicate)
                        .shortcut("Ctrl+D")
                        .enabled(has_selection && state.snapshot.meta.current_folder.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-trash"),
                            Message::Dispatch(Command::Trash {
                                paths: selection.clone(),
                            }),
                        )
                        .icon(Icon::Trash)
                        .shortcut("Del")
                        .enabled(has_selection),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy-path"),
                            single_selected
                                .map(Message::BrowserCopyPath)
                                .unwrap_or(Message::Noop),
                        )
                        .icon(Icon::Copy)
                        .enabled(selection.len() == 1),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-all"),
                            Message::Dispatch(Command::Select { paths: all_paths }),
                        )
                        .shortcut("Ctrl+A")
                        .enabled(!state.snapshot.entries.is_empty()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-clear-selection"),
                            Message::Dispatch(Command::Select { paths: Vec::new() }),
                        )
                        .enabled(has_selection),
                    ),
                ],
            )
        }
        (Workspace::Browser, ActiveMenu::BrowserView) => (
            Some(56.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-grid"),
                        Message::SetBrowserViewMode(BrowserViewMode::Grid),
                    )
                    .icon(Icon::Grid)
                    .checked(layout.browser_view_mode == BrowserViewMode::Grid),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-list"),
                        Message::SetBrowserViewMode(BrowserViewMode::List),
                    )
                    .icon(Icon::List)
                    .checked(layout.browser_view_mode == BrowserViewMode::List),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-view-details"),
                        Message::SetBrowserViewMode(BrowserViewMode::Details),
                    )
                    .icon(Icon::Table)
                    .checked(layout.browser_view_mode == BrowserViewMode::Details),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-toggle-sidebar"),
                        Message::Dispatch(Command::ToggleLeftPanel),
                    )
                    .checked(layout.panel("browser.sidebar")),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-view-detail-panel"),
                        Message::ToggleBrowserInspector,
                    )
                    .checked(state.browser_inspector_open),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-theme-toggle"),
                        Message::Dispatch(Command::ToggleTheme),
                    )
                    .icon(if state.snapshot.meta.theme == Theme::Dark {
                        Icon::Sun
                    } else {
                        Icon::Moon
                    }),
                ),
            ],
        ),
        (Workspace::Browser, ActiveMenu::BrowserTools) => {
            let has_images = state.snapshot.entries.iter().any(|e| e.is_image);
            (
                Some(104.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-resize"),
                            Message::OpenBatchDialog(BatchDialogKind::Resize),
                        )
                        .icon(Icon::Resize)
                        .enabled(has_images),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-crop"),
                            Message::OpenBatchDialog(BatchDialogKind::Crop),
                        )
                        .icon(Icon::Crop)
                        .enabled(has_images),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-convert"),
                            Message::OpenBatchDialog(BatchDialogKind::Convert),
                        )
                        .icon(Icon::Export)
                        .enabled(has_images),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-batch-transform"),
                            Message::OpenBatchDialog(BatchDialogKind::Transform),
                        )
                        .icon(Icon::Transform)
                        .enabled(has_images),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "menu-tools-edit"),
                            Message::BrowserOpenSelectedInEditor,
                        )
                        .icon(Icon::Editor)
                        .enabled(has_selected_image),
                    ),
                ],
            )
        }
        (Workspace::Browser, ActiveMenu::BrowserPlugins) => (
            Some(156.0),
            build_browser_plugins_menu_entries(state.browser_view_state())
                .into_iter()
                .map(|e| e.map(Message::Browser))
                .collect(),
        ),
        (Workspace::Browser, ActiveMenu::BrowserWindow) => {
            let mut win_entries: Vec<MenuEntry<'_, Message>> =
                build_browser_window_menu_entries(state.browser_view_state())
                    .into_iter()
                    .filter(|entry| {
                        !matches!(
                            entry,
                            MenuEntry::Item(item)
                                if matches!(item.on_press, BrowserMessage::OpenSettings)
                        )
                    })
                    .map(|e| e.map(Message::Browser))
                    .collect();
            while matches!(win_entries.last(), Some(MenuEntry::Separator)) {
                win_entries.pop();
            }
            (Some(214.0), win_entries)
        }
        (Workspace::Browser, ActiveMenu::BrowserHelp) => (
            Some(276.0),
            build_browser_help_menu_entries(lang)
                .into_iter()
                .map(|e| e.map(Message::Browser))
                .collect(),
        ),
        (Workspace::Browser, ActiveMenu::BrowserSort) => {
            return Some((
                None,
                build_browser_sort_menu(state.browser_view_state()).map(Message::Browser),
            ));
        }
        (Workspace::Editor, ActiveMenu::EditorFile) => (
            Some(12.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-editor-new"),
                        Message::OpenNewDocumentDialog,
                    )
                    .icon(Icon::Plus)
                    .shortcut("Ctrl+N"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-new-from-clipboard"),
                        Message::EditorNewFromClipboard,
                    )
                    .icon(Icon::Paste),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-save"),
                        Message::Dispatch(Command::EditorSave),
                    )
                    .icon(Icon::Save)
                    .shortcut("Ctrl+S")
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-save-as"), Message::PickSaveAsDialog)
                        .icon(Icon::Save)
                        .shortcut("Ctrl+Shift+S")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-save-openraster"),
                        Message::PickSaveOpenRasterDialog,
                    )
                    .icon(Icon::Layers)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-export"), Message::OpenExportDialog)
                        .icon(Icon::Export)
                        .shortcut("Ctrl+Shift+E")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-open-viewer"),
                        Message::RequestEditorLeave {
                            close_editor: false,
                        },
                    )
                    .icon(Icon::Viewer)
                    .enabled(
                        ed.is_some()
                            || state.snapshot.meta.editor_unsupported.is_some()
                            || !state.snapshot.viewer_images.is_empty(),
                    ),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-file-open-browser"),
                        Message::RequestEditorLeaveToBrowser,
                    )
                    .icon(Icon::Folder),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-close"),
                        Message::RequestEditorLeave { close_editor: true },
                    )
                    .icon(Icon::Close)
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorEdit) => {
            let has_sel = ed.is_some_and(|e| e.selection.has_selection);
            (
                Some(54.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-undo"),
                            Message::Dispatch(Command::EditorUndo),
                        )
                        .icon(Icon::Undo)
                        .shortcut("Ctrl+Z")
                        .enabled(ed.is_some_and(|d| d.can_undo)),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-redo"),
                            Message::Dispatch(Command::EditorRedo),
                        )
                        .icon(Icon::Redo)
                        .shortcut("Ctrl+Shift+Z")
                        .enabled(ed.is_some_and(|d| d.can_redo)),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(tr(lang, "action-cut"), Message::EditorCutPixels)
                            .icon(Icon::Cut)
                            .shortcut("Ctrl+X")
                            .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy"),
                            Message::EditorCopyPixels { merged: false },
                        )
                        .icon(Icon::Copy)
                        .shortcut("Ctrl+C")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-copy-merged"),
                            Message::EditorCopyPixels { merged: true },
                        )
                        .icon(Icon::Copy)
                        .shortcut("Ctrl+Shift+C")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-paste"),
                            Message::EditorPastePixels {
                                into_selection: false,
                            },
                        )
                        .icon(Icon::Paste)
                        .shortcut("Ctrl+V")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-paste-into-selection"),
                            Message::EditorPastePixels {
                                into_selection: true,
                            },
                        )
                        .icon(Icon::Paste)
                        .shortcut("Ctrl+Shift+V")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-transform"),
                            Message::EditorSelectTool(EditorTool::Transform),
                        )
                        .icon(Icon::Transform)
                        .shortcut("Ctrl+T")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-reset"),
                            Message::Dispatch(Command::EditorReset),
                        )
                        .icon(Icon::Reset)
                        .enabled(ed.is_some_and(|d| d.modified)),
                    ),
                ],
            )
        }
        (Workspace::Editor, ActiveMenu::EditorImage) => (
            Some(96.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-duplicate-image"),
                        Message::EditorDuplicateDocument(None),
                    )
                    .icon(Icon::Duplicate)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-rotate-left"),
                        Message::Dispatch(Command::EditorRotate {
                            degrees: -90,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::RotateLeft)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-rotate-right"),
                        Message::Dispatch(Command::EditorRotate {
                            degrees: 90,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::RotateRight)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "tool-rotate-angle"),
                        Message::EditorSelectTool(EditorTool::RotateAngle),
                    )
                    .icon(Icon::RotateAngle)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-flip-h"),
                        Message::Dispatch(Command::EditorFlip {
                            axis: FlipAxis::Horizontal,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::FlipH)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-flip-v"),
                        Message::Dispatch(Command::EditorFlip {
                            axis: FlipAxis::Vertical,
                            target: state.editor_ui.apply_target,
                        }),
                    )
                    .icon(Icon::FlipV)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-crop"), Message::ToggleCropMode)
                        .icon(Icon::Crop)
                        .checked(state.crop_uv.is_some())
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-image-size"), Message::OpenResizeDialog)
                        .icon(Icon::Resize)
                        .shortcut("Ctrl+R")
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-canvas-size-dialog"),
                        Message::OpenCanvasSizeDialog,
                    )
                    .icon(Icon::CanvasSize)
                    .shortcut("Ctrl+Shift+R")
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorLayer) => {
            let active_id = ed.map(|e| e.active_layer_id).unwrap_or(0);
            let layer_count = ed.map(|e| e.layers.len()).unwrap_or(0);
            let active_idx = ed
                .and_then(|e| e.layers.iter().position(|l| l.id == e.active_layer_id))
                .unwrap_or(0);
            (
                Some(148.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-layer-add"),
                            Message::Dispatch(Command::EditorLayerAdd {
                                name: format!("Layer {}", layer_count + 1),
                            }),
                        )
                        .icon(Icon::Plus)
                        .shortcut("Ctrl+Shift+N")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-layer-duplicate"),
                            Message::Dispatch(Command::EditorLayerDuplicate { id: active_id }),
                        )
                        .icon(Icon::Duplicate)
                        .shortcut("Ctrl+J")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-layer-merge-down"),
                            Message::Dispatch(Command::EditorLayerMergeDown { id: active_id }),
                        )
                        .shortcut("Ctrl+E")
                        .enabled(ed.is_some() && active_idx > 0),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-layer-flatten"),
                            Message::Dispatch(Command::EditorLayerFlatten),
                        )
                        .enabled(ed.is_some() && layer_count > 1),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-mask-add-reveal"),
                            Message::Dispatch(Command::EditorLayerMaskAdd {
                                id: active_id,
                                reveal_all: true,
                            }),
                        )
                        .icon(Icon::Mask)
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-mask-add-hide"),
                            Message::Dispatch(Command::EditorLayerMaskAdd {
                                id: active_id,
                                reveal_all: false,
                            }),
                        )
                        .icon(Icon::Mask)
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-layer-delete"),
                            Message::Dispatch(Command::EditorLayerDelete { id: active_id }),
                        )
                        .icon(Icon::Trash)
                        .enabled(ed.is_some() && layer_count > 1),
                    ),
                ],
            )
        }
        (Workspace::Editor, ActiveMenu::EditorVector) => (
            Some(190.0),
            build_editor_vector_menu_entries(state.editor_view_state())
                .into_iter()
                .map(|e| e.map(Message::Editor))
                .collect(),
        ),
        (Workspace::Editor, ActiveMenu::EditorFilter) => (
            Some(236.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-color"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
                    )
                    .icon(Icon::Sliders)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-hsv"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Hsv),
                    )
                    .icon(Icon::Palette)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-balance"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorBalance),
                    )
                    .icon(Icon::ColorBalance)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "menu-filter-curves"),
                        Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Curves),
                    )
                    .icon(Icon::Curves)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "menu-filter-denoise"), Message::OpenDenoiseDialog)
                        .icon(Icon::Denoise)
                        .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "tool-liquify"),
                        Message::EditorSelectTool(EditorTool::Liquify),
                    )
                    .icon(Icon::Liquify)
                    .shortcut("Ctrl+Shift+X")
                    .enabled(ed.is_some()),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorPlugins) => (
            Some(260.0),
            build_editor_plugins_menu_entries(state.editor_view_state())
                .into_iter()
                .map(|e| e.map(Message::Editor))
                .collect(),
        ),
        (Workspace::Editor, ActiveMenu::EditorSelect) => {
            let has_sel = ed.is_some_and(|e| e.selection.has_selection);
            let can_reselect = ed.is_some_and(|e| e.selection.can_reselect);
            let quick_mask = ed.is_some_and(|e| e.selection.quick_mask_active);
            (
                Some(152.0),
                vec![
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-all"),
                            Message::Dispatch(Command::EditorSelectAll),
                        )
                        .shortcut("Ctrl+A")
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-deselect"),
                            Message::Dispatch(Command::EditorDeselect),
                        )
                        .shortcut("Ctrl+D")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-reselect"),
                            Message::Dispatch(Command::EditorReselect),
                        )
                        .shortcut("Ctrl+Shift+D")
                        .enabled(can_reselect),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-invert"),
                            Message::Dispatch(Command::EditorSelectInvert),
                        )
                        .shortcut("Ctrl+Shift+I")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-feather"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Feather),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-expand"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Expand),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-contract"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Contract),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-border"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Border),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-smooth"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Smooth),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-grow"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Grow),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-similar"),
                            Message::OpenSelectionModifyDialog(SelectionModifyKind::Similar),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-quick-mask"),
                            Message::Dispatch(Command::EditorSelectToggleQuickMask),
                        )
                        .icon(Icon::QuickMask)
                        .shortcut("Q")
                        .checked(quick_mask)
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-from-alpha"),
                            Message::Dispatch(Command::EditorSelectFromLayerAlpha {
                                op: state.editor_ui.select_op,
                            }),
                        )
                        .enabled(ed.is_some()),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-to-mask"),
                            Message::Dispatch(Command::EditorSelectionToMask),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Separator,
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-duplicate-layer"),
                            Message::Dispatch(Command::EditorSelectionDuplicateLayer),
                        )
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-select-clear-pixels"),
                            Message::Dispatch(Command::EditorSelectionClearPixels),
                        )
                        .shortcut("Del")
                        .enabled(has_sel),
                    ),
                    MenuEntry::Item(
                        MenuItem::new(
                            tr(lang, "action-crop-to-selection"),
                            Message::Dispatch(Command::EditorCropToSelection),
                        )
                        .icon(Icon::Crop)
                        .enabled(has_sel),
                    ),
                ],
            )
        }
        (Workspace::Editor, ActiveMenu::EditorView) => (
            Some(360.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-in"), Message::ZoomStep(1.25))
                        .icon(Icon::ZoomIn)
                        .shortcut("Ctrl++"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-zoom-out"), Message::ZoomStep(0.8))
                        .icon(Icon::ZoomOut)
                        .shortcut("Ctrl+-"),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-fit"),
                        Message::Canvas(CanvasEvent::ResetFit),
                    )
                    .icon(Icon::Fit)
                    .shortcut("Ctrl+0"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-actual-size"), Message::EditorZoomActual)
                        .shortcut("Ctrl+1"),
                ),
                MenuEntry::Separator,
                MenuEntry::Header(tr(lang, "settings-canvas-aa")),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-crisp"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Crisp),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::Crisp),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-smooth"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::Smooth),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::Smooth),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "aa-mode-high-quality"),
                        Message::SetAntiAliasingMode(CanvasAntiAliasingMode::HighQuality),
                    )
                    .checked(layout.canvas_anti_aliasing == CanvasAntiAliasingMode::HighQuality),
                ),
                MenuEntry::Separator,
                MenuEntry::Header(tr(lang, "settings-viewport-denoise")),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-off"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Off),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Off),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-light"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Light),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Light),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-medium"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Medium),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Medium),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "denoise-mode-strong"),
                        Message::SetViewportDenoiseMode(ViewportDenoiseMode::Strong),
                    )
                    .checked(layout.viewport_denoise == ViewportDenoiseMode::Strong),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-compare-original"),
                        Message::EditorCycleCompareMode,
                    )
                    .icon(Icon::Compare)
                    .checked(state.editor_ui.compare_mode != CompareMode::Off)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-split"),
                        Message::EditorSetCompareMode(CompareMode::Split),
                    )
                    .checked(state.editor_ui.compare_mode == CompareMode::Split)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-side-by-side"),
                        Message::EditorSetCompareMode(CompareMode::SideBySide),
                    )
                    .icon(Icon::ViewDouble)
                    .checked(state.editor_ui.compare_mode == CompareMode::SideBySide)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-compare-hold"),
                        Message::EditorSetCompareMode(CompareMode::HoldOriginal),
                    )
                    .checked(state.editor_ui.compare_mode == CompareMode::HoldOriginal)
                    .enabled(ed.is_some()),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-theme-toggle"),
                        Message::Dispatch(Command::ToggleTheme),
                    )
                    .icon(if state.snapshot.meta.theme == Theme::Dark {
                        Icon::Sun
                    } else {
                        Icon::Moon
                    }),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorWindow) => (
            Some(364.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "panel-tools"), Message::ToggleEditorToolsVisible)
                        .checked(layout.panel("editor.tools")),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "panel-tool-options-history"),
                        Message::ToggleEditorPanelVisible(EditorRightPanel::ToolOptionsHistory),
                    )
                    .checked(state.editor_history_open),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "panel-layers"),
                        Message::ToggleEditorPanelVisible(EditorRightPanel::Layers),
                    )
                    .checked(state.editor_layers_open),
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
                    MenuItem::new(tr(lang, "action-reset-layout"), Message::RequestResetLayout)
                        .icon(Icon::Reset),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "action-back-to-browser"),
                        Message::OpenBrowserWindow,
                    )
                    .icon(Icon::Browser)
                    .shortcut("Ctrl+1"),
                ),
            ],
        ),
        (Workspace::Editor, ActiveMenu::EditorHelp) => (
            Some(428.0),
            vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-shortcuts"), Message::OpenShortcutsDialog)
                        .icon(Icon::Info)
                        .shortcut("?"),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-about"), Message::OpenAboutDialog)
                        .icon(Icon::Editor),
                ),
            ],
        ),
        (Workspace::Viewer, ActiveMenu::ReaderOptions) => {
            let mut entries = vec![
                MenuEntry::IconRow(vec![
                    MenuIconButton::new(
                        Icon::ViewSingle,
                        format!("{} (S)", tr(lang, "action-viewer-single")),
                        Message::SetViewerViewMode(ViewerViewMode::Single),
                    )
                    .active(state.viewer_mode == ViewerViewMode::Single),
                    MenuIconButton::new(
                        Icon::ViewDouble,
                        format!("{} (D)", tr(lang, "action-viewer-double")),
                        Message::SetViewerViewMode(ViewerViewMode::Double),
                    )
                    .active(state.viewer_mode == ViewerViewMode::Double),
                    MenuIconButton::new(
                        Icon::ViewManhwa,
                        format!("{} (M)", tr(lang, "action-viewer-manhwa")),
                        Message::SetViewerViewMode(ViewerViewMode::Webtoon),
                    )
                    .active(state.viewer_mode == ViewerViewMode::Webtoon),
                ]),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-dir-ltr"), Message::SetMangaRtl(false))
                        .checked(!state.viewer_manga_rtl),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-dir-rtl"), Message::SetMangaRtl(true))
                        .checked(state.viewer_manga_rtl),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "label-separate-cover"),
                        Message::SetMangaSeparateCover(!state.viewer_manga_cover_separate),
                    )
                    .checked(state.viewer_manga_cover_separate),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 0 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(0),
                    )
                    .checked(state.viewer_manga_page_gap == 0),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 8 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(8),
                    )
                    .checked(state.viewer_manga_page_gap == 8),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!("{}: 16 px", tr(lang, "label-page-gap")),
                        Message::SetMangaPageGap(16),
                    )
                    .checked(state.viewer_manga_page_gap == 16),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-auto")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Auto),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Auto),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-width")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Width),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Width),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-height")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Height),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Height),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        format!(
                            "{}: {}",
                            tr(lang, "label-fit-mode"),
                            tr(lang, "action-fit-original")
                        ),
                        Message::SetWebtoonFitMode(WebtoonFitMode::Original),
                    )
                    .checked(state.viewer_webtoon_fit_mode == WebtoonFitMode::Original),
                ),
                MenuEntry::Separator,
            ];
            entries.extend(
                build_viewer_quality_menu_entries(state.viewer_view_state(true))
                    .into_iter()
                    .map(|e| e.map(Message::Viewer)),
            );
            (None, entries)
        }
        (Workspace::Viewer, ActiveMenu::ViewerUtilities) => {
            // Same order as the expanded toolbar group: Filmstrip → Fullscreen →
            // View Adjustments → Info. Info stays last.
            let entries = vec![
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-filmstrip"), Message::ToggleFilmstrip)
                        .icon(Icon::Filmstrip)
                        .checked(state.viewer_filmstrip_open),
                ),
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
                    MenuItem::new(
                        tr(lang, "panel-view-adjustments"),
                        Message::ToggleViewerColorPopover,
                    )
                    .icon(Icon::Palette)
                    .checked(state.viewer_adjustments_open),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        tr(lang, "panel-translate"),
                        Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
                    )
                    .icon(Icon::Translate)
                    .shortcut("L")
                    .checked(state.viewer_translate.active),
                ),
                MenuEntry::Item(
                    MenuItem::new(tr(lang, "action-info"), Message::ToggleViewerInspector)
                        .icon(Icon::Info)
                        .shortcut("I")
                        .checked(state.viewer_info_open),
                ),
            ];
            (None, entries)
        }
        _ => return None,
    };

    Some((offset, menu_panel_entries(entries, 230.0)))
}
