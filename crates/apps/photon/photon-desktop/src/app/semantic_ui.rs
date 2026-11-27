use super::*;

pub(super) fn sync_ui_tree_task(state: &PhotonGui) -> Task<Message> {
    let core = state.core.clone();
    let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let viewer_ui = state.viewer_ui_presentation();
    let effective_scale =
        (state.os_scale_factor * state.snapshot.meta.layout.ui_scale()).clamp(0.25, 8.0);
    let tree = build_ui_tree(
        &state.snapshot,
        state.windows.len(),
        state.window_size,
        effective_scale,
        state.zoom,
        state.pan_x,
        state.pan_y,
        backend,
        &viewer_ui,
        &state.editor_ui,
        state.context_menu.as_ref(),
        state.settings_open,
        state.settings_tab.as_str(),
    );
    Task::perform(
        async move {
            core.update_ui_tree(tree).await;
        },
        |_| Message::Noop,
    )
}

pub(super) fn handle_semantic_ui_action(state: &mut PhotonGui, target_id: &str) -> Task<Message> {
    match target_id {
        "nav.back" => {
            if state.snapshot.meta.workspace == Workspace::Editor {
                update(
                    state,
                    Message::RequestEditorLeave {
                        close_editor: false,
                    },
                )
            } else {
                dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
            }
        }
        "nav.workspace.browser" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Browser,
            },
        ),
        "nav.workspace.viewer" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Viewer,
            },
        ),
        "nav.workspace.editor" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ),
        "toolbar.refresh" => dispatch_to_core(state.core.clone(), Command::Refresh),
        "toolbar.theme_toggle" => dispatch_to_core(state.core.clone(), Command::ToggleTheme),
        "toolbar.panels_menu" => update(state, Message::ToggleMenu(ActiveMenu::WorkspacePanels)),
        "toolbar.settings" | "settings.dialog" => update(state, Message::OpenSettings),
        "settings.tab.general"
        | "settings.tab.viewer"
        | "settings.tab.browser"
        | "settings.tab.editor"
        | "settings.tab.plugins" => {
            let tab = match target_id {
                "settings.tab.general" => SettingsTab::General,
                "settings.tab.viewer" => SettingsTab::Viewer,
                "settings.tab.browser" => SettingsTab::Browser,
                "settings.tab.editor" => SettingsTab::Editor,
                _ => SettingsTab::Plugins,
            };
            if !state.settings_open {
                let open_task = update(state, Message::OpenSettings);
                let tab_task = update(state, Message::SelectSettingsTab(tab));
                Task::batch([open_task, tab_task])
            } else {
                update(state, Message::SelectSettingsTab(tab))
            }
        }
        "browser.filter_toggle" => update(state, Message::ToggleBrowserFilterBar),
        "browser.view_mode" => commit_layout_with(state, |layout| {
            layout.browser_view_mode = match layout.browser_view_mode {
                BrowserViewMode::Grid => BrowserViewMode::List,
                BrowserViewMode::List => BrowserViewMode::Details,
                BrowserViewMode::Details => BrowserViewMode::Grid,
            };
        }),
        "browser.menu.file" => update(state, Message::ToggleMenu(ActiveMenu::BrowserFile)),
        "browser.menu.file.open_viewer" => update(state, Message::BrowserOpenSelectedInViewer),
        "browser.menu.edit" => update(state, Message::ToggleMenu(ActiveMenu::BrowserEdit)),
        "browser.menu.edit.edit_in_editor" | "browser.menu.tools.edit" => {
            update(state, Message::BrowserOpenSelectedInEditor)
        }
        "browser.menu.edit.copy_to" => update(
            state,
            Message::BrowserCopyTo(state.snapshot.meta.selection.clone()),
        ),
        "browser.menu.edit.move_to" => update(
            state,
            Message::BrowserMoveTo(state.snapshot.meta.selection.clone()),
        ),
        "browser.menu.edit.rename" => state
            .snapshot
            .meta
            .selection
            .first()
            .cloned()
            .map(|p| update(state, Message::BrowserStartRename(p)))
            .unwrap_or_else(Task::none),
        "browser.menu.edit.duplicate" => state
            .snapshot
            .meta
            .current_folder
            .clone()
            .map(|destination| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::CopyTo {
                        paths: state.snapshot.meta.selection.clone(),
                        destination,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "browser.menu.edit.delete" => dispatch_to_core(
            state.core.clone(),
            Command::Trash {
                paths: state.snapshot.meta.selection.clone(),
            },
        ),
        "browser.menu.edit.copy_path" => state
            .snapshot
            .meta
            .selection
            .first()
            .cloned()
            .map(|p| update(state, Message::BrowserCopyPath(p)))
            .unwrap_or_else(Task::none),
        "browser.menu.edit.select_all" => {
            let paths = state
                .snapshot
                .entries
                .iter()
                .map(|e| e.path.clone())
                .collect();
            dispatch_to_core(state.core.clone(), Command::Select { paths })
        }
        "browser.menu.edit.clear_selection" => {
            dispatch_to_core(state.core.clone(), Command::Select { paths: Vec::new() })
        }
        "browser.menu.view" => update(state, Message::ToggleMenu(ActiveMenu::BrowserView)),
        "browser.menu.view.detail_panel" => update(state, Message::ToggleBrowserInspector),
        "browser.menu.tools" => update(state, Message::ToggleMenu(ActiveMenu::BrowserTools)),
        "browser.menu.tools.batch_resize" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Resize))
        }
        "browser.menu.tools.batch_crop" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Crop))
        }
        "browser.menu.tools.batch_convert" => {
            update(state, Message::OpenBatchDialog(BatchDialogKind::Convert))
        }
        "browser.menu.plugins" => update(state, Message::ToggleMenu(ActiveMenu::BrowserPlugins)),
        "browser.menu.plugins.manage" | "editor.menu.plugins.manage" => {
            update(state, Message::OpenManagePlugins)
        }
        "browser.menu.plugins.reload" | "editor.menu.plugins.reload" => {
            dispatch_to_core(state.core.clone(), Command::PluginRefresh)
        }
        "browser.menu.window" => update(state, Message::ToggleMenu(ActiveMenu::BrowserWindow)),
        "browser.menu.window.quick_access" => update(
            state,
            Message::ToggleBrowserPanelVisible(BrowserLeftPanel::QuickAccess),
        ),
        "browser.menu.window.explorer" => update(
            state,
            Message::ToggleBrowserPanelVisible(BrowserLeftPanel::Explorer),
        ),
        "browser.menu.window.detail" => update(state, Message::ToggleBrowserInspector),
        "browser.menu.window.preview" => update(
            state,
            Message::ToggleBrowserRightPanelVisible(BrowserRightPanel::Preview),
        ),
        "browser.menu.window.info" => update(
            state,
            Message::ToggleBrowserRightPanelVisible(BrowserRightPanel::Info),
        ),
        "browser.right_panel.reorder" => update(state, Message::SwapBrowserRightPanels),
        "browser.menu.help" => update(state, Message::ToggleMenu(ActiveMenu::BrowserHelp)),
        "browser.panel.reorder" => update(state, Message::SwapBrowserLeftPanels),
        "viewer.prev" => {
            if state.viewer_mode == ViewerViewMode::Double {
                update(state, Message::DualStepSpread(-1))
            } else {
                dispatch_to_core(state.core.clone(), Command::ViewerPrevious)
            }
        }
        "viewer.next" => {
            if state.viewer_mode == ViewerViewMode::Double {
                update(state, Message::DualStepSpread(1))
            } else {
                dispatch_to_core(state.core.clone(), Command::ViewerNext)
            }
        }
        "viewer.mode_single" => update(state, Message::SetViewerViewMode(ViewerViewMode::Single)),
        "viewer.mode_double" => update(state, Message::SetViewerViewMode(ViewerViewMode::Double)),
        "viewer.mode_manhwa" => update(state, Message::SetViewerViewMode(ViewerViewMode::Webtoon)),
        "viewer.reader_options" => update(state, Message::ToggleMenu(ActiveMenu::ReaderOptions)),
        "viewer.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "viewer.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "viewer.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "viewer.edit" => dispatch_to_core(
            state.core.clone(),
            Command::SetWorkspace {
                workspace: Workspace::Editor,
            },
        ),
        "viewer.info_toggle" => update(state, Message::ToggleViewerInspector),
        "viewer.panel.reorder" => update(state, Message::SwapViewerRightPanels),
        "viewer.actual_size" => update(state, Message::ViewerZoomActual),
        "viewer.rotate_left" => update(state, Message::ViewerRotatePreview(-1)),
        "viewer.rotate_right" => update(state, Message::ViewerRotatePreview(1)),
        "viewer.filmstrip_toggle" => update(state, Message::ToggleFilmstrip),
        "viewer.fullscreen" => update(state, Message::ToggleFullscreen),
        "viewer.utilities_menu" => update(state, Message::ToggleMenu(ActiveMenu::ViewerUtilities)),
        "viewer.color_popover.toggle" => update(state, Message::ToggleViewerColorPopover),
        "viewer.color_popover.reset" => update(state, Message::ResetViewerAdj),
        "viewer.color_popover.open_editor" => update(state, Message::ViewerAdjOpenInEditor),
        "editor.tool_tab.options" => {
            update(state, Message::SelectEditorToolTab(EditorToolTab::Options))
        }
        "editor.tool_tab.history" => {
            update(state, Message::SelectEditorToolTab(EditorToolTab::History))
        }
        "editor.panel.reorder" => update(state, Message::SwapEditorPanelOrder),
        "editor.tool.pointer" => update(state, Message::EditorSelectTool(EditorTool::Pointer)),
        "editor.tool.hand" => update(state, Message::EditorSelectTool(EditorTool::Hand)),
        "editor.tool.zoom" => update(state, Message::EditorSelectTool(EditorTool::Zoom)),
        "editor.tool.marquee_rect" => {
            update(state, Message::EditorSelectTool(EditorTool::MarqueeRect))
        }
        "editor.tool.marquee_ellipse" => {
            update(state, Message::EditorSelectTool(EditorTool::MarqueeEllipse))
        }
        "editor.tool.single_row" => update(state, Message::EditorSelectTool(EditorTool::SingleRow)),
        "editor.tool.single_col" => update(state, Message::EditorSelectTool(EditorTool::SingleCol)),
        "editor.tool.lasso" => update(state, Message::EditorSelectTool(EditorTool::Lasso)),
        "editor.tool.polygon_lasso" => {
            update(state, Message::EditorSelectTool(EditorTool::PolygonLasso))
        }
        "editor.tool.magnetic_lasso" => {
            update(state, Message::EditorSelectTool(EditorTool::MagneticLasso))
        }
        "editor.tool.bezier_select" => {
            update(state, Message::EditorSelectTool(EditorTool::BezierSelect))
        }
        "editor.tool.quick_select" => {
            update(state, Message::EditorSelectTool(EditorTool::QuickSelect))
        }
        "editor.tool.magic_wand" => update(state, Message::EditorSelectTool(EditorTool::MagicWand)),
        "editor.menu.select" => update(state, Message::ToggleMenu(ActiveMenu::EditorSelect)),
        "editor.tool.brush" => update(state, Message::EditorSelectTool(EditorTool::Brush)),
        "editor.tool.eraser" => update(state, Message::EditorSelectTool(EditorTool::Eraser)),
        "editor.tool.liquify" => update(state, Message::EditorSelectTool(EditorTool::Liquify)),
        "editor.tool.eyedropper" => {
            update(state, Message::EditorSelectTool(EditorTool::Eyedropper))
        }
        "editor.tool.crop" => update(state, Message::EditorSelectTool(EditorTool::Crop)),
        "editor.tool.rotate_angle" => {
            update(state, Message::EditorSelectTool(EditorTool::RotateAngle))
        }
        "editor.tool.draw_line" => update(state, Message::EditorSelectTool(EditorTool::DrawLine)),
        "editor.tool.draw_rect" => update(state, Message::EditorSelectTool(EditorTool::DrawRect)),
        "editor.tool.draw_ellipse" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawEllipse))
        }
        "editor.tool.draw_polygon" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawPolygon))
        }
        "editor.tool.draw_polyline" => {
            update(state, Message::EditorSelectTool(EditorTool::DrawPolyline))
        }
        "editor.tool.bezier_pen" => update(state, Message::EditorSelectTool(EditorTool::BezierPen)),
        "editor.tool.path_select" => {
            update(state, Message::EditorSelectTool(EditorTool::PathSelect))
        }
        "editor.tool.text" => update(state, Message::EditorSelectTool(EditorTool::Text)),
        "editor.tool.paint_bucket" => {
            update(state, Message::EditorSelectTool(EditorTool::PaintBucket))
        }
        "editor.tool.gradient" => update(state, Message::EditorSelectTool(EditorTool::Gradient)),
        "editor.tool.healing_brush" => {
            update(state, Message::EditorSelectTool(EditorTool::HealingBrush))
        }
        "editor.tool.transform" => update(state, Message::EditorSelectTool(EditorTool::Transform)),
        "editor.tool.remove_bg" => update(state, Message::EditorSelectTool(EditorTool::RemoveBg)),
        "editor.tool.watermark" => update(state, Message::EditorSelectTool(EditorTool::Watermark)),
        "editor.tool.caption" => update(state, Message::EditorSelectTool(EditorTool::Caption)),
        "settings.plugins.refresh" => {
            dispatch_to_core(state.core.clone(), Command::AiRefreshPlugins)
        }
        "viewer.translate.toggle" => update(
            state,
            Message::ViewerTranslate(ViewerMessage::ToggleTranslateOverlay),
        ),
        "viewer.translate.scan" => update(
            state,
            Message::ViewerTranslate(ViewerMessage::ScanTranslatePage),
        ),
        "viewer.translate.settings" => update(state, Message::OpenTranslateSettings),
        "editor.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "editor.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "editor.zoom_fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "editor.zoom_actual" => update(state, Message::EditorZoomActual),
        "editor.adjust_popup.open" | "editor.adjust_popup.tab.color" | "editor.adjust.color" => {
            update(
                state,
                Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
            )
        }
        "editor.adjust_popup.tab.hsv" | "editor.adjust.hsv" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Hsv),
        ),
        "editor.adjust_popup.tab.balance" | "editor.adjust.balance" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorBalance),
        ),
        "editor.adjust_popup.tab.curves" | "editor.adjust.curves" => update(
            state,
            Message::EditorOpenAdjustPopup(AdjustmentPopupTab::Curves),
        ),
        "editor.adjust_popup.reset" => update(state, Message::EditorResetPendingAdjustments),
        "editor.adjust_popup.cancel" => {
            update(state, Message::EditorCloseAdjustPopup { apply: false })
        }
        "editor.adjust_popup.apply" => {
            update(state, Message::EditorCloseAdjustPopup { apply: true })
        }
        "editor.menu.file" => update(state, Message::ToggleMenu(ActiveMenu::EditorFile)),
        "editor.menu.file.new" => update(state, Message::OpenNewDocumentDialog),
        "editor.menu.file.new_from_clipboard" => update(state, Message::EditorNewFromClipboard),
        "editor.menu.file.save" => dispatch_to_core(state.core.clone(), Command::EditorSave),
        "editor.menu.file.save_as" => update(state, Message::PickSaveAsDialog),
        "editor.menu.file.export" => update(state, Message::OpenExportDialog),
        "editor.menu.file.close" => {
            update(state, Message::RequestEditorLeave { close_editor: true })
        }
        "editor.menu.edit" => update(state, Message::ToggleMenu(ActiveMenu::EditorEdit)),
        "editor.menu.edit.undo" => dispatch_to_core(state.core.clone(), Command::EditorUndo),
        "editor.menu.edit.redo" => dispatch_to_core(state.core.clone(), Command::EditorRedo),
        "editor.menu.edit.cut" => update(state, Message::EditorCutPixels),
        "editor.menu.edit.copy" => update(state, Message::EditorCopyPixels { merged: false }),
        "editor.menu.edit.copy_merged" => update(state, Message::EditorCopyPixels { merged: true }),
        "editor.menu.edit.paste" => update(
            state,
            Message::EditorPastePixels {
                into_selection: false,
            },
        ),
        "editor.menu.edit.paste_into_selection" => update(
            state,
            Message::EditorPastePixels {
                into_selection: true,
            },
        ),
        "editor.menu.edit.transform" => {
            update(state, Message::EditorSelectTool(EditorTool::Transform))
        }
        "editor.menu.image" => update(state, Message::ToggleMenu(ActiveMenu::EditorImage)),
        "editor.menu.image.duplicate" => update(state, Message::EditorDuplicateDocument(None)),
        "editor.menu.image.rotate_left" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: -90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.menu.image.rotate_right" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: 90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.menu.image.flip_h" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Horizontal,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.menu.image.flip_v" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Vertical,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.menu.image.crop" => update(state, Message::ToggleCropMode),
        "editor.menu.image.resize" => update(state, Message::OpenResizeDialog),
        "editor.menu.image.canvas_size" => update(state, Message::OpenCanvasSizeDialog),
        "editor.menu.layer" => update(state, Message::ToggleMenu(ActiveMenu::EditorLayer)),
        "editor.menu.layer.new" => {
            let next_idx = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.layers.len() + 1)
                .unwrap_or(1);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLayerAdd {
                    name: format!("Layer {next_idx}"),
                },
            )
        }
        "editor.menu.layer.duplicate" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDuplicate {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.menu.layer.merge_down" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMergeDown {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.menu.layer.delete" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDelete {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.menu.layer.flatten" => {
            dispatch_to_core(state.core.clone(), Command::EditorLayerFlatten)
        }
        "editor.menu.vector" => update(state, Message::ToggleMenu(ActiveMenu::EditorVector)),
        "editor.menu.vector.new_layer" => {
            let next_idx = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map_or(1, |e| e.layers.len() + 1);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAddVectorLayer {
                    name: format!("Vector {next_idx}"),
                    data: VectorLayerData::new(),
                },
            )
        }
        "editor.menu.vector.convert_to_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorConvertShapeToPath {
                layer_id: None,
                shape_id: None,
            },
        ),
        "editor.menu.vector.join_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::JoinPaths {
                    second_shape_id: None,
                    tolerance: 24.0,
                },
            },
        ),
        "editor.menu.vector.split_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::SplitAtNode {
                    index: state.editor_ui.selected_vector_node.unwrap_or(1),
                },
            },
        ),
        "editor.menu.vector.close_open_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::ToggleClosed,
            },
        ),
        "editor.menu.vector.reverse_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::ReversePath,
            },
        ),
        "editor.menu.vector.simplify_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::Simplify {
                    tolerance: state.editor_ui.vector_simplify_tolerance,
                },
            },
        ),
        "editor.menu.vector.smooth_path" => dispatch_to_core(
            state.core.clone(),
            Command::EditorVectorPathOp {
                layer_id: None,
                shape_id: None,
                op: VectorPathOperation::Smooth {
                    tension: state.editor_ui.vector_smooth_tension,
                },
            },
        ),
        "editor.menu.vector.transform_each" => {
            handle_editor_vector_text_message(state, EditorMessage::OpenTransformEachDialog)
        }
        "editor.menu.vector.blend" => {
            handle_editor_vector_text_message(state, EditorMessage::OpenVectorBlendDialog)
        }
        "editor.menu.vector.rasterize" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRasterizeLayer { id: None },
        ),
        "editor.menu.vector.export_svg" => {
            handle_editor_vector_text_message(state, EditorMessage::OpenExportVectorSvgDialog)
        }
        "editor.menu.view" => update(state, Message::ToggleMenu(ActiveMenu::EditorView)),
        "editor.menu.view.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "editor.menu.view.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "editor.menu.view.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "editor.menu.view.actual_size" => update(state, Message::EditorZoomActual),
        "editor.tab.context_menu" => {
            if let Some(tab_id) = state.snapshot.meta.active_editor_tab_id {
                state.active_menu = None;
                state.context_menu = Some(ActiveContextMenu::EditorTab {
                    x: 120.0,
                    y: 72.0,
                    tab_id,
                });
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        "editor.tab.close_others" => {
            if let Some(tab_id) = state.snapshot.meta.active_editor_tab_id {
                let ids = photon_editor::tabs::target_tab_ids(
                    &state.snapshot.meta.editor_tabs,
                    TabCloseScope::Others(tab_id),
                );
                update(state, Message::EditorRequestCloseTabs(ids))
            } else {
                Task::none()
            }
        }
        "editor.tab.close_left" => {
            if let Some(tab_id) = state.snapshot.meta.active_editor_tab_id {
                let ids = photon_editor::tabs::target_tab_ids(
                    &state.snapshot.meta.editor_tabs,
                    TabCloseScope::ToLeft(tab_id),
                );
                update(state, Message::EditorRequestCloseTabs(ids))
            } else {
                Task::none()
            }
        }
        "editor.tab.close_right" => {
            if let Some(tab_id) = state.snapshot.meta.active_editor_tab_id {
                let ids = photon_editor::tabs::target_tab_ids(
                    &state.snapshot.meta.editor_tabs,
                    TabCloseScope::ToRight(tab_id),
                );
                update(state, Message::EditorRequestCloseTabs(ids))
            } else {
                Task::none()
            }
        }
        "editor.menu.select.save_selection" => update(state, Message::OpenSaveSelectionDialog),
        "editor.menu.select.load_selection" => update(state, Message::OpenLoadSelectionDialog),
        "editor.menu.view.rulers" => update(state, Message::Editor(EditorMessage::ToggleRulers)),
        "editor.menu.view.grid" => update(state, Message::Editor(EditorMessage::ToggleGrid)),
        "editor.menu.view.grid_settings" => update(state, Message::OpenGridSettingsDialog),
        "editor.menu.view.guides" => update(state, Message::Editor(EditorMessage::ToggleGuides)),
        "editor.menu.view.new_guide" => update(state, Message::OpenNewGuideDialog),
        "editor.smart_paste.keep_original" => update(
            state,
            Message::SmartPasteSetMode(SmartPasteFitMode::KeepOriginal),
        ),
        "editor.smart_paste.fit" => update(
            state,
            Message::SmartPasteSetMode(SmartPasteFitMode::FitToCanvas),
        ),
        "editor.smart_paste.stretch" => update(
            state,
            Message::SmartPasteSetMode(SmartPasteFitMode::StretchFill),
        ),
        "editor.smart_paste.apply" => update(state, Message::SmartPasteApply),
        "editor.smart_paste.dismiss" => update(state, Message::SmartPasteDismiss),
        "dialog.new_document.size_from_clipboard" => update(state, Message::NewDocUseClipboardSize),
        "dialog.new_document.create_from_clipboard" => {
            update(state, Message::NewDocCreateFromClipboard)
        }
        "editor.menu.filter" => update(state, Message::ToggleMenu(ActiveMenu::EditorFilter)),
        "editor.menu.filter.denoise" => update(state, Message::OpenDenoiseDialog),
        "editor.menu.plugins" => update(state, Message::ToggleMenu(ActiveMenu::EditorPlugins)),
        "editor.menu.file.open_viewer" => update(
            state,
            Message::RequestEditorLeave {
                close_editor: false,
            },
        ),
        "editor.menu.file.open_browser" => update(state, Message::RequestEditorLeaveToBrowser),
        "editor.menu.window" => update(state, Message::ToggleMenu(ActiveMenu::EditorWindow)),
        "editor.menu.window.tools" => update(state, Message::ToggleEditorToolsVisible),
        "editor.menu.window.history" => update(
            state,
            Message::ToggleEditorPanelVisible(EditorRightPanel::ToolOptionsHistory),
        ),
        "editor.menu.window.layers" => update(
            state,
            Message::ToggleEditorPanelVisible(EditorRightPanel::Layers),
        ),
        "editor.menu.help" => update(state, Message::ToggleMenu(ActiveMenu::EditorHelp)),
        "editor.layer.mask.add" | "editor.layer.mask.add_reveal" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskAdd {
                        id: e.active_layer_id,
                        reveal_all: true,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.add_hide" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskAdd {
                        id: e.active_layer_id,
                        reveal_all: false,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.edit" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .and_then(|e| {
                e.layers
                    .iter()
                    .find(|l| l.id == e.active_layer_id)
                    .map(|l| {
                        dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLayerMaskSetEditing {
                                id: l.id,
                                editing: !l.editing_mask,
                            },
                        )
                    })
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.enable" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .and_then(|e| {
                e.layers
                    .iter()
                    .find(|l| l.id == e.active_layer_id)
                    .map(|l| {
                        dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLayerMaskSetEnabled {
                                id: l.id,
                                enabled: !l.mask_enabled,
                            },
                        )
                    })
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.invert" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskInvert {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.apply" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskApply {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.mask.delete" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMaskDelete {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.compare" => update(state, Message::EditorCycleCompareMode),
        "editor.export" => update(state, Message::OpenExportDialog),
        "editor.rotate_left" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: -90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.rotate_right" => dispatch_to_core(
            state.core.clone(),
            Command::EditorRotate {
                degrees: 90,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.flip_h" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Horizontal,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.flip_v" => dispatch_to_core(
            state.core.clone(),
            Command::EditorFlip {
                axis: FlipAxis::Vertical,
                target: state.editor_ui.apply_target,
            },
        ),
        "editor.layer.add" => {
            let count = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.layers.len() + 1)
                .unwrap_or(1);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLayerAdd {
                    name: format!("Layer {count}"),
                },
            )
        }
        "editor.layer.duplicate" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDuplicate {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.delete" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDelete {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.merge_down" => state
            .snapshot
            .meta
            .editor
            .as_ref()
            .map(|e| {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerMergeDown {
                        id: e.active_layer_id,
                    },
                )
            })
            .unwrap_or_else(Task::none),
        "editor.layer.flatten" => dispatch_to_core(state.core.clone(), Command::EditorLayerFlatten),
        "editor.crop" => update(state, Message::ToggleCropMode),
        "editor.resize" => update(state, Message::OpenResizeDialog),
        "editor.canvas_size" => update(state, Message::OpenCanvasSizeDialog),
        "editor.new" => update(state, Message::OpenNewDocumentDialog),
        "editor.tab.new" => update(state, Message::EditorOpenNewTabDialog),
        "editor.undo" => dispatch_to_core(state.core.clone(), Command::EditorUndo),
        "editor.redo" => dispatch_to_core(state.core.clone(), Command::EditorRedo),
        "editor.reset" => dispatch_to_core(state.core.clone(), Command::EditorReset),
        "editor.save" => dispatch_to_core(state.core.clone(), Command::EditorSave),
        "viewer.context_menu" => {
            if matches!(state.context_menu, Some(ActiveContextMenu::Viewer { .. })) {
                update(state, Message::CloseContextMenu)
            } else {
                update(
                    state,
                    Message::OpenViewerContextMenu {
                        path: state.snapshot.meta.viewer_path.clone(),
                        pos: Some([
                            state.window_size.width * 0.5,
                            state.window_size.height * 0.5,
                        ]),
                    },
                )
            }
        }
        "viewer.context_menu.edit" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            state.context_menu = None;
            p.map(|path| open_in_editor_or_forward(state, path))
                .unwrap_or_else(Task::none)
        }
        "viewer.context_menu.copy_image" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            update(state, Message::CopyImageToClipboard(p))
        }
        "viewer.context_menu.copy_path" => update(state, Message::CopyViewerPath),
        "viewer.context_menu.reveal_folder" => {
            let p = match &state.context_menu {
                Some(ActiveContextMenu::Viewer { target_path, .. }) => target_path
                    .clone()
                    .or_else(|| state.snapshot.meta.viewer_path.clone()),
                _ => state.snapshot.meta.viewer_path.clone(),
            };
            p.map(|path| update(state, Message::RevealInFolder(path)))
                .unwrap_or_else(Task::none)
        }
        "viewer.context_menu.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "viewer.context_menu.actual_size" => update(state, Message::ViewerZoomActual),
        "viewer.context_menu.zoom_in" => update(state, Message::ZoomStep(1.25)),
        "viewer.context_menu.zoom_out" => update(state, Message::ZoomStep(0.8)),
        "viewer.context_menu.rotate_left" => update(state, Message::ViewerRotatePreview(-1)),
        "viewer.context_menu.rotate_right" => update(state, Message::ViewerRotatePreview(1)),
        "editor.context_menu" => {
            if matches!(
                state.context_menu,
                Some(ActiveContextMenu::EditorCanvas { .. })
            ) {
                update(state, Message::CloseContextMenu)
            } else if state.snapshot.meta.editor.is_some() {
                state.active_menu = None;
                state.context_menu = Some(ActiveContextMenu::EditorCanvas {
                    x: state.window_size.width * 0.5,
                    y: state.window_size.height * 0.5,
                });
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        "editor.context_menu.undo" => {
            update(state, Message::ContextMenuDispatch(Command::EditorUndo))
        }
        "editor.context_menu.redo" => {
            update(state, Message::ContextMenuDispatch(Command::EditorRedo))
        }
        "editor.context_menu.copy_image" => update(state, Message::CopyImageToClipboard(None)),
        "editor.context_menu.duplicate_layer" => {
            if let Some(id) = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.active_layer_id)
            {
                update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDuplicate { id }),
                )
            } else {
                Task::none()
            }
        }
        "editor.context_menu.rotate_left" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorRotate {
                degrees: -90,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.rotate_right" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorRotate {
                degrees: 90,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.flip_h" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorFlip {
                axis: FlipAxis::Horizontal,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.flip_v" => update(
            state,
            Message::ContextMenuDispatch(Command::EditorFlip {
                axis: FlipAxis::Vertical,
                target: ApplyTarget::WholeImage,
            }),
        ),
        "editor.context_menu.crop" => update(state, Message::ToggleCropMode),
        "editor.context_menu.resize" => update(state, Message::OpenResizeDialog),
        "editor.context_menu.canvas_size" => update(state, Message::OpenCanvasSizeDialog),
        "editor.context_menu.fit" => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        "editor.context_menu.compare" => update(state, Message::EditorCycleCompareMode),
        "editor.context_menu.compare_side_by_side" => update(
            state,
            Message::EditorSetCompareMode(CompareMode::SideBySide),
        ),
        "editor.context_menu.reset" => {
            update(state, Message::ContextMenuDispatch(Command::EditorReset))
        }
        "editor.layer.context_menu" => {
            if matches!(
                state.context_menu,
                Some(ActiveContextMenu::EditorLayer { .. })
            ) {
                update(state, Message::CloseContextMenu)
            } else if let Some(ed) = &state.snapshot.meta.editor {
                state.active_menu = None;
                state.context_menu = Some(ActiveContextMenu::EditorLayer {
                    x: (state.window_size.width - 220.0).max(20.0),
                    y: 260.0,
                    layer_id: ed.active_layer_id,
                });
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        "editor.layer.context_menu.duplicate"
        | "editor.layer.context_menu.rename"
        | "editor.layer.context_menu.move_up"
        | "editor.layer.context_menu.move_down"
        | "editor.layer.context_menu.merge_down"
        | "editor.layer.context_menu.flatten"
        | "editor.layer.context_menu.toggle_visible"
        | "editor.layer.context_menu.toggle_locked"
        | "editor.layer.context_menu.rasterize"
        | "editor.layer.context_menu.delete" => {
            let Some(ed) = state.snapshot.meta.editor.clone() else {
                return Task::none();
            };
            let layer_id = match state.context_menu {
                Some(ActiveContextMenu::EditorLayer { layer_id, .. }) => layer_id,
                _ => ed.active_layer_id,
            };
            let Some(idx) = ed.layers.iter().position(|l| l.id == layer_id) else {
                state.context_menu = None;
                return Task::none();
            };
            let layer = ed.layers[idx].clone();
            match target_id {
                "editor.layer.context_menu.duplicate" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDuplicate { id: layer.id }),
                ),
                "editor.layer.context_menu.rename" => update(
                    state,
                    Message::EditorStartLayerRename(layer.id, layer.name.clone()),
                ),
                "editor.layer.context_menu.move_up" => {
                    if idx + 1 < ed.layers.len() {
                        update(
                            state,
                            Message::ContextMenuDispatch(Command::EditorLayerReorder {
                                id: layer.id,
                                new_index: idx + 1,
                            }),
                        )
                    } else {
                        state.context_menu = None;
                        Task::none()
                    }
                }
                "editor.layer.context_menu.move_down" => {
                    if idx > 0 {
                        update(
                            state,
                            Message::ContextMenuDispatch(Command::EditorLayerReorder {
                                id: layer.id,
                                new_index: idx - 1,
                            }),
                        )
                    } else {
                        state.context_menu = None;
                        Task::none()
                    }
                }
                "editor.layer.context_menu.merge_down" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerMergeDown { id: layer.id }),
                ),
                "editor.layer.context_menu.flatten" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerFlatten),
                ),
                "editor.layer.context_menu.toggle_visible" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                        id: layer.id,
                        name: None,
                        visible: Some(!layer.visible),
                        locked: None,
                        clipped_to_below: None,
                        lock_alpha: None,
                        opacity: None,
                        blend_mode: None,
                        offset_x: None,
                        offset_y: None,
                    }),
                ),
                "editor.layer.context_menu.toggle_locked" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerSetProps {
                        id: layer.id,
                        name: None,
                        visible: None,
                        locked: Some(!layer.locked),
                        clipped_to_below: None,
                        lock_alpha: None,
                        opacity: None,
                        blend_mode: None,
                        offset_x: None,
                        offset_y: None,
                    }),
                ),
                "editor.layer.context_menu.rasterize" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorRasterizeLayer {
                        id: Some(layer.id),
                    }),
                ),
                "editor.layer.context_menu.delete" => update(
                    state,
                    Message::ContextMenuDispatch(Command::EditorLayerDelete { id: layer.id }),
                ),
                _ => Task::none(),
            }
        }
        other => {
            if let Some(rest) = other.strip_prefix("editor.tab.") {
                if let Some(idx_str) = rest.strip_suffix(".close")
                    && let Ok(idx) = idx_str.parse::<usize>()
                    && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
                {
                    return update(state, Message::EditorRequestCloseTab(tab.id));
                }
                if let Ok(idx) = rest.parse::<usize>()
                    && let Some(tab) = state.snapshot.meta.editor_tabs.get(idx)
                {
                    return update(state, Message::EditorSwitchTab(tab.id));
                }
            }
            Task::none()
        }
    }
}
