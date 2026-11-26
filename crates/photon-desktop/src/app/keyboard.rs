use super::*;

pub(super) fn handle_key_event(state: &mut PhotonGui, ev: iced::keyboard::Event) -> Task<Message> {
    if let iced::keyboard::Event::KeyReleased {
        key: Key::Named(Named::Space),
        ..
    } = &ev
    {
        state.viewer_translate.space_pan_held = false;
        return Task::none();
    }
    let iced::keyboard::Event::KeyPressed { key, modifiers, .. } = ev else {
        return Task::none();
    };
    if modifiers.logo() {
        return Task::none();
    }
    if modifiers.alt() {
        if modifiers.control()
            && matches!(key.as_ref(), Key::Character("c") | Key::Character("C"))
            && state.snapshot.meta.workspace == Workspace::Viewer
        {
            return update(state, Message::CopyViewerPath);
        }
        if !modifiers.control() && state.snapshot.meta.workspace == Workspace::Editor {
            let tab_idx = match key.as_ref() {
                Key::Character("1") => Some(0),
                Key::Character("2") => Some(1),
                Key::Character("3") => Some(2),
                Key::Character("4") => Some(3),
                Key::Character("5") => Some(4),
                Key::Character("6") => Some(5),
                Key::Character("7") => Some(6),
                Key::Character("8") => Some(7),
                Key::Character("9") => Some(8),
                _ => None,
            };
            if let Some(idx) = tab_idx
                && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
            {
                return update(state, Message::EditorSwitchTab(tab.id));
            }
        }
        return Task::none();
    }

    if matches!(key, Key::Named(Named::Escape)) {
        if let Some(d) = state.dialog.take() {
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::ViewportBackground,
                previous_viewport_bg,
                ..
            } = &d
            {
                state.viewport_background = *previous_viewport_bg;
            }
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::ViewerViewportBg,
                previous_viewport_bg,
                ..
            } = &d
            {
                state.viewer_viewport_background = *previous_viewport_bg;
            }
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::CanvasSizeFill,
                ..
            } = &d
                && std::mem::take(&mut state.reopen_canvas_size_after_picker)
                && let Some((dims, anchor, fill)) = state.canvas_size_draft.take()
            {
                state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                state.dialog = Some(InAppDialog::CanvasSize {
                    dims,
                    anchor,
                    fill,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::CanvasSize);
                return Task::none();
            }
            if let InAppDialog::ColorPicker {
                target: ColorPickerTarget::CanvasBackground,
                ..
            } = &d
                && std::mem::take(&mut state.reopen_new_doc_after_picker)
                && let Some((dims, custom_rgb)) = state.new_doc_draft.take()
            {
                state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                state.dialog = Some(InAppDialog::NewDocument {
                    dims,
                    background: NewDocBackground::Custom(custom_rgb),
                    custom_rgb,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::NewDocument);
                return Task::none();
            }
            state.canvas_size_draft = None;
            state.reopen_canvas_size_after_picker = false;
            state.new_doc_draft = None;
            state.reopen_new_doc_after_picker = false;
            state.dialog_manager.close(&dialog_id_for_in_app(&d));
            return Task::none();
        }
        if state.settings_open {
            state.settings_open = false;
            state.settings_confirm_factory_reset = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            return sync_ui_tree_task(state);
        }
        if state.context_menu.take().is_some() {
            return sync_ui_tree_task(state);
        }
        if state.active_menu.is_some() {
            state.active_menu = None;
            return Task::none();
        }
        if state.path_editing {
            state.path_editing = false;
            return Task::none();
        }
        if state.editor_ui.smart_paste.is_some() {
            return update(state, Message::SmartPasteDismiss);
        }
        if state.editor_ui.adjust_popup_tab.is_some() {
            return update(state, Message::EditorCloseAdjustPopup { apply: false });
        }
        if state.is_fullscreen {
            return update(state, Message::ToggleFullscreen);
        }
        if state.crop_uv.is_some() {
            state.crop_uv = None;
            return sync_ui_tree_task(state);
        }
        if state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.renaming_layer_id.is_some()
        {
            state.editor_ui.renaming_layer_id = None;
            state.editor_ui.layer_rename_input.clear();
            return sync_ui_tree_task(state);
        }
        if state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.tool == EditorTool::Transform
        {
            return update(state, Message::EditorTransformCancel);
        }
        if state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.tool == EditorTool::Gradient
            && state.editor_ui.gradient_draft.is_some()
        {
            return update(state, Message::EditorCancelGradientDraft);
        }
        match state.snapshot.meta.workspace {
            Workspace::Editor => {
                if state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .is_some_and(|ed| ed.selection.has_selection)
                {
                    return dispatch_to_core(state.core.clone(), Command::EditorDeselect);
                }
                if state.editor_ui.tool != EditorTool::Pointer {
                    return update(state, Message::EditorSelectTool(EditorTool::Pointer));
                }
                return Task::none();
            }
            Workspace::Viewer => {
                if !is_standalone_viewer() {
                    return dispatch_to_core(state.core.clone(), Command::WorkspaceBack);
                }
            }
            Workspace::Browser => {
                if state.browser_filter_bar_open {
                    if !state.browser_filter.is_empty() {
                        state.browser_filter.clear();
                        return widget_operation::focus(browser_filter_input_id());
                    }
                    state.browser_filter_bar_open = false;
                    return sync_ui_tree_task(state);
                }
            }
        }
    }

    if matches!(key, Key::Named(Named::F11)) {
        return update(state, Message::ToggleFullscreen);
    }

    if state.settings_open
        || state.dialog.is_some()
        || (state.snapshot.meta.workspace == Workspace::Editor
            && state.editor_ui.adjust_popup_tab.is_some())
    {
        return Task::none();
    }

    if matches!(key, Key::Named(Named::F5)) && state.snapshot.meta.workspace == Workspace::Browser {
        return dispatch_to_core(state.core.clone(), Command::Refresh);
    }

    // Editor workspace shortcuts are resolved from the canonical Photoshop-aligned
    // shortcut table in `photon_editor::shortcuts` so Editor bindings never conflict
    // with Browser/Viewer shortcuts (e.g. Ctrl+1 = 100% zoom, Ctrl+Shift+N = New Layer,
    // Ctrl+J = Duplicate Layer, Ctrl+E = Merge Down, L = Lasso, Ctrl+Shift+X = Liquify).
    if state.snapshot.meta.workspace == Workspace::Editor
        && state.editor_ui.renaming_layer_id.is_none()
    {
        if matches!(key, Key::Named(Named::Enter)) {
            if state.editor_ui.smart_paste.is_some() {
                return update(state, Message::SmartPasteApply);
            }
            if state.editor_ui.tool == EditorTool::Transform {
                return update(state, Message::EditorTransformCommit);
            }
            if state.editor_ui.tool == EditorTool::Gradient
                && state.editor_ui.gradient_draft.is_some()
            {
                return update(state, Message::EditorApplyGradientDraft);
            }
        }
        let shortcut_key = match key.as_ref() {
            Key::Character(s) => s.chars().next().map(ShortcutKey::from_char),
            Key::Named(Named::Delete) => Some(ShortcutKey::Named(ShortcutNamedKey::Delete)),
            Key::Named(Named::Backspace) => Some(ShortcutKey::Named(ShortcutNamedKey::Backspace)),
            Key::Named(Named::Space) => Some(ShortcutKey::Named(ShortcutNamedKey::Space)),
            _ => None,
        };
        if let Some(sk) = shortcut_key
            && let Some(action) =
                resolve_editor_shortcut(modifiers.control(), modifiers.shift(), modifiers.alt(), sk)
        {
            return dispatch_editor_shortcut(state, action);
        }
    }

    if modifiers.contains(Modifiers::CTRL) {
        match key.as_ref() {
            Key::Character(",") => {
                return update(state, Message::OpenSettings);
            }
            Key::Character("o") if modifiers.shift() => {
                return update(state, Message::PickImagesDialog);
            }
            Key::Character("o") => {
                return update(state, Message::PickFolderDialog);
            }
            Key::Character("n") | Key::Character("N")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::OpenNewFolderDialog);
            }
            Key::Character("n") | Key::Character("N")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::OpenNewDocumentDialog);
            }
            Key::Character("s")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::PickSaveAsDialog);
            }
            Key::Character("e")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::OpenExportDialog);
            }
            Key::Character("c") | Key::Character("C")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCopyPixels { merged: true });
            }
            Key::Character("c") | Key::Character("C")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCopyPixels { merged: false });
            }
            Key::Character("x") | Key::Character("X")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorCutPixels);
            }
            Key::Character("v") | Key::Character("V")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(
                    state,
                    Message::EditorPastePixels {
                        into_selection: modifiers.shift(),
                    },
                );
            }
            Key::Character("t") | Key::Character("T")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::EditorSelectTool(EditorTool::Transform));
            }
            Key::Character("c") | Key::Character("C")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Browser =>
            {
                let selected_img = state
                    .snapshot
                    .meta
                    .selection
                    .iter()
                    .find(|p| {
                        state
                            .snapshot
                            .entries
                            .iter()
                            .any(|e| &e.path == *p && e.is_image)
                    })
                    .cloned();
                if selected_img.is_some() {
                    return update(state, Message::CopyImageToClipboard(selected_img));
                }
                return Task::none();
            }
            Key::Character("c") | Key::Character("C")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(
                    state,
                    Message::BrowserCopyFiles(state.snapshot.meta.selection.clone()),
                );
            }
            Key::Character("v") | Key::Character("V")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::BrowserPasteFiles);
            }
            Key::Character("c")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Viewer =>
            {
                return update(
                    state,
                    Message::CopyImageToClipboard(state.snapshot.meta.viewer_path.clone()),
                );
            }
            Key::Character("r") | Key::Character("R")
                if modifiers.shift() && state.snapshot.meta.workspace == Workspace::Editor =>
            {
                return update(state, Message::OpenCanvasSizeDialog);
            }
            Key::Character("r") if state.snapshot.meta.workspace == Workspace::Editor => {
                return update(state, Message::OpenResizeDialog);
            }
            Key::Character("w") | Key::Character("W")
                if state.snapshot.meta.workspace == Workspace::Editor =>
            {
                if let Some(doc) = &state.snapshot.meta.editor {
                    return update(state, Message::EditorRequestCloseTab(doc.id));
                }
                return Task::none();
            }
            Key::Named(Named::Tab)
                if state.snapshot.meta.workspace == Workspace::Editor
                    && state.snapshot.meta.editor_tabs.len() > 1 =>
            {
                let tabs = &state.snapshot.meta.editor_tabs;
                let cur_idx = state
                    .snapshot
                    .meta
                    .active_editor_tab_id
                    .and_then(|id| tabs.iter().position(|t| t.id == id))
                    .unwrap_or(0);
                let next_idx = if modifiers.shift() {
                    if cur_idx == 0 {
                        tabs.len() - 1
                    } else {
                        cur_idx - 1
                    }
                } else {
                    (cur_idx + 1) % tabs.len()
                };
                if let Some(tab) = tabs.get(next_idx) {
                    return update(state, Message::EditorSwitchTab(tab.id));
                }
                return Task::none();
            }
            Key::Character("l") | Key::Character("L")
                if state.snapshot.meta.workspace == Workspace::Editor && modifiers.shift() =>
            {
                return update(state, Message::OpenTranslatePanel);
            }
            Key::Character("l") if state.snapshot.meta.workspace == Workspace::Browser => {
                state.path_editing = !state.path_editing;
                return Task::none();
            }
            Key::Character("i") | Key::Character("I")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::ToggleBrowserFilterBar);
            }
            Key::Character("h") | Key::Character("H")
                if state.snapshot.meta.workspace == Workspace::Browser =>
            {
                return update(state, Message::ToggleBrowserShowHidden);
            }
            Key::Character("q") | Key::Character("Q") => {
                if is_standalone_viewer() || is_standalone_editor() {
                    return iced::exit();
                }
                let core = state.core.clone();
                return Task::perform(
                    async move {
                        let env = Envelope::new(
                            format!("gui-ctrlq-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::SaveUiState { force: true },
                        );
                        let _ = core.dispatch(env).await;
                    },
                    |_| Message::ShutdownAfterSave,
                );
            }
            Key::Character("1") => {
                if is_standalone_viewer() || is_standalone_editor() {
                    return forward_or_spawn_browser(browser_dir_for_state(state));
                }
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    },
                );
            }
            Key::Character("2") if !is_standalone_viewer() => {
                if is_standalone_editor() {
                    return open_editor_doc_in_viewer(state);
                }
                return dispatch_to_core(
                    state.core.clone(),
                    Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    },
                );
            }
            Key::Character("3") => {
                if is_standalone_viewer() {
                    return switch_workspace_from_standalone_viewer(state, Workspace::Editor);
                }
                if is_standalone_editor() {
                    return Task::none();
                }
                if let Some(path) = state
                    .snapshot
                    .meta
                    .selection
                    .first()
                    .cloned()
                    .or_else(|| state.snapshot.meta.viewer_path.clone())
                {
                    return open_in_editor_or_forward(state, path);
                }
                return forward_or_spawn_editor_only();
            }
            Key::Character("z") if modifiers.shift() => {
                return dispatch_to_core(state.core.clone(), Command::EditorRedo);
            }
            Key::Character("z") => {
                return dispatch_to_core(state.core.clone(), Command::EditorUndo);
            }
            Key::Character("y") => {
                return dispatch_to_core(state.core.clone(), Command::EditorRedo);
            }
            Key::Character("s") => {
                return dispatch_to_core(state.core.clone(), Command::EditorSave);
            }
            Key::Character("a") if state.snapshot.meta.workspace == Workspace::Editor => {
                return dispatch_to_core(state.core.clone(), Command::EditorSelectAll);
            }
            Key::Character("d")
                if state.snapshot.meta.workspace == Workspace::Editor && modifiers.shift() =>
            {
                return dispatch_to_core(state.core.clone(), Command::EditorReselect);
            }
            Key::Character("d") if state.snapshot.meta.workspace == Workspace::Editor => {
                return dispatch_to_core(state.core.clone(), Command::EditorDeselect);
            }
            Key::Character("i")
                if state.snapshot.meta.workspace == Workspace::Editor && modifiers.shift() =>
            {
                return dispatch_to_core(state.core.clone(), Command::EditorSelectInvert);
            }
            _ => {}
        }
    }

    match state.snapshot.meta.workspace {
        Workspace::Editor => Task::none(),
        Workspace::Viewer => match key.as_ref() {
            Key::Named(Named::ArrowLeft) => {
                if state.viewer_mode == ViewerViewMode::Double {
                    let delta = if state.viewer_manga_rtl { 1 } else { -1 };
                    update(state, Message::DualStepSpread(delta))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
                }
            }
            Key::Named(Named::ArrowRight) => {
                if state.viewer_mode == ViewerViewMode::Double {
                    let delta = if state.viewer_manga_rtl { -1 } else { 1 };
                    update(state, Message::DualStepSpread(delta))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerNext)
                }
            }
            Key::Named(Named::PageDown) => {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                    widget_operation::scroll_to(
                        webtoon_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(state.webtoon_scroll_y + step),
                        },
                    )
                } else if state.viewer_mode == ViewerViewMode::Double {
                    update(state, Message::DualStepSpread(1))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerNext)
                }
            }
            Key::Named(Named::PageUp) => {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                    widget_operation::scroll_to(
                        webtoon_scroll_id(),
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some((state.webtoon_scroll_y - step).max(0.0)),
                        },
                    )
                } else if state.viewer_mode == ViewerViewMode::Double {
                    update(state, Message::DualStepSpread(-1))
                } else {
                    dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
                }
            }
            Key::Named(Named::ArrowDown) if state.viewer_mode == ViewerViewMode::Webtoon => {
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(state.webtoon_scroll_y + 260.0),
                    },
                )
            }
            Key::Named(Named::ArrowUp) if state.viewer_mode == ViewerViewMode::Webtoon => {
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some((state.webtoon_scroll_y - 260.0).max(0.0)),
                    },
                )
            }
            Key::Named(Named::Backspace) => {
                if is_standalone_viewer() {
                    Task::none()
                } else {
                    dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
                }
            }
            Key::Character("s") | Key::Character("S") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Single))
            }
            Key::Character("d") | Key::Character("D") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Double))
            }
            Key::Character("m") | Key::Character("M") => {
                update(state, Message::SetViewerViewMode(ViewerViewMode::Webtoon))
            }
            Key::Character("i") | Key::Character("I") => {
                update(state, Message::ToggleViewerInspector)
            }
            Key::Character("f") | Key::Character("F") | Key::Character("0") => {
                update(state, Message::Canvas(CanvasEvent::ResetFit))
            }
            Key::Character("+") | Key::Character("=") => update(state, Message::ZoomStep(1.25)),
            Key::Character("-") => update(state, Message::ZoomStep(0.8)),
            Key::Named(Named::Space)
                if state.viewer_mode != ViewerViewMode::Webtoon
                    && state.viewer_translate.active =>
            {
                state.viewer_translate.space_pan_held = true;
                Task::none()
            }
            Key::Named(Named::Space) if state.viewer_mode == ViewerViewMode::Webtoon => {
                let step = (state.window_size.height - 140.0).max(240.0) * 0.85;
                widget_operation::scroll_to(
                    webtoon_scroll_id(),
                    scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(state.webtoon_scroll_y + step),
                    },
                )
            }
            Key::Character("r") if modifiers.shift() => {
                update(state, Message::ViewerRotatePreview(-1))
            }
            Key::Character("r") | Key::Character("R") => {
                update(state, Message::ViewerRotatePreview(1))
            }
            Key::Character("1") => update(state, Message::ViewerZoomActual),
            Key::Character("l") | Key::Character("L") => update(
                state,
                Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
            ),
            _ => Task::none(),
        },
        Workspace::Browser => match key.as_ref() {
            Key::Character("l") | Key::Character("L")
                if !state.path_editing
                    && !state.browser_filter_bar_open
                    && !modifiers.control()
                    && !modifiers.shift() =>
            {
                update(state, Message::OpenTranslatePanel)
            }
            _ => Task::none(),
        },
    }
}
