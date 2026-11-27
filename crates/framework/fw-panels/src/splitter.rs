//! Panel splitter helpers wrapping [`fw_ui::splitter`] for docked sidebars and stacked splits.

use iced::{Element, Length};

pub use fw_ui::splitter::{PanelSplitterEdge, panel_splitter, vertical_panel_splitter};

/// Convenience namespace for constructing horizontal and vertical panel splitters.
pub struct PanelSplitter;

impl PanelSplitter {
    /// Interactive 6px horizontal resize handle for a docked left/right sidebar.
    pub fn horizontal<'a, M: Clone + 'a>(
        edge: PanelSplitterEdge,
        width_px: f32,
        min_w: f32,
        max_w: f32,
        default_w: f32,
        on_drag: impl Fn(f32) -> M + 'a,
        on_release: impl Fn(f32) -> M + 'a,
    ) -> Element<'a, M> {
        panel_splitter(
            width_px,
            edge.direction(),
            min_w,
            max_w,
            default_w,
            on_drag,
            on_release,
        )
    }

    /// Interactive 6px vertical resize handle between two stacked panels inside a sidebar.
    pub fn vertical<'a, M: Clone + 'a>(
        ratio_pct: f32,
        container_height_hint: f32,
        min_pct: f32,
        max_pct: f32,
        default_pct: f32,
        on_drag: impl Fn(f32) -> M + 'a,
        on_release: impl Fn(f32) -> M + 'a,
    ) -> Element<'a, M> {
        vertical_panel_splitter(
            ratio_pct,
            container_height_hint,
            min_pct,
            max_pct,
            default_pct,
            on_drag,
            on_release,
        )
    }
}

/// Compute clamped `(top_portion, bottom_portion)` integers summing to 100 for `Length::FillPortion`.
pub fn split_portions(split_pct: f32, min_pct: f32, max_pct: f32) -> (u16, u16) {
    let lo = (min_pct.round() as i32).clamp(1, 99) as u16;
    let hi = (max_pct.round() as i32).clamp(i32::from(lo), 99) as u16;
    let top = (split_pct.round() as i32).clamp(i32::from(lo), i32::from(hi)) as u16;
    let bottom = 100u16.saturating_sub(top).max(100u16.saturating_sub(hi));
    (top, bottom)
}

/// Compute `(top_height, bottom_height)` for a two-panel vertical sidebar stack given collapse state
/// and split percentage.
pub fn stacked_panel_heights(
    top_collapsed: bool,
    bottom_collapsed: bool,
    split_pct: f32,
    min_pct: f32,
    max_pct: f32,
) -> (Length, Length) {
    match (top_collapsed, bottom_collapsed) {
        (true, true) => (Length::Shrink, Length::Shrink),
        (true, false) => (Length::Shrink, Length::Fill),
        (false, true) => (Length::Fill, Length::Shrink),
        (false, false) => {
            let (top, bottom) = split_portions(split_pct, min_pct, max_pct);
            (Length::FillPortion(top), Length::FillPortion(bottom))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_portions_clamps_and_sums_to_100() {
        assert_eq!(split_portions(58.0, 20.0, 80.0), (58, 42));
        assert_eq!(split_portions(5.0, 20.0, 80.0), (20, 80));
        assert_eq!(split_portions(95.0, 20.0, 80.0), (80, 20));
    }

    #[test]
    fn stacked_panel_heights_handles_all_collapse_combinations() {
        assert_eq!(
            stacked_panel_heights(true, true, 58.0, 20.0, 80.0),
            (Length::Shrink, Length::Shrink)
        );
        assert_eq!(
            stacked_panel_heights(true, false, 58.0, 20.0, 80.0),
            (Length::Shrink, Length::Fill)
        );
        assert_eq!(
            stacked_panel_heights(false, true, 58.0, 20.0, 80.0),
            (Length::Fill, Length::Shrink)
        );
        assert_eq!(
            stacked_panel_heights(false, false, 58.0, 20.0, 80.0),
            (Length::FillPortion(58), Length::FillPortion(42))
        );
    }

    #[test]
    fn panel_splitter_helpers_construct_without_panicking() {
        let _h: Element<'_, f32> = PanelSplitter::horizontal(
            PanelSplitterEdge::Right,
            300.0,
            220.0,
            560.0,
            300.0,
            |v| v,
            |v| v,
        );
        let _v: Element<'_, f32> =
            PanelSplitter::vertical(58.0, 600.0, 20.0, 80.0, 58.0, |v| v, |v| v);
    }
}
