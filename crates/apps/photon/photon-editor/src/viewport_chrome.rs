use crate::widgets::toolbar_divider;
use crate::*;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::icons::Icon;
use fw_ui::theme::{PaletteTokens, TEXT_SIZE_CAPTION};
use fw_ui::typography::text;
use fw_ui::{TooltipPosition, with_tooltip_pos};
use iced::mouse;
use iced::widget::{canvas, container, row};
use iced::window;
use iced::{Alignment, Element, Point, Rectangle, Size};
use photon_i18n::tr;

/// Exportable Editor zoom controls (`-` / `%` / `+` / `Fit (0)` / `1:1 (1)`) for the bottom
/// status bar.
pub fn build_editor_zoom_controls<'a>(state: EditorViewState<'a>) -> Element<'a, EditorMessage> {
    let lang = state.snapshot.meta.language;
    let has_doc = state.snapshot.meta.editor.is_some() || state.editor_frame.is_some();

    let zoom_pct = container(with_tooltip_pos(
        text(format!("{:.0}%", state.zoom * 100.0))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_secondary),
            }),
        tr(lang, "tooltip-editor-zoom-percent"),
        TooltipPosition::Top,
    ))
    .padding([0, 4]);

    let fit_btn = with_tooltip_pos(
        button::labeled(
            tr(lang, "action-fit"),
            Some(Icon::Fit),
            ButtonVariant::Ghost,
            has_doc.then_some(EditorMessage::ZoomFit),
        )
        .padding([2, 6]),
        tr(lang, "tooltip-editor-fit-zero"),
        TooltipPosition::Top,
    );

    let actual_btn = with_tooltip_pos(
        button::labeled(
            tr(lang, "action-actual-ratio"),
            None,
            ButtonVariant::Ghost,
            has_doc.then_some(EditorMessage::ZoomActual),
        )
        .padding([2, 6]),
        tr(lang, "tooltip-editor-actual-size"),
        TooltipPosition::Top,
    );

    row![
        with_tooltip_pos(
            button::compact_icon(
                Icon::ZoomOut,
                ButtonVariant::Ghost,
                has_doc.then_some(EditorMessage::ZoomStep(0.8)),
            ),
            tr(lang, "tooltip-editor-zoom-out"),
            TooltipPosition::Top,
        ),
        zoom_pct,
        with_tooltip_pos(
            button::compact_icon(
                Icon::ZoomIn,
                ButtonVariant::Ghost,
                has_doc.then_some(EditorMessage::ZoomStep(1.25)),
            ),
            tr(lang, "tooltip-editor-zoom-in"),
            TooltipPosition::Top,
        ),
        toolbar_divider(),
        fit_btn,
        actual_btn,
    ]
    .spacing(3)
    .align_y(Alignment::Center)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollbarAxisMetrics {
    thumb_ratio: f32,
    frac: f32,
    max_pan: f32,
}

pub(crate) fn canvas_display_dims(iw: u32, ih: u32, zoom: f32, vp_w: f32, vp_h: f32) -> (f32, f32) {
    let fw = iw.max(1) as f32;
    let fh = ih.max(1) as f32;
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let vp_aspect = vp_w / vp_h;
    let img_aspect = fw / fh;
    let (fit_w, fit_h) = if img_aspect > vp_aspect {
        (vp_w, vp_w / img_aspect)
    } else {
        (vp_h * img_aspect, vp_h)
    };
    let z = zoom.max(0.02);
    (fit_w * z, fit_h * z)
}

pub(crate) fn scrollbar_axis_metrics(disp: f32, vp: f32, pan: f32) -> Option<ScrollbarAxisMetrics> {
    if disp <= vp + 1.0 {
        return None;
    }
    let max_pan = ((disp - vp) * 0.5).max(1.0);
    let thumb_ratio = (vp / disp).clamp(0.06, 1.0);
    let frac = ((max_pan - pan) / (2.0 * max_pan)).clamp(0.0, 1.0);
    Some(ScrollbarAxisMetrics {
        thumb_ratio,
        frac,
        max_pan,
    })
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CanvasScrollbarProgram {
    pub(crate) horizontal: bool,
    pub(crate) metrics: ScrollbarAxisMetrics,
    pub(crate) other_axis_pan: f32,
}

impl CanvasScrollbarProgram {
    fn make_pan_msg(&self, new_pan: f32) -> EditorMessage {
        if self.horizontal {
            EditorMessage::CanvasPanTo {
                pan_x: new_pan,
                pan_y: self.other_axis_pan,
            }
        } else {
            EditorMessage::CanvasPanTo {
                pan_x: self.other_axis_pan,
                pan_y: new_pan,
            }
        }
    }

    fn thumb_geometry(&self, bounds: Rectangle) -> (f32, f32, f32, f32) {
        let track_len = if self.horizontal {
            bounds.width
        } else {
            bounds.height
        }
        .max(1.0);
        let thumb_len = (track_len * self.metrics.thumb_ratio).clamp(20.0, track_len);
        let travel = (track_len - thumb_len).max(0.0);
        let thumb_start = self.metrics.frac.clamp(0.0, 1.0) * travel;
        (track_len, thumb_len, travel, thumb_start)
    }
}

#[derive(Debug, Default)]
pub(crate) struct CanvasScrollbarState {
    dragging_grab_offset: Option<f32>,
    hovered: bool,
}

impl canvas::Program<EditorMessage> for CanvasScrollbarProgram {
    type State = CanvasScrollbarState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<EditorMessage>> {
        let (_track_len, thumb_len, travel, thumb_start) = self.thumb_geometry(bounds);

        match event {
            canvas::Event::Window(window::Event::Unfocused)
            | canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging_grab_offset.take().is_some() {
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let pos_along = if self.horizontal { pos.x } else { pos.y };
                if pos_along >= thumb_start && pos_along <= thumb_start + thumb_len {
                    state.dragging_grab_offset = Some(pos_along - thumb_start);
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                let grab_off = thumb_len * 0.5;
                state.dragging_grab_offset = Some(grab_off);
                let new_start = (pos_along - grab_off).clamp(0.0, travel);
                let frac = if travel > 0.0 {
                    (new_start / travel).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                let new_pan = self.metrics.max_pan * (1.0 - 2.0 * frac);
                Some(canvas::Action::publish(self.make_pan_msg(new_pan)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(grab_off) = state.dragging_grab_offset
                    && let Some(pos) = cursor.position_from(bounds.position())
                {
                    let pos_along = if self.horizontal { pos.x } else { pos.y };
                    let new_start = (pos_along - grab_off).clamp(0.0, travel);
                    let frac = if travel > 0.0 {
                        (new_start / travel).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    let new_pan = self.metrics.max_pan * (1.0 - 2.0 * frac);
                    return Some(canvas::Action::publish(self.make_pan_msg(new_pan)).and_capture());
                }
                let over = cursor.is_over(bounds);
                if over != state.hovered {
                    state.hovered = over;
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_grab_offset.is_some() || cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let t = PaletteTokens::for_theme(theme);
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let size = bounds.size();
        frame.fill_rectangle(Point::ORIGIN, size, t.bg_surface.scale_alpha(0.72));

        let (_track_len, thumb_len, _travel, thumb_start) = self.thumb_geometry(bounds);
        let thumb_color = if state.dragging_grab_offset.is_some() {
            t.accent
        } else if state.hovered {
            t.text_secondary
        } else {
            t.border_strong
        };
        let pad = 2.0;
        let (pt, sz) = if self.horizontal {
            (
                Point::new(thumb_start, pad),
                Size::new(thumb_len, (size.height - pad * 2.0).max(2.0)),
            )
        } else {
            (
                Point::new(pad, thumb_start),
                Size::new((size.width - pad * 2.0).max(2.0), thumb_len),
            )
        };
        let radius = (if self.horizontal { sz.height } else { sz.width }) * 0.5;
        let path = canvas::Path::rounded_rectangle(pt, sz, radius.into());
        frame.fill(&path, thumb_color);
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_metrics_show_only_when_display_exceeds_viewport() {
        let (dw, dh) = canvas_display_dims(4000, 3000, 1.0, 800.0, 600.0);
        assert!(scrollbar_axis_metrics(dw, 800.0, 0.0).is_none());
        assert!(scrollbar_axis_metrics(dh, 600.0, 0.0).is_none());

        let (dw2, dh2) = canvas_display_dims(4000, 3000, 3.0, 800.0, 600.0);
        let mx = scrollbar_axis_metrics(dw2, 800.0, 0.0).expect("horizontal bar expected");
        let my = scrollbar_axis_metrics(dh2, 600.0, 0.0).expect("vertical bar expected");
        assert!((mx.frac - 0.5).abs() < 1e-4);
        assert!((my.frac - 0.5).abs() < 1e-4);
        let left_edge = scrollbar_axis_metrics(dw2, 800.0, mx.max_pan).unwrap();
        assert!((left_edge.frac - 0.0).abs() < 1e-4);
        let right_edge = scrollbar_axis_metrics(dw2, 800.0, -mx.max_pan).unwrap();
        assert!((right_edge.frac - 1.0).abs() < 1e-4);
    }
}
