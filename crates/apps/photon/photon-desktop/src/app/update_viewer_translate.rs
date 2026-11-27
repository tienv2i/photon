use super::*;

pub(super) fn tesseract_to_iso(code: &str) -> String {
    let primary = code.trim().split('+').next().unwrap_or("").trim();
    match primary.to_ascii_lowercase().as_str() {
        "eng" | "en" => "en".to_owned(),
        "jpn" | "jpn_vert" | "ja" => "ja".to_owned(),
        "chi_sim" | "chi_sim_vert" | "zh-cn" | "zh_cn" | "zh" => "zh-CN".to_owned(),
        "chi_tra" | "chi_tra_vert" | "zh-tw" | "zh_tw" => "zh-TW".to_owned(),
        "kor" | "kor_vert" | "ko" => "ko".to_owned(),
        "vie" | "vi" => "vi".to_owned(),
        "fra" | "fre" | "fr" => "fr".to_owned(),
        "deu" | "ger" | "de" => "de".to_owned(),
        "spa" | "es" => "es".to_owned(),
        "rus" | "ru" => "ru".to_owned(),
        "ita" | "it" => "it".to_owned(),
        "por" | "pt" => "pt".to_owned(),
        "ara" | "ar" => "ar".to_owned(),
        "tha" | "th" => "th".to_owned(),
        _ => primary.to_owned(),
    }
}

pub(super) fn iso_to_tesseract(code: &str) -> String {
    let norm = code.trim().to_ascii_lowercase();
    match norm.as_str() {
        "en" | "eng" => "eng".to_owned(),
        "ja" | "jpn" => "jpn".to_owned(),
        "jpn_vert" => "jpn_vert".to_owned(),
        "zh-cn" | "zh_cn" | "zh" | "chi_sim" => "chi_sim".to_owned(),
        "zh-tw" | "zh_tw" | "chi_tra" => "chi_tra".to_owned(),
        "ko" | "kor" => "kor".to_owned(),
        "vi" | "vie" => "vie".to_owned(),
        "fr" | "fra" => "fra".to_owned(),
        "de" | "deu" => "deu".to_owned(),
        "es" | "spa" => "spa".to_owned(),
        "ru" | "rus" => "rus".to_owned(),
        "it" | "ita" => "ita".to_owned(),
        "pt" | "por" => "por".to_owned(),
        "ar" | "ara" => "ara".to_owned(),
        "th" | "tha" => "tha".to_owned(),
        _ => code.trim().to_owned(),
    }
}

pub(super) fn viewer_viewport_size(state: &PhotonGui) -> (f32, f32) {
    let layout = &state.snapshot.meta.layout;
    let right_open = (state.viewer_adjustments_open
        && state.viewer_mode != ViewerViewMode::Webtoon)
        || state.viewer_info_open;
    let right_w = if right_open {
        state
            .viewer_right_width_preview
            .unwrap_or(layout.right_width as f32)
            .clamp(200.0, 600.0)
            + 7.0
    } else {
        0.0
    };
    let filmstrip_h = if state.viewer_mode != ViewerViewMode::Webtoon
        && state.snapshot.viewer_images.len() > 1
        && state.viewer_filmstrip_open
    {
        if state.snapshot.meta.viewer_archive.is_some() {
            116.0
        } else {
            92.0
        }
    } else {
        0.0
    };
    let vp_w = (state.window_size.width - right_w).max(120.0);
    let vp_h = (state.window_size.height - 48.0 - 28.0 - filmstrip_h).max(120.0);
    (vp_w, vp_h)
}

pub(super) fn viewer_fit_scale(
    doc_w: f32,
    doc_h: f32,
    quarter_turns: u8,
    zoom: f32,
    vp_w: f32,
    vp_h: f32,
) -> f32 {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let (fw, fh) = if quarter_turns % 2 == 1 {
        (doc_h.max(1.0), doc_w.max(1.0))
    } else {
        (doc_w.max(1.0), doc_h.max(1.0))
    };
    let vp_aspect = vp_w / vp_h;
    let img_aspect = fw / fh;
    let fit_w = if img_aspect > vp_aspect {
        vp_w
    } else {
        vp_h * img_aspect
    };
    ((fit_w * zoom.max(0.02)) / fw).max(1e-4)
}

pub(super) fn screen_delta_to_image_delta(
    dx_px: f32,
    dy_px: f32,
    scale: f32,
    quarter_turns: u8,
) -> (i32, i32) {
    let s = scale.max(1e-4);
    let rx = dx_px / s;
    let ry = dy_px / s;
    let (ix, iy) = match quarter_turns % 4 {
        1 => (ry, -rx),
        2 => (-rx, -ry),
        3 => (-ry, rx),
        _ => (rx, ry),
    };
    (ix.round() as i32, iy.round() as i32)
}

pub(super) fn sync_translate_bubble_cache(state: &mut PhotonGui) {
    if let Some(path) = state
        .viewer_translate
        .active_page_path
        .as_ref()
        .or(state.snapshot.meta.viewer_path.as_ref())
        .or(state.viewer_translate.last_path.as_ref())
        .cloned()
    {
        state
            .viewer_translate
            .bubble_cache
            .insert(path, state.viewer_translate.blocks.clone());
    }
}

pub(super) fn dispatch_viewer_translate_region(
    state: &mut PhotonGui,
    path: String,
    region: [i32; 4],
) -> Task<Message> {
    if path.is_empty() {
        return Task::none();
    }
    state.viewer_translate.last_was_region = true;
    state.viewer_translate.pending_replace_idx = None;
    state.active_translate_target = Some(path.clone());
    let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
        String::new()
    } else {
        state.viewer_translate.target_lang.clone()
    };
    dispatch_to_core(
        state.core.clone(),
        Command::ViewerTranslate {
            path,
            source_lang: state.viewer_translate.source_lang.clone(),
            target_lang,
            region: Some(region),
            ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
        },
    )
}

pub(super) fn resolve_double_marquee_region(
    marquee_display: [f32; 4],
    left_path: Option<&str>,
    left_size: (u32, u32),
    right_path: Option<&str>,
    right_size: Option<(u32, u32)>,
    dual_gap_px: f32,
    quarter_turns: u8,
) -> Option<(String, [i32; 4])> {
    let lp = left_path?;
    let [dx, dy, dw, dh] = marquee_display;
    let lw = left_size.0.max(1) as f32;
    let lh = left_size.1.max(1) as f32;
    let (Some(rp), Some(r_sz)) = (right_path, right_size) else {
        // Single page shown in Double mode (e.g. separate cover or landscape)
        let (ux, uy, uw, uh) = match quarter_turns % 4 {
            1 => (dy, (lh - (dx + dw)).max(0.0), dh, dw),
            2 => ((lw - (dx + dw)).max(0.0), (lh - (dy + dh)).max(0.0), dw, dh),
            3 => ((lw - (dy + dh)).max(0.0), dx, dh, dw),
            _ => (dx, dy, dw, dh),
        };
        return Some((
            lp.to_owned(),
            [
                ux.round().max(0.0) as i32,
                uy.round().max(0.0) as i32,
                uw.round().max(8.0) as i32,
                uh.round().max(8.0) as i32,
            ],
        ));
    };

    let (spread_w, spread_h, left_disp_w, gap_doc) =
        photon_canvas::dual_spread_geometry(left_size, r_sz, dual_gap_px);
    let right_disp_w = (spread_w - left_disp_w - gap_doc).max(1.0);
    let (ux, uy, uw, uh) = match quarter_turns % 4 {
        1 => (dy, (spread_h - (dx + dw)).max(0.0), dh, dw),
        2 => (
            (spread_w - (dx + dw)).max(0.0),
            (spread_h - (dy + dh)).max(0.0),
            dw,
            dh,
        ),
        3 => ((spread_w - (dy + dh)).max(0.0), dx, dh, dw),
        _ => (dx, dy, dw, dh),
    };
    let cx = ux + uw * 0.5;
    if cx < left_disp_w + gap_doc * 0.5 {
        let sx = lw / left_disp_w.max(1.0);
        let sy = lh / spread_h.max(1.0);
        Some((
            lp.to_owned(),
            [
                (ux * sx).round().clamp(0.0, lw) as i32,
                (uy * sy).round().clamp(0.0, lh) as i32,
                (uw * sx).round().max(8.0) as i32,
                (uh * sy).round().max(8.0) as i32,
            ],
        ))
    } else {
        let rw = r_sz.0.max(1) as f32;
        let rh = r_sz.1.max(1) as f32;
        let rx = (ux - (left_disp_w + gap_doc)).max(0.0);
        let sx = rw / right_disp_w;
        let sy = rh / spread_h.max(1.0);
        Some((
            rp.to_owned(),
            [
                (rx * sx).round().clamp(0.0, rw) as i32,
                (uy * sy).round().clamp(0.0, rh) as i32,
                (uw * sx).round().max(8.0) as i32,
                (uh * sy).round().max(8.0) as i32,
            ],
        ))
    }
}

pub(super) fn commit_translate_bubble_drag(
    state: &mut PhotonGui,
    page_path: Option<&str>,
    idx: usize,
    dx_px: f32,
    dy_px: f32,
) {
    if dx_px.abs() < 0.5 && dy_px.abs() < 0.5 {
        return;
    }
    let resolved_path = page_path
        .map(str::to_owned)
        .or_else(|| state.viewer_translate.active_page_path.clone())
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let Some(target_path) = resolved_path else {
        return;
    };

    let (dx_img, dy_img, max_x, max_y) = match state.viewer_mode {
        ViewerViewMode::Webtoon => {
            let Some((img_w, img_h)) = state.image_sizes.get(&target_path).copied() else {
                return;
            };
            let (_, pages) = photon_viewer::compute_webtoon_layout(state.viewer_view_state(true));
            let Some((_, disp_w, disp_h)) = pages.into_iter().find(|(p, _, _)| p == &target_path)
            else {
                return;
            };
            let dx = (dx_px * (img_w.max(1) as f32 / disp_w.max(1.0))).round() as i32;
            let dy = (dy_px * (img_h.max(1) as f32 / disp_h.max(1.0))).round() as i32;
            (dx, dy, img_w as i32, img_h as i32)
        }
        ViewerViewMode::Double => {
            let Some(left_frame) = &state.viewer_frame else {
                return;
            };
            let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
            let is_right = right_slot.is_some_and(|(_, p)| p == target_path);
            let left_size = (left_frame.width, left_frame.height);
            let right_size = state
                .viewer_pair_frame
                .as_ref()
                .map(|f| (f.width, f.height));
            let (vp_w, vp_h) = viewer_viewport_size(state);
            let geom = viewer_double_fit_geometries(
                left_size,
                right_size,
                f32::from(state.viewer_manga_page_gap),
                state.viewer_quarter_turns,
                state.zoom,
                state.pan_x,
                state.pan_y,
                vp_w,
                vp_h,
            );
            let spread_scale = (geom.drawn_w / geom.fw.max(1.0)).max(1e-4);
            let (dx_s, dy_s) =
                screen_delta_to_image_delta(dx_px, dy_px, spread_scale, state.viewer_quarter_turns);
            let (img_w, img_h, page_disp_w) = if is_right && let Some((rw, rh)) = right_size {
                (rw, rh, geom.right_disp_w.max(1.0))
            } else if left_slot.is_some_and(|(_, p)| p == target_path) {
                (left_size.0, left_size.1, geom.left_disp_w.max(1.0))
            } else {
                return;
            };
            let dx = (dx_s as f32 * (img_w.max(1) as f32 / page_disp_w)).round() as i32;
            let dy = (dy_s as f32 * (img_h.max(1) as f32 / geom.spread_h.max(1.0))).round() as i32;
            (dx, dy, img_w as i32, img_h as i32)
        }
        ViewerViewMode::Single => {
            let Some(frame) = &state.viewer_frame else {
                return;
            };
            let doc_w = frame.width.max(1) as f32;
            let doc_h = frame.height.max(1) as f32;
            let (vp_w, vp_h) = viewer_viewport_size(state);
            let scale = viewer_fit_scale(
                doc_w,
                doc_h,
                state.viewer_quarter_turns,
                state.zoom,
                vp_w,
                vp_h,
            );
            let (dx, dy) =
                screen_delta_to_image_delta(dx_px, dy_px, scale, state.viewer_quarter_turns);
            (dx, dy, frame.width as i32, frame.height as i32)
        }
    };
    if dx_img == 0 && dy_img == 0 {
        return;
    }

    if let Some(cached) = state.viewer_translate.bubble_cache.get_mut(&target_path)
        && let Some(b) = cached.get_mut(idx)
    {
        b.x = (b.x + dx_img).clamp(0, (max_x - b.w).max(0));
        b.y = (b.y + dy_img).clamp(0, (max_y - b.h).max(0));
        if state.viewer_translate.active_page_path.as_deref() == Some(target_path.as_str())
            || state.snapshot.meta.viewer_path.as_deref() == Some(target_path.as_str())
        {
            state.viewer_translate.blocks = cached.clone();
        }
        return;
    }

    if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
        b.x = (b.x + dx_img).clamp(0, (max_x - b.w).max(0));
        b.y = (b.y + dy_img).clamp(0, (max_y - b.h).max(0));
    }
    sync_translate_bubble_cache(state);
}

pub(super) fn handle_viewer_translate_message(
    state: &mut PhotonGui,
    msg: ViewerMessage,
) -> Task<Message> {
    let lang = state.snapshot.meta.language;
    match msg {
        ViewerMessage::ToggleTranslateOverlay => {
            state.active_menu = None;
            state.context_menu = None;
            state.viewer_translate.active = !state.viewer_translate.active;
            if state.viewer_translate.active {
                if state.viewer_translate.mode == ViewerTranslateMode::Pan {
                    state.viewer_translate.mode = ViewerTranslateMode::Translate;
                }
                state.viewer_translate.active_page_path = state.snapshot.meta.viewer_path.clone();
                if let Some(saved) = &state.snapshot.meta.viewer_saved_translation
                    && Some(saved.path.as_str()) == state.snapshot.meta.viewer_path.as_deref()
                {
                    state.viewer_translate.blocks = saved.blocks.clone();
                    state.viewer_translate.saved_sidecar = true;
                }
            } else {
                state.viewer_translate.picking_region = false;
                state.viewer_translate.editing_block_idx = None;
                state.viewer_translate.result_card_visible = false;
                state.viewer_translate.active_drag_delta = None;
                state.viewer_translate.active_drag_page = None;
                state.viewer_translate.webtoon_marquee = None;
                state.viewer_translate.space_pan_held = false;
                state.translate_drag = TranslateDragTarget::None;
                state.pending_translate_queue.clear();
                state.active_translate_target = None;
                state.webtoon_marquee_drag = None;
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        ViewerMessage::CloseTranslateOverlay => {
            state.active_menu = None;
            state.context_menu = None;
            state.viewer_translate.active = false;
            state.viewer_translate.picking_region = false;
            state.viewer_translate.editing_block_idx = None;
            state.viewer_translate.result_card_visible = false;
            state.viewer_translate.active_drag_delta = None;
            state.viewer_translate.active_drag_page = None;
            state.viewer_translate.webtoon_marquee = None;
            state.viewer_translate.space_pan_held = false;
            state.translate_drag = TranslateDragTarget::None;
            state.pending_translate_queue.clear();
            state.active_translate_target = None;
            state.webtoon_marquee_drag = None;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        ViewerMessage::SetTranslateMode(mode) => {
            state.viewer_translate.mode = mode;
            if mode != ViewerTranslateMode::Pan {
                state.viewer_translate.picking_region = false;
            }
            Task::none()
        }
        ViewerMessage::SetTranslateOcrEngine(engine) => {
            state.viewer_translate.ocr_engine = engine;
            Task::none()
        }
        ViewerMessage::SetTranslateSourceLang(s) => {
            state.viewer_translate.source_lang = s;
            Task::none()
        }
        ViewerMessage::SetTranslateTargetLang(s) => {
            state.viewer_translate.target_lang = s;
            Task::none()
        }
        ViewerMessage::SwapTranslateLangs => {
            let old_src = state.viewer_translate.source_lang.clone();
            let old_dst = state.viewer_translate.target_lang.clone();
            state.viewer_translate.source_lang = iso_to_tesseract(&old_dst);
            state.viewer_translate.target_lang = tesseract_to_iso(&old_src);
            Task::none()
        }
        ViewerMessage::StartTranslateRegionPick => {
            state.context_menu = None;
            state.viewer_translate.active = true;
            if state.viewer_translate.mode == ViewerTranslateMode::Pan {
                state.viewer_translate.mode = ViewerTranslateMode::Translate;
            }
            state.viewer_translate.picking_region = true;
            sync_ui_tree_task(state)
        }
        ViewerMessage::ScanTranslatePage => {
            state.viewer_translate.picking_region = false;
            state.viewer_translate.last_was_region = false;
            state.viewer_translate.pending_replace_idx = None;
            state.pending_translate_queue.clear();
            let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                String::new()
            } else {
                state.viewer_translate.target_lang.clone()
            };
            let ocr_engine = state.viewer_translate.ocr_engine.as_str().to_owned();
            let source_lang = state.viewer_translate.source_lang.clone();

            let mut scan_paths: Vec<String> = Vec::new();
            match state.viewer_mode {
                ViewerViewMode::Double => {
                    let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                    if let Some((_, lp)) = left_slot {
                        scan_paths.push(lp.to_owned());
                    }
                    if let Some((_, rp)) = right_slot
                        && !scan_paths.iter().any(|p| p == rp)
                    {
                        scan_paths.push(rp.to_owned());
                    }
                }
                ViewerViewMode::Webtoon => {
                    let vis_path = state
                        .webtoon_visible_index
                        .or(state.snapshot.meta.viewer_index)
                        .and_then(|i| state.snapshot.viewer_images.get(i).cloned())
                        .or_else(|| state.snapshot.meta.viewer_path.clone());
                    if let Some(p) = vis_path {
                        scan_paths.push(p);
                    }
                }
                ViewerViewMode::Single => {
                    if let Some(p) = state.snapshot.meta.viewer_path.clone() {
                        scan_paths.push(p);
                    }
                }
            }
            if scan_paths.is_empty() {
                return Task::none();
            }
            let first = scan_paths.remove(0);
            for rest in scan_paths {
                state
                    .pending_translate_queue
                    .push_back(Command::ViewerTranslate {
                        path: rest,
                        source_lang: source_lang.clone(),
                        target_lang: target_lang.clone(),
                        region: None,
                        ocr_engine: ocr_engine.clone(),
                    });
            }
            state.active_translate_target = Some(first.clone());
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerTranslate {
                    path: first,
                    source_lang,
                    target_lang,
                    region: None,
                    ocr_engine,
                },
            )
        }
        ViewerMessage::ToggleTranslateBubbles => {
            state.viewer_translate.show_bubbles = !state.viewer_translate.show_bubbles;
            Task::none()
        }
        ViewerMessage::ToggleTranslateBubbleStyle => {
            state.viewer_translate.frosted_bubbles = !state.viewer_translate.frosted_bubbles;
            Task::none()
        }
        ViewerMessage::ToggleTranslateBlockOriginal(idx) => {
            if !state.viewer_translate.show_original_indices.remove(&idx) {
                state.viewer_translate.show_original_indices.insert(idx);
            }
            Task::none()
        }
        ViewerMessage::SetTranslateFontScale(scale) => {
            state.viewer_translate.font_scale = scale.clamp(0.7, 2.0);
            Task::none()
        }
        ViewerMessage::SelectTranslateBlock(idx) => {
            state.viewer_translate.editing_block_idx = idx;
            state.viewer_translate.result_card_visible = idx.is_some();
            Task::none()
        }
        ViewerMessage::UpdateTranslateBlockOriginal(idx, text_val) => {
            if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
                b.original = text_val;
            }
            sync_translate_bubble_cache(state);
            Task::none()
        }
        ViewerMessage::UpdateTranslateBlockText(idx, text_val) => {
            if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
                if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                    b.original = text_val;
                } else {
                    b.translated = text_val;
                }
            }
            sync_translate_bubble_cache(state);
            Task::none()
        }
        ViewerMessage::MoveTranslateBlock(idx, dx, dy) => {
            if let Some(b) = state.viewer_translate.blocks.get_mut(idx) {
                b.x = (b.x + dx).max(0);
                b.y = (b.y + dy).max(0);
            }
            sync_translate_bubble_cache(state);
            Task::none()
        }
        ViewerMessage::RetryTranslateBlock(idx) => {
            let Some(path) = state.snapshot.meta.viewer_path.clone() else {
                return Task::none();
            };
            let Some(b) = state.viewer_translate.blocks.get(idx).cloned() else {
                return Task::none();
            };
            state.viewer_translate.last_was_region = true;
            state.viewer_translate.pending_replace_idx = Some(idx);
            let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                String::new()
            } else {
                state.viewer_translate.target_lang.clone()
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerTranslate {
                    path,
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang,
                    region: Some([b.x.max(0), b.y.max(0), b.w.max(8), b.h.max(8)]),
                    ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
                },
            )
        }
        ViewerMessage::RemoveTranslateBlock(idx) => {
            if idx < state.viewer_translate.blocks.len() {
                state.viewer_translate.blocks.remove(idx);
                if state.viewer_translate.editing_block_idx == Some(idx) {
                    state.viewer_translate.editing_block_idx = None;
                    state.viewer_translate.result_card_visible = false;
                }
                sync_translate_bubble_cache(state);
            }
            Task::none()
        }
        ViewerMessage::ClearTranslateBlocks => {
            state.viewer_translate.blocks.clear();
            state.viewer_translate.editing_block_idx = None;
            state.viewer_translate.result_card_visible = false;
            state.viewer_translate.show_original_indices.clear();
            state.viewer_translate.pending_replace_idx = None;
            if let Some(path) = state
                .viewer_translate
                .active_page_path
                .clone()
                .or_else(|| state.snapshot.meta.viewer_path.clone())
            {
                state.viewer_translate.bubble_cache.remove(&path);
            }
            Task::none()
        }
        ViewerMessage::BeginDragTranslateBubble(idx) => {
            if idx < state.viewer_translate.blocks.len() {
                let start_cursor = state.last_cursor.unwrap_or(Point::ORIGIN);
                state.translate_drag = TranslateDragTarget::Bubble {
                    page_path: None,
                    idx,
                    start_cursor,
                    dx_px: 0.0,
                    dy_px: 0.0,
                };
                state.viewer_translate.active_drag_delta = Some((idx, [0.0, 0.0]));
                state.viewer_translate.active_drag_page = None;
                state.viewer_translate.editing_block_idx = Some(idx);
                state.viewer_translate.result_card_visible = true;
            }
            Task::none()
        }
        ViewerMessage::BeginDragTranslatePageBubble(path, idx) => {
            if let Some(cached) = state.viewer_translate.bubble_cache.get(&path).cloned()
                && idx < cached.len()
            {
                let start_cursor = state.last_cursor.unwrap_or(Point::ORIGIN);
                state.viewer_translate.blocks = cached;
                state.viewer_translate.active_page_path = Some(path.clone());
                state.viewer_translate.active_drag_page = Some(path.clone());
                state.viewer_translate.active_drag_delta = Some((idx, [0.0, 0.0]));
                state.viewer_translate.editing_block_idx = Some(idx);
                state.viewer_translate.result_card_visible = true;
                state.translate_drag = TranslateDragTarget::Bubble {
                    page_path: Some(path),
                    idx,
                    start_cursor,
                    dx_px: 0.0,
                    dy_px: 0.0,
                };
            }
            Task::none()
        }
        ViewerMessage::SelectTranslatePageBlock(path, idx) => {
            if let Some(cached) = state.viewer_translate.bubble_cache.get(&path).cloned()
                && idx < cached.len()
            {
                state.viewer_translate.blocks = cached;
                state.viewer_translate.active_page_path = Some(path);
                state.viewer_translate.editing_block_idx = Some(idx);
                state.viewer_translate.result_card_visible = true;
            }
            Task::none()
        }
        ViewerMessage::ToggleTranslatePageBlockOriginal(path, idx) => {
            let key = (path, idx);
            if !state
                .viewer_translate
                .show_original_page_blocks
                .remove(&key)
            {
                state.viewer_translate.show_original_page_blocks.insert(key);
            }
            Task::none()
        }
        ViewerMessage::RetryTranslatePageBlock(path, idx) => {
            let Some(b) = state
                .viewer_translate
                .bubble_cache
                .get(&path)
                .and_then(|v| v.get(idx))
                .cloned()
            else {
                return Task::none();
            };
            state.viewer_translate.active_page_path = Some(path.clone());
            state.viewer_translate.last_was_region = true;
            state.viewer_translate.pending_replace_idx = Some(idx);
            state.active_translate_target = Some(path.clone());
            let target_lang = if state.viewer_translate.mode == ViewerTranslateMode::OcrOnly {
                String::new()
            } else {
                state.viewer_translate.target_lang.clone()
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerTranslate {
                    path,
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang,
                    region: Some([b.x.max(0), b.y.max(0), b.w.max(8), b.h.max(8)]),
                    ocr_engine: state.viewer_translate.ocr_engine.as_str().to_owned(),
                },
            )
        }
        ViewerMessage::RemoveTranslatePageBlock(path, idx) => {
            if let Some(cached) = state.viewer_translate.bubble_cache.get_mut(&path)
                && idx < cached.len()
            {
                cached.remove(idx);
                if state.viewer_translate.active_page_path.as_deref() == Some(path.as_str()) {
                    state.viewer_translate.blocks = cached.clone();
                    if state.viewer_translate.editing_block_idx == Some(idx) {
                        state.viewer_translate.editing_block_idx = None;
                        state.viewer_translate.result_card_visible = false;
                    }
                }
            }
            Task::none()
        }
        ViewerMessage::WebtoonHoverPage {
            path,
            local_pos,
            disp_w,
            disp_h,
        } => {
            state.webtoon_hover = Some((path.clone(), local_pos, disp_w, disp_h));
            if let Some(drag) = &mut state.webtoon_marquee_drag
                && drag.path == path
            {
                drag.cur_local = local_pos;
                let x = drag.start_local[0].min(local_pos[0]);
                let y = drag.start_local[1].min(local_pos[1]);
                let w = (drag.start_local[0] - local_pos[0]).abs();
                let h = (drag.start_local[1] - local_pos[1]).abs();
                state.viewer_translate.webtoon_marquee = Some((path, [x, y, w, h]));
            }
            Task::none()
        }
        ViewerMessage::BeginWebtoonTranslateRegion {
            path,
            disp_w,
            disp_h,
        } => {
            let start_local = state
                .webtoon_hover
                .as_ref()
                .filter(|(p, _, _, _)| p == &path)
                .map(|(_, pos, _, _)| *pos)
                .unwrap_or([0.0, 0.0]);
            state.webtoon_marquee_drag = Some(WebtoonMarqueeDrag {
                path: path.clone(),
                start_local,
                cur_local: start_local,
                start_cursor: state.last_cursor.unwrap_or(Point::ORIGIN),
                disp_w,
                disp_h,
            });
            state.viewer_translate.webtoon_marquee =
                Some((path, [start_local[0], start_local[1], 0.0, 0.0]));
            Task::none()
        }
        ViewerMessage::BeginDragTranslateTopBar => {
            let start_cursor = state.last_cursor.unwrap_or(Point::ORIGIN);
            state.translate_drag = TranslateDragTarget::TopBar {
                start_cursor,
                base_offset: state.viewer_translate.top_bar_offset,
            };
            Task::none()
        }
        ViewerMessage::BeginDragTranslateResultCard => {
            let start_cursor = state.last_cursor.unwrap_or(Point::ORIGIN);
            state.translate_drag = TranslateDragTarget::ResultCard {
                start_cursor,
                base_offset: state.viewer_translate.result_card_offset,
            };
            Task::none()
        }
        ViewerMessage::ResetTranslateTopBarPos => {
            state.viewer_translate.top_bar_offset = [0.0, 0.0];
            Task::none()
        }
        ViewerMessage::ResetTranslateResultCardPos => {
            state.viewer_translate.result_card_offset = [0.0, 0.0];
            Task::none()
        }
        ViewerMessage::DismissTranslateResultCard => {
            state.viewer_translate.result_card_visible = false;
            state.viewer_translate.editing_block_idx = None;
            Task::none()
        }
        ViewerMessage::NextTranslateBubble => {
            let len = state.viewer_translate.blocks.len();
            if len > 0 {
                let next = match state.viewer_translate.editing_block_idx {
                    Some(i) => (i + 1) % len,
                    None => 0,
                };
                state.viewer_translate.editing_block_idx = Some(next);
                state.viewer_translate.result_card_visible = true;
            }
            Task::none()
        }
        ViewerMessage::ToggleTranslateResultCardOriginal(show_orig) => {
            state.viewer_translate.result_card_show_original = show_orig;
            Task::none()
        }
        ViewerMessage::SaveTranslationSidecar => {
            let Some(path) = state
                .viewer_translate
                .active_page_path
                .clone()
                .or_else(|| state.snapshot.meta.viewer_path.clone())
            else {
                return Task::none();
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerSaveTranslation {
                    path,
                    blocks: state.viewer_translate.blocks.clone(),
                    source_lang: state.viewer_translate.source_lang.clone(),
                    target_lang: state.viewer_translate.target_lang.clone(),
                },
            )
        }
        ViewerMessage::DeleteTranslationSidecar => {
            let Some(path) = state
                .viewer_translate
                .active_page_path
                .clone()
                .or_else(|| state.snapshot.meta.viewer_path.clone())
            else {
                return Task::none();
            };
            dispatch_to_core(
                state.core.clone(),
                Command::ViewerDeleteTranslation { path },
            )
        }
        ViewerMessage::CopyTranslatedText => {
            let ocr_only = state.viewer_translate.mode == ViewerTranslateMode::OcrOnly;
            let combined: String = state
                .viewer_translate
                .blocks
                .iter()
                .map(|b| {
                    if ocr_only || b.translated.trim().is_empty() {
                        b.original.trim()
                    } else {
                        b.translated.trim()
                    }
                })
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if combined.is_empty() {
                return Task::none();
            }
            let toast_task = show_toast(
                state,
                tr(lang, "toast-clipboard-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(combined), toast_task])
        }
        _ => Task::none(),
    }
}
