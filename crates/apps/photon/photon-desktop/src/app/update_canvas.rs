use super::*;

pub(super) fn handle_editor_canvas_event(state: &mut PhotonGui, ev: CanvasEvent) -> Task<Message> {
    match Message::Canvas(ev) {
        Message::Canvas(ev) => {
            match ev {
                CanvasEvent::ContextMenuRequested { pos: [x, y], .. } => {
                    if state.snapshot.meta.workspace == Workspace::Editor
                        && state.snapshot.meta.editor.is_some()
                    {
                        state.active_menu = None;
                        state.context_menu = Some(ActiveContextMenu::EditorCanvas { x, y });
                        return sync_ui_tree_task(state);
                    }
                    return Task::none();
                }
                CanvasEvent::ZoomBy {
                    factor,
                    cursor,
                    bounds,
                } => {
                    let (min_z, max_z) = active_workspace_zoom_bounds(state, Some(bounds));
                    apply_canvas_zoom_at_cursor(
                        &mut state.zoom,
                        &mut state.pan_x,
                        &mut state.pan_y,
                        factor,
                        cursor,
                        bounds,
                        min_z,
                        max_z,
                    );
                }
                CanvasEvent::PanBy { dx, dy } => {
                    apply_canvas_pan(&mut state.pan_x, &mut state.pan_y, dx, dy);
                }
                CanvasEvent::CropUpdated(uv) => {
                    state.crop_uv = Some(uv);
                    return Task::none();
                }
                CanvasEvent::HealingSourcePicked([x, y]) => {
                    state.editor_ui.healing_source_px = Some([x as f32, y as f32]);
                    return Task::none();
                }
                CanvasEvent::StrokeBegin([x, y]) => {
                    if (state.editor_ui.tool == EditorTool::RemoveBg
                        && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off)
                        || (state.editor_ui.tool == EditorTool::Watermark
                            && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                            && state.editor_ui.ai_wm_detector == "brush")
                    {
                        state.editor_ui.stroke_active = true;
                        state.editor_ai_stroke_pts = vec![[x, y]];
                        return Task::none();
                    }
                    state.editor_ui.stroke_active = true;
                    let stroke = if state.editor_ui.tool == EditorTool::Liquify {
                        LiveStrokeKind::Liquify {
                            mode: state.editor_ui.liquify_mode,
                            radius: state.editor_ui.liquify_radius,
                            strength: state.editor_ui.liquify_strength,
                        }
                    } else if state.editor_ui.tool == EditorTool::HealingBrush {
                        LiveStrokeKind::Healing {
                            mode: state.editor_ui.healing_mode,
                            radius: state.editor_ui.healing_radius,
                            hardness: state.editor_ui.healing_hardness,
                            strength: state.editor_ui.healing_strength,
                            source: state
                                .editor_ui
                                .healing_source_px
                                .map(|[sx, sy]| [f64::from(sx), f64::from(sy)]),
                        }
                    } else {
                        LiveStrokeKind::Paint {
                            color: state.editor_ui.fg_color,
                            radius: state.editor_ui.brush_radius,
                            opacity: state.editor_ui.brush_opacity,
                            erase: state.editor_ui.tool == EditorTool::Eraser,
                            brush: state.editor_ui.brush_settings,
                        }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorLiveStrokeBegin { stroke, x, y },
                    );
                }
                CanvasEvent::StrokeStep(points) => {
                    if state.editor_ui.stroke_active {
                        if (state.editor_ui.tool == EditorTool::RemoveBg
                            && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off)
                            || (state.editor_ui.tool == EditorTool::Watermark
                                && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                                && state.editor_ui.ai_wm_detector == "brush")
                        {
                            state.editor_ai_stroke_pts.extend(points);
                            return Task::none();
                        }
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLiveStrokeStep { points },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::StrokeEnd(commit) => {
                    if state.editor_ui.stroke_active {
                        state.editor_ui.stroke_active = false;
                        if state.editor_ui.tool == EditorTool::RemoveBg
                            && state.editor_ui.ai_rembg_refine != AlphaRefineMode::Off
                        {
                            let pts = std::mem::take(&mut state.editor_ai_stroke_pts);
                            if commit && !pts.is_empty() {
                                let stroke = AiMaskStroke {
                                    radius: state.editor_ui.ai_rembg_refine_radius,
                                    points: pts
                                        .into_iter()
                                        .map(|[sx, sy]| [sx as f32, sy as f32])
                                        .collect(),
                                };
                                let (erase_strokes, restore_strokes) = if state
                                    .editor_ui
                                    .ai_rembg_refine
                                    == AlphaRefineMode::EraseBg
                                {
                                    (vec![stroke], Vec::new())
                                } else {
                                    (Vec::new(), vec![stroke])
                                };
                                return dispatch_to_core(
                                    state.core.clone(),
                                    Command::EditorRefineAlpha {
                                        erase_strokes,
                                        restore_strokes,
                                        softness: state.editor_ui.ai_rembg_refine_softness,
                                    },
                                );
                            }
                            return Task::none();
                        }
                        if state.editor_ui.tool == EditorTool::Watermark
                            && state.editor_ui.ai_wm_tab == WatermarkTab::AiAuto
                            && state.editor_ui.ai_wm_detector == "brush"
                        {
                            let pts = std::mem::take(&mut state.editor_ai_stroke_pts);
                            if commit && !pts.is_empty() {
                                state.editor_ui.ai_wm_mask_strokes.push(AiMaskStroke {
                                    radius: state.editor_ui.ai_wm_brush_radius,
                                    points: pts
                                        .into_iter()
                                        .map(|[sx, sy]| [sx as f32, sy as f32])
                                        .collect(),
                                });
                            }
                            return Task::none();
                        }
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLiveStrokeEnd { commit },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::SampleColor([px, py]) => {
                    if let Some(frame) = &state.editor_frame
                        && px < frame.width
                        && py < frame.height
                    {
                        let idx = ((py as usize) * (frame.width as usize) + (px as usize)) * 4;
                        if idx + 3 < frame.rgba.len() {
                            let rgba = [
                                frame.rgba[idx],
                                frame.rgba[idx + 1],
                                frame.rgba[idx + 2],
                                frame.rgba[idx + 3],
                            ];
                            state.editor_ui.sampled_color = Some(rgba);
                            state.editor_ui.fg_color = [rgba[0], rgba[1], rgba[2]];
                        }
                    }
                    return Task::none();
                }
                CanvasEvent::SampleBgColor([px, py]) => {
                    if let Some(frame) = &state.editor_frame
                        && px < frame.width
                        && py < frame.height
                    {
                        let idx = ((py as usize) * (frame.width as usize) + (px as usize)) * 4;
                        if idx + 3 < frame.rgba.len() {
                            let rgba = [
                                frame.rgba[idx],
                                frame.rgba[idx + 1],
                                frame.rgba[idx + 2],
                                frame.rgba[idx + 3],
                            ];
                            state.editor_ui.sampled_color = Some(rgba);
                            state.editor_ui.bg_color = Some([rgba[0], rgba[1], rgba[2]]);
                        }
                    }
                    return Task::none();
                }
                CanvasEvent::CompareSplitChanged(frac) => {
                    state.editor_ui.compare_split_x = frac.clamp(0.02, 0.98);
                    return Task::none();
                }
                CanvasEvent::SelectClear => {
                    if state.editor_ui.select_op == SelectionBooleanOp::Replace {
                        return dispatch_to_core(state.core.clone(), Command::EditorDeselect);
                    }
                    return Task::none();
                }
                CanvasEvent::SelectMarqueeCommit {
                    is_ellipse,
                    x,
                    y,
                    width,
                    height,
                    shift,
                    alt,
                } => {
                    if state.editor_ui.tool == EditorTool::Watermark
                        && let Some(ed) = &state.snapshot.meta.editor
                    {
                        let ew = ed.width.max(1) as f32;
                        let eh = ed.height.max(1) as f32;
                        state.editor_ui.ai_wm_region_x = ((x / ew) * 100.0).clamp(0.0, 99.0);
                        state.editor_ui.ai_wm_region_y = ((y / eh) * 100.0).clamp(0.0, 99.0);
                        state.editor_ui.ai_wm_region_w = ((width / ew) * 100.0).clamp(1.0, 100.0);
                        state.editor_ui.ai_wm_region_h = ((height / eh) * 100.0).clamp(1.0, 100.0);
                        return Task::none();
                    }
                    let op = resolve_select_op(state, shift, alt);
                    let shape = if is_ellipse {
                        SelectionShape::Ellipse {
                            cx: x + width * 0.5,
                            cy: y + height * 0.5,
                            rx: width * 0.5,
                            ry: height * 0.5,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        }
                    } else {
                        SelectionShape::Rect {
                            x,
                            y,
                            width,
                            height,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectLineCommit {
                    is_row,
                    coord,
                    shift,
                    alt,
                } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = if is_row {
                        SelectionShape::SingleRow { y: coord }
                    } else {
                        SelectionShape::SingleCol { x: coord }
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectPolygonCommit { points, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::Polygon {
                        points,
                        feather: state.editor_ui.select_feather,
                        anti_alias: state.editor_ui.select_anti_alias,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectMagicWand { x, y, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::MagicWand {
                        x,
                        y,
                        tolerance: state.editor_ui.wand_tolerance,
                        contiguous: state.editor_ui.wand_contiguous,
                        sample_all_layers: state.editor_ui.select_sample_all,
                        anti_alias: state.editor_ui.select_anti_alias,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::SelectQuickStroke { points, shift, alt } => {
                    let op = resolve_select_op(state, shift, alt);
                    let shape = SelectionShape::QuickSelectStroke {
                        points,
                        radius: state.editor_ui.quick_select_radius,
                        sample_all_layers: state.editor_ui.select_sample_all,
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorSelectShape { shape, op },
                    );
                }
                CanvasEvent::ViewportResized(w, h) => {
                    state.editor_ui.canvas_viewport_size = Some([w, h]);
                }
                CanvasEvent::ResetFit => {
                    state.context_menu = None;
                    if state.snapshot.meta.workspace == Workspace::Viewer
                        && state.viewer_mode == ViewerViewMode::Webtoon
                    {
                        state.viewer_webtoon_fit_mode = WebtoonFitMode::Auto;
                        state.webtoon_zoom = 1.0;
                        return sync_ui_tree_task(state);
                    }
                    state.zoom = 1.0;
                    state.pan_x = 0.0;
                    state.pan_y = 0.0;
                }
                CanvasEvent::DrawShapeCommit {
                    kind,
                    points,
                    nodes,
                    closed,
                } => {
                    let use_vector = state.editor_ui.vector_mode
                        || state.snapshot.meta.editor.as_ref().is_some_and(|ed| {
                            !ed.selection.quick_mask_active
                                && ed
                                    .layers
                                    .iter()
                                    .find(|l| l.id == ed.active_layer_id)
                                    .is_some_and(|l| l.kind.is_vector() && !l.editing_mask)
                        });
                    if use_vector
                        && let Some(shape) = build_vector_shape_from_draw_commit(
                            kind,
                            &points,
                            &nodes,
                            closed,
                            state.editor_ui.shape_fill_mode,
                            state.editor_ui.fg_color,
                            state.editor_ui.bg_color,
                            state.editor_ui.shape_stroke_width,
                            state.editor_ui.shape_corner_radius,
                            state.editor_ui.brush_opacity,
                        )
                    {
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorAddVectorShape {
                                layer_id: None,
                                shape,
                            },
                        );
                    }
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorDrawShape {
                            kind,
                            points,
                            nodes,
                            fill_mode: state.editor_ui.shape_fill_mode,
                            closed,
                            color: Some(state.editor_ui.fg_color),
                            radius: Some(state.editor_ui.shape_stroke_width),
                            opacity: Some(state.editor_ui.brush_opacity),
                        },
                    );
                }
                CanvasEvent::BezierToSelectionCommit { nodes } => {
                    let points: Vec<[f32; 2]> =
                        photon_image::drawing::tessellate_bezier_path(&nodes, true, 0.5)
                            .into_iter()
                            .map(|[x, y]| [x as f32, y as f32])
                            .collect();
                    if points.len() >= 3 {
                        let shape = SelectionShape::Polygon {
                            points,
                            feather: state.editor_ui.select_feather,
                            anti_alias: state.editor_ui.select_anti_alias,
                        };
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorSelectShape {
                                shape,
                                op: state.editor_ui.select_op,
                            },
                        );
                    }
                    return Task::none();
                }
                CanvasEvent::FloodFillClick { x, y } => {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorFloodFill {
                            x,
                            y,
                            tolerance: state.editor_ui.flood_fill_tolerance,
                            contiguous: state.editor_ui.flood_fill_contiguous,
                            color: Some(state.editor_ui.fg_color),
                            opacity: Some(state.editor_ui.flood_fill_opacity),
                            sample_all_layers: state.editor_ui.flood_fill_sample_all,
                        },
                    );
                }
                CanvasEvent::DrawGradientCommit { start, end } => {
                    state.editor_ui.gradient_draft = None;
                    return dispatch_to_core(
                        state.core.clone(),
                        build_draw_gradient_command(&state.editor_ui, start, end),
                    );
                }
                CanvasEvent::GradientDraftUpdated { start, end } => {
                    state.editor_ui.gradient_draft = Some((start, end));
                    return Task::none();
                }
                CanvasEvent::GradientDraftApply => {
                    return update(state, Message::EditorApplyGradientDraft);
                }
                CanvasEvent::GradientDraftCancel => {
                    return update(state, Message::EditorCancelGradientDraft);
                }
                CanvasEvent::TransformUpdated(spec) => {
                    state.editor_ui.transform_spec = spec;
                    if let Some(tp) = &mut state.transform_preview {
                        tp.spec = spec;
                    }
                    return Task::none();
                }
                CanvasEvent::TransformCommit => {
                    return update(state, Message::EditorTransformCommit);
                }
                CanvasEvent::TransformCancel => {
                    return update(state, Message::EditorTransformCancel);
                }
                CanvasEvent::MoveLayerCommit { dx, dy } => {
                    if state.editor_ui.selected_layer_ids.len() >= 2 {
                        return dispatch_to_core(
                            state.core.clone(),
                            Command::EditorLayerMoveMultiple {
                                ids: state.editor_ui.selected_layer_ids.clone(),
                                dx,
                                dy,
                            },
                        );
                    }
                    if let Some(tp) = &mut state.transform_preview {
                        tp.spec.translate_x += dx;
                        tp.spec.translate_y += dy;
                        state.pending_preview_commit_rev =
                            state.editor_frame.as_ref().map(|f| f.revision);
                    }
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorTransformLayer {
                            layer_id: None,
                            transform: LayerTransformSpec {
                                translate_x: dx,
                                translate_y: dy,
                                ..Default::default()
                            },
                        },
                    );
                }
                CanvasEvent::BrushRadiusChanged(r) => {
                    return update(state, Message::EditorSetBrushRadius(r));
                }
                CanvasEvent::ShapeStrokeWidthChanged(w) => {
                    state.editor_ui.shape_stroke_width = w.clamp(1.0, 64.0);
                    return Task::none();
                }
                CanvasEvent::GuideMoved { id, position_px } => {
                    if let Some(g) = state.editor_ui.guides.iter_mut().find(|g| g.id == id) {
                        g.position_px = position_px;
                    }
                    return Task::none();
                }
                CanvasEvent::GuideRemoved(id) => {
                    state.editor_ui.guides.retain(|g| g.id != id);
                    return Task::none();
                }
                CanvasEvent::CursorDocMoved(_) => return Task::none(),
                CanvasEvent::VectorSelectMarquee {
                    shape_ids,
                    additive,
                } => {
                    state.editor_ui.selected_vector_node = None;
                    let next_ids = if additive
                        && let Some(vdata) = state
                            .snapshot
                            .meta
                            .editor
                            .as_ref()
                            .and_then(|ed| ed.active_vector_data.as_ref())
                    {
                        let mut next = vdata.selected_shape_ids.clone();
                        for sid in shape_ids {
                            if !next.contains(&sid) {
                                next.push(sid);
                            }
                        }
                        next
                    } else {
                        shape_ids
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: next_ids.first().copied(),
                            op: VectorPathOperation::SelectShapes {
                                shape_ids: next_ids,
                            },
                        },
                    );
                }
                CanvasEvent::VectorSelectShape {
                    shape_id,
                    node_index,
                    additive,
                } => {
                    state.editor_ui.selected_vector_node = node_index;
                    let shape_ids = if additive
                        && let Some(vdata) = state
                            .snapshot
                            .meta
                            .editor
                            .as_ref()
                            .and_then(|ed| ed.active_vector_data.as_ref())
                    {
                        let mut next = vdata.selected_shape_ids.clone();
                        if let Some(pos) = next.iter().position(|&id| id == shape_id) {
                            if next.len() > 1 {
                                next.remove(pos);
                            }
                        } else {
                            next.push(shape_id);
                        }
                        next
                    } else {
                        vec![shape_id]
                    };
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::SelectShapes { shape_ids },
                        },
                    );
                }
                CanvasEvent::VectorMoveNode {
                    shape_id,
                    node_index,
                    anchor,
                } => {
                    state.editor_ui.selected_vector_node = Some(node_index);
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::MoveNodeAnchor {
                                index: node_index,
                                anchor,
                            },
                        },
                    );
                }
                CanvasEvent::VectorMoveHandle {
                    shape_id,
                    node_index,
                    handle,
                    position,
                    mode,
                } => {
                    state.editor_ui.selected_vector_node = Some(node_index);
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::MoveNodeHandle {
                                index: node_index,
                                handle,
                                position,
                                mode,
                            },
                        },
                    );
                }
                CanvasEvent::VectorAddNodeAtPoint {
                    shape_id,
                    point,
                    max_distance,
                } => {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::AddNodeAtPoint {
                                point,
                                max_distance,
                            },
                        },
                    );
                }
                CanvasEvent::VectorDeleteNode {
                    shape_id,
                    node_index,
                } => {
                    state.editor_ui.selected_vector_node = None;
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::DeleteNode { index: node_index },
                        },
                    );
                }
                CanvasEvent::VectorTranslateShape { shape_id, dx, dy } => {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorVectorPathOp {
                            layer_id: None,
                            shape_id: Some(shape_id),
                            op: VectorPathOperation::TranslateShape { dx, dy },
                        },
                    );
                }
                CanvasEvent::TextCanvasClick { x, y } => {
                    if let Some(ed) = &state.snapshot.meta.editor {
                        if let Some(mut existing) = ed.active_text_data.clone() {
                            existing.x = x;
                            existing.y = y;
                            state.editor_ui.text_draft = existing.clone();
                            return dispatch_to_core(
                                state.core.clone(),
                                Command::EditorUpdateTextLayer {
                                    id: None,
                                    data: existing,
                                },
                            );
                        } else {
                            let mut data = state.editor_ui.text_draft.clone();
                            data.x = x;
                            data.y = y;
                            if data.text.trim().is_empty() {
                                data.text = "Text".to_owned();
                            }
                            let fg = state.editor_ui.fg_color;
                            if data.color == [24, 24, 27] || data.color == [0, 0, 0] {
                                data.color = fg;
                            }
                            state.editor_ui.text_draft = data.clone();
                            let next_idx = ed.layers.len() + 1;
                            return dispatch_to_core(
                                state.core.clone(),
                                Command::EditorAddTextLayer {
                                    name: format!("Text {next_idx}"),
                                    data,
                                },
                            );
                        }
                    }
                    return Task::none();
                }
            }
            sync_ui_tree_task(state)
        }

        _ => unreachable!(),
    }
}

pub(super) fn handle_viewer_canvas_event(
    state: &mut PhotonGui,
    pane_path: Option<String>,
    ev: CanvasEvent,
) -> Task<Message> {
    match Message::ViewerCanvas(pane_path, ev) {
        Message::ViewerCanvas(pane_path, ev) => match ev {
            CanvasEvent::ContextMenuRequested { pos, .. } => update(
                state,
                Message::OpenViewerContextMenu {
                    path: pane_path,
                    pos: Some(pos),
                },
            ),
            CanvasEvent::SelectMarqueeCommit {
                x,
                y,
                width,
                height,
                ..
            } if state.viewer_translate.active
                && (state.viewer_translate.picking_region
                    || matches!(
                        state.viewer_translate.mode,
                        ViewerTranslateMode::Translate | ViewerTranslateMode::OcrOnly
                    )) =>
            {
                if state.viewer_mode == ViewerViewMode::Double
                    && let Some(left_frame) = &state.viewer_frame
                {
                    let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                    let left_path = left_slot.map(|(_, p)| p.to_owned());
                    let right_path = right_slot.map(|(_, p)| p.to_owned());
                    let left_size = (left_frame.width, left_frame.height);
                    let right_size = state
                        .viewer_pair_frame
                        .as_ref()
                        .map(|f| (f.width, f.height));
                    if let Some((path, region)) = resolve_double_marquee_region(
                        [x, y, width, height],
                        left_path.as_deref(),
                        left_size,
                        right_path.as_deref(),
                        right_size,
                        f32::from(state.viewer_manga_page_gap),
                        state.viewer_quarter_turns,
                    ) {
                        return dispatch_viewer_translate_region(state, path, region);
                    }
                    return Task::none();
                }
                let path = pane_path
                    .or_else(|| state.snapshot.meta.viewer_path.clone())
                    .unwrap_or_default();
                if path.is_empty() {
                    return Task::none();
                }
                let region = [
                    x.round().max(0.0) as i32,
                    y.round().max(0.0) as i32,
                    width.round().max(8.0) as i32,
                    height.round().max(8.0) as i32,
                ];
                dispatch_viewer_translate_region(state, path, region)
            }
            other @ (CanvasEvent::PanBy { .. }
            | CanvasEvent::ZoomBy { .. }
            | CanvasEvent::ResetFit
            | CanvasEvent::CompareSplitChanged(_)) => update(state, Message::Canvas(other)),
            _ => Task::none(),
        },

        _ => unreachable!(),
    }
}

pub(crate) fn apply_canvas_pan(pan_x: &mut f32, pan_y: &mut f32, dx: f32, dy: f32) {
    *pan_x += dx;
    *pan_y += dy;
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_canvas_zoom_at_cursor(
    zoom: &mut f32,
    pan_x: &mut f32,
    pan_y: &mut f32,
    factor: f32,
    cursor: Option<[f32; 2]>,
    bounds: [f32; 2],
    min_zoom: f32,
    max_zoom: f32,
) {
    let old_zoom = *zoom;
    let new_zoom = (old_zoom * factor).clamp(min_zoom, max_zoom);
    let ratio = new_zoom / old_zoom.max(0.001);
    if let Some([cx, cy]) = cursor {
        let half_w = bounds[0] * 0.5;
        let half_h = bounds[1] * 0.5;
        *pan_x = (cx - half_w) - ((cx - half_w) - *pan_x) * ratio;
        *pan_y = (cy - half_h) - ((cy - half_h) - *pan_y) * ratio;
    }
    *zoom = new_zoom;
}

pub(crate) fn resolved_editor_canvas_bounds(state: &PhotonGui) -> Rectangle {
    let layout = &state.snapshot.meta.layout;
    let left_w = if layout.panel("editor.tools") {
        97.0
    } else {
        0.0
    };
    let right_w = if layout.panel("editor.inspector") {
        6.0 + state.dock_drag.effective_editor_right(layout)
    } else {
        0.0
    };
    let top_h = 47.0;
    let bottom_h = 31.0;
    let w = (state.window_size.width - left_w - right_w).max(160.0);
    let h = (state.window_size.height - top_h - bottom_h).max(120.0);
    Rectangle::new(Point::new(left_w, top_h), Size::new(w, h))
}

pub(super) fn resolve_select_op(state: &PhotonGui, shift: bool, alt: bool) -> SelectionBooleanOp {
    if shift && alt {
        SelectionBooleanOp::Intersect
    } else if shift {
        SelectionBooleanOp::Add
    } else if alt {
        SelectionBooleanOp::Subtract
    } else {
        state.editor_ui.select_op
    }
}

pub(super) fn build_draw_gradient_command(
    ui: &photon_editor::EditorUiState,
    start: [f32; 2],
    end: [f32; 2],
) -> Command {
    Command::EditorDrawGradient {
        start,
        end,
        kind: ui.gradient_kind,
        color_mode: ui.gradient_color_mode,
        fg_color: Some(ui.fg_color),
        bg_color: ui.bg_color,
        stops: if ui.gradient_color_mode == GradientColorMode::Custom {
            ui.resolved_gradient_stops()
        } else {
            Vec::new()
        },
        opacity: Some(ui.gradient_opacity),
        repeat: ui.gradient_repeat,
        reverse: ui.gradient_reverse,
        dither: ui.gradient_dither,
        blend_mode: ui.gradient_blend_mode,
    }
}

pub(super) fn sample_gradient_stops(stops: &[GradientStop], pos: f64) -> ([u8; 3], f64) {
    if stops.is_empty() {
        return ([0, 0, 0], 1.0);
    }
    if pos <= stops[0].pos {
        return (stops[0].color, stops[0].alpha);
    }
    for win in stops.windows(2) {
        let (a, b) = (&win[0], &win[1]);
        if pos <= b.pos {
            let span = (b.pos - a.pos).max(1e-9);
            let t = ((pos - a.pos) / span).clamp(0.0, 1.0);
            let lerp_u8 = |c0: u8, c1: u8| -> u8 {
                (c0 as f64 + (c1 as f64 - c0 as f64) * t)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            return (
                [
                    lerp_u8(a.color[0], b.color[0]),
                    lerp_u8(a.color[1], b.color[1]),
                    lerp_u8(a.color[2], b.color[2]),
                ],
                a.alpha + (b.alpha - a.alpha) * t,
            );
        }
    }
    let last = &stops[stops.len() - 1];
    (last.color, last.alpha)
}

pub(crate) fn actual_zoom_for_bounds(
    img_w: u32,
    img_h: u32,
    vp_w: f32,
    vp_h: f32,
) -> (f32, f32, f32) {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let iw = img_w.max(1) as f32;
    let ih = img_h.max(1) as f32;
    let fit_scale = (vp_w / iw).min(vp_h / ih).max(1e-6);
    let min_zoom = 0.02_f32;
    let max_zoom = (32.0 / fit_scale).max(32.0);
    let actual_zoom = (1.0 / fit_scale).clamp(min_zoom, max_zoom);
    (actual_zoom, min_zoom, max_zoom)
}

pub(super) fn active_workspace_zoom_bounds(
    state: &PhotonGui,
    bounds_override: Option<[f32; 2]>,
) -> (f32, f32) {
    match state.snapshot.meta.workspace {
        Workspace::Editor => {
            if let Some(frame) = &state.editor_frame {
                let [vp_w, vp_h] = bounds_override.unwrap_or_else(|| {
                    let r = resolved_editor_canvas_bounds(state);
                    [r.width, r.height]
                });
                let (_, min_z, max_z) =
                    actual_zoom_for_bounds(frame.width, frame.height, vp_w, vp_h);
                (min_z, max_z)
            } else {
                (0.02, 32.0)
            }
        }
        Workspace::Viewer => {
            if let Some(frame) = &state.viewer_frame {
                let (base_w, base_h) = if state.viewer_mode == ViewerViewMode::Double
                    && let Some(pair) = &state.viewer_pair_frame
                {
                    let (sw, sh, _, _) = dual_spread_geometry(
                        (frame.width, frame.height),
                        (pair.width, pair.height),
                        f32::from(state.viewer_manga_page_gap),
                    );
                    (sw, sh)
                } else {
                    (frame.width as f32, frame.height as f32)
                };
                let odd = (state.viewer_quarter_turns % 2) == 1;
                let (iw, ih) = if odd {
                    (base_h, base_w)
                } else {
                    (base_w, base_h)
                };
                let [vp_w, vp_h] = bounds_override.unwrap_or([
                    (state.window_size.width - 40.0).max(120.0),
                    (state.window_size.height - 130.0).max(120.0),
                ]);
                let (_, min_z, max_z) =
                    actual_zoom_for_bounds(iw.max(1.0) as u32, ih.max(1.0) as u32, vp_w, vp_h);
                (min_z, max_z)
            } else {
                (0.02, 32.0)
            }
        }
        Workspace::Browser => (0.02, 32.0),
    }
}

pub(super) fn compute_luminance_histogram(frame: &CpuImageFrame) -> Arc<[u32; 256]> {
    let mut bins = [0u32; 256];
    let total_pixels = frame.rgba.len() / 4;
    let step = (total_pixels / 250_000).max(1);
    let mut i = 0usize;
    while i < total_pixels {
        let off = i * 4;
        if off + 3 < frame.rgba.len() && frame.rgba[off + 3] > 0 {
            let r = u32::from(frame.rgba[off]);
            let g = u32::from(frame.rgba[off + 1]);
            let b = u32::from(frame.rgba[off + 2]);
            let luma = ((54 * r + 183 * g + 19 * b) >> 8).min(255) as usize;
            bins[luma] = bins[luma].saturating_add(1);
        }
        i += step;
    }
    Arc::new(bins)
}

pub(crate) fn compute_editor_curve_lut(curves: &photon_protocol::CurvesAdjustment) -> [u8; 1024] {
    let lut_rgb = photon_image::pixels::build_curve_lut(&curves.rgb);
    let lut_r = photon_image::pixels::build_curve_lut(&curves.red);
    let lut_g = photon_image::pixels::build_curve_lut(&curves.green);
    let lut_b = photon_image::pixels::build_curve_lut(&curves.blue);
    let mut out = [0u8; 1024];
    for i in 0..256 {
        let base = i * 4;
        out[base] = lut_rgb[usize::from(lut_r[i])];
        out[base + 1] = lut_rgb[usize::from(lut_g[i])];
        out[base + 2] = lut_rgb[usize::from(lut_b[i])];
        out[base + 3] = 255;
    }
    out
}
