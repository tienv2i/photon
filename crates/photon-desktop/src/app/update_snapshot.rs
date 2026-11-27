use super::*;

pub(super) fn handle_state_updated(
    state: &mut PhotonGui,
    snap: Arc<UiStateSnapshot>,
) -> Task<Message> {
    match Message::StateUpdated(snap) {
        Message::StateUpdated(snap) => {
            let prev_ws = state.snapshot.meta.workspace;
            let prev_tab_id = state.snapshot.meta.active_editor_tab_id;
            let next_tab_id = snap.meta.active_editor_tab_id;
            let focus_changed =
                snap.meta.focus_request_seq != state.snapshot.meta.focus_request_seq;
            let viewer_changed = snap.meta.viewer_path != state.snapshot.meta.viewer_path;
            let was_ai_running = state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .is_some_and(|t| t.running);
            let prev_ai_saved = state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .and_then(|t| t.saved_path.clone());
            let caption_changed = snap.meta.editor_caption != state.snapshot.meta.editor_caption;
            let translation_changed = snap.meta.translation != state.snapshot.meta.translation;
            let saved_tr_changed =
                snap.meta.viewer_saved_translation != state.snapshot.meta.viewer_saved_translation;
            let archive_changed = snap.meta.viewer_archive != state.snapshot.meta.viewer_archive;
            let root_changed = snap.meta.root_folder != state.snapshot.meta.root_folder;
            let tree_changed = snap.meta.tree_revision != state.snapshot.meta.tree_revision;
            let folder_changed = snap.meta.current_folder != state.snapshot.meta.current_folder;
            let listing_changed = snap.meta.listing_epoch != state.snapshot.meta.listing_epoch;
            let editor_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.pixel_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.pixel_revision));
            let editor_orig_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.original_revision))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.original_revision));
            let editor_selection_changed = snap.meta.editor.as_ref().map(|e| {
                (
                    e.id,
                    e.path.clone(),
                    e.selection.selection_revision,
                    e.selection.has_selection,
                )
            }) != state.snapshot.meta.editor.as_ref().map(|e| {
                (
                    e.id,
                    e.path.clone(),
                    e.selection.selection_revision,
                    e.selection.has_selection,
                )
            });
            let editor_active_layer_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.active_layer_id))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.active_layer_id));
            let editor_dims_changed = snap
                .meta
                .editor
                .as_ref()
                .map(|e| (e.id, e.path.clone(), e.width, e.height))
                != state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| (e.id, e.path.clone(), e.width, e.height));
            if let Some(folder) = &snap.meta.current_folder {
                state.folder_input = folder.clone();
            }
            if folder_changed {
                state.browser_filter.clear();
                state.last_click = None;
            }
            if let Some(ed) = &snap.meta.editor {
                if state.editor_ui.adjust_popup_tab.is_none() || editor_changed {
                    state.pending_adj = ed.adjustments;
                }
                if editor_dims_changed {
                    state.editor_ui.sync_document_dims(ed.width, ed.height);
                }
            }
            if !state.settings_open
                || snap.meta.translate_settings != state.snapshot.meta.translate_settings
            {
                state.translate_draft = snap.meta.translate_settings.clone();
            }
            if state.pending_layout_commands == 0 && snap.meta.layout != state.last_core_layout {
                state.browser_preview_open = snap.meta.layout.panel("browser.preview")
                    || (snap.meta.layout.panel("browser.inspector")
                        && !snap.meta.layout.panel("browser.info"));
                state.browser_info_open = snap.meta.layout.panel("browser.info")
                    || (snap.meta.layout.panel("browser.inspector")
                        && !snap.meta.layout.panel("browser.preview"));
                state.browser_inspector_open =
                    state.browser_preview_open || state.browser_info_open;
                state.browser_quick_access_open = snap.meta.layout.panel("browser.quick_access");
                state.browser_explorer_open = snap.meta.layout.panel("browser.explorer");
                state.browser_left_panels.set_order(
                    snap.meta.layout.browser_left_panel_order.clone(),
                    &BrowserLeftPanel::ALL,
                );
                state.browser_right_panels.set_order(
                    snap.meta.layout.browser_right_panel_order.clone(),
                    &BrowserRightPanel::ALL,
                );
                state.viewer_info_open = snap.meta.layout.panel("viewer.info")
                    || snap.meta.layout.panel("viewer.inspector");
                state.viewer_adjustments_open = snap.meta.layout.panel("viewer.adjustments");
                state.viewer_color_popover_open = state.viewer_adjustments_open;
                state.viewer_translate.active = snap.meta.layout.panel("viewer.translate");
                state.viewer_right_panels.set_order(
                    snap.meta.layout.viewer_right_panel_order.clone(),
                    &ViewerRightPanel::ALL,
                );
                state.viewer_filmstrip_open = snap.meta.layout.panel("viewer.filmstrip");
                state.editor_tool_tab = snap.meta.layout.editor_tool_tab;
                state.editor_history_open = snap.meta.layout.panel("editor.history");
                state.editor_layers_open = snap.meta.layout.panel("editor.layers");
                state.editor_panels.set_order(
                    snap.meta.layout.editor_right_panel_order.clone(),
                    &EditorRightPanel::ALL,
                );
                state.viewer_viewport_background = snap.meta.layout.viewer_viewport_background;
                sync_editor_panel_compat(state);
            }
            let open_tab_ids: BTreeSet<u64> = snap.meta.editor_tabs.iter().map(|t| t.id).collect();
            state
                .editor_tab_view_states
                .retain(|id, _| open_tab_ids.contains(id));

            if (prev_tab_id != next_tab_id
                || (prev_ws == Workspace::Editor && snap.meta.workspace != Workspace::Editor))
                && prev_ws == Workspace::Editor
                && let Some(old_id) = prev_tab_id
                && open_tab_ids.contains(&old_id)
            {
                state.editor_tab_view_states.insert(
                    old_id,
                    EditorTabUiCache {
                        zoom: state.zoom,
                        pan_x: state.pan_x,
                        pan_y: state.pan_y,
                        crop_uv: state.crop_uv,
                        tool: state.editor_ui.tool,
                        pending_adj: state.pending_adj,
                        adjust_popup_tab: state.editor_ui.adjust_popup_tab,
                    },
                );
            }

            let incoming_core_layout = snap.meta.layout.clone();
            if snap.meta.workspace != prev_ws {
                state.active_menu = None;
                state.context_menu = None;
                state.crop_uv = None;
                state.editor_ui.renaming_layer_id = None;
                state.editor_ui.adjust_popup_tab = None;
                close_all_adjustment_dialogs(state);
                state.browser_left_panels.cancel_drag();
                state.browser_right_panels.cancel_drag();
                state.viewer_right_panels.cancel_drag();
                state.editor_panels.cancel_drag();
                sync_editor_panel_compat(state);
                state.viewer_color_popover_open = false;
                if snap.meta.workspace == Workspace::Viewer {
                    state.reset_viewer_presentation_from_layout(&snap.meta.layout);
                    state.viewer_adj = ColorAdjustments::default();
                } else {
                    state.viewer_quarter_turns = 0;
                    state.viewer_translate.space_pan_held = false;
                    state.translate_drag = TranslateDragTarget::None;
                    state.viewer_translate.active_drag_delta = None;
                }
                if snap.meta.workspace == Workspace::Editor {
                    if let Some(cached) = next_tab_id
                        .and_then(|id| state.editor_tab_view_states.get(&id))
                        .cloned()
                    {
                        state.zoom = cached.zoom;
                        state.pan_x = cached.pan_x;
                        state.pan_y = cached.pan_y;
                        state.crop_uv = cached.crop_uv;
                        state.editor_ui.tool = cached.tool;
                        state.pending_adj = cached.pending_adj;
                        state.editor_ui.adjust_popup_tab = cached.adjust_popup_tab;
                        if let Some(tab) = cached.adjust_popup_tab {
                            open_dialog_window(state, adjustment_dialog_id(tab));
                        }
                    } else {
                        state.zoom = 1.0;
                        state.pan_x = 0.0;
                        state.pan_y = 0.0;
                    }
                } else {
                    state.transform_preview = None;
                    state.transform_layer_id = None;
                    state.editor_ui.transform_spec = LayerTransformSpec::default();
                }
            } else if prev_tab_id != next_tab_id && snap.meta.workspace == Workspace::Editor {
                state.active_menu = None;
                state.context_menu = None;
                state.editor_ui.renaming_layer_id = None;
                close_all_adjustment_dialogs(state);
                state.transform_preview = None;
                state.transform_layer_id = None;
                state.editor_ui.transform_spec = LayerTransformSpec::default();
                if let Some(cached) = next_tab_id
                    .and_then(|id| state.editor_tab_view_states.get(&id))
                    .cloned()
                {
                    state.zoom = cached.zoom;
                    state.pan_x = cached.pan_x;
                    state.pan_y = cached.pan_y;
                    state.crop_uv = cached.crop_uv;
                    state.editor_ui.tool = cached.tool;
                    state.pending_adj = cached.pending_adj;
                    state.editor_ui.adjust_popup_tab = cached.adjust_popup_tab;
                    if let Some(tab) = cached.adjust_popup_tab {
                        open_dialog_window(state, adjustment_dialog_id(tab));
                    }
                } else {
                    state.zoom = 1.0;
                    state.pan_x = 0.0;
                    state.pan_y = 0.0;
                    state.crop_uv = None;
                    state.editor_ui.adjust_popup_tab = None;
                }
            }
            if snap.meta.editor.is_some()
                && let Some(adj) = state.transfer_adj_to_editor.take()
            {
                state.pending_adj = adj;
                let tab = AdjustmentPopupTab::ColorAndLight;
                state.editor_ui.adjust_popup_tab = Some(tab);
                close_all_adjustment_dialogs(state);
                open_dialog_window(state, adjustment_dialog_id(tab));
            }
            if editor_changed {
                state.crop_uv = None;
            }
            state.last_core_layout = incoming_core_layout;
            state.snapshot = snap;
            if state.pending_layout_commands > 0 {
                let mut merged = state.snapshot.meta.layout.clone();
                apply_live_layout_fields(state, &mut merged);
                Arc::make_mut(&mut state.snapshot).meta.layout = merged;
            }
            if archive_changed {
                state.image_sizes.clear();
                state.thumbnails_loading.clear();
                state.viewer_filmstrip_offset = 0.0;
            }
            let size_task = probe_viewer_image_sizes(state);

            let mut tasks = vec![sync_ui_tree_task(state), size_task];
            if viewer_changed && state.viewer_filmstrip_open {
                tasks.push(scroll_filmstrip_to_active(state));
            }
            if caption_changed || editor_changed {
                state.editor_ui.ai_caption_draft = state
                    .snapshot
                    .meta
                    .editor_caption
                    .clone()
                    .unwrap_or_default();
            }
            if viewer_changed {
                if let Some(prev_path) = state.viewer_translate.last_path.clone()
                    && !state.viewer_translate.blocks.is_empty()
                {
                    state
                        .viewer_translate
                        .bubble_cache
                        .insert(prev_path, state.viewer_translate.blocks.clone());
                }
                state.viewer_translate.picking_region = false;
                state.viewer_translate.editing_block_idx = None;
                state.viewer_translate.result_card_visible = false;
                state.viewer_translate.result_card_show_original = false;
                state.viewer_translate.active_drag_delta = None;
                state.viewer_translate.active_drag_page = None;
                state.viewer_translate.active_page_path = state.snapshot.meta.viewer_path.clone();
                state.viewer_translate.webtoon_marquee = None;
                state.viewer_translate.space_pan_held = false;
                state.translate_drag = TranslateDragTarget::None;
                state.pending_translate_queue.clear();
                state.active_translate_target = None;
                state.webtoon_marquee_drag = None;
                state.viewer_translate.show_original_indices.clear();
                state.viewer_translate.pending_replace_idx = None;
                state.viewer_translate.last_was_region = false;
                state.viewer_translate.blocks = state
                    .snapshot
                    .meta
                    .viewer_path
                    .as_ref()
                    .and_then(|p| state.viewer_translate.bubble_cache.get(p).cloned())
                    .unwrap_or_default();
                state.viewer_translate.saved_sidecar = false;
                state.viewer_translate.last_path = state.snapshot.meta.viewer_path.clone();
            }
            if (saved_tr_changed || viewer_changed)
                && let Some(saved) = &state.snapshot.meta.viewer_saved_translation
                && Some(saved.path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
            {
                state.viewer_translate.blocks = saved.blocks.clone();
                state.viewer_translate.show_original_indices.clear();
                state
                    .viewer_translate
                    .bubble_cache
                    .insert(saved.path.clone(), saved.blocks.clone());
                state.viewer_translate.saved_sidecar = true;
            } else if saved_tr_changed && state.snapshot.meta.viewer_saved_translation.is_none() {
                state.viewer_translate.saved_sidecar = false;
            }
            if translation_changed && let Some(tr_res) = &state.snapshot.meta.translation {
                let res_path = tr_res.path.clone();
                let is_primary = Some(res_path.as_str())
                    == state.snapshot.meta.viewer_path.as_deref()
                    || state.viewer_translate.active_page_path.as_deref()
                        == Some(res_path.as_str())
                    || state.active_translate_target.as_deref() == Some(res_path.as_str());
                if is_primary || state.viewer_mode != ViewerViewMode::Single {
                    let mut target_blocks = state
                        .viewer_translate
                        .bubble_cache
                        .get(&res_path)
                        .cloned()
                        .unwrap_or_else(|| {
                            if Some(res_path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
                            {
                                state.viewer_translate.blocks.clone()
                            } else {
                                Vec::new()
                            }
                        });
                    let mut new_sel_idx = None;
                    if let Some(rep_idx) = state.viewer_translate.pending_replace_idx.take()
                        && rep_idx < target_blocks.len()
                    {
                        if let Some(first) = tr_res.blocks.first() {
                            target_blocks[rep_idx] = first.clone();
                            for (offset, extra) in tr_res.blocks.iter().skip(1).enumerate() {
                                target_blocks.insert(rep_idx + 1 + offset, extra.clone());
                            }
                            new_sel_idx = Some(rep_idx);
                        }
                    } else if state.viewer_translate.last_was_region {
                        let start_idx = target_blocks.len();
                        target_blocks.extend(tr_res.blocks.iter().cloned());
                        if !tr_res.blocks.is_empty() {
                            new_sel_idx = Some(start_idx);
                        }
                    } else {
                        target_blocks = tr_res.blocks.clone();
                        state.viewer_translate.show_original_indices.clear();
                        state
                            .viewer_translate
                            .show_original_page_blocks
                            .retain(|(p, _)| p != &res_path);
                    }
                    state.viewer_translate.last_was_region = false;
                    state
                        .viewer_translate
                        .bubble_cache
                        .insert(res_path.clone(), target_blocks.clone());
                    let should_focus_blocks = Some(res_path.as_str())
                        == state.snapshot.meta.viewer_path.as_deref()
                        || state.active_translate_target.as_deref() == Some(res_path.as_str())
                        || new_sel_idx.is_some()
                        || state.viewer_translate.blocks.is_empty();
                    if should_focus_blocks {
                        state.viewer_translate.blocks = target_blocks;
                        state.viewer_translate.active_page_path = Some(res_path);
                        state.viewer_translate.editing_block_idx = new_sel_idx;
                        state.viewer_translate.result_card_visible =
                            !state.viewer_translate.blocks.is_empty() || new_sel_idx.is_some();
                    }
                    state.viewer_translate.saved_sidecar =
                        state.snapshot.meta.viewer_saved_translation.is_some();
                }
            }
            if let Some(task) = state.snapshot.meta.ai_task.clone()
                && !task.running
            {
                if let Some(next_cmd) = state.pending_translate_queue.pop_front() {
                    if let Command::ViewerTranslate { ref path, .. } = next_cmd {
                        state.active_translate_target = Some(path.clone());
                    }
                    tasks.push(dispatch_to_core(state.core.clone(), next_cmd));
                } else {
                    state.active_translate_target = None;
                }
                if let Some(err) = &task.error
                    && state.last_ai_error.as_ref() != Some(err)
                {
                    state.last_ai_error = Some(err.clone());
                    tasks.push(show_toast(state, err.clone(), ToastKind::Error));
                } else {
                    if task.error.is_none() {
                        state.last_ai_error = None;
                    }
                    if let Some(clip) = &task.clipboard_text
                        && !clip.is_empty()
                        && (was_ai_running || state.last_ai_clipboard.as_ref() != Some(clip))
                    {
                        state.last_ai_clipboard = Some(clip.clone());
                        tasks.push(iced::clipboard::write(clip.clone()));
                        tasks.push(show_toast(
                            state,
                            tr(state.snapshot.meta.language, "toast-clipboard-copied"),
                            ToastKind::Success,
                        ));
                    }
                    if let Some(saved_p) = task.saved_path.as_ref()
                        && task.saved_path != prev_ai_saved
                    {
                        state.thumbnails.remove(saved_p);
                        state.thumbnails_loading.remove(saved_p);
                        state.webtoon_pages.remove(saved_p);
                        state.webtoon_loading.remove(saved_p);
                        if state.snapshot.meta.workspace == Workspace::Viewer {
                            if state.viewer_mode == ViewerViewMode::Double {
                                let (left_slot, right_slot) =
                                    dual_spread_slots(state.viewer_view_state(true));
                                if let Some((_, lp)) = left_slot {
                                    tasks.push(load_viewer_frame_task(&state.core, lp.to_owned()));
                                }
                                if let Some((_, rp)) = right_slot {
                                    tasks.push(load_viewer_pair_frame_task(
                                        &state.core,
                                        Some(rp.to_owned()),
                                    ));
                                }
                            } else if let Some(vp) = state.snapshot.meta.viewer_path.clone() {
                                tasks.push(load_viewer_frame_task(&state.core, vp));
                            }
                        }
                    }
                }
            } else if state
                .snapshot
                .meta
                .ai_task
                .as_ref()
                .is_some_and(|t| t.running)
            {
                state.last_ai_error = None;
            }
            if root_changed {
                state.expanded_folders.clear();
                state.folder_children.clear();
            }
            if root_changed || tree_changed || folder_changed {
                tasks.push(sync_expanded_tree_tasks(
                    state,
                    root_changed || tree_changed,
                ));
            }
            if folder_changed {
                state.webtoon_pages.clear();
                state.webtoon_loading.clear();
                state.webtoon_order.clear();
                state.webtoon_scroll_y = 0.0;
                state.webtoon_visible_index = None;
                state.browser_visible_range = Some((0, BROWSER_INITIAL_VISIBLE_ENTRIES));
                state.thumbnails_loading.clear();
                state.viewer_filmstrip_offset = 0.0;
                tasks.push(schedule_missing_thumbnails(state));
            } else if listing_changed {
                for entry in &state.snapshot.entries {
                    state.thumbnails.remove(&entry.path);
                    state.image_sizes.remove(&entry.path);
                }
                state.webtoon_pages.clear();
                state.webtoon_loading.clear();
                state.thumbnails_loading.clear();
                tasks.push(schedule_missing_thumbnails(state));
                if state.viewer_frame.is_some() || state.viewer_pair_frame.is_some() {
                    let v_pres = state.viewer_ui_presentation();
                    tasks.push(reload_active_frames(
                        &state.core,
                        &state.snapshot,
                        &state.image_sizes,
                        &v_pres,
                    ));
                }
            }
            if viewer_changed {
                state.zoom = 1.0;
                state.pan_x = 0.0;
                state.pan_y = 0.0;
                state.viewer_quarter_turns = 0;
                state.webtoon_visible_index = state.snapshot.meta.viewer_index;
                let vmode = state.viewer_mode;
                if vmode == ViewerViewMode::Double {
                    let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                    let lp = left_slot.map(|(_, p)| p.to_owned());
                    let rp = right_slot.map(|(_, p)| p.to_owned());
                    if let Some(lp) = lp {
                        tasks.push(load_viewer_frame_task(&state.core, lp));
                    } else {
                        state.viewer_frame = None;
                    }
                    if let Some(rp) = rp {
                        tasks.push(load_viewer_pair_frame_task(&state.core, Some(rp)));
                    } else {
                        state.viewer_pair_frame = None;
                    }
                } else {
                    if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                        tasks.push(load_viewer_frame_task(&state.core, path));
                    } else {
                        state.viewer_frame = None;
                    }
                    state.viewer_pair_frame = None;
                }
                if vmode == ViewerViewMode::Webtoon {
                    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
                    tasks.push(schedule_webtoon_pages_near(state, center));
                    if let Some(y) = webtoon_offset_for_index(state.viewer_view_state(true), center)
                    {
                        tasks.push(widget_operation::scroll_to(
                            webtoon_scroll_id(),
                            scrollable::AbsoluteOffset {
                                x: None,
                                y: Some(y),
                            },
                        ));
                    }
                }
            } else if state.viewer_mode == ViewerViewMode::Double {
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                let left_mismatch = match (left_slot, &state.viewer_frame) {
                    (Some((_, lp)), Some(f)) => f.key != format!("viewer:{lp}"),
                    (Some(_), None) | (None, Some(_)) => true,
                    (None, None) => false,
                };
                let right_mismatch = match (right_slot, &state.viewer_pair_frame) {
                    (Some((_, rp)), Some(f)) => f.key != format!("viewer:{rp}"),
                    (Some(_), None) | (None, Some(_)) => true,
                    (None, None) => false,
                };
                if left_mismatch || right_mismatch {
                    tasks.push(reload_dual_frames_task(state));
                }
            } else {
                state.viewer_pair_frame = None;
            }
            if state.snapshot.meta.workspace == Workspace::Viewer
                && snap_viewer_mode_needs_thumbs(
                    &state.snapshot,
                    state.viewer_mode,
                    state.viewer_filmstrip_open,
                )
            {
                tasks.push(schedule_missing_thumbnails(state));
            }
            if editor_active_layer_changed
                && matches!(
                    state.editor_ui.tool,
                    EditorTool::Transform | EditorTool::Pointer
                )
            {
                state.transform_layer_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                state.editor_ui.transform_spec = LayerTransformSpec::default();
            }
            if editor_changed {
                if state.snapshot.meta.editor.is_some() {
                    let tab_id = state.snapshot.meta.active_editor_tab_id.unwrap_or(0);
                    tasks.push(load_editor_frame_task(state.core.clone(), tab_id));
                    tasks.push(load_editor_thumbnails_task(state.core.clone()));
                    if matches!(
                        state.editor_ui.tool,
                        EditorTool::Transform | EditorTool::Pointer
                    ) || state.editor_ui.smart_paste.is_some()
                    {
                        let layer_id = state
                            .editor_ui
                            .smart_paste
                            .map(|sp| sp.layer_id)
                            .or(state.transform_layer_id);
                        tasks.push(load_editor_transform_frames_task(
                            state.core.clone(),
                            layer_id,
                        ));
                    }
                } else {
                    state.editor_frame = None;
                    state.editor_histogram = None;
                    state.transform_preview = None;
                    state.transform_layer_id = None;
                    state.editor_layer_thumbs.clear();
                    state.editor_mask_thumbs.clear();
                }
            } else if (editor_selection_changed || editor_active_layer_changed)
                && state.snapshot.meta.editor.is_some()
                && matches!(
                    state.editor_ui.tool,
                    EditorTool::Transform | EditorTool::Pointer
                )
            {
                let layer_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                state.transform_layer_id = layer_id;
                tasks.push(load_editor_transform_frames_task(
                    state.core.clone(),
                    layer_id,
                ));
            }
            if editor_orig_changed
                || (state.snapshot.meta.editor.is_some() && state.editor_original_frame.is_none())
            {
                if state.snapshot.meta.editor.is_some() {
                    let tab_id = state.snapshot.meta.active_editor_tab_id.unwrap_or(0);
                    tasks.push(load_editor_original_frame_task(state.core.clone(), tab_id));
                } else {
                    state.editor_original_frame = None;
                }
            }
            if editor_selection_changed {
                if state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .is_some_and(|ed| ed.selection.has_selection)
                {
                    tasks.push(load_editor_selection_mask_task(state.core.clone()));
                } else {
                    state.editor_selection_mask = None;
                }
            }
            if viewer_changed && is_standalone_viewer() {
                let active_canonical_path = state
                    .snapshot
                    .meta
                    .viewer_path
                    .as_deref()
                    .map(canonicalize_path_str)
                    .unwrap_or_default();
                let open_canonical_paths = if active_canonical_path.as_os_str().is_empty() {
                    Vec::new()
                } else {
                    vec![active_canonical_path.clone()]
                };
                tasks.push(Task::perform(
                    async move {
                        let app_paths = photon_storage::AppPaths::current();
                        let pid = std::process::id();
                        let info = photon_ipc::ViewerInstanceInfo {
                            pid,
                            socket_path: photon_ipc::viewer_socket_path(&app_paths, pid),
                            active_canonical_path,
                            open_canonical_paths,
                        };
                        let _ = photon_ipc::update_viewer_instance(&app_paths, &info);
                    },
                    |_| Message::Noop,
                ));
            }
            if focus_changed
                && let Some(win_id) = state
                    .main_window
                    .filter(|id| state.windows.contains_key(id))
                    .or(state
                        .last_active_window
                        .filter(|id| state.windows.contains_key(id)))
            {
                tasks.push(Task::done(Message::WakeWindow(win_id)));
            }
            if !state.recovery_prompted
                && state.snapshot.meta.pending_recovery_count > 0
                && state.dialog.is_none()
            {
                state.recovery_prompted = true;
                state.dialog = Some(InAppDialog::RecoveryPrompt {
                    count: state.snapshot.meta.pending_recovery_count,
                });
                open_dialog_window(state, DesktopDialogId::RecoveryPrompt);
            }
            if let Some(closing_id) = state.exit_closing_tab_id {
                if !state
                    .snapshot
                    .meta
                    .editor_tabs
                    .iter()
                    .any(|t| t.id == closing_id)
                {
                    state.exit_closing_tab_id = None;
                    if state.pending_exit && state.dialog.is_none() {
                        tasks.push(update(state, Message::RequestAppExit));
                    }
                }
            } else if state.pending_exit && state.dialog.is_none() {
                tasks.push(update(state, Message::RequestAppExit));
            }
            Task::batch(tasks)
        }

        _ => unreachable!(),
    }
}

pub(super) fn handle_core_event(
    state: &mut PhotonGui,
    ev: photon_protocol::Event,
) -> Task<Message> {
    match Message::CoreEvent(ev) {
        Message::CoreEvent(ev) => match ev.kind {
            EventKind::ScreenshotRequested {
                request_id,
                output_path,
            } => {
                if let Some(win_id) = state.main_window {
                    let path = output_path.unwrap_or_else(|| "/tmp/photon-shot.png".to_owned());
                    state.pending_screenshots.insert(win_id, (request_id, path));
                    // Wait briefly so any pending StateUpdated relayout + RedrawRequested
                    // finishes populating iced_wgpu's layers before capturing.
                    return Task::perform(
                        tokio::time::sleep(std::time::Duration::from_millis(40)),
                        move |_| Message::TriggerWindowScreenshot(win_id),
                    );
                }
                Task::none()
            }
            EventKind::UiActionRequested { target_id, .. } => {
                handle_semantic_ui_action(state, &target_id)
            }
            _ => Task::none(),
        },

        _ => unreachable!(),
    }
}

pub(super) fn handle_command_completed(
    state: &mut PhotonGui,
    res: Result<CommandResponse, String>,
) -> Task<Message> {
    match Message::CommandCompleted(res) {
        Message::CommandCompleted(Ok(resp)) => {
            let lang = state.snapshot.meta.language;
            if !resp.ok {
                state.pending_exit = false;
                state.exit_closing_tab_id = None;
                state.pending_preview_commit_rev = None;
                if resp
                    .error
                    .as_ref()
                    .is_some_and(|e| e.code == "save_as_required")
                {
                    return update(state, Message::PickSaveAsDialog);
                }
                let msg = resp
                    .ui
                    .as_ref()
                    .map(|u| tr_ui_text(lang, u))
                    .or_else(|| resp.error.as_ref().map(|e| e.message.clone()))
                    .unwrap_or_else(|| tr(lang, "dialog-error-title"));
                return show_toast(state, msg, ToastKind::Error);
            }
            if let Some(ui) = &resp.ui
                && matches!(
                    ui.code.as_str(),
                    "editor_saved"
                        | "editor_exported"
                        | "created_folder"
                        | "config_reset_done"
                        | "favorite_added"
                        | "favorite_removed"
                        | "trashed_items"
                        | "deleted_items"
                        | "copied_items"
                        | "moved_items"
                        | "renamed_item"
                        | "translate_done"
                        | "batch_done"
                        | "plugins_refreshed"
                        | "plugin_enabled"
                        | "plugin_disabled"
                        | "plugin_installed"
                        | "plugin_uninstalled"
                        | "plugin_executed"
                        | "toast_copied"
                        | "toast_cut"
                        | "toast_pasted"
                )
            {
                if matches!(ui.code.as_str(), "editor_saved" | "editor_exported")
                    && let Some(ed_path) = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .map(|ed| ed.path.clone())
                        .filter(|p| !p.is_empty())
                {
                    state.thumbnails.remove(&ed_path);
                    state.thumbnails_loading.remove(&ed_path);
                    state.image_sizes.remove(&ed_path);
                    state.webtoon_pages.remove(&ed_path);
                    state.webtoon_loading.remove(&ed_path);
                    let mut tasks =
                        vec![show_toast(state, tr_ui_text(lang, ui), ToastKind::Success)];
                    if state.snapshot.meta.workspace == Workspace::Browser {
                        tasks.push(schedule_missing_thumbnails(state));
                    }
                    if state.snapshot.meta.viewer_path.as_deref() == Some(ed_path.as_str()) {
                        tasks.push(load_viewer_frame_task(&state.core, ed_path));
                    }
                    return Task::batch(tasks);
                }
                return show_toast(state, tr_ui_text(lang, ui), ToastKind::Success);
            }
            Task::none()
        }
        Message::CommandCompleted(Err(err)) => {
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::Alert,
                title: tr(state.snapshot.meta.language, "dialog-error-title"),
                message: err,
            });
            open_dialog_window(state, DesktopDialogId::Alert);
            Task::none()
        }

        _ => unreachable!(),
    }
}

pub(super) const ALL_ADJUSTMENT_DIALOG_IDS: [DesktopDialogId; 4] = [
    DesktopDialogId::AdjustColor,
    DesktopDialogId::AdjustHsv,
    DesktopDialogId::AdjustColorBalance,
    DesktopDialogId::AdjustCurves,
];

pub(super) const fn adjustment_dialog_id(tab: AdjustmentPopupTab) -> DesktopDialogId {
    match tab {
        AdjustmentPopupTab::ColorAndLight => DesktopDialogId::AdjustColor,
        AdjustmentPopupTab::Hsv => DesktopDialogId::AdjustHsv,
        AdjustmentPopupTab::ColorBalance => DesktopDialogId::AdjustColorBalance,
        AdjustmentPopupTab::Curves => DesktopDialogId::AdjustCurves,
    }
}

pub(super) fn close_all_adjustment_dialogs(state: &mut PhotonGui) {
    for id in ALL_ADJUSTMENT_DIALOG_IDS {
        state.dialog_manager.close(&id);
    }
}

pub(super) fn sync_editor_panel_compat(state: &mut PhotonGui) {
    state.editor_panel_order = state.editor_panels.order().to_vec();
    state.dragging_editor_panel = state.editor_panels.dragging();
    state.editor_panel_drop_target = state.editor_panels.drop_target();
}

pub(super) fn toggle_browser_right_panel(
    state: &mut PhotonGui,
    panel: BrowserRightPanel,
) -> Task<Message> {
    match panel {
        BrowserRightPanel::Preview => state.browser_preview_open = !state.browser_preview_open,
        BrowserRightPanel::Info => state.browser_info_open = !state.browser_info_open,
    }
    state.browser_inspector_open = state.browser_preview_open || state.browser_info_open;
    let layout_task = commit_layout_with(state, |_| {});
    Task::batch([layout_task, sync_ui_tree_task(state)])
}

pub(super) fn apply_live_layout_fields(
    state: &PhotonGui,
    layout: &mut photon_protocol::LayoutState,
) {
    let any_browser_right = state.browser_preview_open || state.browser_info_open;
    layout
        .panels
        .insert("browser.inspector".to_owned(), any_browser_right);
    layout
        .panels
        .insert("browser.preview".to_owned(), state.browser_preview_open);
    layout
        .panels
        .insert("browser.info".to_owned(), state.browser_info_open);
    layout.panels.insert(
        "browser.quick_access".to_owned(),
        state.browser_quick_access_open,
    );
    layout
        .panels
        .insert("browser.explorer".to_owned(), state.browser_explorer_open);
    layout
        .panels
        .insert("viewer.inspector".to_owned(), state.viewer_info_open);
    layout
        .panels
        .insert("viewer.info".to_owned(), state.viewer_info_open);
    layout.panels.insert(
        "viewer.adjustments".to_owned(),
        state.viewer_adjustments_open,
    );
    layout
        .panels
        .insert("viewer.translate".to_owned(), state.viewer_translate.active);
    layout
        .panels
        .insert("viewer.filmstrip".to_owned(), state.viewer_filmstrip_open);
    layout
        .panels
        .insert("editor.history".to_owned(), state.editor_history_open);
    layout
        .panels
        .insert("editor.layers".to_owned(), state.editor_layers_open);
    layout.browser_left_panel_order = state.browser_left_panels.order().to_vec();
    layout.browser_right_panel_order = state.browser_right_panels.order().to_vec();
    layout.viewer_right_panel_order = state.viewer_right_panels.order().to_vec();
    layout.editor_tool_tab = state.editor_tool_tab;
    layout.editor_right_panel_order = state.editor_panels.order().to_vec();
}

pub(super) fn editor_status_summary(state: &PhotonGui, lang: Language) -> String {
    if let Some(ed) = &state.snapshot.meta.editor {
        let modified_mark = if ed.modified { " *" } else { "" };
        format!("{}{} · {}×{}", ed.name, modified_mark, ed.width, ed.height)
    } else {
        let s = tr_ui_text(lang, &state.snapshot.meta.status);
        if s.is_empty() { tr(lang, "ready") } else { s }
    }
}

pub(super) fn open_dialog_window(state: &mut PhotonGui, id: DesktopDialogId) {
    if id != DesktopDialogId::Settings && state.settings_open {
        state.settings_open = false;
        state.settings_confirm_factory_reset = false;
        state.dialog_manager.close(&DesktopDialogId::Settings);
    }
    state.pending_focus_widget = Some("photon.dialog.first_input");
    state.active_dialog_window = state
        .last_active_window
        .filter(|wid| state.windows.contains_key(wid))
        .or(state.main_window)
        .or_else(|| state.windows.keys().next().copied());
    state
        .dialog_manager
        .open_with(id, default_window_state_for(id), false);
}

pub(super) fn commit_layout_with(
    state: &mut PhotonGui,
    mutate: impl FnOnce(&mut photon_protocol::LayoutState),
) -> Task<Message> {
    let mut layout = state.snapshot.meta.layout.clone();
    apply_live_layout_fields(state, &mut layout);
    mutate(&mut layout);
    Arc::make_mut(&mut state.snapshot).meta.layout = layout.clone();
    state.pending_layout_commands += 1;
    let core = state.core.clone();
    Task::perform(
        async move {
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                Command::SetLayout { layout },
            );
            core.dispatch(env).await
        },
        Message::LayoutCommandCompleted,
    )
}

pub(super) const fn batch_dialog_id(kind: BatchDialogKind) -> DesktopDialogId {
    match kind {
        BatchDialogKind::Resize => DesktopDialogId::BatchResize,
        BatchDialogKind::Crop => DesktopDialogId::BatchCrop,
        BatchDialogKind::Convert => DesktopDialogId::BatchConvert,
        BatchDialogKind::Transform => DesktopDialogId::BatchTransform,
    }
}

pub(super) fn dialog_id_for_in_app(dialog: &InAppDialog) -> DesktopDialogId {
    match dialog {
        InAppDialog::SelectionModify { .. } => DesktopDialogId::SelectionModify,
        InAppDialog::NewFolder { .. } => DesktopDialogId::NewFolder,
        InAppDialog::NewDocument { .. } => DesktopDialogId::NewDocument,
        InAppDialog::ConfirmResetLayout => DesktopDialogId::ConfirmResetLayout,
        InAppDialog::ConfirmUnsavedLeave { .. } => DesktopDialogId::ConfirmUnsavedLeave,
        InAppDialog::Resize { .. } => DesktopDialogId::Resize,
        InAppDialog::CanvasSize { .. } => DesktopDialogId::CanvasSize,
        InAppDialog::Export { .. } => DesktopDialogId::Export,
        InAppDialog::ColorPicker { .. } => DesktopDialogId::ColorPicker,
        InAppDialog::Batch(b) => batch_dialog_id(b.kind),
        InAppDialog::RenameItem { .. } => DesktopDialogId::RenameItem,
        InAppDialog::ConfirmDeletePermanently { .. } => DesktopDialogId::ConfirmDeletePermanently,
        InAppDialog::AdjustDenoise { .. } => DesktopDialogId::AdjustDenoise,
        InAppDialog::ManagePlugins { .. } => DesktopDialogId::ManagePlugins,
        InAppDialog::PluginParams { .. } => DesktopDialogId::PluginParams,
        InAppDialog::PluginModels { .. } => DesktopDialogId::PluginModels,
        InAppDialog::TranslateSettings => DesktopDialogId::TranslateSettings,
        InAppDialog::TransformEach { .. } => DesktopDialogId::TransformEach,
        InAppDialog::VectorBlend { .. } => DesktopDialogId::VectorBlend,
        InAppDialog::ExportVectorSvg { .. } => DesktopDialogId::ExportVectorSvg,
        InAppDialog::SliceImage { .. } => DesktopDialogId::SliceImage,
        InAppDialog::RecoveryPrompt { .. } => DesktopDialogId::RecoveryPrompt,
        InAppDialog::SaveSelection { .. } => DesktopDialogId::SaveSelection,
        InAppDialog::LoadSelection { .. } => DesktopDialogId::LoadSelection,
        InAppDialog::NewGuide { .. } => DesktopDialogId::NewGuide,
        InAppDialog::GridSettings { .. } => DesktopDialogId::GridSettings,
        InAppDialog::Alert { id, .. } => *id,
    }
}

pub(super) fn show_toast(state: &mut PhotonGui, message: String, kind: ToastKind) -> Task<Message> {
    state.toast_seq += 1;
    let id = state.toast_seq;
    state.toast = Some(ToastState { id, message, kind });
    Task::perform(tokio::time::sleep(Duration::from_secs(3)), move |_| {
        Message::DismissToast(Some(id))
    })
}

pub(super) fn dispatch_to_core(core: CoreHandle, command: Command) -> Task<Message> {
    Task::perform(
        async move {
            let env = Envelope::new(
                format!("gui-{}", std::process::id()),
                ClientKind::Gui,
                command,
            );
            core.dispatch(env).await
        },
        Message::CommandCompleted,
    )
}
