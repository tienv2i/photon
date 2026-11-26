//! Unified theme-consistent tooltip helpers built on `iced::widget::tooltip`.

use std::time::Duration;

use crate::typography::text;
use iced::Element;
use iced::widget::{container, tooltip};

use crate::theme::tooltip_style;

pub use iced::widget::tooltip::Position as TooltipPosition;

const DEFAULT_TOOLTIP_DELAY_MS: u64 = 350;
const DEFAULT_TOOLTIP_GAP: f32 = 6.0;

/// Wrap `content` with a styled tooltip placed below the element (`TooltipPosition::Bottom`).
///
/// Empty or whitespace-only tooltip strings are ignored and return `content.into()` unwrapped.
pub fn with_tooltip<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: impl Into<String>,
) -> Element<'a, M> {
    with_tooltip_pos(content, tip, TooltipPosition::Bottom)
}

/// Wrap `content` with a styled tooltip at an explicit [`TooltipPosition`].
///
/// Uses crisp 11px typography, rounded corners, 1px strong border, theme-aware background,
/// and viewport snapping so tooltips never clip off-screen.
pub fn with_tooltip_pos<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: impl Into<String>,
    position: TooltipPosition,
) -> Element<'a, M> {
    let tip_str = tip.into();
    if tip_str.trim().is_empty() {
        return content.into();
    }

    tooltip(
        content,
        container(text(tip_str).size(11)).padding([5, 8]),
        position,
    )
    .gap(DEFAULT_TOOLTIP_GAP)
    .padding(0.0)
    .delay(Duration::from_millis(DEFAULT_TOOLTIP_DELAY_MS))
    .snap_within_viewport(true)
    .style(tooltip_style)
    .into()
}

/// Conditionally wrap `content` with a tooltip when `tip` is `Some` and non-empty.
pub fn with_optional_tooltip<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    tip: Option<String>,
    position: TooltipPosition,
) -> Element<'a, M> {
    match tip {
        Some(t) if !t.trim().is_empty() => with_tooltip_pos(content, t, position),
        _ => content.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::text;

    #[test]
    fn empty_tooltip_is_ignored_without_panicking() {
        let _el: Element<'_, ()> = with_tooltip(text("x"), "   ");
        let _none: Element<'_, ()> = with_optional_tooltip(text("x"), None, TooltipPosition::Top);
        let _some: Element<'_, ()> = with_optional_tooltip(
            text("x"),
            Some("Zoom in (+)".to_owned()),
            TooltipPosition::Right,
        );
    }
}
