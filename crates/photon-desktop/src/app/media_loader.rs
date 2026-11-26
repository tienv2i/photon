use super::*;

pub(super) fn sync_expanded_tree_tasks(
    state: &mut PhotonGui,
    force_reload_expanded: bool,
) -> Task<Message> {
    let Some(root_str) = state.snapshot.meta.root_folder.clone() else {
        return Task::none();
    };
    let mut to_load = Vec::new();
    if state.expanded_folders.insert(root_str.clone()) || force_reload_expanded {
        to_load.push(root_str.clone());
    }
    if let Some(cur) = &state.snapshot.meta.current_folder {
        let root_path = Path::new(&root_str);
        let mut cur_path = PathBuf::from(cur);
        while cur_path.starts_with(root_path) {
            if let Some(parent) = cur_path.parent() {
                let p_str = parent.to_string_lossy().into_owned();
                if parent.starts_with(root_path)
                    && (state.expanded_folders.insert(p_str.clone())
                        || !state.folder_children.contains_key(&p_str))
                    && !to_load.contains(&p_str)
                {
                    to_load.push(p_str);
                }
                if parent == root_path {
                    break;
                }
                cur_path = parent.to_path_buf();
            } else {
                break;
            }
        }
    }
    if force_reload_expanded {
        for expanded in &state.expanded_folders {
            if !to_load.contains(expanded) {
                to_load.push(expanded.clone());
            }
        }
    }
    Task::batch(
        to_load
            .into_iter()
            .map(|p| load_folder_children_task(state.core.clone(), p)),
    )
}

pub(super) fn load_folder_children_task(core: CoreHandle, path: String) -> Task<Message> {
    Task::perform(
        async move {
            let res = core.query(Query::ListFolders { path: path.clone() }).await;
            let mut children = Vec::new();
            if let Ok(Ok(val)) = res
                && let Some(arr) = val.get("folders").and_then(|v| v.as_array())
            {
                for item in arr {
                    if let (Some(name), Some(child_path)) = (
                        item.get("name").and_then(|v| v.as_str()),
                        item.get("path").and_then(|v| v.as_str()),
                    ) {
                        children.push((name.to_owned(), child_path.to_owned()));
                    }
                }
            }
            (path, children)
        },
        |(path, children)| Message::FolderChildrenLoaded(path, children),
    )
}

pub(super) const ARCHIVE_THUMB_WINDOW_BEFORE: usize = 10;
pub(super) const ARCHIVE_THUMB_WINDOW_AFTER: usize = 18;
/// Viewer page probe radius (both sides) for non-archive images.
pub(super) const VIEWER_PROBE_RADIUS: usize = 8;
/// Thumbnail prefetch radius around the active standalone-viewer page.
pub(super) const VIEWER_THUMB_RADIUS: usize = 8;
/// Webtoon thumbnail window: before the active page and after it (inclusive).
pub(super) const WEBTOON_THUMB_BEFORE: usize = 4;
pub(super) const WEBTOON_THUMB_AFTER: usize = 11;
/// Entries scheduled for thumbnails on the first browser paint (~2 screens).
pub(super) const BROWSER_INITIAL_VISIBLE_ENTRIES: usize = 120;
/// Maximum number of thumbnail jobs in flight at once.
pub(super) const THUMBNAIL_MAX_IN_FLIGHT: usize = 24;
/// Hard cap for retained thumbnail textures; evicted ones are re-requested
/// when the visible range changes.
pub(super) const THUMBNAIL_CACHE_MAX: usize = 512;

/// Pure half-open index window around `center`, clamped to `0..len`.
pub(super) fn probe_window(
    len: usize,
    center: usize,
    before: usize,
    after: usize,
) -> std::ops::Range<usize> {
    if len == 0 {
        return 0..0;
    }
    let c = center.min(len - 1);
    let start = c.saturating_sub(before);
    let end = c.saturating_add(after).saturating_add(1).min(len);
    start..end
}

pub(super) fn archive_thumbnail_window_indices(
    total: usize,
    center: usize,
    before: usize,
    after: usize,
) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    let c = center.min(total - 1);
    let start = c.saturating_sub(before);
    let end = (c + after + 1).min(total);
    let mut indices = Vec::with_capacity(end - start);
    indices.push(c);
    let mut step = 1;
    while indices.len() < end - start {
        if c + step < end {
            indices.push(c + step);
        }
        if let Some(left) = c.checked_sub(step)
            && left >= start
        {
            indices.push(left);
        }
        step += 1;
    }
    indices
}

pub(super) fn browser_visible_entry_range(state: &PhotonGui) -> std::ops::Range<usize> {
    let len = state.snapshot.entries.len();
    if len == 0 {
        return 0..0;
    }
    let (start, end) = state
        .browser_visible_range
        .unwrap_or((0, BROWSER_INITIAL_VISIBLE_ENTRIES));
    let start = start.min(len);
    let end = end.min(len).max(start);
    start..end
}

/// Bounded thumbnail window for a non-archive viewer. Webtoon uses its
/// near-page window; single/double share a small radius around the active page.
pub(super) fn viewer_thumbnail_window(state: &PhotonGui) -> std::ops::Range<usize> {
    let total = state.snapshot.viewer_images.len();
    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
    if state.viewer_mode == ViewerViewMode::Webtoon {
        if total <= 24 {
            0..total
        } else {
            probe_window(total, center, WEBTOON_THUMB_BEFORE, WEBTOON_THUMB_AFTER)
        }
    } else {
        probe_window(total, center, VIEWER_THUMB_RADIUS, VIEWER_THUMB_RADIUS)
    }
}

/// Paths that survive a thumbnail-cache prune: in-flight first (never pruned),
/// then the browser visible range, viewer window, and current selection.
pub(super) fn thumbnail_keep_paths(state: &PhotonGui) -> Vec<String> {
    let mut ordered: Vec<String> = state.thumbnails_loading.iter().cloned().collect();
    if state.snapshot.meta.workspace == Workspace::Browser {
        let range = browser_visible_entry_range(state);
        for entry in &state.snapshot.entries[range] {
            if entry.is_image {
                ordered.push(entry.path.clone());
            }
        }
    }
    if state.snapshot.meta.viewer_archive.is_some() {
        let center = state.snapshot.meta.viewer_index.unwrap_or(0);
        for idx in archive_thumbnail_window_indices(
            state.snapshot.viewer_images.len(),
            center,
            ARCHIVE_THUMB_WINDOW_BEFORE,
            ARCHIVE_THUMB_WINDOW_AFTER,
        ) {
            if let Some(p) = state.snapshot.viewer_images.get(idx) {
                ordered.push(p.clone());
            }
        }
    } else {
        for idx in viewer_thumbnail_window(state) {
            if let Some(p) = state.snapshot.viewer_images.get(idx) {
                ordered.push(p.clone());
            }
        }
    }
    if state.viewer_filmstrip_open {
        for idx in viewer_filmstrip_thumb_range(state) {
            if let Some(p) = state.snapshot.viewer_images.get(idx) {
                ordered.push(p.clone());
            }
        }
    }
    ordered.extend(state.snapshot.meta.selection.iter().cloned());
    ordered
}

pub(super) fn thumbnail_keep_set(
    ordered: &[String],
    loading: &BTreeSet<String>,
    cap: usize,
) -> BTreeSet<String> {
    let mut keep: BTreeSet<String> = BTreeSet::new();
    for path in ordered {
        if keep.len() >= cap && !loading.contains(path) {
            continue;
        }
        keep.insert(path.clone());
    }
    keep
}

pub(super) fn prune_thumbnails(state: &mut PhotonGui) {
    if state.thumbnails.len() <= THUMBNAIL_CACHE_MAX {
        return;
    }
    let ordered = thumbnail_keep_paths(state);
    let keep = thumbnail_keep_set(&ordered, &state.thumbnails_loading, THUMBNAIL_CACHE_MAX);
    state.thumbnails.retain(|k, _| keep.contains(k.as_str()));
}

/// Add candidates until the in-flight cap is reached. Candidates already
/// cached or loading are skipped so repeated syncs are cheap.
pub(super) fn queue_thumbnail_candidates(
    state: &mut PhotonGui,
    candidates: impl IntoIterator<Item = String>,
    pending: &mut Vec<String>,
) {
    for path in candidates {
        if state.thumbnails_loading.len() >= THUMBNAIL_MAX_IN_FLIGHT {
            break;
        }
        if state.thumbnails.contains_key(&path) {
            continue;
        }
        if !state.thumbnails_loading.insert(path.clone()) {
            continue;
        }
        pending.push(path);
    }
}

/// Approximate filmstrip viewport width (window minus the close-button column).
pub(super) fn viewer_filmstrip_viewport_width(state: &PhotonGui) -> f32 {
    (state.window_size.width - 48.0).max(120.0)
}

/// Scroll the filmstrip so the active page is centered when it opens or changes.
pub(super) fn scroll_filmstrip_to_active(state: &PhotonGui) -> Task<Message> {
    if !state.viewer_filmstrip_open {
        return Task::none();
    }
    let index = state.snapshot.meta.viewer_index.unwrap_or(0);
    let viewport_w = viewer_filmstrip_viewport_width(state);
    widget_operation::scroll_to(
        filmstrip_scroll_id(),
        scrollable::AbsoluteOffset {
            x: Some(filmstrip_offset_for_index(index, viewport_w)),
            y: None,
        },
    )
}

/// Index range the filmstrip needs thumbnails for, derived from its last known
/// scroll offset and the current viewport width so resizes are covered too.
pub(super) fn viewer_filmstrip_thumb_range(state: &PhotonGui) -> std::ops::Range<usize> {
    let total = state.snapshot.viewer_images.len();
    if total == 0 {
        return 0..0;
    }
    let (start, end) = filmstrip_visible_range(
        state.viewer_filmstrip_offset,
        viewer_filmstrip_viewport_width(state),
        total,
        FILMSTRIP_VISIBLE_OVERSCAN,
    );
    let start = start.min(total);
    let end = end.min(total).max(start);
    start..end
}

pub(super) fn schedule_missing_thumbnails(state: &mut PhotonGui) -> Task<Message> {
    let is_archive = state.snapshot.meta.viewer_archive.is_some();
    prune_thumbnails(state);

    let workspace = state.snapshot.meta.workspace;
    let mut pending: Vec<String> = Vec::new();

    // Browser: only the entries currently inside the viewport (plus overscan).
    if workspace == Workspace::Browser {
        let range = browser_visible_entry_range(state);
        let candidates: Vec<String> = state.snapshot.entries[range]
            .iter()
            .filter(|e| e.is_image)
            .map(|e| e.path.clone())
            .collect();
        queue_thumbnail_candidates(state, candidates, &mut pending);
    }

    // Filmstrip first: schedule exactly what the strip currently shows, for both
    // plain folders and archives (their pages are virtual paths).
    if workspace == Workspace::Viewer && state.viewer_filmstrip_open {
        let candidates: Vec<String> = viewer_filmstrip_thumb_range(state)
            .filter_map(|idx| state.snapshot.viewer_images.get(idx))
            .cloned()
            .collect();
        queue_thumbnail_candidates(state, candidates, &mut pending);
    }
    if is_archive {
        let center = state.snapshot.meta.viewer_index.unwrap_or(0);
        let candidates: Vec<String> = archive_thumbnail_window_indices(
            state.snapshot.viewer_images.len(),
            center,
            ARCHIVE_THUMB_WINDOW_BEFORE,
            ARCHIVE_THUMB_WINDOW_AFTER,
        )
        .into_iter()
        .filter_map(|idx| state.snapshot.viewer_images.get(idx))
        .cloned()
        .collect();
        queue_thumbnail_candidates(state, candidates, &mut pending);
    } else if workspace == Workspace::Viewer {
        // Never enqueue the whole folder for a standalone Viewer.
        let candidates: Vec<String> = viewer_thumbnail_window(state)
            .filter_map(|idx| state.snapshot.viewer_images.get(idx))
            .cloned()
            .collect();
        queue_thumbnail_candidates(state, candidates, &mut pending);
    }

    if pending.is_empty() {
        return Task::none();
    }

    let archive = is_archive.then(|| state.core.archive_session());
    let chunk_size = if is_archive { 4 } else { 8 };
    let tasks = pending
        .chunks(chunk_size)
        .map(|chunk| {
            let paths = chunk.to_vec();
            let archive = archive.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let mut out = Vec::new();
                        for path in &paths {
                            let thumb_res = if let Some(ref session) = archive {
                                match photon_core::Core::archive_thumbnail_for_path(
                                    session,
                                    Path::new(path),
                                    240,
                                ) {
                                    Ok(Some(img)) => Ok(img),
                                    Ok(None) => {
                                        photon_image::generate_thumbnail(Path::new(path), 240)
                                    }
                                    Err(e) => Err(e),
                                }
                            } else {
                                photon_image::generate_thumbnail(Path::new(path), 240)
                            };
                            if let Ok(thumb) = thumb_res {
                                let (w, h) = thumb.dimensions();
                                out.push((path.clone(), w, h, thumb.into_raw()));
                            }
                        }
                        (paths, out)
                    })
                    .await
                    .unwrap_or_default()
                },
                |(attempted, items)| Message::ThumbnailsLoaded(attempted, items),
            )
        })
        .collect::<Vec<_>>();

    Task::batch(tasks)
}

/// Compute KogaView-style fitted window dimensions when opened directly on an image file.
pub(super) fn window_size_for_image(snap: &UiStateSnapshot, img_w: u32, img_h: u32) -> Size {
    if img_w == 0 || img_h == 0 {
        return Size::new(
            (snap.meta.layout.window_width as f32).clamp(680.0, 1728.0),
            (snap.meta.layout.window_height as f32).clamp(500.0, 972.0),
        );
    }
    let layout = &snap.meta.layout;
    let is_editor = snap.meta.workspace == Workspace::Editor;
    let info_open = if is_editor {
        layout.panel("editor.inspector")
    } else {
        layout.panel("viewer.inspector")
    };
    let tools_open = is_editor && layout.panel("editor.tools");
    let filmstrip_open = !is_editor
        && layout.viewer_view_mode != ViewerViewMode::Webtoon
        && snap.meta.viewer_count > 1
        && layout.panel("viewer.filmstrip");

    let chrome_w = if info_open { 246.0 } else { 0.0 } + if tools_open { 68.0 } else { 0.0 } + 16.0;
    let chrome_h = 50.0 + 28.0 + if filmstrip_open { 68.0 } else { 0.0 } + 16.0;

    // Conservative 90% work-area cap for typical 1080p+ desktops without needing screen probe.
    let max_w = 1728.0_f32;
    let max_h = 972.0_f32;
    let avail_w = (max_w - chrome_w).max(200.0);
    let avail_h = (max_h - chrome_h).max(200.0);

    let iw = img_w as f32;
    let ih = img_h as f32;
    let mut scale = (avail_w / iw).min(avail_h / ih).min(1.0);
    if scale < 1.0 {
        scale *= 0.98;
    }

    let w = (iw * scale + chrome_w).round().clamp(680.0, max_w);
    let h = (ih * scale + chrome_h).round().clamp(500.0, max_h);
    Size::new(w, h)
}

pub(super) fn probe_viewer_image_sizes(state: &mut PhotonGui) -> Task<Message> {
    let is_archive = state.snapshot.meta.viewer_archive.is_some();
    let total = state.snapshot.viewer_images.len();
    if total == 0 {
        return Task::none();
    }
    if !is_archive {
        // Browser only probes when a viewer image will actually be shown
        // (e.g. the preview); never walk every sibling in a large folder.
        if state.snapshot.meta.workspace == Workspace::Browser
            && state.snapshot.meta.viewer_path.is_none()
        {
            return Task::none();
        }
        let center = state.snapshot.meta.viewer_index.unwrap_or(0);
        let range = probe_window(total, center, VIEWER_PROBE_RADIUS, VIEWER_PROBE_RADIUS);
        for path in &state.snapshot.viewer_images[range] {
            if !state.image_sizes.contains_key(path)
                && let Some(dims) = photon_image::probe_dimensions(Path::new(path))
            {
                state.image_sizes.insert(path.clone(), dims);
            }
        }
        return Task::none();
    }
    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
    let indices =
        archive_thumbnail_window_indices(state.snapshot.viewer_images.len(), center, 2, 5);
    let pending: Vec<String> = indices
        .into_iter()
        .filter_map(|idx| state.snapshot.viewer_images.get(idx))
        .filter(|p| !state.image_sizes.contains_key(*p))
        .cloned()
        .collect();
    if pending.is_empty() {
        return Task::none();
    }
    let archive = state.core.archive_session();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut probed = Vec::new();
                for path in pending {
                    let resolved =
                        photon_core::Core::ensure_archive_page_for_path(&archive, Path::new(&path))
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| PathBuf::from(&path));
                    if let Some(dims) = photon_image::probe_dimensions(&resolved) {
                        probed.push((path, dims));
                    }
                }
                probed
            })
            .await
            .unwrap_or_default()
        },
        Message::ViewerImageSizesProbed,
    )
}

pub(super) fn schedule_webtoon_pages_near(
    state: &mut PhotonGui,
    center_idx: usize,
) -> Task<Message> {
    let total = state.snapshot.viewer_images.len();
    if total == 0 {
        return Task::none();
    }
    let (start, end) = if total <= 24 {
        (0, total)
    } else {
        (center_idx.saturating_sub(4), (center_idx + 12).min(total))
    };
    let pending: Vec<String> = state.snapshot.viewer_images[start..end]
        .iter()
        .filter(|p| {
            !state.webtoon_pages.contains_key(*p) && state.webtoon_loading.insert((*p).clone())
        })
        .cloned()
        .collect();
    if pending.is_empty() {
        return Task::none();
    }
    let archive = state
        .snapshot
        .meta
        .viewer_archive
        .as_ref()
        .map(|_| state.core.archive_session());
    let tasks = pending
        .chunks(4)
        .map(|chunk| {
            let paths = chunk.to_vec();
            let archive = archive.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let mut out = Vec::new();
                        for path in paths {
                            let resolved = if let Some(ref session) = archive {
                                photon_core::Core::ensure_archive_page_for_path(
                                    session,
                                    Path::new(&path),
                                )
                                .ok()
                                .flatten()
                                .unwrap_or_else(|| PathBuf::from(&path))
                            } else {
                                PathBuf::from(&path)
                            };
                            if let Ok(rgba) = photon_image::decode_rgba(&resolved) {
                                let (w, h) = rgba.dimensions();
                                if w > 2200 || h > 4096 {
                                    let thumb =
                                        image::DynamicImage::ImageRgba8(rgba).thumbnail(2200, 4096);
                                    let r = thumb.to_rgba8();
                                    let (nw, nh) = r.dimensions();
                                    out.push((path, nw, nh, w, h, r.into_raw()));
                                } else {
                                    out.push((path, w, h, w, h, rgba.into_raw()));
                                }
                            }
                        }
                        out
                    })
                    .await
                    .unwrap_or_default()
                },
                Message::WebtoonPagesLoaded,
            )
        })
        .collect::<Vec<_>>();
    Task::batch(tasks)
}

pub(super) fn snap_viewer_mode_needs_thumbs(
    snap: &UiStateSnapshot,
    viewer_mode: ViewerViewMode,
    filmstrip_open: bool,
) -> bool {
    viewer_mode == ViewerViewMode::Webtoon || (snap.viewer_images.len() > 1 && filmstrip_open)
}

pub(super) fn reload_dual_frames_task(state: &PhotonGui) -> Task<Message> {
    let (left_slot, right_slot) = dual_spread_slots_for(
        &state.snapshot.viewer_images,
        &state.image_sizes,
        state.snapshot.meta.viewer_index,
        state.viewer_manga_cover_separate,
        state.viewer_manga_rtl,
    );
    let mut tasks = Vec::new();
    match left_slot {
        Some((_, lp)) => {
            if state
                .viewer_frame
                .as_ref()
                .is_none_or(|f| f.key != format!("viewer:{lp}"))
            {
                tasks.push(load_viewer_frame_task(&state.core, lp.to_owned()));
            }
        }
        None => tasks.push(Task::done(Message::ViewerFrameLoaded(None))),
    }
    match right_slot {
        Some((_, rp)) => {
            if state
                .viewer_pair_frame
                .as_ref()
                .is_none_or(|f| f.key != format!("viewer:{rp}"))
            {
                tasks.push(load_viewer_pair_frame_task(
                    &state.core,
                    Some(rp.to_owned()),
                ));
            }
        }
        None => tasks.push(load_viewer_pair_frame_task(&state.core, None)),
    }
    Task::batch(tasks)
}

pub(super) fn reload_active_frames(
    core: &CoreHandle,
    snap: &UiStateSnapshot,
    image_sizes: &HashMap<String, (u32, u32)>,
    viewer_ui: &ViewerUiPresentation,
) -> Task<Message> {
    let mut tasks = Vec::new();
    if viewer_ui.view_mode == ViewerViewMode::Double {
        let (left_slot, right_slot) = dual_spread_slots_for(
            &snap.viewer_images,
            image_sizes,
            snap.meta.viewer_index,
            snap.meta.layout.manga_cover_separate,
            snap.meta.layout.manga_rtl,
        );
        if let Some((_, lp)) = left_slot {
            tasks.push(load_viewer_frame_task(core, lp.to_owned()));
        }
        tasks.push(load_viewer_pair_frame_task(
            core,
            right_slot.map(|(_, rp)| rp.to_owned()),
        ));
    } else if let Some(path) = &snap.meta.viewer_path {
        tasks.push(load_viewer_frame_task(core, path.clone()));
    }
    if snap.meta.editor.is_some() {
        let tab_id = snap.meta.active_editor_tab_id.unwrap_or(0);
        tasks.push(load_editor_frame_task(core.clone(), tab_id));
        tasks.push(load_editor_original_frame_task(core.clone(), tab_id));
        tasks.push(load_editor_thumbnails_task(core.clone()));
    }
    Task::batch(tasks)
}

pub(super) fn decode_viewer_frame_sync(
    path: &Path,
    archive: Option<&Arc<Mutex<Option<photon_image::archive::ArchiveSession>>>>,
) -> Option<CpuImageFrame> {
    let resolved = if let Some(session) = archive {
        photon_core::Core::ensure_archive_page_for_path(session, path)
            .ok()
            .flatten()
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    };
    let rgba = photon_image::decode_rgba(&resolved).ok()?;
    let (w, h) = rgba.dimensions();
    static VIEWER_FRAME_REV: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let rev = VIEWER_FRAME_REV.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(CpuImageFrame {
        key: format!("viewer:{}", path.display()),
        revision: rev,
        width: w,
        height: h,
        rgba: Arc::new(rgba.into_raw()),
    })
}

pub(super) fn load_viewer_frame_task(core: &CoreHandle, path: String) -> Task<Message> {
    let archive = core.archive_session();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                decode_viewer_frame_sync(Path::new(&path), Some(&archive))
            })
            .await
            .ok()
            .flatten()
        },
        Message::ViewerFrameLoaded,
    )
}

pub(super) fn load_viewer_pair_frame_task(
    core: &CoreHandle,
    path: Option<String>,
) -> Task<Message> {
    let Some(path) = path else {
        return Task::done(Message::ViewerPairFrameLoaded(None));
    };
    let archive = core.archive_session();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                decode_viewer_frame_sync(Path::new(&path), Some(&archive))
            })
            .await
            .ok()
            .flatten()
        },
        Message::ViewerPairFrameLoaded,
    )
}

pub(super) fn load_editor_selection_mask_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, alpha) = core.read_editor_selection_mask().await?;
            Some(CpuMaskFrame {
                key: format!("editor:sel:{rev}"),
                revision: rev,
                width: w,
                height: h,
                alpha: Arc::new(alpha),
            })
        },
        Message::EditorSelectionMaskLoaded,
    )
}

pub(super) fn load_editor_frame_task(core: CoreHandle, tab_id: u64) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, bytes) = core.read_editor_pixels().await?;
            Some(CpuImageFrame {
                key: format!("editor:active:{tab_id}"),
                revision: rev,
                width: w,
                height: h,
                rgba: Arc::new(bytes),
            })
        },
        Message::EditorFrameLoaded,
    )
}

pub(super) fn load_editor_original_frame_task(core: CoreHandle, tab_id: u64) -> Task<Message> {
    Task::perform(
        async move {
            let (rev, w, h, bytes) = core.read_editor_original_pixels().await?;
            Some(CpuImageFrame {
                key: format!("editor:original:{tab_id}"),
                revision: rev,
                width: w,
                height: h,
                rgba: Arc::new(bytes),
            })
        },
        Message::EditorOriginalFrameLoaded,
    )
}

pub(super) fn load_editor_thumbnails_task(core: CoreHandle) -> Task<Message> {
    Task::perform(
        async move { core.read_editor_thumbnails().await.unwrap_or_default() },
        |(rev, thumbs)| Message::EditorThumbnailsLoaded(rev, thumbs),
    )
}

pub(super) fn load_editor_transform_frames_task(
    core: CoreHandle,
    layer_id: Option<u64>,
) -> Task<Message> {
    Task::perform(
        async move { core.read_editor_transform_frames_for(layer_id).await },
        Message::EditorTransformFramesLoaded,
    )
}
