//! Interactive movable & 8-way resizable dialog frame with scrollable body and `iced 0.14` overlays.

use std::time::{Duration, Instant};

use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke};
use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, rule, scrollable, stack, text,
};
use iced::{
    Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Renderer, Shadow, Size,
    Theme, Vector,
};

use crate::manager::{DialogEvent, DialogManager, DialogWindowState, ResizeEdge};
use crate::overlay::{OverlayMode, apply_overlay};

/// Visual styling tokens for a managed dialog frame and overlay.
#[derive(Debug, Clone, Copy)]
pub struct DialogStyle {
    pub surface_bg: Color,
    pub header_bg: Color,
    pub border_color: Color,
    pub border_strong: Color,
    pub accent: Color,
    pub text_primary: Color,
    pub text_muted: Color,
    pub scrim_color: Color,
    pub shadow_color: Color,
    pub radius: f32,
}

impl Default for DialogStyle {
    fn default() -> Self {
        Self {
            surface_bg: Color::from_rgb8(0x1b, 0x1e, 0x23),
            header_bg: Color::from_rgb8(0x24, 0x28, 0x30),
            border_color: Color::from_rgb8(0x2a, 0x2e, 0x36),
            border_strong: Color::from_rgb8(0x3a, 0x40, 0x4c),
            accent: Color::from_rgb8(0x3b, 0x82, 0xf6),
            text_primary: Color::from_rgb8(0xe8, 0xec, 0xf2),
            text_muted: Color::from_rgb8(0x7a, 0x82, 0x90),
            scrim_color: Color::from_rgb8(0x06, 0x08, 0x0c),
            shadow_color: Color::from_rgba(0.0, 0.0, 0.0, 0.52),
            radius: 12.0,
        }
    }
}

/// Specification for a movable/resizable dialog rendered via [`render_managed_dialog`].
pub struct ManagedDialogSpec<'a, Id, M> {
    pub id: Id,
    pub open: bool,
    pub title: String,
    pub defaults: DialogWindowState,
    pub resizable: bool,
    pub draggable: bool,
    pub overlay: OverlayMode,
    pub body: Element<'a, M>,
    pub footer: Option<Element<'a, M>>,
    pub close_button: Option<Element<'a, M>>,
    pub style: DialogStyle,
    pub on_event: Box<dyn Fn(DialogEvent<Id>) -> M + 'a>,
    pub on_close: M,
    pub on_noop: M,
}

/// Render a managed dialog (interactive titlebar drag, double-click recenter, 8-way resize,
/// scrollable body, and [`OverlayMode`] scrim) layered over `base`.
pub fn render_managed_dialog<'a, Id, M>(
    base: impl Into<Element<'a, M>>,
    manager: &DialogManager<Id>,
    viewport: Size,
    spec: ManagedDialogSpec<'a, Id, M>,
) -> Element<'a, M>
where
    Id: Ord + Clone + 'static,
    M: Clone + 'a,
{
    render_managed_dialog_with_header(base, manager, viewport, spec, None)
}

/// Render a managed dialog with an optional non-scrolling `header_strip` pinned between the
/// title bar and the scrollable body.
pub fn render_managed_dialog_with_header<'a, Id, M>(
    base: impl Into<Element<'a, M>>,
    manager: &DialogManager<Id>,
    viewport: Size,
    spec: ManagedDialogSpec<'a, Id, M>,
    header_strip: Option<Element<'a, M>>,
) -> Element<'a, M>
where
    Id: Ord + Clone + 'static,
    M: Clone + 'a,
{
    let base_el = base.into();
    if !spec.open {
        return base_el;
    }

    let rect = manager.resolved_rect(&spec.id, &spec.defaults, viewport);
    let win_state = manager.state(&spec.id).unwrap_or(&spec.defaults);
    let user_moved = win_state.user_moved;
    let user_resized = win_state.user_resized;
    let margin = win_state.margin;
    let overlay_mode = spec.overlay;
    let scrim_color = spec.style.scrim_color;
    let border_strong = spec.style.border_strong;
    let on_noop = spec.on_noop.clone();

    let frame_card = build_dialog_frame(
        rect,
        viewport,
        user_moved,
        user_resized,
        margin,
        spec,
        header_strip,
    );

    // Only the dialog card itself is wrapped in `opaque(...)`; the full-viewport positioning
    // wrapper leaves empty space pass-through for `Floating` and `Cutout` modes.
    // Always anchor the top edge at `rect.y` so auto-shrinking/expanding dialogs never drift
    // back toward the center when their content height changes.
    let safe_margin = margin.max(0.0);
    let positioned = if !user_moved && !user_resized {
        container(opaque(frame_card))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: rect.y.max(safe_margin),
                left: safe_margin,
                right: safe_margin,
                bottom: safe_margin,
            })
    } else {
        container(opaque(frame_card))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Top)
            .padding(Padding {
                top: rect.y.max(0.0),
                left: rect.x.max(0.0),
                right: safe_margin,
                bottom: safe_margin,
            })
    };

    apply_overlay(
        base_el,
        overlay_mode,
        scrim_color,
        Some(border_strong),
        None,
        on_noop,
        positioned.into(),
    )
}

/// Compute `(max_card_height, max_body_height)` for a managed dialog frame so the header,
/// optional header strip, scrollable body, and footer stay within the viewport.
pub fn resolve_card_and_body_limits(
    viewport: Size,
    rect: Rectangle,
    user_moved: bool,
    user_resized: bool,
    margin: f32,
    has_header_strip: bool,
    has_footer: bool,
) -> (f32, f32) {
    let m = margin.max(0.0);
    let max_card_h = if user_resized {
        rect.height.max(160.0)
    } else if user_moved {
        (viewport.height - rect.y.max(0.0) - m).max(160.0)
    } else {
        (viewport.height - m * 2.0).max(160.0)
    };
    let footer_reserve = if has_footer { 60.0 } else { 0.0 };
    let header_strip_reserve = if has_header_strip { 56.0 } else { 0.0 };
    let max_body_h = (max_card_h - 44.0 - footer_reserve - header_strip_reserve).max(60.0);
    (max_card_h, max_body_h)
}

fn build_dialog_frame<'a, Id, M>(
    rect: Rectangle,
    viewport: Size,
    user_moved: bool,
    user_resized: bool,
    margin: f32,
    spec: ManagedDialogSpec<'a, Id, M>,
    header_strip: Option<Element<'a, M>>,
) -> Element<'a, M>
where
    Id: Ord + Clone + 'static,
    M: Clone + 'a,
{
    let style = spec.style;
    let on_event: std::rc::Rc<dyn Fn(DialogEvent<Id>) -> M + 'a> = std::rc::Rc::from(spec.on_event);
    let close_el: Element<'a, M> = spec.close_button.unwrap_or_else(|| {
        button(text("×").size(14))
            .padding([2, 8])
            .on_press(spec.on_close.clone())
            .style(move |_, status| {
                let bg = match status {
                    button::Status::Hovered => style.border_color,
                    button::Status::Pressed => style.border_strong,
                    _ => Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: style.text_primary,
                    border: Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .into()
    });

    let title_label = container(text(spec.title).size(14).style(move |_| text::Style {
        color: Some(style.text_primary),
    }))
    .width(Length::Fill)
    .padding([9, 14])
    .align_y(Alignment::Center);

    let title_region: Element<'a, M> = if spec.draggable {
        let drag_cb = on_event.clone();
        let drag_canvas = iced::widget::canvas(TitleDragProgram {
            id: spec.id.clone(),
            defaults: spec.defaults.clone(),
            viewport,
            on_event: drag_cb,
        })
        .width(Length::Fill)
        .height(Length::Fixed(40.0));
        stack![title_label, drag_canvas]
            .width(Length::Fill)
            .height(Length::Fixed(40.0))
            .into()
    } else {
        title_label.height(Length::Fixed(40.0)).into()
    };

    let header = container(
        row![title_region, container(close_el).padding([0, 8])]
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(style.header_bg.into()),
        border: Border {
            radius: iced::border::Radius {
                top_left: style.radius,
                top_right: style.radius,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            ..Default::default()
        },
        ..Default::default()
    });

    let sep_color = style.border_color;
    let hairline = move || {
        rule::horizontal(1).style(move |_| rule::Style {
            color: sep_color,
            radius: 0.0.into(),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
    };

    let has_header_strip = header_strip.is_some();
    let has_footer = spec.footer.is_some();
    let (max_card_h, max_body_h) = resolve_card_and_body_limits(
        viewport,
        rect,
        user_moved,
        user_resized,
        margin,
        has_header_strip,
        has_footer,
    );
    let body_scroll = scrollable(container(spec.body).padding(14).width(Length::Fill))
        .width(Length::Fill)
        .height(if user_resized {
            Length::Fill
        } else {
            Length::Shrink
        });

    let body_el: Element<'a, M> = if user_resized {
        body_scroll.into()
    } else {
        container(body_scroll)
            .width(Length::Fill)
            .height(Length::Shrink)
            .max_height(max_body_h)
            .into()
    };

    let mut content_col = column![header, hairline()]
        .width(Length::Fill)
        .height(if user_resized {
            Length::Fill
        } else {
            Length::Shrink
        });

    if let Some(strip) = header_strip {
        content_col = content_col
            .push(container(strip).padding([8, 14]).width(Length::Fill))
            .push(hairline());
    }

    content_col = content_col.push(body_el);

    if let Some(footer) = spec.footer {
        content_col = content_col
            .push(hairline())
            .push(container(footer).padding([10, 14]).width(Length::Fill));
    }

    let card_base = container(content_col)
        .width(Length::Fixed(rect.width))
        .style(move |_| container::Style {
            background: Some(style.surface_bg.into()),
            text_color: Some(style.text_primary),
            border: Border {
                color: style.border_strong,
                width: 1.0,
                radius: style.radius.into(),
            },
            shadow: Shadow {
                color: style.shadow_color,
                offset: Vector::new(0.0, 12.0),
                blur_radius: 28.0,
            },
            ..Default::default()
        });

    let card = if user_resized {
        card_base.height(Length::Fixed(rect.height))
    } else {
        card_base.height(Length::Shrink).max_height(max_card_h)
    };

    let on_noop_scroll = spec.on_noop.clone();
    let card_with_scroll_guard: Element<'a, M> = mouse_area(card)
        .on_scroll(move |_| on_noop_scroll.clone())
        .into();

    if !spec.resizable {
        return card_with_scroll_guard;
    }

    let defaults_for_handles = spec.defaults.clone();
    let make_handle = |edge: ResizeEdge, w: Length, h: Length, draw_grip: bool| -> Element<'a, M> {
        iced::widget::canvas(ResizeHandleProgram {
            id: spec.id.clone(),
            edge,
            defaults: defaults_for_handles.clone(),
            viewport,
            grip_color: if draw_grip {
                Some(style.text_muted.scale_alpha(0.65))
            } else {
                None
            },
            on_event: on_event.clone(),
        })
        .width(w)
        .height(h)
        .into()
    };

    let handles_overlay = column![
        row![
            make_handle(
                ResizeEdge::NW,
                Length::Fixed(10.0),
                Length::Fixed(4.0),
                false
            ),
            make_handle(ResizeEdge::N, Length::Fill, Length::Fixed(4.0), false),
            make_handle(
                ResizeEdge::NE,
                Length::Fixed(10.0),
                Length::Fixed(4.0),
                false
            ),
        ]
        .width(Length::Fill)
        .height(Length::Fixed(4.0)),
        row![
            make_handle(ResizeEdge::W, Length::Fixed(5.0), Length::Fill, false),
            Space::new().width(Length::Fill).height(Length::Fill),
            // Keep the top-right 36px below the 4px NE handle clear so the close button in the
            // 40px header is never overlapped by the East resize edge.
            column![
                Space::new()
                    .width(Length::Fixed(5.0))
                    .height(Length::Fixed(36.0)),
                make_handle(ResizeEdge::E, Length::Fixed(5.0), Length::Fill, false),
            ]
            .width(Length::Fixed(5.0))
            .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill),
        row![
            make_handle(
                ResizeEdge::SW,
                Length::Fixed(12.0),
                Length::Fixed(5.0),
                false
            ),
            make_handle(ResizeEdge::S, Length::Fill, Length::Fixed(5.0), false),
            make_handle(
                ResizeEdge::SE,
                Length::Fixed(12.0),
                Length::Fixed(12.0),
                true
            ),
        ]
        .width(Length::Fill)
        .height(Length::Fixed(12.0))
        .align_y(Alignment::End),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    stack![card_with_scroll_guard, handles_overlay]
        .width(Length::Fixed(rect.width))
        .height(if user_resized {
            Length::Fixed(rect.height)
        } else {
            Length::Shrink
        })
        .into()
}

struct TitleDragProgram<'a, Id, M> {
    id: Id,
    defaults: DialogWindowState,
    viewport: Size,
    on_event: std::rc::Rc<dyn Fn(DialogEvent<Id>) -> M + 'a>,
}

#[derive(Debug, Default)]
struct GestureCanvasState {
    dragging: bool,
    last_click: Option<Instant>,
}

impl<'a, Id, M> canvas::Program<M> for TitleDragProgram<'a, Id, M>
where
    Id: Clone + 'static,
    M: Clone + 'a,
{
    type State = GestureCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let pos = cursor.position()?;
                let now = Instant::now();
                if let Some(prev) = state.last_click
                    && now.duration_since(prev) <= Duration::from_millis(350)
                {
                    state.last_click = None;
                    state.dragging = false;
                    return Some(
                        Action::publish((self.on_event)(DialogEvent::Center {
                            id: self.id.clone(),
                            viewport: self.viewport,
                        }))
                        .and_capture(),
                    );
                }
                state.last_click = Some(now);
                state.dragging = true;
                Some(
                    Action::publish((self.on_event)(DialogEvent::BeginMove {
                        id: self.id.clone(),
                        defaults: self.defaults.clone(),
                        cursor: pos,
                        viewport: self.viewport,
                    }))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => Some(
                Action::publish((self.on_event)(DialogEvent::UpdateGesture {
                    cursor: *position,
                    viewport: self.viewport,
                }))
                .and_capture(),
            ),
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging =>
            {
                state.dragging = false;
                Some(Action::publish((self.on_event)(DialogEvent::EndGesture)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        Vec::new()
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
}

struct ResizeHandleProgram<'a, Id, M> {
    id: Id,
    edge: ResizeEdge,
    defaults: DialogWindowState,
    viewport: Size,
    grip_color: Option<Color>,
    on_event: std::rc::Rc<dyn Fn(DialogEvent<Id>) -> M + 'a>,
}

impl<'a, Id, M> canvas::Program<M> for ResizeHandleProgram<'a, Id, M>
where
    Id: Clone + 'static,
    M: Clone + 'a,
{
    type State = GestureCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<M>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let pos = cursor.position()?;
                state.dragging = true;
                Some(
                    Action::publish((self.on_event)(DialogEvent::BeginResize {
                        id: self.id.clone(),
                        edge: self.edge,
                        defaults: self.defaults.clone(),
                        cursor: pos,
                        viewport: self.viewport,
                    }))
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) if state.dragging => Some(
                Action::publish((self.on_event)(DialogEvent::UpdateGesture {
                    cursor: *position,
                    viewport: self.viewport,
                }))
                .and_capture(),
            ),
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging =>
            {
                state.dragging = false;
                Some(Action::publish((self.on_event)(DialogEvent::EndGesture)).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let Some(color) = self.grip_color else {
            return Vec::new();
        };
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let l1 = Path::line(Point::new(w - 10.0, h - 3.0), Point::new(w - 3.0, h - 10.0));
        let l2 = Path::line(Point::new(w - 6.0, h - 3.0), Point::new(w - 3.0, h - 6.0));
        let stroke = Stroke::default().with_width(1.3).with_color(color);
        frame.stroke(&l1, stroke);
        frame.stroke(&l2, stroke);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging || cursor.is_over(bounds) {
            self.edge.cursor_interaction()
        } else {
            mouse::Interaction::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_and_body_limits_clamp_for_centered_moved_and_resized_states() {
        let vp = Size::new(1200.0, 800.0);
        let rect = Rectangle::new(Point::new(300.0, 500.0), Size::new(500.0, 400.0));

        // Centered: full viewport minus 2*margin
        let (card_c, body_c) =
            resolve_card_and_body_limits(vp, rect, false, false, 16.0, true, true);
        assert_eq!(card_c, 768.0);
        assert_eq!(body_c, 768.0 - 44.0 - 60.0 - 56.0);

        // User moved without resizing: bounded by remaining space from rect.y to bottom margin
        let (card_m, body_m) =
            resolve_card_and_body_limits(vp, rect, true, false, 16.0, false, true);
        assert_eq!(card_m, 284.0);
        assert_eq!(body_m, 284.0 - 44.0 - 60.0);

        // User resized: uses explicit rect.height
        let (card_r, _) = resolve_card_and_body_limits(vp, rect, true, true, 16.0, false, false);
        assert_eq!(card_r, 400.0);
    }
}
