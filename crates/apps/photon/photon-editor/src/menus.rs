use crate::*;
use fw_ui::icons::Icon;
use fw_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use iced::Element;
use photon_i18n::tr;
use photon_protocol::{
    ApplyTarget, ColorAdjustments, ColorBalanceAdjustment, Command, CurvesAdjustment, FlipAxis,
    HsvAdjustment, LayerEffect, PluginCapability,
};

pub fn build_editor_canvas_context_menu<'a>(
    state: EditorViewState<'a>,
) -> Option<(f32, Element<'a, EditorMessage>)> {
    let ed = state.snapshot.meta.editor.as_ref()?;
    let lang = state.snapshot.meta.language;
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-undo"),
                EditorMessage::ContextMenuDispatch(Command::EditorUndo),
            )
            .icon(Icon::Undo)
            .shortcut("Ctrl+Z")
            .enabled(ed.can_undo),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-redo"),
                EditorMessage::ContextMenuDispatch(Command::EditorRedo),
            )
            .icon(Icon::Redo)
            .shortcut("Ctrl+Y")
            .enabled(ed.can_redo),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-cut"), EditorMessage::EditorCutPixels)
                .icon(Icon::Cut)
                .shortcut("Ctrl+X")
                .enabled(state.editor_frame.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy"),
                EditorMessage::EditorCopyPixels { merged: false },
            )
            .icon(Icon::Copy)
            .shortcut("Ctrl+C")
            .enabled(state.editor_frame.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-copy-merged"),
                EditorMessage::EditorCopyPixels { merged: true },
            )
            .icon(Icon::Copy)
            .shortcut("Ctrl+Shift+C")
            .enabled(state.editor_frame.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-paste"),
                EditorMessage::EditorPastePixels {
                    into_selection: false,
                },
            )
            .icon(Icon::Paste)
            .shortcut("Ctrl+V"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-paste-into-selection"),
                EditorMessage::EditorPastePixels {
                    into_selection: true,
                },
            )
            .icon(Icon::Paste)
            .enabled(ed.selection.has_selection),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-transform"),
                EditorMessage::EditorSelectTool(EditorTool::Transform),
            )
            .icon(Icon::Transform)
            .shortcut("Ctrl+T")
            .enabled(!ed.layers.is_empty()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-duplicate"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerDuplicate {
                    id: ed.active_layer_id,
                }),
            )
            .icon(Icon::Duplicate)
            .enabled(!ed.layers.is_empty()),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-adjustments"),
                EditorMessage::EditorOpenAdjustPopup(AdjustmentPopupTab::ColorAndLight),
            )
            .icon(Icon::Sliders),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-left"),
                EditorMessage::ContextMenuDispatch(Command::EditorRotate {
                    degrees: -90,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::RotateLeft),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-right"),
                EditorMessage::ContextMenuDispatch(Command::EditorRotate {
                    degrees: 90,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::RotateRight),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-flip-h"),
                EditorMessage::ContextMenuDispatch(Command::EditorFlip {
                    axis: FlipAxis::Horizontal,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::FlipH),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-flip-v"),
                EditorMessage::ContextMenuDispatch(Command::EditorFlip {
                    axis: FlipAxis::Vertical,
                    target: ApplyTarget::WholeImage,
                }),
            )
            .icon(Icon::FlipV),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-crop"), EditorMessage::ToggleCropMode)
                .icon(Icon::Crop)
                .checked(state.crop_uv.is_some()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-image-size"),
                EditorMessage::OpenResizeDialog,
            )
            .icon(Icon::Resize)
            .shortcut("Ctrl+R"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-canvas-size-dialog"),
                EditorMessage::OpenCanvasSizeDialog,
            )
            .icon(Icon::CanvasSize)
            .shortcut("Ctrl+Shift+R"),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fit"),
                EditorMessage::Canvas(photon_canvas::CanvasEvent::ResetFit),
            )
            .icon(Icon::Fit)
            .shortcut("F"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-compare-original"),
                EditorMessage::EditorCycleCompareMode,
            )
            .icon(Icon::Compare)
            .checked(state.editor_ui.compare_mode != CompareMode::Off),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "label-compare-side-by-side"),
                EditorMessage::EditorSetCompareMode(CompareMode::SideBySide),
            )
            .icon(Icon::ViewDouble)
            .checked(state.editor_ui.compare_mode == CompareMode::SideBySide),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-reset"),
                EditorMessage::ContextMenuDispatch(Command::EditorReset),
            )
            .icon(Icon::Reset)
            .enabled(ed.modified),
        ),
    ];
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 225.0)))
}

pub fn build_editor_layer_context_menu<'a>(
    state: EditorViewState<'a>,
    layer_id: u64,
) -> Option<(f32, Element<'a, EditorMessage>)> {
    let ed = state.snapshot.meta.editor.as_ref()?;
    let stack_idx = ed.layers.iter().position(|l| l.id == layer_id)?;
    let layer = &ed.layers[stack_idx];
    let lang = state.snapshot.meta.language;

    let can_move_up = stack_idx + 1 < ed.layers.len();
    let can_move_down = stack_idx > 0;
    let can_merge_down = stack_idx > 0 && !layer.locked && !ed.layers[stack_idx - 1].locked;
    let can_flatten = ed.layers.len() > 1;
    let can_delete = ed.layers.len() > 1 && !layer.locked;

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-duplicate"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerDuplicate { id: layer_id }),
            )
            .icon(Icon::Duplicate),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rename"),
                EditorMessage::EditorStartLayerRename(layer_id, layer.name.clone()),
            )
            .icon(Icon::Editor),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-move-up"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerReorder {
                    id: layer_id,
                    new_index: stack_idx + 1,
                }),
            )
            .icon(Icon::ArrowUp)
            .enabled(can_move_up),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-move-down"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerReorder {
                    id: layer_id,
                    new_index: stack_idx.saturating_sub(1),
                }),
            )
            .icon(Icon::ArrowDown)
            .enabled(can_move_down),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-merge-down"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerMergeDown { id: layer_id }),
            )
            .icon(Icon::MergeDown)
            .enabled(can_merge_down),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-merge-selected"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerMergeSelected {
                    ids: state.editor_ui.selected_layer_ids.clone(),
                }),
            )
            .enabled(state.editor_ui.selected_layer_ids.len() >= 2),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-merge-visible"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerMergeVisible),
            )
            .enabled(ed.layers.iter().filter(|l| l.visible).count() >= 2),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-stamp-visible"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerStampVisible),
            )
            .shortcut("Ctrl+Shift+Alt+E")
            .enabled(ed.layers.iter().any(|l| l.visible)),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-flatten"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerFlatten),
            )
            .icon(Icon::Flatten)
            .enabled(can_flatten),
        ),
        MenuEntry::Separator,
    ];

    if layer.has_mask {
        entries.extend([
            MenuEntry::Item(
                MenuItem::new(
                    tr(
                        lang,
                        if layer.editing_mask {
                            "action-mask-edit-pixels"
                        } else {
                            "action-mask-edit"
                        },
                    ),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskSetEditing {
                        id: layer_id,
                        editing: !layer.editing_mask,
                    }),
                )
                .icon(Icon::Mask)
                .checked(layer.editing_mask),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(
                        lang,
                        if layer.mask_enabled {
                            "action-mask-disable"
                        } else {
                            "action-mask-enable"
                        },
                    ),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskSetEnabled {
                        id: layer_id,
                        enabled: !layer.mask_enabled,
                    }),
                )
                .icon(if layer.mask_enabled {
                    Icon::EyeOff
                } else {
                    Icon::Eye
                }),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-invert"),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskInvert {
                        id: layer_id,
                    }),
                )
                .icon(Icon::Swap)
                .enabled(!layer.locked),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-apply"),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskApply {
                        id: layer_id,
                    }),
                )
                .icon(Icon::Check)
                .enabled(!layer.locked),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-delete"),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskDelete {
                        id: layer_id,
                    }),
                )
                .icon(Icon::Trash)
                .enabled(!layer.locked),
            ),
            MenuEntry::Separator,
        ]);
    } else {
        entries.extend([
            MenuEntry::Item(
                MenuItem::new(
                    tr(lang, "action-mask-add"),
                    EditorMessage::ContextMenuDispatch(Command::EditorLayerMaskAdd {
                        id: layer_id,
                        reveal_all: true,
                    }),
                )
                .icon(Icon::Mask)
                .enabled(!layer.locked),
            ),
            MenuEntry::Separator,
        ]);
    }

    entries.extend([
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.visible {
                        "action-layer-hide"
                    } else {
                        "action-layer-show"
                    },
                ),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerSetProps {
                    id: layer_id,
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
            )
            .icon(if layer.visible {
                Icon::EyeOff
            } else {
                Icon::Eye
            }),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.locked {
                        "action-layer-unlock"
                    } else {
                        "action-layer-lock"
                    },
                ),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerSetProps {
                    id: layer_id,
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
            )
            .icon(if layer.locked {
                Icon::Unlock
            } else {
                Icon::Lock
            }),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.lock_alpha {
                        "action-layer-unlock-alpha"
                    } else {
                        "action-layer-lock-alpha"
                    },
                ),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: None,
                    locked: None,
                    clipped_to_below: None,
                    lock_alpha: Some(!layer.lock_alpha),
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                }),
            )
            .icon(if layer.lock_alpha {
                Icon::Unlock
            } else {
                Icon::Lock
            })
            .enabled(layer.kind.is_raster()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(
                    lang,
                    if layer.clipped_to_below {
                        "action-layer-release-clipping-mask"
                    } else {
                        "action-layer-create-clipping-mask"
                    },
                ),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerToggleClippingMask {
                    id: Some(layer_id),
                }),
            )
            .shortcut("Ctrl+Alt+G")
            .icon(Icon::Layers),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-rasterize"),
                EditorMessage::ContextMenuDispatch(Command::EditorRasterizeLayer {
                    id: Some(layer_id),
                }),
            )
            .icon(Icon::Layers)
            .enabled(!layer.locked && !layer.kind.is_raster()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-delete"),
                EditorMessage::ContextMenuDispatch(Command::EditorLayerDelete { id: layer_id }),
            )
            .icon(Icon::Trash)
            .enabled(can_delete),
        ),
    ]);

    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0)))
}

pub fn build_editor_add_layer_menu<'a>(
    state: EditorViewState<'a>,
) -> (f32, Element<'a, EditorMessage>) {
    let lang = state.snapshot.meta.language;
    let next_idx = state
        .snapshot
        .meta
        .editor
        .as_ref()
        .map_or(1, |e| e.layers.len() + 1);
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-add-paint"),
                EditorMessage::Dispatch(Command::EditorLayerAdd {
                    name: format!("Layer {next_idx}"),
                }),
            )
            .icon(Icon::Brush),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-add-vector"),
                EditorMessage::Dispatch(Command::EditorAddVectorLayer {
                    name: format!("Vector {next_idx}"),
                    data: photon_protocol::VectorLayerData::new(),
                }),
            )
            .icon(Icon::BezierPen),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-layer-add-text"),
                EditorMessage::Dispatch(Command::EditorAddTextLayer {
                    name: format!("Text {next_idx}"),
                    data: state.editor_ui.text_draft.clone(),
                }),
            )
            .icon(Icon::Text),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "action-layer-add-filter")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-blur"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-blur"),
                    effect: LayerEffect::Blur { radius: 4.0 },
                }),
            )
            .icon(Icon::Liquify),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-sharpen"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-sharpen"),
                    effect: LayerEffect::Sharpen { amount: 0.8 },
                }),
            )
            .icon(Icon::Compare),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-denoise"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-denoise"),
                    effect: LayerEffect::Denoise {
                        strength: 0.35,
                        detail_preserve: 0.6,
                    },
                }),
            )
            .icon(Icon::Denoise),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-invert"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-invert"),
                    effect: LayerEffect::Invert,
                }),
            )
            .icon(Icon::Swap),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-desaturate"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-desaturate"),
                    effect: LayerEffect::Desaturate,
                }),
            )
            .icon(Icon::Palette),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "filter-sepia"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "filter-sepia"),
                    effect: LayerEffect::Sepia,
                }),
            )
            .icon(Icon::Image),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "action-layer-add-adjustment")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-color"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-color"),
                    effect: LayerEffect::ColorLight(ColorAdjustments::default()),
                }),
            )
            .icon(Icon::Sliders),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-hsv"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-hsv"),
                    effect: LayerEffect::Hsv(HsvAdjustment::default()),
                }),
            )
            .icon(Icon::Palette),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-balance"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-balance"),
                    effect: LayerEffect::ColorBalance(ColorBalanceAdjustment::default()),
                }),
            )
            .icon(Icon::ColorBalance),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-filter-curves"),
                EditorMessage::Dispatch(Command::EditorLayerAddEffect {
                    name: tr(lang, "menu-filter-curves"),
                    effect: LayerEffect::Curves(CurvesAdjustment::default()),
                }),
            )
            .icon(Icon::Curves),
        ),
    ];

    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, menu_panel_entries(entries, 224.0))
}

/// Reusable Editor Plugins menu entries for `photon-desktop`.
pub fn build_editor_plugins_menu_entries<'a>(
    state: EditorViewState<'a>,
) -> Vec<MenuEntry<'a, EditorMessage>> {
    let lang = state.snapshot.meta.language;
    let enabled_plugins: Vec<_> = state
        .snapshot
        .meta
        .plugins
        .iter()
        .filter(|p| p.enabled && p.capabilities.contains(&PluginCapability::EditorFilter))
        .collect();

    let has_doc = state.snapshot.meta.editor.is_some();
    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-plugins-rembg"),
                EditorMessage::EditorSelectTool(EditorTool::RemoveBg),
            )
            .icon(Icon::Mask)
            .enabled(has_doc),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-plugins-watermark"),
                EditorMessage::EditorSelectTool(EditorTool::Watermark),
            )
            .icon(Icon::Denoise)
            .enabled(has_doc),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-plugins-caption"),
                EditorMessage::EditorSelectTool(EditorTool::Caption),
            )
            .icon(Icon::Plug)
            .enabled(has_doc),
        ),
        MenuEntry::Separator,
    ];
    if enabled_plugins.is_empty() {
        entries.push(MenuEntry::Item(
            MenuItem::new(tr(lang, "menu-plugins-no-enabled"), EditorMessage::Noop).enabled(false),
        ));
    } else {
        for plugin in enabled_plugins {
            entries.push(MenuEntry::Item(
                MenuItem::new(
                    plugin.name.clone(),
                    EditorMessage::RunPlugin(plugin.id.clone()),
                )
                .icon(Icon::Plug)
                .enabled(has_doc),
            ));
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-manage-plugins"),
            EditorMessage::OpenManagePlugins,
        )
        .icon(Icon::Plug),
    ));
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-reload-plugins"),
            EditorMessage::ReloadPlugins,
        )
        .icon(Icon::Refresh),
    ));
    entries
}
