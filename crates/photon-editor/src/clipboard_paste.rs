//! Smart Paste tool options (`Keep Original Size`, `Fit to Canvas`,
//! `Stretch / Fill Canvas`, plus quick width/height/position controls).
//!
//! Activated immediately after pasting an image into an Editor document so the
//! user can choose how the pasted layer fits the canvas and fine-tune its
//! size/position using the two-line slider+stepper template.

use std::collections::BTreeMap;

use iced::widget::{column, row};
use iced::{Alignment, Element, Fill};
use photon_i18n::{tr, tr_args};
use photon_panels::section_card;
use photon_protocol::{Language, LayerTransformSpec};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::icons::Icon;
use photon_ui::theme::PaletteTokens;
use photon_ui::typography::text;

use crate::{EditorMessage, slider_stepper_control};

/// Quick placement mode for a newly pasted image layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartPasteFitMode {
    /// Keep original 1:1 pixel dimensions (centered on canvas).
    KeepOriginal,
    /// Scale proportionally so the entire pasted image fits inside the canvas.
    FitToCanvas,
    /// Scale independently on X and Y so the pasted image fills the exact canvas bounds.
    StretchFill,
}

/// Transient Tool Options state shown immediately after pasting an image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmartPasteState {
    pub layer_id: u64,
    pub source_width: u32,
    pub source_height: u32,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub initial_offset_x: i32,
    pub initial_offset_y: i32,
    pub mode: SmartPasteFitMode,
    pub target_width: f32,
    pub target_height: f32,
    pub translate_x: f32,
    pub translate_y: f32,
}

impl SmartPasteState {
    /// Create a new Smart Paste session initialized to `KeepOriginal` (1:1 pixels).
    #[must_use]
    pub fn new(
        layer_id: u64,
        source_width: u32,
        source_height: u32,
        canvas_width: u32,
        canvas_height: u32,
        initial_offset_x: i32,
        initial_offset_y: i32,
    ) -> Self {
        let sw = source_width.max(1);
        let sh = source_height.max(1);
        Self {
            layer_id,
            source_width: sw,
            source_height: sh,
            canvas_width: canvas_width.max(1),
            canvas_height: canvas_height.max(1),
            initial_offset_x,
            initial_offset_y,
            mode: SmartPasteFitMode::KeepOriginal,
            target_width: sw as f32,
            target_height: sh as f32,
            translate_x: 0.0,
            translate_y: 0.0,
        }
    }

    /// Apply a preset fit mode and update target width/height/translation.
    pub fn apply_mode(&mut self, mode: SmartPasteFitMode) {
        self.mode = mode;
        let spec = compute_paste_transform(
            self.source_width,
            self.source_height,
            self.canvas_width,
            self.canvas_height,
            mode,
        );
        self.target_width = (self.source_width as f32 * spec.scale_x).max(1.0);
        self.target_height = (self.source_height as f32 * spec.scale_y).max(1.0);
        match mode {
            SmartPasteFitMode::KeepOriginal => {
                self.translate_x = 0.0;
                self.translate_y = 0.0;
            }
            SmartPasteFitMode::FitToCanvas | SmartPasteFitMode::StretchFill => {
                // Center the scaled layer on the canvas regardless of its initial paste offset.
                let orig_cx = self.initial_offset_x as f32 + self.source_width as f32 * 0.5;
                let orig_cy = self.initial_offset_y as f32 + self.source_height as f32 * 0.5;
                let canvas_cx = self.canvas_width as f32 * 0.5;
                let canvas_cy = self.canvas_height as f32 * 0.5;
                self.translate_x = canvas_cx - orig_cx;
                self.translate_y = canvas_cy - orig_cy;
            }
        }
    }

    /// Convert current Smart Paste width/height/offset controls into a [`LayerTransformSpec`].
    #[must_use]
    pub fn to_transform_spec(&self) -> LayerTransformSpec {
        let sw = self.source_width.max(1) as f32;
        let sh = self.source_height.max(1) as f32;
        LayerTransformSpec {
            translate_x: self.translate_x,
            translate_y: self.translate_y,
            scale_x: (self.target_width / sw).clamp(0.05, 16.0),
            scale_y: (self.target_height / sh).clamp(0.05, 16.0),
            ..LayerTransformSpec::default()
        }
    }
}

/// Compute the [`LayerTransformSpec`] for a [`SmartPasteFitMode`].
#[must_use]
pub fn compute_paste_transform(
    source_width: u32,
    source_height: u32,
    canvas_width: u32,
    canvas_height: u32,
    mode: SmartPasteFitMode,
) -> LayerTransformSpec {
    let sw = source_width.max(1) as f32;
    let sh = source_height.max(1) as f32;
    let cw = canvas_width.max(1) as f32;
    let ch = canvas_height.max(1) as f32;

    let (scale_x, scale_y) = match mode {
        SmartPasteFitMode::KeepOriginal => (1.0, 1.0),
        SmartPasteFitMode::FitToCanvas => {
            let s = (cw / sw).min(ch / sh).clamp(0.05, 16.0);
            (s, s)
        }
        SmartPasteFitMode::StretchFill => {
            ((cw / sw).clamp(0.05, 16.0), (ch / sh).clamp(0.05, 16.0))
        }
    };

    LayerTransformSpec {
        scale_x,
        scale_y,
        ..LayerTransformSpec::default()
    }
}

/// Build the Smart Paste Tool Options section card shown after pasting pixels.
pub fn build_smart_paste_card<'a>(
    paste: &SmartPasteState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let max_w = (paste.canvas_width.max(paste.source_width) as f32 * 2.0).max(256.0);
    let max_h = (paste.canvas_height.max(paste.source_height) as f32 * 2.0).max(256.0);
    let max_x = (paste.canvas_width as f32).max(200.0);
    let max_y = (paste.canvas_height as f32).max(200.0);

    let source_args = BTreeMap::from([
        ("width".to_owned(), paste.source_width.to_string()),
        ("height".to_owned(), paste.source_height.to_string()),
    ]);
    let source_info = text(tr_args(lang, "smart-paste-source", &source_args))
        .size(11)
        .style(|theme| text::Style {
            color: Some(PaletteTokens::for_theme(theme).text_muted),
        });

    let mode_buttons = column![
        button::labeled(
            tr(lang, "smart-paste-keep-original"),
            Some(Icon::Transform),
            ButtonVariant::Tab {
                active: paste.mode == SmartPasteFitMode::KeepOriginal,
            },
            Some(EditorMessage::SmartPasteSetMode(
                SmartPasteFitMode::KeepOriginal
            )),
        )
        .width(Fill),
        button::labeled(
            tr(lang, "smart-paste-fit-canvas"),
            Some(Icon::Fit),
            ButtonVariant::Tab {
                active: paste.mode == SmartPasteFitMode::FitToCanvas,
            },
            Some(EditorMessage::SmartPasteSetMode(
                SmartPasteFitMode::FitToCanvas
            )),
        )
        .width(Fill),
        button::labeled(
            tr(lang, "smart-paste-stretch-fill"),
            Some(Icon::Resize),
            ButtonVariant::Tab {
                active: paste.mode == SmartPasteFitMode::StretchFill,
            },
            Some(EditorMessage::SmartPasteSetMode(
                SmartPasteFitMode::StretchFill
            )),
        )
        .width(Fill),
    ]
    .spacing(4);

    let width_slider = slider_stepper_control(
        tr(lang, "smart-paste-width"),
        paste.target_width.clamp(1.0, max_w),
        1.0..=max_w,
        1.0_f32,
        format!("{:.0} px", paste.target_width.round()),
        EditorMessage::SmartPasteSetWidth,
    );

    let height_slider = slider_stepper_control(
        tr(lang, "smart-paste-height"),
        paste.target_height.clamp(1.0, max_h),
        1.0..=max_h,
        1.0_f32,
        format!("{:.0} px", paste.target_height.round()),
        EditorMessage::SmartPasteSetHeight,
    );

    let x_slider = slider_stepper_control(
        tr(lang, "smart-paste-x"),
        paste.translate_x.clamp(-max_x, max_x),
        -max_x..=max_x,
        1.0_f32,
        format!("{:.0} px", paste.translate_x.round()),
        EditorMessage::SmartPasteSetOffsetX,
    );

    let y_slider = slider_stepper_control(
        tr(lang, "smart-paste-y"),
        paste.translate_y.clamp(-max_y, max_y),
        -max_y..=max_y,
        1.0_f32,
        format!("{:.0} px", paste.translate_y.round()),
        EditorMessage::SmartPasteSetOffsetY,
    );

    let actions = row![
        button::labeled(
            tr(lang, "smart-paste-dismiss"),
            Some(Icon::Close),
            ButtonVariant::Ghost,
            Some(EditorMessage::SmartPasteDismiss),
        )
        .width(Fill),
        button::labeled(
            tr(lang, "action-apply"),
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(EditorMessage::SmartPasteApply),
        )
        .width(Fill),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    section_card(
        tr(lang, "smart-paste-title"),
        column![
            source_info,
            mode_buttons,
            width_slider,
            height_slider,
            x_slider,
            y_slider,
            actions,
        ]
        .spacing(8),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_modes_compute_exact_scales() {
        let orig = compute_paste_transform(200, 100, 800, 600, SmartPasteFitMode::KeepOriginal);
        assert!(orig.is_identity());

        // 200x100 into 800x600 -> width scale = 4.0, height scale = 6.0 -> min = 4.0
        let fit = compute_paste_transform(200, 100, 800, 600, SmartPasteFitMode::FitToCanvas);
        assert!((fit.scale_x - 4.0).abs() < 1e-4);
        assert!((fit.scale_y - 4.0).abs() < 1e-4);

        // Stretch/Fill -> 4.0 x 6.0
        let stretch = compute_paste_transform(200, 100, 800, 600, SmartPasteFitMode::StretchFill);
        assert!((stretch.scale_x - 4.0).abs() < 1e-4);
        assert!((stretch.scale_y - 6.0).abs() < 1e-4);
    }

    #[test]
    fn smart_paste_state_updates_and_builds_card() {
        let mut st = SmartPasteState::new(42, 400, 200, 800, 800, 200, 300);
        assert_eq!(st.mode, SmartPasteFitMode::KeepOriginal);
        assert!(st.to_transform_spec().is_identity());

        st.apply_mode(SmartPasteFitMode::FitToCanvas);
        assert_eq!(st.mode, SmartPasteFitMode::FitToCanvas);
        assert!((st.target_width - 800.0).abs() < 1e-3);
        assert!((st.target_height - 400.0).abs() < 1e-3);

        st.apply_mode(SmartPasteFitMode::StretchFill);
        assert!((st.target_width - 800.0).abs() < 1e-3);
        assert!((st.target_height - 800.0).abs() < 1e-3);

        let _card = build_smart_paste_card(&st, Language::English);
    }
}
