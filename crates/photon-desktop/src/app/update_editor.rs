use super::*;

pub(super) fn handle_editor_message(state: &mut PhotonGui, message: Message) -> Task<Message> {
    match message {
        Message::SetBrightness(v) => {
            state.pending_adj.brightness = v;
            Task::none()
        }
        Message::SetContrast(v) => {
            state.pending_adj.contrast = v;
            Task::none()
        }
        Message::SetSaturation(v) => {
            state.pending_adj.saturation = v;
            Task::none()
        }
        Message::SetExposure(v) => {
            state.pending_adj.exposure = v;
            Task::none()
        }
        Message::SetGamma(v) => {
            state.pending_adj.gamma = v.clamp(0.2, 3.0);
            Task::none()
        }
        Message::SetHighlights(v) => {
            state.pending_adj.highlights = v;
            Task::none()
        }
        Message::SetShadows(v) => {
            state.pending_adj.shadows = v;
            Task::none()
        }
        Message::ApplyColorAdjustments => dispatch_to_core(
            state.core.clone(),
            Command::EditorAdjustColor {
                brightness: state.pending_adj.brightness,
                contrast: state.pending_adj.contrast,
                saturation: state.pending_adj.saturation,
                exposure: state.pending_adj.exposure,
                highlights: state.pending_adj.highlights,
                shadows: state.pending_adj.shadows,
                gamma: state.pending_adj.gamma,
                target: state.editor_ui.apply_target,
                selection_only: state.editor_ui.apply_to_selection,
            },
        ),
        Message::EditorToggleApplyToSelection(v) => {
            state.editor_ui.apply_to_selection = v;
            Task::none()
        }
        Message::EditorToggleAdjustLivePreview => {
            state.editor_ui.adjust_live_preview = !state.editor_ui.adjust_live_preview;
            Task::none()
        }
        Message::EditorResetPendingAdjustments => {
            state.pending_adj = ColorAdjustments::default();
            state.editor_ui.reset_adjustments();
            Task::none()
        }
        Message::EditorOpenAdjustPopup(tab) => {
            state.active_menu = None;
            state.context_menu = None;
            if let Some(ed) = &state.snapshot.meta.editor {
                if let Some(active) = ed.layers.iter().find(|l| l.id == ed.active_layer_id)
                    && let Some(effect) = &active.effect
                {
                    match effect {
                        LayerEffect::ColorLight(adj) => state.pending_adj = *adj,
                        LayerEffect::Hsv(hsv) => state.editor_ui.hsv = *hsv,
                        LayerEffect::ColorBalance(cb) => state.editor_ui.balance = *cb,
                        LayerEffect::Curves(c) => state.editor_ui.curves = c.clone(),
                        _ => {}
                    }
                }
                close_all_adjustment_dialogs(state);
                state.editor_ui.adjust_popup_tab = Some(tab);
                open_dialog_window(state, adjustment_dialog_id(tab));
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::EditorCloseAdjustPopup { apply } => {
            let tab = state.editor_ui.adjust_popup_tab.take();
            close_all_adjustment_dialogs(state);
            if !apply {
                state.pending_adj = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|e| e.adjustments)
                    .unwrap_or_default();
                state.editor_ui.reset_adjustments();
                return sync_ui_tree_task(state);
            }
            let matching_effect_update = state.snapshot.meta.editor.as_ref().and_then(|ed| {
                let active = ed.layers.iter().find(|l| l.id == ed.active_layer_id)?;
                let eff = active.effect.as_ref()?;
                let next = match (tab, eff) {
                    (Some(AdjustmentPopupTab::ColorAndLight), LayerEffect::ColorLight(_)) => {
                        LayerEffect::ColorLight(state.pending_adj)
                    }
                    (Some(AdjustmentPopupTab::Hsv), LayerEffect::Hsv(_)) => {
                        LayerEffect::Hsv(state.editor_ui.hsv)
                    }
                    (Some(AdjustmentPopupTab::ColorBalance), LayerEffect::ColorBalance(_)) => {
                        LayerEffect::ColorBalance(state.editor_ui.balance)
                    }
                    (Some(AdjustmentPopupTab::Curves), LayerEffect::Curves(_)) => {
                        LayerEffect::Curves(state.editor_ui.curves.clone())
                    }
                    _ => return None,
                };
                Some(Command::EditorLayerUpdateEffect {
                    id: active.id,
                    effect: next,
                })
            });
            let cmd_task = if let Some(cmd) = matching_effect_update {
                dispatch_to_core(state.core.clone(), cmd)
            } else {
                match tab {
                    Some(AdjustmentPopupTab::ColorAndLight) => {
                        update(state, Message::ApplyColorAdjustments)
                    }
                    Some(AdjustmentPopupTab::Hsv) => update(state, Message::EditorApplyHsv),
                    Some(AdjustmentPopupTab::ColorBalance) => {
                        update(state, Message::EditorApplyColorBalance)
                    }
                    Some(AdjustmentPopupTab::Curves) => update(state, Message::EditorApplyCurves),
                    None => Task::none(),
                }
            };
            state.pending_adj = ColorAdjustments::default();
            state.editor_ui.reset_adjustments();
            Task::batch([cmd_task, sync_ui_tree_task(state)])
        }
        Message::EditorSetLiquifyMode(m) => {
            state.editor_ui.liquify_mode = m;
            Task::none()
        }
        Message::EditorSetLiquifyRadius(r) => {
            state.editor_ui.liquify_radius = r.clamp(5.0, 150.0);
            Task::none()
        }
        Message::EditorSetLiquifyStrength(s) => {
            state.editor_ui.liquify_strength = s.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorSelectTool(tool) => {
            state.active_menu = None;
            state.context_menu = None;
            state.editor_tool_hold = None;
            state.editor_ui.tool = tool;
            if tool.is_marquee_group() {
                state.editor_ui.marquee_subtool = tool;
            } else if tool.is_lasso_group() {
                state.editor_ui.lasso_subtool = tool;
            } else if tool.is_smart_group() {
                state.editor_ui.smart_subtool = tool;
            } else if tool.is_shape_group() {
                state.editor_ui.shape_subtool = tool;
            } else if tool.is_fill_group() {
                state.editor_ui.fill_subtool = tool;
            } else if matches!(tool, EditorTool::Pointer | EditorTool::Transform) {
                state.editor_ui.move_subtool = tool;
            } else if matches!(tool, EditorTool::Crop | EditorTool::RotateAngle) {
                state.editor_ui.crop_subtool = tool;
            } else if matches!(tool, EditorTool::HealingBrush | EditorTool::Liquify) {
                state.editor_ui.retouch_subtool = tool;
            } else if matches!(tool, EditorTool::Brush | EditorTool::Eraser) {
                state.editor_ui.brush_subtool = tool;
            } else if matches!(tool, EditorTool::BezierPen | EditorTool::PathSelect) {
                state.editor_ui.pen_subtool = tool;
            } else if matches!(tool, EditorTool::Hand | EditorTool::Zoom) {
                state.editor_ui.nav_subtool = tool;
            } else if matches!(
                tool,
                EditorTool::RemoveBg | EditorTool::Watermark | EditorTool::Caption
            ) {
                state.editor_ui.ai_subtool = tool;
            }
            if tool == EditorTool::Crop {
                if state.crop_uv.is_none() {
                    state.crop_uv =
                        initial_crop_uv_from_editor(state.snapshot.meta.editor.as_ref());
                }
                state.pending_focus_widget = Some("photon.tool.first_input");
            } else {
                state.crop_uv = None;
            }
            if tool != EditorTool::Gradient {
                state.editor_ui.gradient_draft = None;
            }
            if matches!(tool, EditorTool::Transform | EditorTool::Pointer) {
                state.editor_ui.transform_spec = LayerTransformSpec::default();
                state.transform_layer_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                if state.snapshot.meta.editor.is_some() {
                    return Task::batch([
                        load_editor_transform_frames_task(
                            state.core.clone(),
                            state.transform_layer_id,
                        ),
                        sync_ui_tree_task(state),
                    ]);
                }
            } else {
                state.transform_preview = None;
                state.transform_layer_id = None;
                state.editor_ui.transform_spec = LayerTransformSpec::default();
            }
            sync_ui_tree_task(state)
        }
        Message::EditorSetSelectOp(op) => {
            state.editor_ui.select_op = op;
            Task::none()
        }
        Message::EditorSetSelectFeather(v) => {
            state.editor_ui.select_feather = v.clamp(0.0, 250.0);
            Task::none()
        }
        Message::EditorToggleSelectAntiAlias => {
            state.editor_ui.select_anti_alias = !state.editor_ui.select_anti_alias;
            Task::none()
        }
        Message::EditorSetWandTolerance(v) => {
            state.editor_ui.wand_tolerance = v;
            Task::none()
        }
        Message::EditorToggleWandContiguous => {
            state.editor_ui.wand_contiguous = !state.editor_ui.wand_contiguous;
            Task::none()
        }
        Message::EditorToggleSelectSampleAll => {
            state.editor_ui.select_sample_all = !state.editor_ui.select_sample_all;
            Task::none()
        }
        Message::EditorSetQuickSelectRadius(v) => {
            state.editor_ui.quick_select_radius = v.clamp(2.0, 200.0);
            Task::none()
        }
        Message::EditorSetBrushSmoothingMode(m) => {
            state.editor_ui.brush_settings.smoothing_mode = m;
            Task::none()
        }
        Message::EditorSetStabilizerDistance(d) => {
            state.editor_ui.brush_settings.stabilizer_distance = d.clamp(4.0, 100.0);
            Task::none()
        }
        Message::EditorSetShapeFillMode(m) => {
            state.editor_ui.shape_fill_mode = m;
            Task::none()
        }
        Message::EditorSetShapeStrokeWidth(w) => {
            state.editor_ui.shape_stroke_width = w.clamp(1.0, 64.0);
            Task::none()
        }
        Message::EditorSetFloodFillTolerance(t) => {
            state.editor_ui.flood_fill_tolerance = t;
            Task::none()
        }
        Message::EditorToggleFloodFillContiguous => {
            state.editor_ui.flood_fill_contiguous = !state.editor_ui.flood_fill_contiguous;
            Task::none()
        }
        Message::EditorSetGradientKind(k) => {
            state.editor_ui.gradient_kind = k;
            Task::none()
        }
        Message::EditorSetGradientColorMode(c) => {
            state.editor_ui.gradient_color_mode = c;
            match c {
                GradientColorMode::FgToBg => {
                    state.editor_ui.gradient_preset_id = "fg_to_bg".to_owned();
                }
                GradientColorMode::FgToTransparent => {
                    state.editor_ui.gradient_preset_id = "fg_to_transparent".to_owned();
                }
                GradientColorMode::Custom => {
                    state.editor_ui.ensure_custom_gradient_stops();
                }
            }
            Task::none()
        }
        Message::EditorCycleCompareMode => {
            state.context_menu = None;
            state.editor_ui.compare_mode = match state.editor_ui.compare_mode {
                CompareMode::Off => CompareMode::Split,
                CompareMode::Split => CompareMode::SideBySide,
                CompareMode::SideBySide => CompareMode::HoldOriginal,
                CompareMode::HoldOriginal => CompareMode::Off,
            };
            sync_ui_tree_task(state)
        }
        Message::EditorSetCompareMode(mode) => {
            state.active_menu = None;
            state.context_menu = None;
            state.editor_ui.compare_mode = if state.editor_ui.compare_mode == mode {
                CompareMode::Off
            } else {
                mode
            };
            sync_ui_tree_task(state)
        }
        Message::EditorSwapColors => {
            let old_fg = state.editor_ui.fg_color;
            if let Some(bg) = state.editor_ui.bg_color {
                state.editor_ui.fg_color = bg;
            }
            state.editor_ui.bg_color = Some(old_fg);
            Task::none()
        }
        Message::EditorResetColors => {
            state.editor_ui.fg_color = [0, 0, 0];
            state.editor_ui.bg_color = Some([255, 255, 255]);
            Task::none()
        }
        Message::OpenColorPicker(target) => {
            state.active_menu = None;
            let dark = state.snapshot.meta.theme == Theme::Dark;
            let (rgb, transparent) = match target {
                ColorPickerTarget::Foreground => (state.editor_ui.fg_color, false),
                ColorPickerTarget::Background => match state.editor_ui.bg_color {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CanvasBackground => match state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .and_then(|e| e.canvas_background)
                {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CropFill => match state.editor_ui.crop_fill {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::CanvasSizeFill => {
                    if let Some(InAppDialog::CanvasSize {
                        dims, anchor, fill, ..
                    }) = &state.dialog
                    {
                        state.canvas_size_draft = Some((dims.clone(), *anchor, *fill));
                        state.reopen_canvas_size_after_picker = true;
                        state.editor_ui.canvas_fill = *fill;
                    }
                    match state.editor_ui.canvas_fill {
                        Some(c) => (c, false),
                        None => ([255, 255, 255], true),
                    }
                }
                ColorPickerTarget::RotateFill => match state.editor_ui.rotate_fill {
                    Some(c) => (c, false),
                    None => ([255, 255, 255], true),
                },
                ColorPickerTarget::ViewportBackground => {
                    let c = state
                        .viewport_background
                        .unwrap_or_else(|| default_viewport_bg_rgb(dark));
                    (c, false)
                }
                ColorPickerTarget::ViewerViewportBg => {
                    let c = state
                        .viewer_viewport_background
                        .unwrap_or_else(|| default_viewport_bg_rgb(dark));
                    (c, false)
                }
                ColorPickerTarget::GradientStop(idx) => {
                    let stops = state.editor_ui.raw_gradient_stops();
                    if let Some(stop) = stops.get(idx) {
                        (stop.color, stop.alpha <= 0.01)
                    } else {
                        (state.editor_ui.fg_color, false)
                    }
                }
                ColorPickerTarget::VectorShapeFill => {
                    let col = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_vector_data.as_ref())
                        .and_then(|v| {
                            v.selected_shape_ids
                                .first()
                                .and_then(|id| v.shape(*id))
                                .or_else(|| v.shapes.last())
                        })
                        .map(|s| match s.fill {
                            Some(color) => (color, false),
                            None => (state.editor_ui.fg_color, true),
                        });
                    col.unwrap_or((state.editor_ui.fg_color, false))
                }
                ColorPickerTarget::VectorShapeStroke => {
                    let col = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_vector_data.as_ref())
                        .and_then(|v| {
                            v.selected_shape_ids
                                .first()
                                .and_then(|id| v.shape(*id))
                                .or_else(|| v.shapes.last())
                        })
                        .map(|s| (s.stroke.color, !s.stroke.is_visible()));
                    col.unwrap_or((state.editor_ui.fg_color, false))
                }
                ColorPickerTarget::TextLayerColor => {
                    let col = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_text_data.as_ref())
                        .map(|t| t.color)
                        .unwrap_or(state.editor_ui.text_draft.color);
                    (col, false)
                }
            };
            state.dialog = Some(InAppDialog::ColorPicker {
                target,
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
                hex: format_hex(rgb),
                transparent,
                previous_viewport_bg: if target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background
                } else {
                    state.viewport_background
                },
            });
            open_dialog_window(state, DesktopDialogId::ColorPicker);
            Task::none()
        }
        Message::ColorPickerSetRgb(nr, ng, nb) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
            {
                *r = nr;
                *g = ng;
                *b = nb;
                *hex = format_hex([nr, ng, nb]);
                *transparent = false;
                if *target == ColorPickerTarget::ViewportBackground {
                    state.viewport_background = Some([nr, ng, nb]);
                } else if *target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background = Some([nr, ng, nb]);
                }
            }
            Task::none()
        }
        Message::ColorPickerPickScreen => Task::perform(
            photon_dialogs::pick_screen_color(),
            Message::ColorPickerScreenPicked,
        ),
        Message::ColorPickerScreenPicked(Some([nr, ng, nb])) => {
            update(state, Message::ColorPickerSetRgb(nr, ng, nb))
        }
        Message::ColorPickerScreenPicked(None) => Task::none(),
        Message::ColorPickerHexChanged(val) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
            {
                *hex = val.clone();
                if let Some([nr, ng, nb]) = parse_hex_rgb(&val) {
                    *r = nr;
                    *g = ng;
                    *b = nb;
                    *transparent = false;
                    if *target == ColorPickerTarget::ViewportBackground {
                        state.viewport_background = Some([nr, ng, nb]);
                    } else if *target == ColorPickerTarget::ViewerViewportBg {
                        state.viewer_viewport_background = Some([nr, ng, nb]);
                    }
                }
            }
            Task::none()
        }
        Message::ColorPickerToggleTransparent(t) => {
            if let Some(InAppDialog::ColorPicker {
                target,
                transparent,
                ..
            }) = &mut state.dialog
                && *target != ColorPickerTarget::ViewportBackground
                && *target != ColorPickerTarget::ViewerViewportBg
                && *target != ColorPickerTarget::Foreground
            {
                *transparent = t;
            }
            Task::none()
        }
        Message::ResetViewportBackground => {
            let dark = state.snapshot.meta.theme == Theme::Dark;
            let def = default_viewport_bg_rgb(dark);
            if let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                hex,
                transparent,
                ..
            }) = &mut state.dialog
                && matches!(
                    target,
                    ColorPickerTarget::ViewportBackground | ColorPickerTarget::ViewerViewportBg
                )
            {
                if *target == ColorPickerTarget::ViewerViewportBg {
                    state.viewer_viewport_background = None;
                } else {
                    state.viewport_background = None;
                }
                *r = def[0];
                *g = def[1];
                *b = def[2];
                *hex = format_hex(def);
                *transparent = false;
            }
            Task::none()
        }
        Message::ConfirmColorPicker => {
            let Some(InAppDialog::ColorPicker {
                target,
                r,
                g,
                b,
                transparent,
                ..
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::ColorPicker);
            let picked = if transparent { None } else { Some([r, g, b]) };
            match target {
                ColorPickerTarget::Foreground => {
                    state.editor_ui.fg_color = [r, g, b];
                    Task::none()
                }
                ColorPickerTarget::Background => {
                    state.editor_ui.bg_color = picked;
                    Task::none()
                }
                ColorPickerTarget::CropFill => {
                    state.editor_ui.crop_fill = picked;
                    Task::none()
                }
                ColorPickerTarget::CanvasSizeFill => {
                    state.editor_ui.canvas_fill = picked;
                    if std::mem::take(&mut state.reopen_canvas_size_after_picker)
                        && let Some((dims, anchor, _)) = state.canvas_size_draft.take()
                    {
                        state.dialog = Some(InAppDialog::CanvasSize {
                            dims,
                            anchor,
                            fill: picked,
                            error: None,
                        });
                        open_dialog_window(state, DesktopDialogId::CanvasSize);
                    }
                    Task::none()
                }
                ColorPickerTarget::RotateFill => {
                    state.editor_ui.rotate_fill = picked;
                    Task::none()
                }
                ColorPickerTarget::ViewportBackground => {
                    let dark = state.snapshot.meta.theme == Theme::Dark;
                    if state.viewport_background.is_none()
                        && [r, g, b] == default_viewport_bg_rgb(dark)
                    {
                        state.viewport_background = None;
                    } else {
                        state.viewport_background = Some([r, g, b]);
                    }
                    Task::none()
                }
                ColorPickerTarget::ViewerViewportBg => {
                    let bg = Some([r, g, b]);
                    state.viewer_viewport_background = bg;
                    commit_layout_with(state, |layout| {
                        layout.viewer_viewport_background = bg;
                    })
                }
                ColorPickerTarget::CanvasBackground => {
                    if std::mem::take(&mut state.reopen_new_doc_after_picker)
                        && let Some((dims, prev_rgb)) = state.new_doc_draft.take()
                    {
                        let next_rgb = if transparent { prev_rgb } else { [r, g, b] };
                        let bg = if transparent {
                            NewDocBackground::Transparent
                        } else {
                            NewDocBackground::Custom(next_rgb)
                        };
                        state.dialog = Some(InAppDialog::NewDocument {
                            dims,
                            background: bg,
                            custom_rgb: next_rgb,
                            error: None,
                        });
                        open_dialog_window(state, DesktopDialogId::NewDocument);
                        return Task::none();
                    }
                    dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSetCanvasBackground {
                            color: picked,
                            transparent,
                        },
                    )
                }
                ColorPickerTarget::GradientStop(idx) => {
                    state.editor_ui.ensure_custom_gradient_stops();
                    if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                        stop.color = [r, g, b];
                        if transparent {
                            stop.alpha = 0.0;
                        } else if stop.alpha <= 0.01 {
                            stop.alpha = 1.0;
                        }
                    }
                    Task::none()
                }
                ColorPickerTarget::VectorShapeFill => {
                    if let Some(mut shape) = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_vector_data.as_ref())
                        .and_then(|v| {
                            v.selected_shape_ids
                                .first()
                                .and_then(|id| v.shape(*id))
                                .or_else(|| v.shapes.last())
                        })
                        .cloned()
                    {
                        shape.fill = if transparent { None } else { Some([r, g, b]) };
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorUpdateVectorShape {
                                layer_id: None,
                                shape,
                            },
                        );
                    }
                    Task::none()
                }
                ColorPickerTarget::VectorShapeStroke => {
                    if let Some(mut shape) = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_vector_data.as_ref())
                        .and_then(|v| {
                            v.selected_shape_ids
                                .first()
                                .and_then(|id| v.shape(*id))
                                .or_else(|| v.shapes.last())
                        })
                        .cloned()
                    {
                        shape.stroke.color = [r, g, b];
                        if transparent {
                            shape.stroke.width = 0.0;
                        } else if !shape.stroke.is_visible() {
                            shape.stroke.width = state.editor_ui.shape_stroke_width.max(1.0);
                        }
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorUpdateVectorShape {
                                layer_id: None,
                                shape,
                            },
                        );
                    }
                    Task::none()
                }
                ColorPickerTarget::TextLayerColor => {
                    let rgb = [r, g, b];
                    state.editor_ui.text_draft.color = rgb;
                    if let Some(mut data) = state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|e| e.active_text_data.clone())
                    {
                        data.color = rgb;
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorUpdateTextLayer { id: None, data },
                        );
                    }
                    Task::none()
                }
            }
        }
        Message::EditorSetBrushRadius(v) => {
            state.editor_ui.brush_radius = v.clamp(1.0, 200.0);
            Task::none()
        }
        Message::EditorSetBrushOpacity(v) => {
            state.editor_ui.brush_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorSetBrushHardness(v) => {
            state.editor_ui.brush_settings.hardness = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetBrushFlow(v) => {
            state.editor_ui.brush_settings.flow = v.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorSetBrushSpacing(v) => {
            state.editor_ui.brush_settings.spacing = v.clamp(0.05, 2.0);
            Task::none()
        }
        Message::EditorSetBrushTip(tip) => {
            state.editor_ui.brush_settings.tip = tip;
            Task::none()
        }
        Message::EditorSetApplyTarget(target) => {
            state.editor_ui.apply_target = target;
            Task::none()
        }
        Message::EditorSetHsvHue(v) => {
            state.editor_ui.hsv.hue = v;
            Task::none()
        }
        Message::EditorSetHsvSat(v) => {
            state.editor_ui.hsv.saturation = v;
            Task::none()
        }
        Message::EditorSetHsvVal(v) => {
            state.editor_ui.hsv.value = v;
            Task::none()
        }
        Message::EditorApplyHsv => {
            let hsv = state.editor_ui.hsv;
            let target = state.editor_ui.apply_target;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAdjustHsv {
                    hue: hsv.hue,
                    saturation: hsv.saturation,
                    value: hsv.value,
                    target,
                    selection_only: state.editor_ui.apply_to_selection,
                },
            )
        }
        Message::EditorSetBalanceZone(zone) => {
            state.editor_ui.balance_zone = zone;
            Task::none()
        }
        Message::EditorSetBalanceAxis(axis, val) => {
            if axis < 3 {
                match state.editor_ui.balance_zone {
                    ColorBalanceZone::Shadows => state.editor_ui.balance.shadows[axis] = val,
                    ColorBalanceZone::Midtones => state.editor_ui.balance.midtones[axis] = val,
                    ColorBalanceZone::Highlights => state.editor_ui.balance.highlights[axis] = val,
                }
            }
            Task::none()
        }
        Message::EditorTogglePreserveLuminosity(keep) => {
            state.editor_ui.balance.preserve_luminosity = keep;
            Task::none()
        }
        Message::EditorApplyColorBalance => {
            let b = state.editor_ui.balance;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorColorBalance {
                    shadows: b.shadows,
                    midtones: b.midtones,
                    highlights: b.highlights,
                    preserve_luminosity: b.preserve_luminosity,
                    target: state.editor_ui.apply_target,
                    selection_only: state.editor_ui.apply_to_selection,
                },
            )
        }
        Message::EditorSelectCurvesChannel(ch) => {
            state.editor_ui.curves_channel = ch;
            Task::none()
        }
        Message::EditorCurvesChanged(pts) => {
            match state.editor_ui.curves_channel {
                CurvesChannel::Rgb => state.editor_ui.curves.rgb = pts,
                CurvesChannel::Red => state.editor_ui.curves.red = pts,
                CurvesChannel::Green => state.editor_ui.curves.green = pts,
                CurvesChannel::Blue => state.editor_ui.curves.blue = pts,
            }
            Task::none()
        }
        Message::EditorResetActiveCurvesChannel => {
            let linear = vec![[0.0, 0.0], [1.0, 1.0]];
            match state.editor_ui.curves_channel {
                CurvesChannel::Rgb => state.editor_ui.curves.rgb = linear,
                CurvesChannel::Red => state.editor_ui.curves.red = linear,
                CurvesChannel::Green => state.editor_ui.curves.green = linear,
                CurvesChannel::Blue => state.editor_ui.curves.blue = linear,
            }
            Task::none()
        }
        Message::EditorApplyCurves => {
            let c = state.editor_ui.curves.clone();
            state.editor_ui.curves = default_curves();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAdjustCurves {
                    rgb: c.rgb,
                    red: c.red,
                    green: c.green,
                    blue: c.blue,
                    target: state.editor_ui.apply_target,
                    selection_only: state.editor_ui.apply_to_selection,
                },
            )
        }
        Message::EditorSetFloodFillOpacity(v) => {
            state.editor_ui.flood_fill_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorToggleFloodFillSampleAll => {
            state.editor_ui.flood_fill_sample_all = !state.editor_ui.flood_fill_sample_all;
            Task::none()
        }
        Message::EditorSetGradientOpacity(v) => {
            state.editor_ui.gradient_opacity = v.clamp(0.01, 1.0);
            Task::none()
        }
        Message::EditorToggleGradientReverse => {
            state.editor_ui.gradient_reverse = !state.editor_ui.gradient_reverse;
            Task::none()
        }
        Message::EditorToggleGradientDither => {
            state.editor_ui.gradient_dither = !state.editor_ui.gradient_dither;
            Task::none()
        }
        Message::EditorSetGradientRepeat(repeat) => {
            state.editor_ui.gradient_repeat = repeat;
            Task::none()
        }
        Message::EditorSetGradientBlendMode(blend_mode) => {
            state.editor_ui.gradient_blend_mode = blend_mode;
            Task::none()
        }
        Message::EditorToggleGradientInteractive => {
            state.editor_ui.gradient_interactive = !state.editor_ui.gradient_interactive;
            if !state.editor_ui.gradient_interactive {
                state.editor_ui.gradient_draft = None;
            }
            Task::none()
        }
        Message::EditorApplyGradientPreset(preset_id) => {
            state.editor_ui.apply_gradient_preset(&preset_id);
            Task::none()
        }
        Message::EditorSelectGradientStop(idx) => {
            let count = state.editor_ui.raw_gradient_stops().len();
            state.editor_ui.gradient_selected_stop = idx.min(count.saturating_sub(1));
            Task::none()
        }
        Message::EditorAddGradientStop(pos) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if state.editor_ui.gradient_stops.len() < 12 {
                let p = pos.clamp(0.0, 1.0);
                let sorted = state.editor_ui.resolved_gradient_stops();
                let (color, alpha) = sample_gradient_stops(&sorted, p);
                state.editor_ui.gradient_stops.push(GradientStop {
                    pos: p,
                    color,
                    alpha,
                });
                state.editor_ui.gradient_selected_stop =
                    state.editor_ui.gradient_stops.len().saturating_sub(1);
            }
            Task::none()
        }
        Message::EditorRemoveSelectedGradientStop => {
            state.editor_ui.ensure_custom_gradient_stops();
            let idx = state.editor_ui.gradient_selected_stop;
            if state.editor_ui.gradient_stops.len() > 2
                && idx < state.editor_ui.gradient_stops.len()
            {
                state.editor_ui.gradient_stops.remove(idx);
                state.editor_ui.gradient_selected_stop = state
                    .editor_ui
                    .gradient_selected_stop
                    .min(state.editor_ui.gradient_stops.len().saturating_sub(1));
            }
            Task::none()
        }
        Message::EditorSetGradientStopPos(idx, pos) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.pos = pos.clamp(0.0, 1.0);
            }
            Task::none()
        }
        Message::EditorToggleGradientStopAlpha(idx) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.alpha = if stop.alpha > 0.5 { 0.0 } else { 1.0 };
            }
            Task::none()
        }
        Message::EditorSetGradientStopFg(idx) => {
            state.editor_ui.ensure_custom_gradient_stops();
            if let Some(stop) = state.editor_ui.gradient_stops.get_mut(idx) {
                stop.color = state.editor_ui.fg_color;
            }
            Task::none()
        }
        Message::EditorApplyGradientDraft => {
            let Some((start, end)) = state.editor_ui.gradient_draft else {
                return Task::none();
            };
            state.pending_preview_commit_rev = state.editor_frame.as_ref().map(|f| f.revision);
            dispatch_to_core(
                state.core.clone(),
                build_draw_gradient_command(&state.editor_ui, start, end),
            )
        }
        Message::EditorCancelGradientDraft => {
            state.editor_ui.gradient_draft = None;
            Task::none()
        }
        Message::EditorSetHealingMode(m) => {
            state.editor_ui.healing_mode = m;
            Task::none()
        }
        Message::EditorSetHealingRadius(r) => {
            state.editor_ui.healing_radius = r.clamp(1.0, 200.0);
            Task::none()
        }
        Message::EditorSetHealingHardness(h) => {
            state.editor_ui.healing_hardness = h.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetHealingStrength(s) => {
            state.editor_ui.healing_strength = s.clamp(0.05, 1.0);
            Task::none()
        }
        Message::EditorToggleHealingSampleAll => {
            state.editor_ui.healing_sample_all = !state.editor_ui.healing_sample_all;
            Task::none()
        }
        Message::EditorSetHealingSource(src) => {
            state.editor_ui.healing_source_px = src;
            Task::none()
        }
        Message::EditorSetDenoiseStrength(v) => {
            state.editor_ui.denoise_strength = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::EditorSetDenoiseDetail(v) => {
            state.editor_ui.denoise_detail = v.clamp(0.0, 1.0);
            Task::none()
        }
        Message::OpenDenoiseDialog => {
            state.active_menu = None;
            state.context_menu = None;
            if state.snapshot.meta.editor.is_some() {
                state.dialog = Some(InAppDialog::AdjustDenoise {
                    strength: state.editor_ui.denoise_strength,
                    detail_preserve: state.editor_ui.denoise_detail,
                    target: state.editor_ui.apply_target,
                    selection_only: state.editor_ui.apply_to_selection,
                });
                open_dialog_window(state, DesktopDialogId::AdjustDenoise);
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::DenoiseDialogStrengthChanged(v) => {
            let clamped = v.clamp(0.0, 1.0);
            state.editor_ui.denoise_strength = clamped;
            if let Some(InAppDialog::AdjustDenoise { strength, .. }) = &mut state.dialog {
                *strength = clamped;
            }
            Task::none()
        }
        Message::DenoiseDialogDetailChanged(v) => {
            let clamped = v.clamp(0.0, 1.0);
            state.editor_ui.denoise_detail = clamped;
            if let Some(InAppDialog::AdjustDenoise {
                detail_preserve, ..
            }) = &mut state.dialog
            {
                *detail_preserve = clamped;
            }
            Task::none()
        }
        Message::DenoiseDialogTargetChanged(t) => {
            state.editor_ui.apply_target = t;
            if let Some(InAppDialog::AdjustDenoise { target, .. }) = &mut state.dialog {
                *target = t;
            }
            Task::none()
        }
        Message::DenoiseDialogSelectionOnlyChanged(v) => {
            state.editor_ui.apply_to_selection = v;
            if let Some(InAppDialog::AdjustDenoise { selection_only, .. }) = &mut state.dialog {
                *selection_only = v;
            }
            Task::none()
        }
        Message::ConfirmDenoiseDialog => {
            let Some(InAppDialog::AdjustDenoise {
                strength,
                detail_preserve,
                target,
                selection_only,
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::AdjustDenoise);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorDenoise {
                    strength,
                    detail_preserve,
                    target,
                    selection_only,
                },
            )
        }
        Message::OpenSaveSelectionDialog => {
            if let Some(ed) = state.snapshot.meta.editor.as_ref()
                && ed.selection.has_selection
            {
                let next_idx = ed.selection.saved_selections.len() + 1;
                state.dialog = Some(InAppDialog::SaveSelection {
                    name: format!("Selection {next_idx}"),
                    target_id: None,
                    op: SelectionBooleanOp::Replace,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::SaveSelection);
            }
            Task::none()
        }
        Message::SaveSelectionNameChanged(v) => {
            if let Some(InAppDialog::SaveSelection { name, error, .. }) = &mut state.dialog {
                *name = v;
                *error = None;
            }
            Task::none()
        }
        Message::SaveSelectionTargetChanged(tid) => {
            if let Some(InAppDialog::SaveSelection {
                target_id,
                name,
                error,
                ..
            }) = &mut state.dialog
            {
                *target_id = tid;
                if let Some(id) = tid
                    && let Some(ed) = state.snapshot.meta.editor.as_ref()
                    && let Some(ch) = ed.selection.saved_selections.iter().find(|s| s.id == id)
                {
                    *name = ch.name.clone();
                }
                *error = None;
            }
            Task::none()
        }
        Message::SaveSelectionOpChanged(new_op) => {
            if let Some(InAppDialog::SaveSelection { op, .. }) = &mut state.dialog {
                *op = new_op;
            }
            Task::none()
        }
        Message::ConfirmSaveSelection => {
            let Some(InAppDialog::SaveSelection {
                name,
                target_id,
                op,
                ..
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::SaveSelection);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorSaveSelection {
                    name,
                    target_id,
                    op,
                },
            )
        }
        Message::OpenLoadSelectionDialog => {
            if let Some(ed) = state.snapshot.meta.editor.as_ref() {
                let first_id = ed.selection.saved_selections.first().map(|s| s.id);
                if first_id.is_some() {
                    state.dialog = Some(InAppDialog::LoadSelection {
                        selected_id: first_id,
                        op: SelectionBooleanOp::Replace,
                        invert: false,
                        error: None,
                    });
                    open_dialog_window(state, DesktopDialogId::LoadSelection);
                }
            }
            Task::none()
        }
        Message::LoadSelectionSelectedChanged(id) => {
            if let Some(InAppDialog::LoadSelection {
                selected_id, error, ..
            }) = &mut state.dialog
            {
                *selected_id = Some(id);
                *error = None;
            }
            Task::none()
        }
        Message::LoadSelectionOpChanged(new_op) => {
            if let Some(InAppDialog::LoadSelection { op, .. }) = &mut state.dialog {
                *op = new_op;
            }
            Task::none()
        }
        Message::LoadSelectionInvertChanged(inv) => {
            if let Some(InAppDialog::LoadSelection { invert, .. }) = &mut state.dialog {
                *invert = inv;
            }
            Task::none()
        }
        Message::DeleteSavedSelection(id) => {
            if let Some(InAppDialog::LoadSelection { selected_id, .. }) = &mut state.dialog
                && *selected_id == Some(id)
            {
                *selected_id = state.snapshot.meta.editor.as_ref().and_then(|ed| {
                    ed.selection
                        .saved_selections
                        .iter()
                        .find(|s| s.id != id)
                        .map(|s| s.id)
                });
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorDeleteSavedSelection { id },
            )
        }
        Message::ConfirmLoadSelection => {
            let Some(InAppDialog::LoadSelection {
                selected_id,
                op,
                invert,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let Some(id) = selected_id else {
                return Task::none();
            };
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::LoadSelection);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLoadSelection { id, op, invert },
            )
        }
        Message::OpenNewGuideDialog => {
            if state.snapshot.meta.editor.is_some() {
                state.dialog = Some(InAppDialog::NewGuide {
                    axis: photon_canvas::GuideAxis::Vertical,
                    value_input: "50".to_string(),
                    unit_percent: true,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::NewGuide);
            }
            Task::none()
        }
        Message::NewGuideAxisChanged(new_axis) => {
            if let Some(InAppDialog::NewGuide { axis, error, .. }) = &mut state.dialog {
                *axis = new_axis;
                *error = None;
            }
            Task::none()
        }
        Message::NewGuideValueChanged(v) => {
            if let Some(InAppDialog::NewGuide {
                value_input, error, ..
            }) = &mut state.dialog
            {
                *value_input = v;
                *error = None;
            }
            Task::none()
        }
        Message::NewGuideUnitChanged(pct) => {
            if let Some(InAppDialog::NewGuide {
                unit_percent,
                error,
                ..
            }) = &mut state.dialog
            {
                *unit_percent = pct;
                *error = None;
            }
            Task::none()
        }
        Message::ConfirmNewGuide => {
            let Some(InAppDialog::NewGuide {
                axis,
                value_input,
                unit_percent,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let Some(ed) = state.snapshot.meta.editor.as_ref() else {
                return Task::none();
            };
            let max_px = match axis {
                photon_canvas::GuideAxis::Horizontal => ed.height.max(1) as f32,
                photon_canvas::GuideAxis::Vertical => ed.width.max(1) as f32,
            };
            let Ok(raw) = value_input.trim().parse::<f32>() else {
                if let Some(InAppDialog::NewGuide { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "error-invalid-number"));
                }
                return Task::none();
            };
            let pos_px = if unit_percent {
                (raw / 100.0) * max_px
            } else {
                raw
            };
            if !pos_px.is_finite() || pos_px < 0.0 || pos_px > max_px {
                if let Some(InAppDialog::NewGuide { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "error-invalid-number"));
                }
                return Task::none();
            }
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::NewGuide);
            let id = state.editor_ui.next_guide_id;
            state.editor_ui.next_guide_id += 1;
            state.editor_ui.show_guides = true;
            state.editor_ui.guides.push(photon_canvas::EditorGuide {
                id,
                axis,
                position_px: pos_px.round(),
            });
            Task::none()
        }
        Message::OpenGridSettingsDialog => {
            state.dialog = Some(InAppDialog::GridSettings {
                spacing_input: format!("{}", state.editor_ui.grid_spacing_px.round() as u32),
                subdivisions_input: format!("{}", state.editor_ui.grid_subdivisions),
                opacity_input: format!("{}", (state.editor_ui.grid_opacity * 100.0).round() as u32),
                show_grid: state.editor_ui.show_grid,
                snap_to_grid: state.editor_ui.snap.to_grid,
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::GridSettings);
            Task::none()
        }
        Message::GridSettingsSpacingChanged(v) => {
            if let Some(InAppDialog::GridSettings {
                spacing_input,
                error,
                ..
            }) = &mut state.dialog
            {
                *spacing_input = v;
                *error = None;
            }
            Task::none()
        }
        Message::GridSettingsSubdivisionsChanged(v) => {
            if let Some(InAppDialog::GridSettings {
                subdivisions_input,
                error,
                ..
            }) = &mut state.dialog
            {
                *subdivisions_input = v;
                *error = None;
            }
            Task::none()
        }
        Message::GridSettingsOpacityChanged(v) => {
            if let Some(InAppDialog::GridSettings {
                opacity_input,
                error,
                ..
            }) = &mut state.dialog
            {
                *opacity_input = v;
                *error = None;
            }
            Task::none()
        }
        Message::GridSettingsShowGridChanged(v) => {
            if let Some(InAppDialog::GridSettings { show_grid, .. }) = &mut state.dialog {
                *show_grid = v;
            }
            Task::none()
        }
        Message::GridSettingsSnapChanged(v) => {
            if let Some(InAppDialog::GridSettings { snap_to_grid, .. }) = &mut state.dialog {
                *snap_to_grid = v;
            }
            Task::none()
        }
        Message::ConfirmGridSettings => {
            let Some(InAppDialog::GridSettings {
                spacing_input,
                subdivisions_input,
                opacity_input,
                show_grid,
                snap_to_grid,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let Ok(spacing) = spacing_input.trim().parse::<f32>() else {
                if let Some(InAppDialog::GridSettings { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "error-invalid-number"));
                }
                return Task::none();
            };
            let Ok(subdivisions) = subdivisions_input.trim().parse::<u32>() else {
                if let Some(InAppDialog::GridSettings { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "error-invalid-number"));
                }
                return Task::none();
            };
            let Ok(opacity_pct) = opacity_input.trim().parse::<f32>() else {
                if let Some(InAppDialog::GridSettings { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "error-invalid-number"));
                }
                return Task::none();
            };
            state.editor_ui.grid_spacing_px = spacing.clamp(8.0, 512.0);
            state.editor_ui.grid_subdivisions = subdivisions.clamp(1, 16);
            state.editor_ui.grid_opacity = (opacity_pct / 100.0).clamp(0.1, 2.0);
            state.editor_ui.show_grid = show_grid;
            state.editor_ui.snap.to_grid = snap_to_grid;
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::GridSettings);
            Task::none()
        }
        Message::EditorSetTransformSpec(spec) => {
            state.editor_ui.transform_spec = spec;
            if let Some(tp) = &mut state.transform_preview {
                tp.spec = spec;
            }
            Task::none()
        }
        Message::EditorTransformFramesLoaded(maybe_frames) => {
            if !matches!(
                state.editor_ui.tool,
                EditorTool::Transform | EditorTool::Pointer
            ) && state.editor_ui.smart_paste.is_none()
            {
                return Task::none();
            }
            let Some(frames) = maybe_frames else {
                state.transform_preview = None;
                return Task::none();
            };
            let rev_matches = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|ed| ed.pixel_revision == frames.pixel_revision);
            if !rev_matches {
                return Task::none();
            }
            let sel_rev = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map_or(0, |ed| ed.selection.selection_revision);
            let combined_rev = frames.pixel_revision ^ sel_rev.rotate_left(32);
            state.transform_layer_id = Some(frames.layer_id);
            state.transform_preview = Some(TransformPreview {
                base_frame: Some(CpuImageFrame {
                    key: format!("editor:transform-base:{}", frames.layer_id),
                    revision: combined_rev,
                    width: frames.canvas_width,
                    height: frames.canvas_height,
                    rgba: Arc::new(frames.base_rgba_excluding_active),
                }),
                layer_frame: CpuImageFrame {
                    key: format!("editor:transform-layer:{}", frames.layer_id),
                    revision: combined_rev,
                    width: frames.layer_width,
                    height: frames.layer_height,
                    rgba: Arc::new(frames.active_layer_rgba),
                },
                layer_offset: [frames.offset_x, frames.offset_y],
                layer_size: [frames.layer_width, frames.layer_height],
                spec: state.editor_ui.transform_spec,
            });
            Task::none()
        }
        Message::EditorTransformCommit => {
            let spec = state.editor_ui.transform_spec;
            let layer_id = state.transform_layer_id;
            if state.editor_ui.tool == EditorTool::Transform {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            if !spec.is_identity() {
                state.pending_preview_commit_rev = state.editor_frame.as_ref().map(|f| f.revision);
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorTransformLayer {
                        layer_id,
                        transform: spec,
                    },
                );
            }
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            if state.editor_ui.tool == EditorTool::Pointer && state.snapshot.meta.editor.is_some() {
                let active_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                state.transform_layer_id = active_id;
                return Task::batch([
                    load_editor_transform_frames_task(state.core.clone(), active_id),
                    sync_ui_tree_task(state),
                ]);
            }
            sync_ui_tree_task(state)
        }
        Message::EditorTransformCancel => {
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            if state.editor_ui.tool == EditorTool::Transform {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            if state.editor_ui.tool == EditorTool::Pointer && state.snapshot.meta.editor.is_some() {
                let active_id = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map(|ed| ed.active_layer_id);
                state.transform_layer_id = active_id;
                return Task::batch([
                    load_editor_transform_frames_task(state.core.clone(), active_id),
                    sync_ui_tree_task(state),
                ]);
            }
            sync_ui_tree_task(state)
        }

        Message::EditorCopyPixels { merged } => {
            state.active_menu = None;
            state.context_menu = None;
            state.browser_file_clipboard.clear();
            let core = state.core.clone();
            Task::perform(
                async move {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::EditorCopyPixels { merged },
                    );
                    core.dispatch(env).await
                },
                Message::EditorPixelsCopiedSyncClipboard,
            )
        }
        Message::EditorCutPixels => {
            state.active_menu = None;
            state.context_menu = None;
            state.browser_file_clipboard.clear();
            let core = state.core.clone();
            Task::perform(
                async move {
                    let env = Envelope::new(
                        format!("gui-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::EditorCutPixels,
                    );
                    core.dispatch(env).await
                },
                Message::EditorPixelsCopiedSyncClipboard,
            )
        }
        Message::EditorPixelsCopiedSyncClipboard(res) => {
            let toast_task = update(state, Message::CommandCompleted(res.clone()));
            if res.as_ref().is_ok_and(|r| r.ok) {
                let core = state.core.clone();
                let sync_task = Task::perform(
                    async move {
                        let Ok(Ok(val)) = core.query(Query::EditorClipboardPng).await else {
                            return None;
                        };
                        let bytes: Vec<u8> = val
                            .get("pngBytes")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())?;
                        let png_for_hash = bytes.clone();
                        tokio::task::spawn_blocking(move || {
                            let _ = write_png_bytes_to_clipboard(&bytes);
                        })
                        .await
                        .ok()?;
                        Some(png_for_hash)
                    },
                    |maybe_png| {
                        if let Some(png) = maybe_png {
                            Message::RecordSyncedClipboardPng(png)
                        } else {
                            Message::Noop
                        }
                    },
                );
                return Task::batch([toast_task, sync_task]);
            }
            toast_task
        }
        Message::RecordSyncedClipboardPng(png) => {
            state.last_synced_clipboard_png = Some(png);
            Task::none()
        }
        Message::EditorPastePixels { into_selection } => {
            state.active_menu = None;
            state.context_menu = None;
            let has_internal = state.snapshot.meta.has_pixel_clipboard;
            let last_synced = state.last_synced_clipboard_png.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let os_png = read_clipboard_png_bytes();
                        match os_png {
                            Some(bytes) => {
                                if has_internal && last_synced.as_ref() == Some(&bytes) {
                                    (None, false)
                                } else {
                                    (Some(bytes), false)
                                }
                            }
                            None => {
                                if has_internal {
                                    (None, false)
                                } else {
                                    (None, true)
                                }
                            }
                        }
                    })
                    .await
                    .unwrap_or((None, !has_internal))
                },
                move |(png_bytes, clipboard_empty)| Message::EditorPasteResolved {
                    into_selection,
                    png_bytes,
                    clipboard_empty,
                },
            )
        }
        Message::EditorPasteResolved {
            into_selection,
            png_bytes,
            clipboard_empty,
        } => {
            if clipboard_empty {
                return show_toast(
                    state,
                    tr(state.snapshot.meta.language, "toast-clipboard-empty"),
                    ToastKind::Info,
                );
            }
            let core = state.core.clone();
            Task::perform(
                async move {
                    core.dispatch(Envelope::new(
                        format!("gui-paste-{}", std::process::id()),
                        ClientKind::Gui,
                        Command::EditorPastePixels {
                            png_bytes,
                            into_selection,
                        },
                    ))
                    .await
                },
                Message::EditorPasteCompleted,
            )
        }
        Message::EditorPasteCompleted(res) => {
            let mut tasks = Vec::new();
            if let Ok(resp) = &res
                && resp.ok
                && let Some(data) = &resp.data
                && let (Some(layer_id), Some(w), Some(h), Some(cw), Some(ch)) = (
                    data.get("layerId").and_then(|v| v.as_u64()),
                    data.get("width").and_then(|v| v.as_u64()).map(|v| v as u32),
                    data.get("height")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32),
                    data.get("canvasWidth")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32),
                    data.get("canvasHeight")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32),
                )
            {
                let ox = data.get("offsetX").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let oy = data.get("offsetY").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let sp = SmartPasteState::new(layer_id, w, h, cw, ch, ox, oy);
                state.editor_ui.smart_paste = Some(sp);
                state.editor_ui.transform_spec = sp.to_transform_spec();
                state.editor_tool_tab = EditorToolTab::Options;
                tasks.push(load_editor_transform_frames_task(
                    state.core.clone(),
                    Some(layer_id),
                ));
            }
            tasks.push(update(state, Message::CommandCompleted(res)));
            tasks.push(sync_ui_tree_task(state));
            Task::batch(tasks)
        }
        Message::SmartPasteSetMode(mode) => {
            if let Some(sp) = &mut state.editor_ui.smart_paste {
                sp.apply_mode(mode);
                let spec = sp.to_transform_spec();
                let layer_id = sp.layer_id;
                state.editor_ui.transform_spec = spec;
                if let Some(tp) = &mut state.transform_preview {
                    tp.spec = spec;
                } else {
                    return load_editor_transform_frames_task(state.core.clone(), Some(layer_id));
                }
            }
            sync_ui_tree_task(state)
        }
        Message::SmartPasteSetWidth(w) => {
            if let Some(sp) = &mut state.editor_ui.smart_paste {
                sp.target_width = w.max(1.0);
                let spec = sp.to_transform_spec();
                let layer_id = sp.layer_id;
                state.editor_ui.transform_spec = spec;
                if let Some(tp) = &mut state.transform_preview {
                    tp.spec = spec;
                } else {
                    return load_editor_transform_frames_task(state.core.clone(), Some(layer_id));
                }
            }
            Task::none()
        }
        Message::SmartPasteSetHeight(h) => {
            if let Some(sp) = &mut state.editor_ui.smart_paste {
                sp.target_height = h.max(1.0);
                let spec = sp.to_transform_spec();
                let layer_id = sp.layer_id;
                state.editor_ui.transform_spec = spec;
                if let Some(tp) = &mut state.transform_preview {
                    tp.spec = spec;
                } else {
                    return load_editor_transform_frames_task(state.core.clone(), Some(layer_id));
                }
            }
            Task::none()
        }
        Message::SmartPasteSetOffsetX(x) => {
            if let Some(sp) = &mut state.editor_ui.smart_paste {
                sp.translate_x = x;
                let spec = sp.to_transform_spec();
                let layer_id = sp.layer_id;
                state.editor_ui.transform_spec = spec;
                if let Some(tp) = &mut state.transform_preview {
                    tp.spec = spec;
                } else {
                    return load_editor_transform_frames_task(state.core.clone(), Some(layer_id));
                }
            }
            Task::none()
        }
        Message::SmartPasteSetOffsetY(y) => {
            if let Some(sp) = &mut state.editor_ui.smart_paste {
                sp.translate_y = y;
                let spec = sp.to_transform_spec();
                let layer_id = sp.layer_id;
                state.editor_ui.transform_spec = spec;
                if let Some(tp) = &mut state.transform_preview {
                    tp.spec = spec;
                } else {
                    return load_editor_transform_frames_task(state.core.clone(), Some(layer_id));
                }
            }
            Task::none()
        }
        Message::SmartPasteApply => {
            let Some(sp) = state.editor_ui.smart_paste.take() else {
                return Task::none();
            };
            let spec = sp.to_transform_spec();
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            if !spec.is_identity() {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorTransformLayer {
                        layer_id: Some(sp.layer_id),
                        transform: spec,
                    },
                );
            }
            sync_ui_tree_task(state)
        }
        Message::SmartPasteDismiss => {
            state.editor_ui.smart_paste = None;
            state.editor_ui.transform_spec = LayerTransformSpec::default();
            state.transform_preview = None;
            state.transform_layer_id = None;
            sync_ui_tree_task(state)
        }
        Message::EditorSetRotateAngle(deg) => {
            state.editor_ui.rotate_angle = deg.clamp(-180.0, 180.0);
            Task::none()
        }
        Message::EditorApplyRotateAngle => {
            let degrees = state.editor_ui.rotate_angle;
            let fill = state.editor_ui.rotate_fill;
            state.editor_ui.tool = EditorTool::Pointer;
            if degrees.abs() > 1e-4 {
                state.pending_preview_commit_rev = state.editor_frame.as_ref().map(|f| f.revision);
            } else {
                state.editor_ui.rotate_angle = 0.0;
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorRotateAngle {
                    degrees,
                    fill,
                    fill_alpha: if fill.is_some() { 255 } else { 0 },
                    target: state.editor_ui.apply_target,
                },
            )
        }
        Message::OpenExportDialog => {
            state.active_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor {
                let p = Path::new(&doc.path);
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("jpg");
                let default_out = p
                    .parent()
                    .map(|dir| dir.join(format!("{stem}-export.{ext}")))
                    .unwrap_or_else(|| PathBuf::from(format!("{stem}-export.{ext}")));
                state.dialog = Some(InAppDialog::Export {
                    path: default_out.to_string_lossy().into_owned(),
                    quality: 92,
                    max_edge: String::new(),
                    metadata: true,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::Export);
            }
            Task::none()
        }
        Message::PickExportDialog => {
            let title = tr(state.snapshot.meta.language, "action-export");
            let current = match &state.dialog {
                Some(InAppDialog::Export { path, .. }) => PathBuf::from(path),
                _ => PathBuf::new(),
            };
            let start = current.parent().map(Path::to_path_buf);
            let name = current
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToOwned::to_owned);
            Task::perform(
                async move { photon_dialogs::save_image_as(&title, start, name.as_deref()).await },
                Message::ExportPathPicked,
            )
        }
        Message::ExportPathPicked(Some(picked)) => {
            if let Some(InAppDialog::Export { path, error, .. }) = &mut state.dialog {
                *path = picked.to_string_lossy().into_owned();
                *error = None;
            }
            Task::none()
        }
        Message::ExportPathPicked(None) => Task::none(),
        Message::ExportPathChanged(val) => {
            if let Some(InAppDialog::Export { path, error, .. }) = &mut state.dialog {
                *path = val;
                *error = None;
            }
            Task::none()
        }
        Message::ExportQualityChanged(q) => {
            if let Some(InAppDialog::Export { quality, .. }) = &mut state.dialog {
                *quality = q.clamp(10, 100);
            }
            Task::none()
        }
        Message::ExportMaxEdgeChanged(val) => {
            if let Some(InAppDialog::Export {
                max_edge, error, ..
            }) = &mut state.dialog
            {
                *max_edge = val;
                *error = None;
            }
            Task::none()
        }
        Message::ExportMetadataToggled(keep) => {
            if let Some(InAppDialog::Export { metadata, .. }) = &mut state.dialog {
                *metadata = keep;
            }
            Task::none()
        }
        Message::ConfirmExport => {
            let Some(InAppDialog::Export {
                path,
                quality,
                max_edge,
                metadata,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let clean_path = path.trim().to_owned();
            if clean_path.is_empty() {
                if let Some(InAppDialog::Export { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            let parsed_edge = if max_edge.trim().is_empty() {
                None
            } else if let Ok(v) = max_edge.trim().parse::<u32>()
                && v > 0
            {
                Some(v)
            } else {
                if let Some(InAppDialog::Export { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            };
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::Export);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorExport {
                    path: clean_path,
                    quality: Some(quality),
                    metadata,
                    max_edge: parsed_edge,
                },
            )
        }
        Message::EditorStartLayerRename(id, current_name) => {
            state.context_menu = None;
            state.editor_ui.renaming_layer_id = Some(id);
            state.editor_ui.layer_rename_input = current_name;
            Task::none()
        }
        Message::EditorLayerRenameChanged(val) => {
            state.editor_ui.layer_rename_input = val;
            Task::none()
        }
        Message::EditorCommitLayerRename => {
            let Some(id) = state.editor_ui.renaming_layer_id.take() else {
                return Task::none();
            };
            let name = state.editor_ui.layer_rename_input.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorLayerSetProps {
                    id,
                    name: Some(name),
                    visible: None,
                    locked: None,
                    clipped_to_below: None,
                    lock_alpha: None,
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                },
            )
        }
        Message::EditorCancelLayerRename => {
            state.editor_ui.renaming_layer_id = None;
            state.editor_ui.layer_rename_input.clear();
            Task::none()
        }

        _ => unreachable!(),
    }
}

pub(super) fn dispatch_editor_shortcut(
    state: &mut PhotonGui,
    action: EditorShortcutAction,
) -> Task<Message> {
    let active_layer_id = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .map(|e| e.active_layer_id);
    let has_selection = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .is_some_and(|e| e.selection.has_selection);
    match action {
        EditorShortcutAction::ToolMove => {
            update(state, Message::EditorSelectTool(EditorTool::Pointer))
        }
        EditorShortcutAction::ToolMarquee => update(
            state,
            Message::EditorSelectTool(state.editor_ui.marquee_subtool),
        ),
        EditorShortcutAction::ToolMarqueeCycle => {
            let next = match state.editor_ui.marquee_subtool {
                EditorTool::MarqueeRect => EditorTool::MarqueeEllipse,
                EditorTool::MarqueeEllipse => EditorTool::SingleRow,
                EditorTool::SingleRow => EditorTool::SingleCol,
                _ => EditorTool::MarqueeRect,
            };
            update(state, Message::EditorSelectTool(next))
        }
        EditorShortcutAction::ToolLasso => update(
            state,
            Message::EditorSelectTool(state.editor_ui.lasso_subtool),
        ),
        EditorShortcutAction::ToolLassoCycle => {
            let next = match state.editor_ui.lasso_subtool {
                EditorTool::Lasso => EditorTool::PolygonLasso,
                EditorTool::PolygonLasso => EditorTool::MagneticLasso,
                EditorTool::MagneticLasso => EditorTool::BezierSelect,
                _ => EditorTool::Lasso,
            };
            update(state, Message::EditorSelectTool(next))
        }
        EditorShortcutAction::ToolMagicWand => update(
            state,
            Message::EditorSelectTool(state.editor_ui.smart_subtool),
        ),
        EditorShortcutAction::ToolMagicWandCycle => {
            let next = match state.editor_ui.smart_subtool {
                EditorTool::MagicWand => EditorTool::QuickSelect,
                _ => EditorTool::MagicWand,
            };
            update(state, Message::EditorSelectTool(next))
        }
        EditorShortcutAction::ToolCrop => {
            update(state, Message::EditorSelectTool(EditorTool::Crop))
        }
        EditorShortcutAction::ToolEyedropper => {
            update(state, Message::EditorSelectTool(EditorTool::Eyedropper))
        }
        EditorShortcutAction::ToolBrush => {
            update(state, Message::EditorSelectTool(EditorTool::Brush))
        }
        EditorShortcutAction::ToolEraser => {
            update(state, Message::EditorSelectTool(EditorTool::Eraser))
        }
        EditorShortcutAction::ToolFillGroup => update(
            state,
            Message::EditorSelectTool(state.editor_ui.fill_subtool),
        ),
        EditorShortcutAction::ToolFillGroupCycle => {
            let next = match state.editor_ui.fill_subtool {
                EditorTool::Gradient => EditorTool::PaintBucket,
                _ => EditorTool::Gradient,
            };
            update(state, Message::EditorSelectTool(next))
        }
        EditorShortcutAction::ToolPen => {
            update(state, Message::EditorSelectTool(EditorTool::BezierPen))
        }
        EditorShortcutAction::ToolPathSelect => {
            update(state, Message::EditorSelectTool(EditorTool::PathSelect))
        }
        EditorShortcutAction::ToolShape => update(
            state,
            Message::EditorSelectTool(state.editor_ui.shape_subtool),
        ),
        EditorShortcutAction::ToolShapeCycle => {
            let next = match state.editor_ui.shape_subtool {
                EditorTool::DrawRect => EditorTool::DrawEllipse,
                EditorTool::DrawEllipse => EditorTool::DrawLine,
                EditorTool::DrawLine => EditorTool::DrawPolygon,
                EditorTool::DrawPolygon => EditorTool::DrawPolyline,
                _ => EditorTool::DrawRect,
            };
            update(state, Message::EditorSelectTool(next))
        }
        EditorShortcutAction::ToolText => {
            update(state, Message::EditorSelectTool(EditorTool::Text))
        }
        EditorShortcutAction::ToolHealingBrush => {
            update(state, Message::EditorSelectTool(EditorTool::HealingBrush))
        }
        EditorShortcutAction::ToolHand => {
            update(state, Message::EditorSelectTool(EditorTool::Hand))
        }
        EditorShortcutAction::ToolZoom => {
            update(state, Message::EditorSelectTool(EditorTool::Zoom))
        }
        EditorShortcutAction::ToolLiquify => {
            update(state, Message::EditorSelectTool(EditorTool::Liquify))
        }
        EditorShortcutAction::SwapForegroundBackground => update(state, Message::EditorSwapColors),
        EditorShortcutAction::ResetDefaultColors => {
            state.editor_ui.fg_color = [0, 0, 0];
            state.editor_ui.bg_color = Some([255, 255, 255]);
            Task::none()
        }
        EditorShortcutAction::ToggleQuickMask => {
            dispatch_to_core(state.core.clone(), Command::EditorSelectToggleQuickMask)
        }
        EditorShortcutAction::BrushSizeDecrease => {
            if state.editor_ui.tool == EditorTool::HealingBrush {
                update(
                    state,
                    Message::EditorSetHealingRadius(
                        (state.editor_ui.healing_radius - 2.0).max(1.0),
                    ),
                )
            } else if matches!(
                state.editor_ui.tool,
                EditorTool::BezierPen
                    | EditorTool::DrawLine
                    | EditorTool::DrawRect
                    | EditorTool::DrawEllipse
                    | EditorTool::DrawPolygon
                    | EditorTool::DrawPolyline
            ) {
                update(
                    state,
                    Message::Editor(EditorMessage::EditorSetShapeStrokeWidth(
                        (state.editor_ui.shape_stroke_width - 1.0).max(1.0),
                    )),
                )
            } else {
                update(
                    state,
                    Message::EditorSetBrushRadius((state.editor_ui.brush_radius - 2.0).max(1.0)),
                )
            }
        }
        EditorShortcutAction::BrushSizeIncrease => {
            if state.editor_ui.tool == EditorTool::HealingBrush {
                update(
                    state,
                    Message::EditorSetHealingRadius(
                        (state.editor_ui.healing_radius + 2.0).min(200.0),
                    ),
                )
            } else if matches!(
                state.editor_ui.tool,
                EditorTool::BezierPen
                    | EditorTool::DrawLine
                    | EditorTool::DrawRect
                    | EditorTool::DrawEllipse
                    | EditorTool::DrawPolygon
                    | EditorTool::DrawPolyline
            ) {
                update(
                    state,
                    Message::Editor(EditorMessage::EditorSetShapeStrokeWidth(
                        (state.editor_ui.shape_stroke_width + 1.0).min(64.0),
                    )),
                )
            } else {
                update(
                    state,
                    Message::EditorSetBrushRadius((state.editor_ui.brush_radius + 2.0).min(200.0)),
                )
            }
        }
        EditorShortcutAction::NewDocument => update(state, Message::OpenNewDocumentDialog),
        EditorShortcutAction::Save => dispatch_to_core(state.core.clone(), Command::EditorSave),
        EditorShortcutAction::SaveAs => update(state, Message::PickSaveAsDialog),
        EditorShortcutAction::Export => update(state, Message::OpenExportDialog),
        EditorShortcutAction::CloseTab => {
            if let Some(doc) = &state.snapshot.meta.editor {
                update(state, Message::EditorRequestCloseTab(doc.id))
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::ImageSize => update(state, Message::OpenResizeDialog),
        EditorShortcutAction::CanvasSize => update(state, Message::OpenCanvasSizeDialog),
        EditorShortcutAction::Undo => dispatch_to_core(state.core.clone(), Command::EditorUndo),
        EditorShortcutAction::Redo => dispatch_to_core(state.core.clone(), Command::EditorRedo),
        EditorShortcutAction::Cut => update(state, Message::EditorCutPixels),
        EditorShortcutAction::Copy => update(state, Message::EditorCopyPixels { merged: false }),
        EditorShortcutAction::CopyMerged => {
            update(state, Message::EditorCopyPixels { merged: true })
        }
        EditorShortcutAction::Paste => update(
            state,
            Message::EditorPastePixels {
                into_selection: false,
            },
        ),
        EditorShortcutAction::PasteIntoSelection => update(
            state,
            Message::EditorPastePixels {
                into_selection: true,
            },
        ),
        EditorShortcutAction::FreeTransform => {
            update(state, Message::EditorSelectTool(EditorTool::Transform))
        }
        EditorShortcutAction::SelectAll => {
            dispatch_to_core(state.core.clone(), Command::EditorSelectAll)
        }
        EditorShortcutAction::Deselect => {
            dispatch_to_core(state.core.clone(), Command::EditorDeselect)
        }
        EditorShortcutAction::Reselect => {
            dispatch_to_core(state.core.clone(), Command::EditorReselect)
        }
        EditorShortcutAction::InvertSelection => {
            dispatch_to_core(state.core.clone(), Command::EditorSelectInvert)
        }
        EditorShortcutAction::ClearSelectedPixels => {
            if has_selection {
                dispatch_to_core(state.core.clone(), Command::EditorSelectionClearPixels)
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::NewLayer => {
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
        EditorShortcutAction::DuplicateLayer => {
            if has_selection {
                dispatch_to_core(state.core.clone(), Command::EditorSelectionDuplicateLayer)
            } else if state.editor_ui.selected_layer_ids.len() >= 2 {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerDuplicateMultiple {
                        ids: state.editor_ui.selected_layer_ids.clone(),
                    },
                )
            } else if let Some(id) = active_layer_id {
                dispatch_to_core(state.core.clone(), Command::EditorLayerDuplicate { id })
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::LayerViaCut => {
            if has_selection {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerFromSelection { cut: true },
                )
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::MergeDown => {
            if let Some(id) = active_layer_id {
                dispatch_to_core(state.core.clone(), Command::EditorLayerMergeDown { id })
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::StampVisible => {
            dispatch_to_core(state.core.clone(), Command::EditorLayerStampVisible)
        }
        EditorShortcutAction::ToggleClippingMask => {
            if let Some(id) = active_layer_id {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerToggleClippingMask { id: Some(id) },
                )
            } else {
                Task::none()
            }
        }
        EditorShortcutAction::ZoomIn => update(state, Message::ZoomStep(1.25)),
        EditorShortcutAction::ZoomOut => update(state, Message::ZoomStep(0.8)),
        EditorShortcutAction::ZoomFit => update(state, Message::Canvas(CanvasEvent::ResetFit)),
        EditorShortcutAction::ZoomActual => update(state, Message::EditorZoomActual),
        EditorShortcutAction::ToggleRulers => {
            handle_editor_vector_text_message(state, EditorMessage::ToggleRulers)
        }
        EditorShortcutAction::ToggleGrid => {
            handle_editor_vector_text_message(state, EditorMessage::ToggleGrid)
        }
        EditorShortcutAction::ToggleGuides => {
            handle_editor_vector_text_message(state, EditorMessage::ToggleGuides)
        }
        EditorShortcutAction::ToggleSnap => handle_editor_vector_text_message(
            state,
            EditorMessage::ToggleSnap(photon_editor::SnapTarget::Master),
        ),
    }
}

pub(super) fn handle_editor_ai_message(state: &mut PhotonGui, msg: EditorMessage) -> Task<Message> {
    let ui = &mut state.editor_ui;
    match msg {
        EditorMessage::AiSetRembgModel(id) => {
            ui.ai_rembg_model = id;
            Task::none()
        }
        EditorMessage::AiSetRembgFeather(f) => {
            ui.ai_rembg_feather = f.min(20);
            Task::none()
        }
        EditorMessage::AiSetRembgRefine(m) => {
            ui.ai_rembg_refine = m;
            Task::none()
        }
        EditorMessage::AiSetRembgRefineRadius(r) => {
            ui.ai_rembg_refine_radius = r.clamp(2.0, 150.0);
            Task::none()
        }
        EditorMessage::AiSetRembgRefineSoftness(s) => {
            ui.ai_rembg_refine_softness = s.clamp(0.0, 1.0);
            Task::none()
        }
        EditorMessage::AiRunRemoveBackground => dispatch_to_core(
            state.core.clone(),
            Command::EditorAiRemoveBackground {
                model_id: ui.ai_rembg_model.clone(),
                feather: ui.ai_rembg_feather,
                target: ui.apply_target,
            },
        ),
        EditorMessage::AiSetWatermarkTab(t) => {
            ui.ai_wm_tab = t;
            Task::none()
        }
        EditorMessage::AiSetWmDetector(d) => {
            ui.ai_wm_detector = d;
            Task::none()
        }
        EditorMessage::AiSetWmInpainter(i) => {
            ui.ai_wm_inpainter = i;
            Task::none()
        }
        EditorMessage::AiSetWmConfidence(c) => {
            ui.ai_wm_confidence = c.clamp(0.05, 0.95);
            Task::none()
        }
        EditorMessage::AiSetWmMaskPadding(p) => {
            ui.ai_wm_mask_padding = p.min(64);
            Task::none()
        }
        EditorMessage::AiSetWmMaskBrushRadius(r) => {
            ui.ai_wm_brush_radius = r.clamp(4.0, 150.0);
            Task::none()
        }
        EditorMessage::AiClearWmMaskStrokes => {
            ui.ai_wm_mask_strokes.clear();
            Task::none()
        }
        EditorMessage::AiSetWmRegion(x, y, w, h) => {
            ui.ai_wm_region_x = x.clamp(0.0, 99.0);
            ui.ai_wm_region_y = y.clamp(0.0, 99.0);
            ui.ai_wm_region_w = w.clamp(1.0, 100.0);
            ui.ai_wm_region_h = h.clamp(1.0, 100.0);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiSize(s) => {
            ui.ai_wm_gemini_size = s;
            Task::none()
        }
        EditorMessage::AiSetWmGeminiBrightness(b) => {
            ui.ai_wm_gemini_brightness = b.clamp(0.0, 1.0);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiOffsetX(x) => {
            ui.ai_wm_gemini_offset_x = x.clamp(-20, 20);
            Task::none()
        }
        EditorMessage::AiSetWmGeminiOffsetY(y) => {
            ui.ai_wm_gemini_offset_y = y.clamp(-20, 20);
            Task::none()
        }
        EditorMessage::AiToggleWmGeminiForce => {
            ui.ai_wm_gemini_force = !ui.ai_wm_gemini_force;
            Task::none()
        }
        EditorMessage::AiSetWmRegionFeather(f) => {
            ui.ai_wm_region_feather = f.min(32);
            Task::none()
        }
        EditorMessage::AiRunWatermark => {
            let (doc_w, doc_h) = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|ed| (ed.width.max(1) as f32, ed.height.max(1) as f32))
                .unwrap_or((1000.0, 1000.0));
            let rx = ((ui.ai_wm_region_x / 100.0) * doc_w).round() as i32;
            let ry = ((ui.ai_wm_region_y / 100.0) * doc_h).round() as i32;
            let rw = (((ui.ai_wm_region_w / 100.0) * doc_w).round() as i32).max(1);
            let rh = (((ui.ai_wm_region_h / 100.0) * doc_h).round() as i32).max(1);
            let target = ui.apply_target;
            match ui.ai_wm_tab {
                WatermarkTab::AiAuto => {
                    let strokes = ui.ai_wm_mask_strokes.clone();
                    let detector_model = ui.ai_wm_detector.clone();
                    let inpainter_model = ui.ai_wm_inpainter.clone();
                    let confidence = ui.ai_wm_confidence;
                    let mask_padding = ui.ai_wm_mask_padding;
                    dispatch_to_core(
                        state.core.clone(),
                        Command::EditorAiInpaint {
                            detector_model,
                            inpainter_model,
                            confidence,
                            mask_padding,
                            region_x: rx,
                            region_y: ry,
                            region_width: rw,
                            region_height: rh,
                            brush_strokes: strokes,
                            target,
                        },
                    )
                }
                WatermarkTab::Gemini => {
                    let size = match ui.ai_wm_gemini_size {
                        Some(48) => "48".to_owned(),
                        Some(96) => "96".to_owned(),
                        _ => "auto".to_owned(),
                    };
                    dispatch_to_core(
                        state.core.clone(),
                        Command::EditorDewatermarkGemini {
                            size,
                            logo_value: ui.ai_wm_gemini_brightness,
                            force: ui.ai_wm_gemini_force,
                            offset_x: ui.ai_wm_gemini_offset_x,
                            offset_y: ui.ai_wm_gemini_offset_y,
                            target,
                        },
                    )
                }
                WatermarkTab::Region => dispatch_to_core(
                    state.core.clone(),
                    Command::EditorInpaintRegion {
                        x: rx,
                        y: ry,
                        width: rw,
                        height: rh,
                        feather: ui.ai_wm_region_feather,
                        target,
                    },
                ),
            }
        }
        EditorMessage::AiSetCaptionModel(m) => {
            ui.ai_caption_model = m;
            Task::none()
        }
        EditorMessage::AiSetCaptionGeneralThreshold(t) => {
            ui.ai_caption_general_threshold = t.clamp(0.1, 0.9);
            Task::none()
        }
        EditorMessage::AiSetCaptionCharThreshold(t) => {
            ui.ai_caption_char_threshold = t.clamp(0.1, 0.95);
            Task::none()
        }
        EditorMessage::AiToggleCaptionUnderscores => {
            ui.ai_caption_replace_underscores = !ui.ai_caption_replace_underscores;
            Task::none()
        }
        EditorMessage::AiSetCaptionPrefix(p) => {
            ui.ai_caption_prefix = p;
            Task::none()
        }
        EditorMessage::AiSetCaptionSuffix(s) => {
            ui.ai_caption_suffix = s;
            Task::none()
        }
        EditorMessage::AiSetCaptionExclude(e) => {
            ui.ai_caption_exclude = e;
            Task::none()
        }
        EditorMessage::AiSetCaptionDraft(d) => {
            ui.ai_caption_draft = d;
            Task::none()
        }
        EditorMessage::AiRunCaption => {
            let model_id = ui.ai_caption_model.clone();
            let general_threshold = ui.ai_caption_general_threshold;
            let character_threshold = ui.ai_caption_char_threshold;
            let replace_underscores = ui.ai_caption_replace_underscores;
            let prefix = ui.ai_caption_prefix.clone();
            let suffix = ui.ai_caption_suffix.clone();
            let exclude_tags = ui.ai_caption_exclude.clone();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorGenerateCaption {
                    model_id,
                    general_threshold,
                    character_threshold,
                    replace_underscores,
                    prefix,
                    suffix,
                    exclude_tags,
                },
            )
        }
        EditorMessage::AiSaveCaption(text) => {
            ui.ai_caption_draft = text.clone();
            dispatch_to_core(state.core.clone(), Command::EditorSaveCaption { text })
        }
        EditorMessage::ToggleRulers => {
            state.editor_ui.show_rulers = !state.editor_ui.show_rulers;
            Task::none()
        }
        EditorMessage::ToggleGrid => {
            state.editor_ui.show_grid = !state.editor_ui.show_grid;
            Task::none()
        }
        EditorMessage::SetGridSpacing(px) => {
            state.editor_ui.grid_spacing_px = px.clamp(8.0, 512.0);
            Task::none()
        }
        EditorMessage::SetGridSettings {
            spacing_px,
            subdivisions,
            opacity,
        } => {
            state.editor_ui.grid_spacing_px = spacing_px.clamp(8.0, 512.0);
            state.editor_ui.grid_subdivisions = subdivisions.clamp(1, 16);
            state.editor_ui.grid_opacity = opacity.clamp(0.1, 2.0);
            Task::none()
        }
        EditorMessage::ToggleGuides => {
            state.editor_ui.show_guides = !state.editor_ui.show_guides;
            Task::none()
        }
        EditorMessage::ToggleLockGuides => {
            state.editor_ui.lock_guides = !state.editor_ui.lock_guides;
            Task::none()
        }
        EditorMessage::ClearGuides => {
            state.editor_ui.guides.clear();
            state.editor_ui.pending_guide_preview = None;
            Task::none()
        }
        EditorMessage::AddGuide(axis, pos_px) => {
            let id = state.editor_ui.next_guide_id;
            state.editor_ui.next_guide_id += 1;
            state.editor_ui.show_guides = true;
            state.editor_ui.pending_guide_preview = None;
            state.editor_ui.guides.push(photon_canvas::EditorGuide {
                id,
                axis,
                position_px: pos_px.round(),
            });
            Task::none()
        }
        EditorMessage::SetPendingGuidePreview(preview) => {
            state.editor_ui.pending_guide_preview = preview;
            Task::none()
        }
        EditorMessage::ToggleSnap(target) => {
            match target {
                photon_editor::SnapTarget::Master => {
                    state.editor_ui.snap.enabled = !state.editor_ui.snap.enabled;
                }
                photon_editor::SnapTarget::Guides => {
                    state.editor_ui.snap.to_guides = !state.editor_ui.snap.to_guides;
                }
                photon_editor::SnapTarget::Grid => {
                    state.editor_ui.snap.to_grid = !state.editor_ui.snap.to_grid;
                }
                photon_editor::SnapTarget::Canvas => {
                    state.editor_ui.snap.to_canvas = !state.editor_ui.snap.to_canvas;
                }
                photon_editor::SnapTarget::Layers => {
                    state.editor_ui.snap.to_layers = !state.editor_ui.snap.to_layers;
                }
            }
            Task::none()
        }
        _ => Task::none(),
    }
}

pub(super) fn handle_editor_vector_text_message(
    state: &mut PhotonGui,
    msg: EditorMessage,
) -> Task<Message> {
    match msg {
        EditorMessage::EditorSetVectorMode(enabled) => {
            state.editor_ui.vector_mode = enabled;
            Task::none()
        }
        EditorMessage::EditorSetShapeCornerRadius(radius) => {
            state.editor_ui.shape_corner_radius = radius.clamp(0.0, 120.0);
            Task::none()
        }
        EditorMessage::EditorSetAlignTarget(target) => {
            state.editor_ui.align_target = target;
            Task::none()
        }
        EditorMessage::EditorLayerRowClicked(id) => {
            if state.shift_held {
                if let Some(ed) = &state.snapshot.meta.editor {
                    let anchor = state
                        .editor_ui
                        .layer_anchor_id
                        .unwrap_or(ed.active_layer_id);
                    if let (Some(a_idx), Some(b_idx)) = (
                        ed.layers.iter().position(|l| l.id == anchor),
                        ed.layers.iter().position(|l| l.id == id),
                    ) {
                        let lo = a_idx.min(b_idx);
                        let hi = a_idx.max(b_idx);
                        state.editor_ui.selected_layer_ids =
                            ed.layers[lo..=hi].iter().map(|l| l.id).collect();
                        return Task::none();
                    }
                }
                Task::none()
            } else if state.ctrl_held {
                if let Some(pos) = state
                    .editor_ui
                    .selected_layer_ids
                    .iter()
                    .position(|&lid| lid == id)
                {
                    state.editor_ui.selected_layer_ids.remove(pos);
                } else {
                    if state.editor_ui.selected_layer_ids.is_empty()
                        && let Some(ed) = &state.snapshot.meta.editor
                        && ed.active_layer_id != id
                    {
                        state.editor_ui.selected_layer_ids.push(ed.active_layer_id);
                    }
                    state.editor_ui.selected_layer_ids.push(id);
                }
                state.editor_ui.layer_anchor_id = Some(id);
                Task::none()
            } else {
                state.editor_ui.selected_layer_ids.clear();
                state.editor_ui.layer_anchor_id = Some(id);
                dispatch_to_core(state.core.clone(), Command::EditorLayerSelect { id })
            }
        }
        EditorMessage::EditorBeginDragLayer(id) => {
            state.editor_ui.dragging_layer_id = Some(id);
            Task::none()
        }
        EditorMessage::EditorDropLayer(target_id) => {
            let Some(drag_id) = state.editor_ui.dragging_layer_id.take() else {
                return Task::none();
            };
            if drag_id == target_id {
                return Task::none();
            }
            if let Some(ed) = &state.snapshot.meta.editor
                && let Some(new_index) = ed.layers.iter().position(|l| l.id == target_id)
            {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorLayerReorder {
                        id: drag_id,
                        new_index,
                    },
                );
            }
            Task::none()
        }
        EditorMessage::EditorToggleLayerMultiSelect(id) => {
            if let Some(pos) = state
                .editor_ui
                .selected_layer_ids
                .iter()
                .position(|&lid| lid == id)
            {
                state.editor_ui.selected_layer_ids.remove(pos);
            } else {
                if state.editor_ui.selected_layer_ids.is_empty()
                    && let Some(ed) = &state.snapshot.meta.editor
                    && ed.active_layer_id != id
                {
                    state.editor_ui.selected_layer_ids.push(ed.active_layer_id);
                }
                state.editor_ui.selected_layer_ids.push(id);
            }
            Task::none()
        }
        EditorMessage::EditorAlignObjects(op) => {
            let shape_ids = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|ed| ed.active_vector_data.as_ref())
                .map(|v| v.selected_shape_ids.clone())
                .unwrap_or_default();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorAlignObjects {
                    op,
                    target: state.editor_ui.align_target,
                    layer_ids: state.editor_ui.selected_layer_ids.clone(),
                    shape_ids,
                },
            )
        }
        EditorMessage::EditorDistributeObjects(op) => {
            let shape_ids = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|ed| ed.active_vector_data.as_ref())
                .map(|v| v.selected_shape_ids.clone())
                .unwrap_or_default();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorDistributeObjects {
                    op,
                    layer_ids: state.editor_ui.selected_layer_ids.clone(),
                    shape_ids,
                    fixed_gap_px: None,
                },
            )
        }
        EditorMessage::EditorSelectVectorNode(idx) => {
            state.editor_ui.selected_vector_node = idx;
            Task::none()
        }
        EditorMessage::EditorSetVectorSimplifyTolerance(tol) => {
            state.editor_ui.vector_simplify_tolerance = tol.clamp(0.5, 50.0);
            Task::none()
        }
        EditorMessage::EditorSetVectorSmoothTension(tension) => {
            state.editor_ui.vector_smooth_tension = tension.clamp(0.05, 1.0);
            Task::none()
        }
        EditorMessage::EditorUpdateActiveVectorShape(shape) => dispatch_to_core(
            state.core.clone(),
            Command::EditorUpdateVectorShape {
                layer_id: None,
                shape,
            },
        ),
        EditorMessage::OpenTransformEachDialog => {
            state.active_menu = None;
            state.context_menu = None;
            state.dialog = Some(InAppDialog::TransformEach {
                spec: state.editor_ui.transform_each_spec,
            });
            open_dialog_window(state, DesktopDialogId::TransformEach);
            Task::none()
        }
        EditorMessage::TransformEachSetSpec(spec) => {
            state.editor_ui.transform_each_spec = spec;
            if let Some(InAppDialog::TransformEach { spec: dlg_spec }) = &mut state.dialog {
                *dlg_spec = spec;
            }
            Task::none()
        }
        EditorMessage::ConfirmTransformEach => {
            let spec = match state.dialog.take() {
                Some(InAppDialog::TransformEach { spec }) => spec,
                other => {
                    state.dialog = other;
                    state.editor_ui.transform_each_spec
                }
            };
            state.editor_ui.transform_each_spec = spec;
            state.dialog_manager.close(&DesktopDialogId::TransformEach);
            let active_layer_id = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .map(|e| e.active_layer_id)
                .into_iter()
                .collect();
            let shape_ids = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|e| e.active_vector_data.as_ref())
                .map(|v| {
                    if !v.selected_shape_ids.is_empty() {
                        v.selected_shape_ids.clone()
                    } else {
                        v.shapes.iter().map(|s| s.id).collect()
                    }
                })
                .unwrap_or_default();
            dispatch_to_core(
                state.core.clone(),
                Command::EditorTransformEach {
                    layer_ids: active_layer_id,
                    shape_ids,
                    spec,
                },
            )
        }
        EditorMessage::OpenVectorBlendDialog => {
            state.active_menu = None;
            state.context_menu = None;
            state.dialog = Some(InAppDialog::VectorBlend {
                spec: state.editor_ui.vector_blend_spec,
            });
            open_dialog_window(state, DesktopDialogId::VectorBlend);
            Task::none()
        }
        EditorMessage::VectorBlendSetSpec(spec) => {
            state.editor_ui.vector_blend_spec = spec;
            if let Some(InAppDialog::VectorBlend { spec: dlg_spec }) = &mut state.dialog {
                *dlg_spec = spec;
            }
            Task::none()
        }
        EditorMessage::ConfirmVectorBlend => {
            let mut spec = match state.dialog.take() {
                Some(InAppDialog::VectorBlend { spec }) => spec,
                other => {
                    state.dialog = other;
                    state.editor_ui.vector_blend_spec
                }
            };
            state.editor_ui.vector_blend_spec = spec;
            state.dialog_manager.close(&DesktopDialogId::VectorBlend);
            let Some((first, second)) = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|e| e.active_vector_data.as_ref())
                .and_then(|v| {
                    if v.selected_shape_ids.len() >= 2 {
                        Some((v.selected_shape_ids[0], v.selected_shape_ids[1]))
                    } else if v.shapes.len() >= 2 {
                        let n = v.shapes.len();
                        Some((v.shapes[n - 2].id, v.shapes[n - 1].id))
                    } else {
                        None
                    }
                })
            else {
                return Task::none();
            };
            spec.start_shape_id = first;
            spec.end_shape_id = second;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorVectorBlend {
                    layer_id: None,
                    spec,
                },
            )
        }
        EditorMessage::OpenExportVectorSvgDialog => {
            state.active_menu = None;
            state.context_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor {
                let p = Path::new(&doc.path);
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("vector");
                let default_out = p
                    .parent()
                    .map(|dir| dir.join(format!("{stem}.svg")))
                    .unwrap_or_else(|| PathBuf::from(format!("{stem}.svg")));
                state.dialog = Some(InAppDialog::ExportVectorSvg {
                    path: default_out.to_string_lossy().into_owned(),
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::ExportVectorSvg);
            }
            Task::none()
        }
        EditorMessage::PickExportVectorSvgDialog => {
            let title = tr(state.snapshot.meta.language, "menu-vector-export-svg");
            let current = match &state.dialog {
                Some(InAppDialog::ExportVectorSvg { path, .. }) => PathBuf::from(path),
                _ => PathBuf::new(),
            };
            let start = current.parent().map(Path::to_path_buf);
            let name = current
                .file_name()
                .and_then(|n| n.to_str())
                .map(ToOwned::to_owned);
            Task::perform(
                async move {
                    let filters = [photon_dialogs::FileFilter {
                        name: "SVG Vector",
                        extensions: &["svg"],
                    }];
                    photon_dialogs::save_file(&title, start, name.as_deref(), &filters).await
                },
                Message::ExportVectorSvgPathPicked,
            )
        }
        EditorMessage::ExportVectorSvgPathChanged(val) => {
            if let Some(InAppDialog::ExportVectorSvg { path, error }) = &mut state.dialog {
                *path = val;
                *error = None;
            }
            Task::none()
        }
        EditorMessage::ConfirmExportVectorSvg => {
            let Some(InAppDialog::ExportVectorSvg { path, .. }) = state.dialog.clone() else {
                return Task::none();
            };
            let clean = path.trim().to_owned();
            if clean.is_empty() {
                if let Some(InAppDialog::ExportVectorSvg { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ExportVectorSvg);
            dispatch_to_core(
                state.core.clone(),
                Command::EditorExportVectorSvg {
                    layer_id: None,
                    path: clean,
                },
            )
        }
        EditorMessage::EditorSetTextDraft(data) => {
            state.editor_ui.text_draft = data;
            Task::none()
        }
        EditorMessage::EditorAddOrUpdateTextLayer(data) => {
            state.editor_ui.text_draft = data.clone();
            if state
                .snapshot
                .meta
                .editor
                .as_ref()
                .is_some_and(|e| e.active_text_data.is_some())
            {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorUpdateTextLayer { id: None, data },
                )
            } else {
                let next_idx = state
                    .snapshot
                    .meta
                    .editor
                    .as_ref()
                    .map_or(1, |e| e.layers.len() + 1);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorAddTextLayer {
                        name: format!("Text {next_idx}"),
                        data,
                    },
                )
            }
        }
        EditorMessage::ToggleRulers => {
            state.editor_ui.show_rulers = !state.editor_ui.show_rulers;
            Task::none()
        }
        EditorMessage::ToggleGrid => {
            state.editor_ui.show_grid = !state.editor_ui.show_grid;
            Task::none()
        }
        EditorMessage::SetGridSpacing(px) => {
            state.editor_ui.grid_spacing_px = px.clamp(8.0, 512.0);
            Task::none()
        }
        EditorMessage::SetGridSettings {
            spacing_px,
            subdivisions,
            opacity,
        } => {
            state.editor_ui.grid_spacing_px = spacing_px.clamp(8.0, 512.0);
            state.editor_ui.grid_subdivisions = subdivisions.clamp(1, 16);
            state.editor_ui.grid_opacity = opacity.clamp(0.1, 2.0);
            Task::none()
        }
        EditorMessage::ToggleGuides => {
            state.editor_ui.show_guides = !state.editor_ui.show_guides;
            Task::none()
        }
        EditorMessage::ToggleLockGuides => {
            state.editor_ui.lock_guides = !state.editor_ui.lock_guides;
            Task::none()
        }
        EditorMessage::ClearGuides => {
            state.editor_ui.guides.clear();
            state.editor_ui.pending_guide_preview = None;
            Task::none()
        }
        EditorMessage::AddGuide(axis, pos_px) => {
            let id = state.editor_ui.next_guide_id;
            state.editor_ui.next_guide_id += 1;
            state.editor_ui.show_guides = true;
            state.editor_ui.pending_guide_preview = None;
            state.editor_ui.guides.push(photon_canvas::EditorGuide {
                id,
                axis,
                position_px: pos_px.round(),
            });
            Task::none()
        }
        EditorMessage::SetPendingGuidePreview(preview) => {
            state.editor_ui.pending_guide_preview = preview;
            Task::none()
        }
        EditorMessage::ToggleSnap(target) => {
            match target {
                photon_editor::SnapTarget::Master => {
                    state.editor_ui.snap.enabled = !state.editor_ui.snap.enabled;
                }
                photon_editor::SnapTarget::Guides => {
                    state.editor_ui.snap.to_guides = !state.editor_ui.snap.to_guides;
                }
                photon_editor::SnapTarget::Grid => {
                    state.editor_ui.snap.to_grid = !state.editor_ui.snap.to_grid;
                }
                photon_editor::SnapTarget::Canvas => {
                    state.editor_ui.snap.to_canvas = !state.editor_ui.snap.to_canvas;
                }
                photon_editor::SnapTarget::Layers => {
                    state.editor_ui.snap.to_layers = !state.editor_ui.snap.to_layers;
                }
            }
            Task::none()
        }
        _ => Task::none(),
    }
}
