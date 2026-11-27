//! Runtime live typography context for `Font Scale` and `Display Font`.
//!
//! `iced::Settings { default_font, default_text_size }` only applies at application
//! startup, and explicit `.size(...)` calls override `default_text_size`. By routing
//! UI `text(...)` widgets through this module and updating the thread-local active
//! typography at the top of `app::view()`, both **Font Scale** and **Display Font**
//! take effect live across every workspace, menu, panel, and dialog without restarting.

use std::cell::Cell;

use iced::widget::text as iced_text;
use iced::{Color, Element, Font, Length, Pixels, Theme, alignment};

/// Re-exports of `iced::widget::text` helper types so `use fw_ui::typography::text;`
/// serves as a drop-in replacement for `use iced::widget::text;`.
pub mod text {
    pub use iced::widget::text::{
        Alignment, Catalog, Fragment, Highlighter, IntoFragment, LineHeight, Shaping, Span, Style,
        StyleFn, Wrapping,
    };
}

/// Minimum allowed rendered font size in logical pixels after applying `Font Scale`.
pub const MIN_SCALED_FONT_PX: f32 = 8.0;
/// Maximum allowed rendered font size in logical pixels after applying `Font Scale`.
pub const MAX_SCALED_FONT_PX: f32 = 48.0;
/// Default body text size in logical pixels before scaling.
pub const DEFAULT_BASE_FONT_PX: f32 = 13.0;

thread_local! {
    static ACTIVE_FONT_SCALE: Cell<f32> = const { Cell::new(1.0) };
    static ACTIVE_DISPLAY_FONT: Cell<Font> = const { Cell::new(Font::DEFAULT) };
}

/// Updates the active typography settings for the current GUI render pass.
pub fn set_active_typography(font_scale: f32, display_font: Font) {
    let clamped_scale = if font_scale.is_finite() {
        font_scale.clamp(0.5, 3.0)
    } else {
        1.0
    };
    ACTIVE_FONT_SCALE.with(|s| s.set(clamped_scale));
    ACTIVE_DISPLAY_FONT.with(|f| f.set(display_font));
}

/// Returns the active UI `Font Scale` multiplier (1.0 = 100%).
#[must_use]
pub fn active_font_scale() -> f32 {
    ACTIVE_FONT_SCALE.with(Cell::get)
}

/// Returns the active UI `Display Font`.
#[must_use]
pub fn active_display_font() -> Font {
    ACTIVE_DISPLAY_FONT.with(Cell::get)
}

/// Scales a base pixel font size by the active `Font Scale` multiplier.
#[must_use]
pub fn scaled_size(base_px: f32) -> f32 {
    (base_px * active_font_scale()).clamp(MIN_SCALED_FONT_PX, MAX_SCALED_FONT_PX)
}

/// Wrapper around `iced::widget::Text` that automatically applies the active
/// `Display Font` and scales `.size(...)` calls by `Font Scale`.
pub struct ScaledText<'a> {
    inner: iced::widget::Text<'a>,
}

impl<'a> ScaledText<'a> {
    /// Sets the text size in base logical pixels, automatically scaled by `Font Scale`.
    #[must_use]
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        let px = size.into().0;
        self.inner = self.inner.size(scaled_size(px));
        self
    }

    /// Sets the text size directly without multiplying by `Font Scale` again
    /// (useful for explicit preview samples that already computed a scaled size).
    #[must_use]
    pub fn unscaled_size(mut self, size: impl Into<Pixels>) -> Self {
        self.inner = self.inner.size(size);
        self
    }

    /// Overrides the font for this text widget.
    #[must_use]
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.inner = self.inner.font(font);
        self
    }

    /// Sets the text style closure.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> iced_text::Style + 'a) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    /// Sets an explicit foreground color.
    #[must_use]
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.inner = self.inner.color(color);
        self
    }

    /// Sets an optional foreground color.
    #[must_use]
    pub fn color_maybe(mut self, color: Option<impl Into<Color>>) -> Self {
        self.inner = self.inner.color_maybe(color);
        self
    }

    /// Sets the wrapping behavior.
    #[must_use]
    pub fn wrapping(mut self, wrapping: iced_text::Wrapping) -> Self {
        self.inner = self.inner.wrapping(wrapping);
        self
    }

    /// Sets the width of the text widget.
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.inner = self.inner.width(width);
        self
    }

    /// Sets the height of the text widget.
    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.inner = self.inner.height(height);
        self
    }

    /// Sets horizontal alignment.
    #[must_use]
    pub fn align_x(mut self, alignment: impl Into<iced_text::Alignment>) -> Self {
        self.inner = self.inner.align_x(alignment);
        self
    }

    /// Sets vertical alignment.
    #[must_use]
    pub fn align_y(mut self, alignment: impl Into<alignment::Vertical>) -> Self {
        self.inner = self.inner.align_y(alignment);
        self
    }

    /// Centers the text horizontally and vertically.
    #[must_use]
    pub fn center(mut self) -> Self {
        self.inner = self.inner.center();
        self
    }

    /// Sets the text shaping strategy.
    #[must_use]
    pub fn shaping(mut self, shaping: iced_text::Shaping) -> Self {
        self.inner = self.inner.shaping(shaping);
        self
    }

    /// Sets the line height.
    #[must_use]
    pub fn line_height(mut self, line_height: impl Into<iced_text::LineHeight>) -> Self {
        self.inner = self.inner.line_height(line_height);
        self
    }
}

impl<'a, Message: 'a> From<ScaledText<'a>> for Element<'a, Message> {
    fn from(value: ScaledText<'a>) -> Self {
        value.inner.into()
    }
}

/// Creates a live-scaled text widget using the active `Display Font` and `Font Scale`.
pub fn text<'a>(fragment: impl iced_text::IntoFragment<'a>) -> ScaledText<'a> {
    ScaledText {
        inner: iced::widget::text(fragment)
            .font(active_display_font())
            .size(scaled_size(DEFAULT_BASE_FONT_PX)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typography_scales_and_clamps_sizes_live() {
        set_active_typography(1.0, Font::DEFAULT);
        assert!((active_font_scale() - 1.0).abs() < 1e-6);
        assert!((scaled_size(12.0) - 12.0).abs() < 1e-6);

        set_active_typography(1.5, Font::MONOSPACE);
        assert!((active_font_scale() - 1.5).abs() < 1e-6);
        assert_eq!(active_display_font(), Font::MONOSPACE);
        assert!((scaled_size(12.0) - 18.0).abs() < 1e-6);
        assert!((scaled_size(40.0) - MAX_SCALED_FONT_PX).abs() < 1e-6);

        set_active_typography(0.5, Font::DEFAULT);
        assert!((scaled_size(10.0) - MIN_SCALED_FONT_PX).abs() < 1e-6);

        // Reset thread-local state so other tests see default typography.
        set_active_typography(1.0, Font::DEFAULT);
    }

    #[test]
    fn scaled_text_builder_constructs_element() {
        set_active_typography(1.25, Font::MONOSPACE);
        let _el: Element<'_, ()> = text("Hello Photon")
            .size(12)
            .wrapping(text::Wrapping::None)
            .into();
        set_active_typography(1.0, Font::DEFAULT);
    }
}
