use iced::mouse;
use iced::widget::canvas::{
    self, Action, Frame, Geometry, LineCap, LineJoin, Path as CanvasPath, Stroke,
};
use iced::{Point, Rectangle, Renderer, Size};
use photon_protocol::ViewerViewMode;
use photon_ui::theme::PaletteTokens;

use crate::geometry::{viewer_double_fit_geometries, viewer_fit_geometry};
use crate::types::ViewerMessage;

pub const VIEWER_EDGE_NAV_ZONE_WIDTH: f32 = 56.0;

/// Compute whether the rendered Viewer image overlaps the left and right edge-navigation
/// button zones (`[0, zone_w]` and `[vp_w - zone_w, vp_w]`).
///
/// - When an edge does **not** overlap (`false`), the Prev/Next button stays always visible.
/// - When an edge **does** overlap (`true`), the Prev/Next button is hidden by default and
///   revealed only when the cursor hovers over that edge zone.
#[allow(clippy::too_many_arguments)]
pub fn viewer_edge_nav_overlap(
    viewer_mode: ViewerViewMode,
    left_size: Option<(u32, u32)>,
    right_size: Option<(u32, u32)>,
    dual_gap_px: f32,
    quarter_turns: u8,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    vp_w: f32,
    vp_h: f32,
    zone_w: f32,
) -> (bool, bool) {
    let vp_w = vp_w.max(1.0);
    let vp_h = vp_h.max(1.0);
    let zone_w = zone_w.max(0.0);
    let Some((lw, lh)) = left_size.filter(|(w, h)| *w > 0 && *h > 0) else {
        return (false, false);
    };
    let (img_left, img_right) = match viewer_mode {
        ViewerViewMode::Single => {
            let (left, _top, drawn_w, _drawn_h, _, _) = viewer_fit_geometry(
                lw as f32,
                lh as f32,
                quarter_turns,
                zoom,
                pan_x,
                pan_y,
                vp_w,
                vp_h,
            );
            (left, left + drawn_w)
        }
        ViewerViewMode::Double => {
            let valid_right = right_size.filter(|(w, h)| *w > 0 && *h > 0);
            let geom = viewer_double_fit_geometries(
                (lw, lh),
                valid_right,
                dual_gap_px,
                quarter_turns,
                zoom,
                pan_x,
                pan_y,
                vp_w,
                vp_h,
            );
            (geom.spread_left, geom.spread_left + geom.drawn_w)
        }
        ViewerViewMode::Webtoon => return (false, false),
    };
    let left_overlaps = img_left < zone_w && img_right > 0.0;
    let right_overlaps = img_right > (vp_w - zone_w) && img_left < vp_w;
    (left_overlaps, right_overlaps)
}

#[derive(Debug, Clone)]
pub(crate) struct EdgeNavButtonProgram {
    pub(crate) is_next: bool,
    pub(crate) auto_hide: bool,
    pub(crate) on_press: ViewerMessage,
}

#[derive(Debug, Default)]
pub(crate) struct EdgeNavButtonState {
    hovered: bool,
    pressed: bool,
}

impl canvas::Program<ViewerMessage> for EdgeNavButtonProgram {
    type State = EdgeNavButtonState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<ViewerMessage>> {
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
            | canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                let over = cursor.is_over(bounds);
                if over != state.hovered {
                    state.hovered = over;
                    if !over {
                        state.pressed = false;
                    }
                    return Some(Action::request_redraw());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.hovered = true;
                    state.pressed = true;
                    return Some(Action::request_redraw().and_capture());
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.pressed {
                    state.pressed = false;
                    if cursor.is_over(bounds) {
                        return Some(Action::publish(self.on_press.clone()).and_capture());
                    }
                    return Some(Action::request_redraw());
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let hovered = cursor.is_over(bounds) || state.hovered;
        if self.auto_hide && !hovered && !state.pressed {
            return Vec::new();
        }

        let pal = PaletteTokens::for_theme(theme);
        let size = bounds.size();
        let mut frame = Frame::new(renderer, size);

        if hovered || state.pressed {
            let strip_alpha = if state.pressed { 0.55 } else { 0.30 };
            frame.fill_rectangle(
                Point::ORIGIN,
                size,
                pal.bg_elevated.scale_alpha(strip_alpha),
            );
        }

        let pill_w = 36.0_f32.min((size.width - 8.0).max(20.0));
        let pill_h = 52.0_f32.min((size.height - 16.0).max(28.0));
        let pill_origin = Point::new((size.width - pill_w) * 0.5, (size.height - pill_h) * 0.5);
        let pill_path = CanvasPath::rounded_rectangle(
            pill_origin,
            Size::new(pill_w, pill_h),
            iced::border::Radius::from(10.0),
        );
        let pill_bg = if state.pressed {
            pal.bg_pressed.scale_alpha(0.92)
        } else if hovered {
            pal.bg_elevated.scale_alpha(0.90)
        } else {
            pal.bg_elevated.scale_alpha(0.68)
        };
        let pill_border = if hovered || state.pressed {
            pal.border_strong.scale_alpha(0.85)
        } else {
            pal.border.scale_alpha(0.70)
        };
        frame.fill(&pill_path, pill_bg);
        frame.stroke(
            &pill_path,
            Stroke::default().with_width(1.0).with_color(pill_border),
        );

        let cx = size.width * 0.5;
        let cy = size.height * 0.5;
        let chevron = CanvasPath::new(|b| {
            if self.is_next {
                b.move_to(Point::new(cx - 3.5, cy - 7.5));
                b.line_to(Point::new(cx + 4.0, cy));
                b.line_to(Point::new(cx - 3.5, cy + 7.5));
            } else {
                b.move_to(Point::new(cx + 3.5, cy - 7.5));
                b.line_to(Point::new(cx - 4.0, cy));
                b.line_to(Point::new(cx + 3.5, cy + 7.5));
            }
        });
        let ink = if hovered || state.pressed {
            pal.text_primary
        } else {
            pal.text_secondary
        };
        frame.stroke(
            &chevron,
            Stroke::default()
                .with_width(2.2)
                .with_color(ink)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_edge_nav_overlap_distinguishes_margins_and_overflow() {
        // Portrait image centered in wide viewport leaves generous side margins -> always visible.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Single,
                Some((800, 1200)),
                None,
                0.0,
                0,
                1.0,
                0.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (false, false)
        );

        // Wide landscape image touches left/right edges -> auto-hide both edges until hover.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Single,
                Some((1920, 1080)),
                None,
                0.0,
                0,
                1.0,
                0.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (true, true)
        );

        // Zoomed-in portrait image overflows into both edge zones -> auto-hide both.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Single,
                Some((800, 1200)),
                None,
                0.0,
                0,
                2.5,
                0.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (true, true)
        );

        // Portrait image panned left enters only the left edge zone.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Single,
                Some((800, 1200)),
                None,
                0.0,
                0,
                1.0,
                -300.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (true, false)
        );

        // Rotating portrait 90 degrees makes it wide -> overlaps both edge zones.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Single,
                Some((800, 1600)),
                None,
                0.0,
                1,
                1.0,
                0.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (true, true)
        );

        // Double spread fitting inside margins vs filling width.
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Double,
                Some((600, 1200)),
                Some((600, 1200)),
                10.0,
                0,
                1.0,
                0.0,
                0.0,
                1400.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (false, false)
        );
        assert_eq!(
            viewer_edge_nav_overlap(
                ViewerViewMode::Double,
                Some((1000, 1200)),
                Some((1000, 1200)),
                10.0,
                0,
                1.0,
                0.0,
                0.0,
                1200.0,
                800.0,
                VIEWER_EDGE_NAV_ZONE_WIDTH,
            ),
            (true, true)
        );
    }
}
