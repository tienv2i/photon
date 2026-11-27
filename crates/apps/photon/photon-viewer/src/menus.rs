use std::collections::HashMap;

use fw_ui::icons::Icon;
use fw_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use iced::Element;
use photon_canvas::CanvasEvent;
use photon_i18n::tr;
use photon_protocol::{CanvasAntiAliasingMode, Command, ViewerViewMode, ViewportDenoiseMode};

use crate::types::{ViewerMessage, ViewerViewState};

/// Reader/View quality menu entries for Canvas Anti-Aliasing and Viewport Denoise.
pub fn build_viewer_quality_menu_entries<'a>(
    state: ViewerViewState<'a>,
) -> Vec<MenuEntry<'a, ViewerMessage>> {
    let lang = state.snapshot.meta.language;
    vec![
        MenuEntry::Header(tr(lang, "settings-canvas-aa")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-crisp"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::Crisp),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::Crisp),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-smooth"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::Smooth),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::Smooth),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "aa-mode-high-quality"),
                ViewerMessage::SetAntiAliasingMode(CanvasAntiAliasingMode::HighQuality),
            )
            .checked(state.aa_mode == CanvasAntiAliasingMode::HighQuality),
        ),
        MenuEntry::Separator,
        MenuEntry::Header(tr(lang, "settings-viewport-denoise")),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-off"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Off),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Off),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-light"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Light),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Light),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-medium"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Medium),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Medium),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "denoise-mode-strong"),
                ViewerMessage::SetViewportDenoiseMode(ViewportDenoiseMode::Strong),
            )
            .checked(state.denoise_mode == ViewportDenoiseMode::Strong),
        ),
    ]
}

/// Right-click context menu for Viewer canvas & pages.
pub fn build_viewer_context_menu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> (f32, Element<'a, ViewerMessage>) {
    let (est_h, el, _, _) = build_viewer_context_menu_with_offset(state, target_path);
    (est_h, el)
}

/// Right-click context menu for Viewer canvas & pages, returning the Y offsets of the `Zoom`
/// and `Plugins` trigger rows for exact cascading submenu alignment.
pub fn build_viewer_context_menu_with_offset<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> (f32, Element<'a, ViewerMessage>, Option<f32>, Option<f32>) {
    let lang = state.snapshot.meta.language;
    let resolved_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let has_path = resolved_path.is_some();
    let vmode = state.viewer_mode;
    let is_webtoon = vmode == ViewerViewMode::Webtoon;

    let edit_msg = resolved_path
        .clone()
        .map(ViewerMessage::EditPath)
        .unwrap_or(ViewerMessage::Noop);
    let show_browser_msg = resolved_path
        .clone()
        .map(ViewerMessage::ShowInBrowser)
        .unwrap_or(ViewerMessage::Noop);
    let open_with_msg = resolved_path
        .clone()
        .map(ViewerMessage::OpenWith)
        .unwrap_or(ViewerMessage::Noop);
    let copy_img_msg = resolved_path
        .as_ref()
        .map(|p| ViewerMessage::CopyImageToClipboard(Some(p.clone())))
        .unwrap_or(ViewerMessage::Noop);
    let copy_path_msg = resolved_path
        .as_ref()
        .map(|_| ViewerMessage::CopyViewerPath)
        .unwrap_or(ViewerMessage::Noop);

    let trash_msg = resolved_path
        .clone()
        .map(|p| ViewerMessage::ContextMenuDispatch(Command::Trash { paths: vec![p] }))
        .unwrap_or(ViewerMessage::Noop);
    let delete_perm_msg = resolved_path
        .clone()
        .map(|p| ViewerMessage::ConfirmDeletePermanently(vec![p]))
        .unwrap_or(ViewerMessage::Noop);

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-edit"), edit_msg)
                .icon(Icon::Editor)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-show-in-browser"), show_browser_msg)
                .icon(Icon::Browser)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-open-with"), open_with_msg)
                .icon(Icon::Globe)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-image"), copy_img_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Shift+C")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-copy-path"), copy_path_msg)
                .icon(Icon::Copy)
                .shortcut("Ctrl+Alt+C")
                .enabled(has_path),
        ),
    ];

    if let Some(ref p) = resolved_path {
        let quick_zone_actions: Vec<_> = viewer_quick_actions(&state.snapshot.meta)
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
                    .find(|pl| pl.id == qa.plugin_id)
                    .map(|pl| pl.name.as_str());
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
                        ViewerMessage::ContextMenuDispatch(Command::QuickActionRun {
                            plugin_id: qa.plugin_id.clone(),
                            action_id: qa.id.clone(),
                            paths: vec![p.clone()],
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
            MenuItem::new(tr(lang, "action-trash"), trash_msg)
                .icon(Icon::Trash)
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-delete-permanently"), delete_perm_msg)
                .icon(Icon::Trash)
                .enabled(has_path),
        ),
        MenuEntry::Separator,
    ]);
    let zoom_idx = entries.len();
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "tool-zoom"),
            ViewerMessage::ToggleContextZoomSubmenu,
        )
        .icon(Icon::ZoomIn)
        .submenu(true)
        .on_hover(ViewerMessage::OpenContextZoomSubmenu)
        .enabled(has_path),
    ));
    let zoom_offset_y = Some(fw_ui::menu::estimate_entry_offset_y(&entries, zoom_idx));
    entries.extend([
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-left"),
                ViewerMessage::ViewerRotatePreview(-1),
            )
            .icon(Icon::RotateLeft)
            .shortcut("Shift+R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-rotate-right"),
                ViewerMessage::ViewerRotatePreview(1),
            )
            .icon(Icon::RotateRight)
            .shortcut("R")
            .enabled(has_path && !is_webtoon),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-plugins"),
                ViewerMessage::ToggleContextPluginsSubmenu,
            )
            .icon(Icon::Plug)
            .submenu(true)
            .on_hover(ViewerMessage::OpenContextPluginsSubmenu),
        ),
    ]);

    let plugins_offset_y = Some(fw_ui::menu::estimate_entry_offset_y(
        &entries,
        entries.len() - 1,
    ));
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    (
        est_h,
        menu_panel_entries(entries, 225.0),
        zoom_offset_y,
        plugins_offset_y,
    )
}

/// Secondary submenu for Viewer zoom controls (`Fit`, `Actual Size`, `Zoom In`, `Zoom Out`).
pub fn build_viewer_zoom_submenu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> (f32, Element<'a, ViewerMessage>) {
    let lang = state.snapshot.meta.language;
    let has_path = target_path.is_some() || state.snapshot.meta.viewer_path.is_some();
    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-fit"),
                ViewerMessage::Canvas(CanvasEvent::ResetFit),
            )
            .icon(Icon::Fit)
            .shortcut("F")
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-actual-size"),
                ViewerMessage::ViewerZoomActual,
            )
            .icon(Icon::ZoomIn)
            .shortcut("1")
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-in"), ViewerMessage::ZoomStep(1.25))
                .icon(Icon::ZoomIn)
                .shortcut("+")
                .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(tr(lang, "action-zoom-out"), ViewerMessage::ZoomStep(0.8))
                .icon(Icon::ZoomOut)
                .shortcut("-")
                .enabled(has_path),
        ),
    ];
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    (est_h, menu_panel_entries(entries, 210.0))
}

fn plugin_order_rank(meta: &photon_protocol::UiStateMeta, plugin_id: &str) -> u16 {
    meta.plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map_or(100, |p| p.order)
}

/// Remove trailing parenthetical descriptions `(...)` from plugin display names.
pub fn normalize_plugin_display_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((head, tail)) = trimmed.rsplit_once('(')
        && tail.trim_end().ends_with(')')
    {
        let clean = head.trim();
        if !clean.is_empty() {
            return clean.to_owned();
        }
    }
    trimmed.to_owned()
}

fn strip_redundant_plugin_prefix<'a>(plugin_name: &str, raw_action: &'a str) -> &'a str {
    let action = raw_action.trim();
    let plugin_lower = plugin_name.trim().to_ascii_lowercase();
    let action_lower = action.to_ascii_lowercase();
    if !plugin_lower.is_empty()
        && action_lower.starts_with(&plugin_lower)
        && action.len() > plugin_lower.len()
        && action.is_char_boundary(plugin_lower.len())
    {
        let after = &action[plugin_lower.len()..];
        let rest = after.trim();
        if let Some(stripped) = rest
            .strip_prefix('—')
            .or_else(|| rest.strip_prefix('–'))
            .or_else(|| rest.strip_prefix('-'))
            .or_else(|| rest.strip_prefix(':'))
            .or_else(|| rest.strip_prefix('·'))
        {
            let clean = stripped.trim();
            if !clean.is_empty() {
                return clean;
            }
        }
        if rest.starts_with('(') && rest.ends_with(')') {
            let inner = rest[1..rest.len() - 1].trim();
            if !inner.is_empty() {
                return inner;
            }
        }
        if after.starts_with(' ') && !rest.is_empty() {
            return rest;
        }
    }
    action
}

/// Normalize child action labels for a plugin submenu so they stay concise without repeating
/// the parent plugin name or redundant parenthetical descriptions, while guaranteeing sibling
/// labels never collide.
pub fn normalize_plugin_action_labels(plugin_name: &str, raw_labels: &[&str]) -> Vec<String> {
    let stage1: Vec<String> = raw_labels
        .iter()
        .map(|raw| strip_redundant_plugin_prefix(plugin_name, raw).to_owned())
        .collect();

    let stripped: Vec<Option<String>> = stage1
        .iter()
        .map(|s| {
            if let Some((head, tail)) = s.rsplit_once('(')
                && tail.trim_end().ends_with(')')
            {
                let clean = head.trim();
                if !clean.is_empty() {
                    return Some(clean.to_owned());
                }
            }
            None
        })
        .collect();

    // Count how many times each candidate stripped label appears so we never collapse
    // distinct siblings (e.g. `Upscale 4× (General)` vs `Upscale 4× (Anime)`) into duplicates.
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (idx, orig) in stage1.iter().enumerate() {
        let key = stripped[idx]
            .as_deref()
            .unwrap_or(orig.as_str())
            .to_ascii_lowercase();
        *counts.entry(key).or_insert(0) += 1;
    }

    stage1
        .into_iter()
        .enumerate()
        .map(|(idx, orig)| {
            if let Some(clean) = &stripped[idx] {
                if counts
                    .get(&clean.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(0)
                    == 1
                {
                    return clean.clone();
                }
                // Siblings share the same base before `(...)`; extract the concise qualifier.
                if let Some((_, tail)) = orig.rsplit_once('(') {
                    let qual = tail.trim_end().trim_end_matches(')').trim();
                    let qual = qual
                        .strip_prefix(".txt ·")
                        .or_else(|| qual.strip_prefix(".txt·"))
                        .map(str::trim)
                        .unwrap_or(qual);
                    if !qual.is_empty() {
                        if counts.len() == 1 && stage1_shares_base(&stripped) {
                            return qual.to_owned();
                        }
                        return format!("{clean} {qual}");
                    }
                }
            }
            orig
        })
        .collect()
}

fn stage1_shares_base(stripped: &[Option<String>]) -> bool {
    let mut first: Option<&str> = None;
    for item in stripped {
        let Some(base) = item.as_deref() else {
            return false;
        };
        match first {
            None => first = Some(base),
            Some(f) if !f.eq_ignore_ascii_case(base) => return false,
            _ => {}
        }
    }
    first.is_some()
}

fn plugin_display_name(meta: &photon_protocol::UiStateMeta, plugin_id: &str) -> String {
    let raw = meta
        .plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| p.display_name(meta.language))
        .unwrap_or(plugin_id);
    normalize_plugin_display_name(raw)
}

/// Secondary submenu for Viewer context menu containing Read & Translate and installed Plugins
/// grouped by plugin in deterministic logical order (hover/click opens a tertiary submenu with
/// that plugin's concise child actions).
pub fn build_viewer_plugins_submenu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
) -> Option<(f32, Element<'a, ViewerMessage>)> {
    let (est_h, el, _) = build_viewer_plugins_submenu_with_offset(state, target_path, None)?;
    Some((est_h, el))
}

pub fn build_viewer_plugins_submenu_with_offset<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
    active_plugin_id: Option<&str>,
) -> Option<(f32, Element<'a, ViewerMessage>, Option<f32>)> {
    let lang = state.snapshot.meta.language;
    let resolved_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone());
    let has_path = resolved_path.is_some();

    let mut entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-viewer-read-translate"),
                ViewerMessage::ToggleTranslateOverlay,
            )
            .icon(Icon::Translate)
            .shortcut("L")
            .checked(state.translate.active)
            .enabled(has_path),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "menu-viewer-scan-region"),
                ViewerMessage::StartTranslateRegionPick,
            )
            .icon(Icon::Crop)
            .enabled(has_path),
        ),
    ];

    let mut group_offset_y = None;
    if resolved_path.is_some() {
        let qas = viewer_quick_actions(&state.snapshot.meta);
        if !qas.is_empty() {
            entries.push(MenuEntry::Separator);
            let mut plugin_groups: Vec<(String, String)> = Vec::new();
            for qa in qas {
                if !plugin_groups.iter().any(|(id, _)| id == &qa.plugin_id) {
                    let label = plugin_display_name(&state.snapshot.meta, &qa.plugin_id);
                    plugin_groups.push((qa.plugin_id.clone(), label));
                }
            }
            plugin_groups.sort_by(|(id_a, name_a), (id_b, name_b)| {
                plugin_order_rank(&state.snapshot.meta, id_a)
                    .cmp(&plugin_order_rank(&state.snapshot.meta, id_b))
                    .then_with(|| {
                        name_a
                            .to_ascii_lowercase()
                            .cmp(&name_b.to_ascii_lowercase())
                    })
                    .then_with(|| id_a.cmp(id_b))
            });
            for (plugin_id, label) in plugin_groups {
                let row_idx = entries.len();
                if active_plugin_id == Some(plugin_id.as_str()) {
                    group_offset_y = Some(fw_ui::menu::estimate_entry_offset_y(&entries, row_idx));
                }
                entries.push(MenuEntry::Item(
                    MenuItem::new(
                        label,
                        ViewerMessage::ToggleContextPluginGroup(plugin_id.clone()),
                    )
                    .icon(Icon::Plug)
                    .submenu(true)
                    .on_hover(ViewerMessage::OpenContextPluginGroup(plugin_id)),
                ));
            }
        }
    }

    entries.push(MenuEntry::Separator);
    entries.push(MenuEntry::Item(
        MenuItem::new(
            tr(lang, "action-manage-plugins"),
            ViewerMessage::OpenPluginSettings,
        )
        .icon(Icon::Plug),
    ));

    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0), group_offset_y))
}

/// Tertiary submenu listing concise Quick Actions for a single plugin in the Viewer context menu.
pub fn build_viewer_plugin_group_submenu<'a>(
    state: ViewerViewState<'a>,
    target_path: Option<&str>,
    plugin_id: &str,
) -> Option<(f32, Element<'a, ViewerMessage>)> {
    let target_path = target_path
        .map(str::to_owned)
        .or_else(|| state.snapshot.meta.viewer_path.clone())?;
    let actions: Vec<_> = viewer_quick_actions(&state.snapshot.meta)
        .into_iter()
        .filter(|qa| qa.plugin_id == plugin_id)
        .collect();
    if actions.is_empty() {
        return None;
    }
    let plugin_name = plugin_display_name(&state.snapshot.meta, plugin_id);
    let raw_labels: Vec<&str> = actions.iter().map(|qa| qa.label.as_str()).collect();
    let labels = normalize_plugin_action_labels(&plugin_name, &raw_labels);
    let mut entries: Vec<MenuEntry<'a, ViewerMessage>> = actions
        .into_iter()
        .zip(labels)
        .map(|(qa, label)| {
            MenuEntry::Item(
                MenuItem::new(
                    label,
                    ViewerMessage::ContextMenuDispatch(Command::QuickActionRun {
                        plugin_id: qa.plugin_id.clone(),
                        action_id: qa.id.clone(),
                        paths: vec![target_path.clone()],
                    }),
                )
                .icon(Icon::Plug),
            )
        })
        .collect();
    if state
        .snapshot
        .meta
        .plugins
        .iter()
        .any(|p| p.id == plugin_id && !p.params.is_empty())
    {
        entries.push(MenuEntry::Separator);
        entries.push(MenuEntry::Item(
            MenuItem::new("Options…", ViewerMessage::RunPlugin(plugin_id.to_owned()))
                .icon(Icon::Settings),
        ));
    }
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0)))
}

/// Filter `meta.quick_actions` to available Quick Actions targeting `"viewer"`.
pub fn viewer_quick_actions<'a>(
    meta: &'a photon_protocol::UiStateMeta,
) -> Vec<&'a photon_protocol::AiQuickAction> {
    meta.quick_actions
        .iter()
        .filter(|qa| {
            qa.available
                && (qa.workspaces.is_empty() || qa.workspaces.iter().any(|w| w == "viewer"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_menu_normalization_produces_concise_non_colliding_labels() {
        assert_eq!(
            normalize_plugin_display_name("Remove BG (ONNX Runtime)"),
            "Remove BG"
        );
        assert_eq!(normalize_plugin_display_name("Upscale"), "Upscale");
        assert_eq!(
            normalize_plugin_action_labels(
                "Upscale",
                &[
                    "Upscale 2× (Fast)",
                    "Upscale 4× (General)",
                    "Upscale 4× (Anime)"
                ]
            ),
            vec!["2×", "4× General", "4× Anime"]
        );
        assert_eq!(
            normalize_plugin_action_labels(
                "Remove BG",
                &["Remove bg (General)", "Remove bg (Anime)"]
            ),
            vec!["General", "Anime"]
        );
        assert_eq!(
            normalize_plugin_action_labels(
                "AI Caption",
                &["Caption (.txt · WD-ViT)", "Caption (.txt · SwinV2)"]
            ),
            vec!["WD-ViT", "SwinV2"]
        );
        assert_eq!(
            normalize_plugin_action_labels("OCR Copy", &["OCR Copy (EN)", "OCR Copy (VI)"]),
            vec!["EN", "VI"]
        );
        // Already-normalized labels stay untouched.
        assert_eq!(
            normalize_plugin_action_labels("Upscale", &["2×", "4× General", "4× Anime"]),
            vec!["2×", "4× General", "4× Anime"]
        );
    }
}
