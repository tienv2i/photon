//! Custom viewport overlay modes: dimmed frosted modal scrim, 4-strip canvas cutout, and floating.

use iced::widget::{Space, column, container, mouse_area, opaque, responsive, row, stack};
use iced::{Border, Color, Element, Length, Point, Rectangle, Size};

/// Controls how a dialog backdrop dims and locks interaction with underlying UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayMode {
    /// Full-viewport frosted dim scrim using `opaque(...)` to lock all underlying UI.
    DimmedFrosted {
        dim_alpha: f32,
        dismiss_on_backdrop: bool,
    },
    /// Dims and locks only the 4 perimeter strips outside `cutout` so the live-preview
    /// canvas inside `cutout` stays completely undimmed, unblocked, and interactive.
    Cutout {
        cutout: Rectangle,
        dim_alpha: f32,
        lock_outside: bool,
    },
    /// Non-blocking floating tool window: no scrim, only the dialog card itself is `opaque`.
    Floating,
}

impl Default for OverlayMode {
    fn default() -> Self {
        Self::DimmedFrosted {
            dim_alpha: 0.56,
            dismiss_on_backdrop: false,
        }
    }
}

/// Partition of `viewport \ cutout` into 4 non-overlapping perimeter rectangles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutoutStrips {
    /// Clamped cutout rectangle inside `viewport`.
    pub cutout: Rectangle,
    pub top: Rectangle,
    pub bottom: Rectangle,
    pub left: Rectangle,
    pub right: Rectangle,
}

/// Compute 4 non-overlapping perimeter strips around `cutout` within `viewport`.
pub fn compute_cutout_strips(viewport: Size, cutout: Rectangle) -> CutoutStrips {
    let vp_w = viewport.width.max(0.0);
    let vp_h = viewport.height.max(0.0);
    let cx0 = cutout.x.clamp(0.0, vp_w);
    let cy0 = cutout.y.clamp(0.0, vp_h);
    let cx1 = (cutout.x + cutout.width.max(0.0)).clamp(cx0, vp_w);
    let cy1 = (cutout.y + cutout.height.max(0.0)).clamp(cy0, vp_h);
    let cw = (cx1 - cx0).max(0.0);
    let ch = (cy1 - cy0).max(0.0);

    CutoutStrips {
        cutout: Rectangle::new(Point::new(cx0, cy0), Size::new(cw, ch)),
        top: Rectangle::new(Point::new(0.0, 0.0), Size::new(vp_w, cy0)),
        bottom: Rectangle::new(Point::new(0.0, cy1), Size::new(vp_w, (vp_h - cy1).max(0.0))),
        left: Rectangle::new(Point::new(0.0, cy0), Size::new(cx0, ch)),
        right: Rectangle::new(Point::new(cx1, cy0), Size::new((vp_w - cx1).max(0.0), ch)),
    }
}

fn strip_widget<'a, M: Clone + 'a>(
    w: Length,
    h: Length,
    bg: Color,
    lock: bool,
    on_press_msg: Option<M>,
    on_noop: M,
) -> Element<'a, M> {
    let box_el = container(Space::new().width(Length::Fill).height(Length::Fill))
        .width(w)
        .height(h)
        .style(move |_| container::Style {
            background: Some(bg.into()),
            ..Default::default()
        });
    if !lock {
        return box_el.into();
    }
    let press_msg = on_press_msg.unwrap_or_else(|| on_noop.clone());
    opaque(
        mouse_area(box_el)
            .on_press(press_msg.clone())
            .on_right_press(press_msg)
            .on_middle_press(on_noop.clone())
            .on_scroll(move |_| on_noop.clone()),
    )
}

/// Layer `overlay` scrim and `foreground` dialog element on top of `base`.
pub fn apply_overlay<'a, M: Clone + 'a>(
    base: Element<'a, M>,
    overlay: OverlayMode,
    scrim_color: Color,
    frosted_border_color: Option<Color>,
    on_dismiss: Option<M>,
    on_noop: M,
    foreground: Element<'a, M>,
) -> Element<'a, M> {
    match overlay {
        OverlayMode::Floating => stack![base, foreground]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        OverlayMode::DimmedFrosted {
            dim_alpha,
            dismiss_on_backdrop,
        } => {
            let alpha = dim_alpha.clamp(0.0, 0.95);
            let tinted = scrim_color.scale_alpha(alpha);
            let border = frosted_border_color
                .map(|c| Border {
                    color: c.scale_alpha(0.18),
                    width: 1.0,
                    ..Default::default()
                })
                .unwrap_or_default();
            let scrim_box = container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(tinted.into()),
                    border,
                    ..Default::default()
                });
            let click_msg = if dismiss_on_backdrop {
                on_dismiss.unwrap_or_else(|| on_noop.clone())
            } else {
                on_noop.clone()
            };
            let locked_scrim = opaque(
                mouse_area(scrim_box)
                    .on_press(click_msg.clone())
                    .on_right_press(click_msg)
                    .on_middle_press(on_noop.clone())
                    .on_scroll(move |_| on_noop.clone()),
            );
            stack![base, locked_scrim, foreground]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
        OverlayMode::Cutout {
            cutout,
            dim_alpha,
            lock_outside,
        } => {
            let alpha = dim_alpha.clamp(0.0, 0.95);
            let bg = scrim_color.scale_alpha(alpha);
            let scrim_layer: Element<'a, M> = responsive(move |vp| {
                let strips = compute_cutout_strips(vp, cutout);
                let mut rows = column![].width(Length::Fill).height(Length::Fill);

                if strips.top.height > 0.0 {
                    rows = rows.push(strip_widget(
                        Length::Fill,
                        Length::Fixed(strips.top.height),
                        bg,
                        lock_outside,
                        None,
                        on_noop.clone(),
                    ));
                }

                if strips.cutout.height > 0.0 {
                    let mut mid = row![]
                        .width(Length::Fill)
                        .height(Length::Fixed(strips.cutout.height));
                    if strips.left.width > 0.0 {
                        mid = mid.push(strip_widget(
                            Length::Fixed(strips.left.width),
                            Length::Fill,
                            bg,
                            lock_outside,
                            None,
                            on_noop.clone(),
                        ));
                    }
                    // Open cutout hole: no opaque or mouse_area wrapper, so pointer/wheel pass straight to canvas.
                    mid = mid.push(
                        Space::new()
                            .width(Length::Fixed(strips.cutout.width))
                            .height(Length::Fill),
                    );
                    if strips.right.width > 0.0 {
                        mid = mid.push(strip_widget(
                            Length::Fixed(strips.right.width),
                            Length::Fill,
                            bg,
                            lock_outside,
                            None,
                            on_noop.clone(),
                        ));
                    }
                    rows = rows.push(mid);
                }

                if strips.bottom.height > 0.0 {
                    rows = rows.push(strip_widget(
                        Length::Fill,
                        Length::Fixed(strips.bottom.height),
                        bg,
                        lock_outside,
                        None,
                        on_noop.clone(),
                    ));
                }

                rows.into()
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

            stack![base, scrim_layer, foreground]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(r: Rectangle) -> f32 {
        r.width * r.height
    }

    #[test]
    fn cutout_strips_partition_viewport_without_overlap() {
        let vp = Size::new(1280.0, 800.0);
        let cutout = Rectangle::new(Point::new(100.0, 50.0), Size::new(880.0, 700.0));
        let s = compute_cutout_strips(vp, cutout);

        assert_eq!(s.cutout, cutout);
        assert_eq!(
            s.top,
            Rectangle::new(Point::new(0.0, 0.0), Size::new(1280.0, 50.0))
        );
        assert_eq!(
            s.bottom,
            Rectangle::new(Point::new(0.0, 750.0), Size::new(1280.0, 50.0))
        );
        assert_eq!(
            s.left,
            Rectangle::new(Point::new(0.0, 50.0), Size::new(100.0, 700.0))
        );
        assert_eq!(
            s.right,
            Rectangle::new(Point::new(980.0, 50.0), Size::new(300.0, 700.0))
        );

        let total = area(s.cutout) + area(s.top) + area(s.bottom) + area(s.left) + area(s.right);
        assert!((total - vp.width * vp.height).abs() < 1e-3);
    }

    #[test]
    fn cutout_strips_clamp_out_of_bounds_cutout() {
        let vp = Size::new(800.0, 600.0);
        let cutout = Rectangle::new(Point::new(-50.0, -20.0), Size::new(1000.0, 700.0));
        let s = compute_cutout_strips(vp, cutout);
        assert_eq!(s.cutout, Rectangle::new(Point::new(0.0, 0.0), vp));
        assert_eq!(s.top.height, 0.0);
        assert_eq!(s.bottom.height, 0.0);
        assert_eq!(s.left.width, 0.0);
        assert_eq!(s.right.width, 0.0);
    }

    #[test]
    fn cutout_strips_handle_zero_sized_cutout() {
        let vp = Size::new(800.0, 600.0);
        let cutout = Rectangle::new(Point::new(200.0, 150.0), Size::new(0.0, 0.0));
        let s = compute_cutout_strips(vp, cutout);
        assert_eq!(s.top.height, 150.0);
        assert_eq!(s.bottom.height, 450.0);
        assert_eq!(s.left.height, 0.0);
        assert_eq!(s.right.height, 0.0);
    }
}
