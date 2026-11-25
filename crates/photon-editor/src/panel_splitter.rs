//! Workspace panel splitter state and helpers wrapping `photon_ui::splitter`.
//!
//! Keeps smooth 60fps drag overrides in UI state and only commits
//! `Command::SetLayout` when a drag finishes or on double-click reset.

use iced::Element;
use photon_protocol::LayoutState;
use photon_ui::splitter::{PanelSplitterEdge, panel_splitter, vertical_panel_splitter};

use crate::EditorMessage;

pub(crate) const EDITOR_RIGHT_MIN: f32 = 220.0;
pub(crate) const EDITOR_RIGHT_MAX: f32 = 560.0;
pub(crate) const EDITOR_RIGHT_DEFAULT: f32 = 300.0;

pub(crate) const EDITOR_SPLIT_MIN_PCT: f32 = 20.0;
pub(crate) const EDITOR_SPLIT_MAX_PCT: f32 = 80.0;
pub(crate) const EDITOR_SPLIT_DEFAULT_PCT: f32 = 58.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockSplitterKind {
    EditorRight,
    EditorRightSplit,
}

#[derive(Debug, Clone, Default)]
pub struct DockDragState {
    pub editor_right_width: Option<f32>,
    pub editor_right_split_pct: Option<f32>,
}

impl DockDragState {
    pub fn effective_editor_right(&self, layout: &LayoutState) -> f32 {
        self.editor_right_width
            .unwrap_or(layout.editor_right_width as f32)
            .clamp(EDITOR_RIGHT_MIN, EDITOR_RIGHT_MAX)
    }

    pub fn effective_editor_split_pct(&self, layout: &LayoutState) -> f32 {
        self.editor_right_split_pct
            .unwrap_or(layout.editor_right_split_pct as f32)
            .clamp(EDITOR_SPLIT_MIN_PCT, EDITOR_SPLIT_MAX_PCT)
    }

    pub fn set_drag(&mut self, kind: DockSplitterKind, val: f32) {
        match kind {
            DockSplitterKind::EditorRight => {
                self.editor_right_width = Some(val.clamp(EDITOR_RIGHT_MIN, EDITOR_RIGHT_MAX));
            }
            DockSplitterKind::EditorRightSplit => {
                self.editor_right_split_pct =
                    Some(val.clamp(EDITOR_SPLIT_MIN_PCT, EDITOR_SPLIT_MAX_PCT));
            }
        }
    }

    pub fn commit_release(
        &mut self,
        layout: &LayoutState,
        kind: DockSplitterKind,
        val: f32,
    ) -> Option<LayoutState> {
        let mut next = layout.clone();
        match kind {
            DockSplitterKind::EditorRight => {
                self.editor_right_width = None;
                let w = val.round().clamp(EDITOR_RIGHT_MIN, EDITOR_RIGHT_MAX) as u32;
                if next.editor_right_width == w {
                    return None;
                }
                next.editor_right_width = w;
            }
            DockSplitterKind::EditorRightSplit => {
                self.editor_right_split_pct = None;
                let pct = val
                    .round()
                    .clamp(EDITOR_SPLIT_MIN_PCT, EDITOR_SPLIT_MAX_PCT)
                    as u16;
                if next.editor_right_split_pct == pct {
                    return None;
                }
                next.editor_right_split_pct = pct;
            }
        }
        Some(next)
    }
}

pub fn dock_splitter_handle<'a>(
    kind: DockSplitterKind,
    current_px: f32,
) -> Element<'a, EditorMessage> {
    let _ = kind;
    panel_splitter(
        current_px,
        PanelSplitterEdge::Right.direction(),
        EDITOR_RIGHT_MIN,
        EDITOR_RIGHT_MAX,
        EDITOR_RIGHT_DEFAULT,
        move |v| EditorMessage::DockSplitterDrag(DockSplitterKind::EditorRight, v),
        move |v| EditorMessage::DockSplitterRelease(DockSplitterKind::EditorRight, v),
    )
}

pub fn editor_vertical_splitter<'a>(
    ratio_pct: f32,
    window_height: f32,
) -> Element<'a, EditorMessage> {
    let height_hint = (window_height - 120.0).max(260.0);
    vertical_panel_splitter(
        ratio_pct,
        height_hint,
        EDITOR_SPLIT_MIN_PCT,
        EDITOR_SPLIT_MAX_PCT,
        EDITOR_SPLIT_DEFAULT_PCT,
        move |v| EditorMessage::DockSplitterDrag(DockSplitterKind::EditorRightSplit, v),
        move |v| EditorMessage::DockSplitterRelease(DockSplitterKind::EditorRightSplit, v),
    )
}
