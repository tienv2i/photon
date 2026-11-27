use std::path::Path;

use photon_canvas::CpuImageFrame;
use photon_i18n::tr;
use photon_protocol::{Language, ViewerViewMode};

use crate::spread::dual_spread_slots;
use crate::types::ViewerViewState;

pub(crate) fn format_viewer_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Image position counter shown in the Viewer status bar (`1 / 5` or `2·3 / 5`).
pub fn viewer_counter_summary(state: ViewerViewState<'_>) -> Option<String> {
    let meta = &state.snapshot.meta;
    let total = meta.viewer_count;
    if total == 0 {
        return None;
    }
    match state.viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total - 1);
            Some(format!("{} / {total}", active_idx + 1))
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            match (left_slot, right_slot) {
                (Some((li, _)), Some((ri, _))) => Some(format!("{}·{} / {total}", li + 1, ri + 1)),
                (Some((li, _)), None) => Some(format!("{} / {total}", li + 1)),
                _ => meta.viewer_index.map(|i| format!("{} / {total}", i + 1)),
            }
        }
        ViewerViewMode::Single => {
            let idx = meta.viewer_index.unwrap_or(0).min(total - 1);
            Some(format!("{} / {total}", idx + 1))
        }
    }
}

/// Summary text shown on the left side of the bottom status bar while in Viewer workspace.
pub fn viewer_status_summary(state: ViewerViewState<'_>, lang: Language) -> String {
    let meta = &state.snapshot.meta;
    let viewer_mode = state.viewer_mode;
    let idx = meta.viewer_index.unwrap_or(0);
    let total = meta.viewer_count;
    let name_at = |i: usize| -> Option<String> {
        if let Some(arch) = &meta.viewer_archive
            && let Some(label) = arch.page_labels.get(i)
        {
            return Some(label.clone());
        }
        state
            .snapshot
            .viewer_images
            .get(i)
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(ToOwned::to_owned)
    };
    match viewer_mode {
        ViewerViewMode::Webtoon => {
            let active_idx = state
                .webtoon_visible_index
                .or(meta.viewer_index)
                .unwrap_or(0)
                .min(total.saturating_sub(1));
            let mode_name = tr(lang, "action-viewer-manhwa");
            if let Some(name) = name_at(active_idx) {
                if let Some((w, h)) = state
                    .snapshot
                    .viewer_images
                    .get(active_idx)
                    .and_then(|p| state.image_sizes.get(p))
                {
                    format!("{mode_name} · {name} · {w}×{h}")
                } else {
                    format!("{mode_name} · {name}")
                }
            } else {
                mode_name
            }
        }
        ViewerViewMode::Double => {
            let (left_slot, right_slot) = dual_spread_slots(state);
            let dims_str = |path: &str, frame: Option<&CpuImageFrame>| -> Option<String> {
                if let Some(f) = frame {
                    Some(format!("{}×{}", f.width, f.height))
                } else {
                    state.image_sizes.get(path).map(|(w, h)| format!("{w}×{h}"))
                }
            };
            match (left_slot, right_slot) {
                (Some((_, lp)), Some((_, rp))) => {
                    let lname = left_slot
                        .and_then(|(li, _)| name_at(li))
                        .unwrap_or_default();
                    let rname = right_slot
                        .and_then(|(ri, _)| name_at(ri))
                        .unwrap_or_default();
                    let ldims = dims_str(lp, state.viewer_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    let rdims = dims_str(rp, state.viewer_pair_frame)
                        .map(|d| format!(" ({d})"))
                        .unwrap_or_default();
                    format!("{lname}{ldims}  |  {rname}{rdims}")
                }
                (Some((li, lp)), None) => {
                    let lname = name_at(li).unwrap_or_default();
                    if let Some(d) = dims_str(lp, state.viewer_frame) {
                        format!("{lname} · {d}")
                    } else {
                        lname
                    }
                }
                _ => tr(lang, "ready"),
            }
        }
        ViewerViewMode::Single => match (name_at(idx), &state.viewer_frame) {
            (Some(name), Some(frame)) => format!("{name} · {}×{}", frame.width, frame.height),
            (Some(name), None) => name,
            _ => tr(lang, "ready"),
        },
    }
}
