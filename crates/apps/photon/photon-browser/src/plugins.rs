use std::collections::HashMap;

use fw_ui::icons::Icon;
use fw_ui::menu::{MenuEntry, MenuItem, menu_panel_entries};
use iced::Element;
use photon_protocol::Command;

use crate::menus::browser_context_image_paths;
use crate::state::{BrowserMessage, BrowserViewState};

pub(crate) fn order_rank_in(plugins: &[photon_protocol::PluginInfo], plugin_id: &str) -> u16 {
    plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map_or(100, |p| p.order)
}

pub(crate) fn plugin_order_rank(meta: &photon_protocol::UiStateMeta, plugin_id: &str) -> u16 {
    order_rank_in(&meta.plugins, plugin_id)
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

pub(crate) fn plugin_display_name(meta: &photon_protocol::UiStateMeta, plugin_id: &str) -> String {
    let raw = meta
        .plugins
        .iter()
        .find(|p| p.id == plugin_id)
        .map(|p| p.display_name(meta.language))
        .unwrap_or(plugin_id);
    normalize_plugin_display_name(raw)
}

/// Secondary submenu listing installed Plugins grouped by plugin in deterministic logical order
/// for the context-clicked entry.
pub fn build_browser_plugins_submenu<'a>(
    state: BrowserViewState<'a>,
    entry_path: &str,
) -> Option<(f32, Element<'a, BrowserMessage>)> {
    let (est_h, el, _) = build_browser_plugins_submenu_with_offset(state, entry_path, None)?;
    Some((est_h, el))
}

pub fn build_browser_plugins_submenu_with_offset<'a>(
    state: BrowserViewState<'a>,
    entry_path: &str,
    active_plugin_id: Option<&str>,
) -> Option<(f32, Element<'a, BrowserMessage>, Option<f32>)> {
    if browser_context_image_paths(state, entry_path).is_empty() {
        return None;
    }
    let qas = workspace_quick_actions(&state.snapshot.meta, "browser");
    if qas.is_empty() {
        return None;
    }
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
    let active_idx = active_plugin_id.and_then(|id| {
        plugin_groups
            .iter()
            .position(|(plugin_id, _)| plugin_id == id)
    });
    let entries: Vec<MenuEntry<'a, BrowserMessage>> = plugin_groups
        .into_iter()
        .map(|(plugin_id, label)| {
            MenuEntry::Item(
                MenuItem::new(
                    label,
                    BrowserMessage::ToggleContextPluginGroup(plugin_id.clone()),
                )
                .icon(Icon::Plug)
                .submenu(true)
                .on_hover(BrowserMessage::OpenContextPluginGroup(plugin_id)),
            )
        })
        .collect();
    let group_offset_y = active_idx.map(|idx| fw_ui::menu::estimate_entry_offset_y(&entries, idx));
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0), group_offset_y))
}

/// Tertiary submenu listing concise Quick Actions for a single plugin in the Browser context menu.
pub fn build_browser_plugin_group_submenu<'a>(
    state: BrowserViewState<'a>,
    entry_path: &str,
    plugin_id: &str,
) -> Option<(f32, Element<'a, BrowserMessage>)> {
    let image_paths = browser_context_image_paths(state, entry_path);
    if image_paths.is_empty() {
        return None;
    }
    let actions: Vec<_> = workspace_quick_actions(&state.snapshot.meta, "browser")
        .into_iter()
        .filter(|qa| qa.plugin_id == plugin_id)
        .collect();
    if actions.is_empty() {
        return None;
    }
    let plugin_name = plugin_display_name(&state.snapshot.meta, plugin_id);
    let raw_labels: Vec<&str> = actions.iter().map(|qa| qa.label.as_str()).collect();
    let labels = normalize_plugin_action_labels(&plugin_name, &raw_labels);
    let mut entries: Vec<MenuEntry<'a, BrowserMessage>> = actions
        .into_iter()
        .zip(labels)
        .map(|(qa, label)| {
            MenuEntry::Item(
                MenuItem::new(
                    label,
                    BrowserMessage::ContextMenuDispatch(Command::QuickActionRun {
                        plugin_id: qa.plugin_id.clone(),
                        action_id: qa.id.clone(),
                        paths: image_paths.clone(),
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
            MenuItem::new("Options…", BrowserMessage::RunPlugin(plugin_id.to_owned()))
                .icon(Icon::Settings),
        ));
    }
    let est_h = fw_ui::menu::estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 220.0)))
}

pub fn workspace_quick_actions<'a>(
    meta: &'a photon_protocol::UiStateMeta,
    workspace: &str,
) -> Vec<&'a photon_protocol::AiQuickAction> {
    meta.quick_actions
        .iter()
        .filter(|qa| {
            qa.available
                && (qa.workspaces.is_empty() || qa.workspaces.iter().any(|w| w == workspace))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_plugin_normalization_and_ordering_are_concise_and_deterministic() {
        assert_eq!(
            normalize_plugin_display_name("AI Watermark Remover (ONNX)"),
            "AI Watermark Remover"
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
                "Upscale",
                &[
                    "Upscale 2× (Fast)",
                    "Upscale 4× (General)",
                    "Upscale 4× (Anime)"
                ]
            ),
            vec!["2×", "4× General", "4× Anime"]
        );
        assert_eq!(order_rank_in(&[], "any.plugin"), 100);
    }
}
