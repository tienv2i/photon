//! Per-dialog geometry, viewport clamping, and interactive move/resize state management.

use std::collections::BTreeMap;

use iced::{Point, Rectangle, Size, mouse};

/// 8-way resize edge or corner handle on a dialog frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl ResizeEdge {
    /// Cursor interaction icon corresponding to this resize handle.
    pub const fn cursor_interaction(self) -> mouse::Interaction {
        match self {
            Self::N | Self::S => mouse::Interaction::ResizingVertically,
            Self::E | Self::W => mouse::Interaction::ResizingHorizontally,
            Self::NE | Self::SW => mouse::Interaction::ResizingDiagonallyUp,
            Self::NW | Self::SE => mouse::Interaction::ResizingDiagonallyDown,
        }
    }

    const fn moves_left(self) -> bool {
        matches!(self, Self::W | Self::NW | Self::SW)
    }

    const fn moves_right(self) -> bool {
        matches!(self, Self::E | Self::NE | Self::SE)
    }

    const fn moves_top(self) -> bool {
        matches!(self, Self::N | Self::NE | Self::NW)
    }

    const fn moves_bottom(self) -> bool {
        matches!(self, Self::S | Self::SE | Self::SW)
    }
}

/// Active pointer gesture on a managed dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOp {
    Move,
    Resize(ResizeEdge),
}

/// Events emitted by dialog titlebars and resize handles to drive [`DialogManager`].
#[derive(Debug, Clone, PartialEq)]
pub enum DialogEvent<Id> {
    BeginMove {
        id: Id,
        defaults: DialogWindowState,
        cursor: Point,
        viewport: Size,
    },
    BeginResize {
        id: Id,
        edge: ResizeEdge,
        defaults: DialogWindowState,
        cursor: Point,
        viewport: Size,
    },
    UpdateGesture {
        cursor: Point,
        viewport: Size,
    },
    EndGesture,
    Center {
        id: Id,
        viewport: Size,
    },
    BringToFront(Id),
}

/// Persisted/in-flight geometry and constraints for a single dialog instance.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogWindowState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub margin: f32,
    pub user_moved: bool,
    pub user_resized: bool,
}

impl Default for DialogWindowState {
    fn default() -> Self {
        Self::new(500.0, 480.0)
    }
}

impl DialogWindowState {
    /// Construct centered default geometry with standard studio constraints.
    pub fn new(width: f32, height: f32) -> Self {
        let w = width.max(120.0);
        let h = height.max(100.0);
        Self {
            x: None,
            y: None,
            width: w,
            height: h,
            min_width: 280.0,
            min_height: 180.0,
            max_width: w.max(1400.0),
            max_height: h.max(1200.0),
            margin: 16.0,
            user_moved: false,
            user_resized: false,
        }
    }

    /// Set preferred height, expanding `max_height` if needed so normal viewports never clip.
    pub fn preferred_height(mut self, height: f32) -> Self {
        self.height = height.max(100.0);
        self.max_height = self.max_height.max(self.height);
        self
    }

    /// Set preferred `(width, height)`, expanding max bounds if needed.
    pub fn preferred_size(mut self, width: f32, height: f32) -> Self {
        self.width = width.max(120.0);
        self.height = height.max(100.0);
        self.max_width = self.max_width.max(self.width);
        self.max_height = self.max_height.max(self.height);
        self
    }

    /// Set minimum allowed `(width, height)`.
    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_width = width.max(120.0);
        self.min_height = height.max(100.0);
        self
    }

    /// Set maximum allowed `(width, height)`.
    pub fn max_size(mut self, width: f32, height: f32) -> Self {
        self.max_width = width.max(self.min_width);
        self.max_height = height.max(self.min_height);
        self
    }

    /// Set viewport edge margin enforced during clamping.
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);
        self
    }

    /// Set an initial explicit origin `(x, y)` instead of auto-centering.
    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self.user_moved = true;
        self
    }

    /// Whether both coordinates are unset (auto-centered in viewport).
    pub fn is_centered(&self) -> bool {
        self.x.is_none() && self.y.is_none()
    }

    /// Fit this dialog cleanly inside `viewport`, centering unset axes and only shrinking below
    /// preferred dimensions when `viewport - margin * 2` is smaller than the preferred size.
    pub fn fit_viewport(&self, viewport: Size) -> Rectangle {
        self.resolved_rect(viewport)
    }

    /// Resolve a viewport-fitted, forcibly centered rectangle ignoring any explicit `(x, y)`.
    pub fn centered_rect(&self, viewport: Size) -> Rectangle {
        let vp = safe_viewport(viewport);
        let (w, h) = clamp_size(
            self.width,
            self.height,
            self.min_width,
            self.min_height,
            self.max_width,
            self.max_height,
            vp,
            self.margin,
        );
        let raw_x = (vp.width - w) * 0.5;
        let raw_y = (vp.height - h) * 0.5;
        let (x, y) = clamp_origin(raw_x, raw_y, w, h, vp, self.margin);
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    /// Resolve clamped rectangle inside `viewport`, centering when `x` or `y` is unset.
    pub fn resolved_rect(&self, viewport: Size) -> Rectangle {
        let vp = safe_viewport(viewport);
        let (w, h) = clamp_size(
            self.width,
            self.height,
            self.min_width,
            self.min_height,
            self.max_width,
            self.max_height,
            vp,
            self.margin,
        );
        let raw_x = self.x.unwrap_or((vp.width - w) * 0.5);
        let raw_y = self.y.unwrap_or((vp.height - h) * 0.5);
        let (x, y) = clamp_origin(raw_x, raw_y, w, h, vp, self.margin);
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    /// Reset explicit coordinates so the dialog returns to centered placement.
    pub fn reset_position(&mut self) {
        self.x = None;
        self.y = None;
        self.user_moved = false;
    }
}

/// Active drag or resize operation snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveGesture<Id> {
    pub id: Id,
    pub op: DragOp,
    pub start_cursor: Point,
    pub start_rect: Rectangle,
}

/// Coordinates multiple open/reusable dialogs, tracking geometry and active gestures.
#[derive(Debug, Clone)]
pub struct DialogManager<Id: Ord + Clone> {
    states: BTreeMap<Id, DialogWindowState>,
    order: Vec<Id>,
    active_gesture: Option<ActiveGesture<Id>>,
}

impl<Id: Ord + Clone> Default for DialogManager<Id> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Id: Ord + Clone> DialogManager<Id> {
    /// Create an empty dialog manager.
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            order: Vec::new(),
            active_gesture: None,
        }
    }

    /// Ensure `id` has a registered state (inserting `defaults` if absent) and return it.
    pub fn ensure(&mut self, id: Id, defaults: DialogWindowState) -> &DialogWindowState {
        if !self.order.contains(&id) {
            self.order.push(id.clone());
        }
        self.states.entry(id).or_insert(defaults)
    }

    /// Register or reset `id` with `defaults` when `reset_geometry` is true, and bring to front.
    pub fn open_with(
        &mut self,
        id: Id,
        defaults: DialogWindowState,
        reset_geometry: bool,
    ) -> &DialogWindowState {
        if reset_geometry || !self.states.contains_key(&id) {
            self.states.insert(id.clone(), defaults);
        } else if let Some(existing) = self.states.get_mut(&id) {
            if !existing.user_moved {
                existing.x = defaults.x;
                existing.y = defaults.y;
            }
            if !existing.user_resized {
                existing.width = defaults.width;
                existing.height = defaults.height;
                existing.min_width = defaults.min_width;
                existing.min_height = defaults.min_height;
                existing.max_width = defaults.max_width;
                existing.max_height = defaults.max_height;
                existing.margin = defaults.margin;
            }
        }
        self.bring_to_front(&id);
        self.states.get(&id).expect("inserted above")
    }

    /// Inspect stored state for `id`.
    pub fn state(&self, id: &Id) -> Option<&DialogWindowState> {
        self.states.get(id)
    }

    /// Mutably inspect stored state for `id`.
    pub fn state_mut(&mut self, id: &Id) -> Option<&mut DialogWindowState> {
        self.states.get_mut(id)
    }

    /// Remove any active gesture for `id` and unregister its transient position/size.
    pub fn close(&mut self, id: &Id) {
        if self.active_gesture.as_ref().is_some_and(|g| &g.id == id) {
            self.active_gesture = None;
        }
        self.states.remove(id);
        self.order.retain(|x| x != id);
    }

    /// Reset geometry for `id` (next render will re-center with defaults).
    pub fn reset(&mut self, id: &Id) {
        if self.active_gesture.as_ref().is_some_and(|g| &g.id == id) {
            self.active_gesture = None;
        }
        self.states.remove(id);
    }

    /// Move `id` to the top of the manager's z-order list.
    pub fn bring_to_front(&mut self, id: &Id) {
        self.order.retain(|x| x != id);
        self.order.push(id.clone());
    }

    /// Current z-order (back-to-front).
    pub fn z_order(&self) -> &[Id] {
        &self.order
    }

    /// Resolve the clamped viewport rectangle for `id`, falling back to `defaults`.
    pub fn resolved_rect(
        &self,
        id: &Id,
        defaults: &DialogWindowState,
        viewport: Size,
    ) -> Rectangle {
        self.states
            .get(id)
            .unwrap_or(defaults)
            .resolved_rect(viewport)
    }

    /// Fit dialog `id` cleanly inside `viewport` (shrinking below preferred size only when
    /// `viewport - margin * 2` is smaller than the preferred dimensions).
    pub fn fit_viewport(&self, id: &Id, defaults: &DialogWindowState, viewport: Size) -> Rectangle {
        self.states
            .get(id)
            .unwrap_or(defaults)
            .fit_viewport(viewport)
    }

    /// Start dragging dialog `id` by its titlebar, preserving `defaults` if unregistered.
    pub fn begin_move_with(
        &mut self,
        id: Id,
        defaults: DialogWindowState,
        cursor: Point,
        viewport: Size,
    ) {
        let start_rect = self
            .states
            .entry(id.clone())
            .or_insert(defaults)
            .resolved_rect(viewport);
        self.bring_to_front(&id);
        self.active_gesture = Some(ActiveGesture {
            id,
            op: DragOp::Move,
            start_cursor: cursor,
            start_rect,
        });
    }

    /// Start dragging dialog `id` by its titlebar.
    pub fn begin_move(&mut self, id: Id, cursor: Point, viewport: Size) {
        self.begin_move_with(id, DialogWindowState::default(), cursor, viewport);
    }

    /// Start resizing dialog `id` from `edge`, preserving `defaults` if unregistered.
    pub fn begin_resize_with(
        &mut self,
        id: Id,
        edge: ResizeEdge,
        defaults: DialogWindowState,
        cursor: Point,
        viewport: Size,
    ) {
        let start_rect = self
            .states
            .entry(id.clone())
            .or_insert(defaults)
            .resolved_rect(viewport);
        self.bring_to_front(&id);
        self.active_gesture = Some(ActiveGesture {
            id,
            op: DragOp::Resize(edge),
            start_cursor: cursor,
            start_rect,
        });
    }

    /// Start resizing dialog `id` from `edge`.
    pub fn begin_resize(&mut self, id: Id, edge: ResizeEdge, cursor: Point, viewport: Size) {
        self.begin_resize_with(id, edge, DialogWindowState::default(), cursor, viewport);
    }

    /// Apply pointer movement to the active move/resize gesture. Returns `true` if state changed.
    pub fn update_gesture(&mut self, cursor: Point, viewport: Size) -> bool {
        let Some(gesture) = &self.active_gesture else {
            return false;
        };
        let Some(win) = self.states.get_mut(&gesture.id) else {
            self.active_gesture = None;
            return false;
        };
        let dx = cursor.x - gesture.start_cursor.x;
        let dy = cursor.y - gesture.start_cursor.y;
        let next = match gesture.op {
            DragOp::Move => {
                win.user_moved = true;
                apply_move_delta(gesture.start_rect, dx, dy, viewport, win.margin)
            }
            DragOp::Resize(edge) => {
                win.user_moved = true;
                win.user_resized = true;
                apply_resize_delta(
                    gesture.start_rect,
                    edge,
                    dx,
                    dy,
                    win.min_width,
                    win.min_height,
                    win.max_width,
                    win.max_height,
                    viewport,
                    win.margin,
                )
            }
        };
        win.x = Some(next.x);
        win.y = Some(next.y);
        win.width = next.width;
        win.height = next.height;
        true
    }

    /// Complete the active gesture. Returns `true` if one was active.
    pub fn end_gesture(&mut self) -> bool {
        self.active_gesture.take().is_some()
    }

    /// Process a [`DialogEvent`]. Returns `true` if any state changed.
    pub fn handle_event(&mut self, event: DialogEvent<Id>, viewport: Size) -> bool {
        match event {
            DialogEvent::BeginMove {
                id,
                defaults,
                cursor,
                viewport: ev_vp,
            } => {
                let vp = if ev_vp.width > 0.0 && ev_vp.height > 0.0 {
                    ev_vp
                } else {
                    viewport
                };
                self.begin_move_with(id, defaults, cursor, vp);
                true
            }
            DialogEvent::BeginResize {
                id,
                edge,
                defaults,
                cursor,
                viewport: ev_vp,
            } => {
                let vp = if ev_vp.width > 0.0 && ev_vp.height > 0.0 {
                    ev_vp
                } else {
                    viewport
                };
                self.begin_resize_with(id, edge, defaults, cursor, vp);
                true
            }
            DialogEvent::UpdateGesture {
                cursor,
                viewport: ev_vp,
            } => {
                let vp = if ev_vp.width > 0.0 && ev_vp.height > 0.0 {
                    ev_vp
                } else {
                    viewport
                };
                self.update_gesture(cursor, vp)
            }
            DialogEvent::EndGesture => self.end_gesture(),
            DialogEvent::Center { id, .. } => {
                if let Some(win) = self.states.get_mut(&id) {
                    win.reset_position();
                    true
                } else {
                    false
                }
            }
            DialogEvent::BringToFront(id) => {
                self.bring_to_front(&id);
                true
            }
        }
    }

    /// Whether a drag or resize gesture is currently in progress.
    pub fn is_dragging(&self) -> bool {
        self.active_gesture.is_some()
    }

    /// Active gesture descriptor, if any.
    pub fn active_gesture(&self) -> Option<&ActiveGesture<Id>> {
        self.active_gesture.as_ref()
    }
}

fn safe_viewport(viewport: Size) -> Size {
    Size::new(viewport.width.max(240.0), viewport.height.max(180.0))
}

#[allow(clippy::too_many_arguments)]
fn clamp_size(
    raw_w: f32,
    raw_h: f32,
    min_w: f32,
    min_h: f32,
    max_w: f32,
    max_h: f32,
    viewport: Size,
    margin: f32,
) -> (f32, f32) {
    let vp = safe_viewport(viewport);
    let m = margin.max(0.0);
    let max_avail_w = (vp.width - 2.0 * m).max(120.0);
    let max_avail_h = (vp.height - 2.0 * m).max(100.0);
    let hi_w = max_w.min(max_avail_w).max(120.0);
    let hi_h = max_h.min(max_avail_h).max(100.0);
    let lo_w = min_w.min(hi_w);
    let lo_h = min_h.min(hi_h);
    (raw_w.clamp(lo_w, hi_w), raw_h.clamp(lo_h, hi_h))
}

fn clamp_origin(
    raw_x: f32,
    raw_y: f32,
    width: f32,
    height: f32,
    viewport: Size,
    margin: f32,
) -> (f32, f32) {
    let vp = safe_viewport(viewport);
    let m = margin.max(0.0);
    let slack_x = (vp.width - width).max(0.0);
    let slack_y = (vp.height - height).max(0.0);
    let mx = m.min(slack_x * 0.5);
    let my = m.min(slack_y * 0.5);
    let min_x = mx;
    let max_x = (slack_x - mx).max(min_x);
    let min_y = my;
    let max_y = (slack_y - my).max(min_y);
    (
        raw_x.clamp(min_x, max_x).round(),
        raw_y.clamp(min_y, max_y).round(),
    )
}

/// Clamp an arbitrary rectangle to min/max bounds and viewport margin.
pub fn clamp_dialog_rect(
    rect: Rectangle,
    min_w: f32,
    min_h: f32,
    max_w: f32,
    max_h: f32,
    viewport: Size,
    margin: f32,
) -> Rectangle {
    let (w, h) = clamp_size(
        rect.width,
        rect.height,
        min_w,
        min_h,
        max_w,
        max_h,
        viewport,
        margin,
    );
    let (x, y) = clamp_origin(rect.x, rect.y, w, h, viewport, margin);
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}

/// Translate `start` by `(dx, dy)` while keeping the dialog clamped inside `viewport`.
pub fn apply_move_delta(
    start: Rectangle,
    dx: f32,
    dy: f32,
    viewport: Size,
    margin: f32,
) -> Rectangle {
    let vp = safe_viewport(viewport);
    let w = start.width.min((vp.width - 2.0 * margin).max(120.0));
    let h = start.height.min((vp.height - 2.0 * margin).max(100.0));
    let (x, y) = clamp_origin(start.x + dx, start.y + dy, w, h, vp, margin);
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}

/// Resize `start` along `edge` by `(dx, dy)`, pinning the opposite edge and clamping to
/// min/max bounds and viewport margins.
#[allow(clippy::too_many_arguments)]
pub fn apply_resize_delta(
    start: Rectangle,
    edge: ResizeEdge,
    dx: f32,
    dy: f32,
    min_w: f32,
    min_h: f32,
    max_w: f32,
    max_h: f32,
    viewport: Size,
    margin: f32,
) -> Rectangle {
    let vp = safe_viewport(viewport);
    let m = margin.max(0.0);
    let right_pinned = start.x + start.width;
    let bottom_pinned = start.y + start.height;

    let max_avail_w = (vp.width - 2.0 * m).max(120.0);
    let max_avail_h = (vp.height - 2.0 * m).max(100.0);
    let hi_w = max_w.min(max_avail_w).max(120.0);
    let hi_h = max_h.min(max_avail_h).max(100.0);
    let lo_w = min_w.min(hi_w);
    let lo_h = min_h.min(hi_h);

    let (x, width) = if edge.moves_left() {
        let edge_max_w = (right_pinned - m).max(lo_w).min(hi_w);
        let w = (start.width - dx).clamp(lo_w, edge_max_w);
        (right_pinned - w, w)
    } else if edge.moves_right() {
        let edge_max_w = (vp.width - m - start.x).max(lo_w).min(hi_w);
        let w = (start.width + dx).clamp(lo_w, edge_max_w);
        (start.x, w)
    } else {
        (start.x, start.width.clamp(lo_w, hi_w))
    };

    let (y, height) = if edge.moves_top() {
        let edge_max_h = (bottom_pinned - m).max(lo_h).min(hi_h);
        let h = (start.height - dy).clamp(lo_h, edge_max_h);
        (bottom_pinned - h, h)
    } else if edge.moves_bottom() {
        let edge_max_h = (vp.height - m - start.y).max(lo_h).min(hi_h);
        let h = (start.height + dy).clamp(lo_h, edge_max_h);
        (start.y, h)
    } else {
        (start.y, start.height.clamp(lo_h, hi_h))
    };

    let (cx, cy) = clamp_origin(x, y, width, height, vp, m);
    Rectangle::new(Point::new(cx, cy), Size::new(width.round(), height.round()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VP: Size = Size::new(1200.0, 800.0);

    #[test]
    fn move_clamps_within_viewport_margins() {
        let start = Rectangle::new(Point::new(300.0, 200.0), Size::new(400.0, 300.0));
        // Drag far past top-left
        let tl = apply_move_delta(start, -1000.0, -1000.0, VP, 16.0);
        assert_eq!(tl.x, 16.0);
        assert_eq!(tl.y, 16.0);
        assert_eq!(tl.width, 400.0);
        assert_eq!(tl.height, 300.0);

        // Drag far past bottom-right
        let br = apply_move_delta(start, 2000.0, 2000.0, VP, 16.0);
        assert_eq!(br.x, 1200.0 - 400.0 - 16.0);
        assert_eq!(br.y, 800.0 - 300.0 - 16.0);
    }

    #[test]
    fn eight_way_resize_pins_opposite_edges_and_clamps() {
        let start = Rectangle::new(Point::new(300.0, 200.0), Size::new(400.0, 300.0));

        // East grows right without moving x
        let e = apply_resize_delta(
            start,
            ResizeEdge::E,
            120.0,
            50.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!((e.x, e.y, e.width, e.height), (300.0, 200.0, 520.0, 300.0));

        // West grows left and keeps right edge (300 + 400 = 700) pinned
        let w = apply_resize_delta(
            start,
            ResizeEdge::W,
            -100.0,
            0.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(w.x + w.width, 700.0);
        assert_eq!(w.width, 500.0);

        // West shrinking past min_width pins right edge and clamps width at min_w
        let w_min = apply_resize_delta(
            start,
            ResizeEdge::W,
            500.0,
            0.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(w_min.width, 260.0);
        assert_eq!(w_min.x, 700.0 - 260.0);

        // North grows up and keeps bottom edge (200 + 300 = 500) pinned
        let n = apply_resize_delta(
            start,
            ResizeEdge::N,
            0.0,
            -80.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(n.y + n.height, 500.0);
        assert_eq!(n.height, 380.0);

        // South grows down and clamps at viewport - margin
        let s = apply_resize_delta(
            start,
            ResizeEdge::S,
            0.0,
            900.0,
            260.0,
            180.0,
            800.0,
            700.0,
            VP,
            16.0,
        );
        assert_eq!(s.y, 200.0);
        assert_eq!(s.y + s.height, 800.0 - 16.0);

        // NE moves top and right, pins bottom-left (300, 500)
        let ne = apply_resize_delta(
            start,
            ResizeEdge::NE,
            60.0,
            -40.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(ne.x, 300.0);
        assert_eq!(ne.y + ne.height, 500.0);
        assert_eq!((ne.width, ne.height), (460.0, 340.0));

        // NW moves top and left, pins bottom-right (700, 500)
        let nw = apply_resize_delta(
            start,
            ResizeEdge::NW,
            -50.0,
            -40.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(nw.x + nw.width, 700.0);
        assert_eq!(nw.y + nw.height, 500.0);
        assert_eq!((nw.width, nw.height), (450.0, 340.0));

        // SE moves bottom and right, pins top-left (300, 200)
        let se = apply_resize_delta(
            start,
            ResizeEdge::SE,
            80.0,
            90.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(
            (se.x, se.y, se.width, se.height),
            (300.0, 200.0, 480.0, 390.0)
        );

        // SW moves bottom and left, pins top-right (700, 200)
        let sw = apply_resize_delta(
            start,
            ResizeEdge::SW,
            -70.0,
            50.0,
            260.0,
            180.0,
            800.0,
            600.0,
            VP,
            16.0,
        );
        assert_eq!(sw.x + sw.width, 700.0);
        assert_eq!(sw.y, 200.0);
        assert_eq!((sw.width, sw.height), (470.0, 350.0));
    }

    #[test]
    fn dialog_manager_tracks_move_resize_and_center() {
        let mut mgr = DialogManager::new();
        let defaults = DialogWindowState::new(400.0, 300.0).min_size(240.0, 160.0);
        mgr.ensure("settings", defaults.clone());

        let centered = mgr.resolved_rect(&"settings", &defaults, VP);
        assert_eq!(
            centered,
            Rectangle::new(Point::new(400.0, 250.0), Size::new(400.0, 300.0))
        );

        mgr.begin_move("settings", Point::new(500.0, 265.0), VP);
        assert!(mgr.is_dragging());
        mgr.update_gesture(Point::new(560.0, 305.0), VP);
        mgr.end_gesture();
        assert!(!mgr.is_dragging());

        let moved = mgr.resolved_rect(&"settings", &defaults, VP);
        assert_eq!(moved.x, 460.0);
        assert_eq!(moved.y, 290.0);

        // Double-click center resets position while preserving size
        mgr.handle_event(
            DialogEvent::Center {
                id: "settings",
                viewport: VP,
            },
            VP,
        );
        let recentered = mgr.resolved_rect(&"settings", &defaults, VP);
        assert_eq!(recentered.x, 400.0);
        assert_eq!(recentered.y, 250.0);
    }

    #[test]
    fn begin_move_without_prior_ensure_preserves_custom_defaults() {
        let mut mgr = DialogManager::new();
        let custom = DialogWindowState::new(392.0, 480.0).position(24.0, 64.0);
        mgr.handle_event(
            DialogEvent::BeginMove {
                id: "adjust",
                defaults: custom.clone(),
                cursor: Point::new(100.0, 80.0),
                viewport: VP,
            },
            VP,
        );
        mgr.update_gesture(Point::new(140.0, 110.0), VP);
        mgr.end_gesture();
        let rect = mgr.resolved_rect(&"adjust", &custom, VP);
        assert_eq!(rect.x, 64.0);
        assert_eq!(rect.y, 94.0);
        assert_eq!(rect.width, 392.0);
        assert_eq!(rect.height, 480.0);
    }

    #[test]
    fn cursor_interactions_match_all_eight_edges() {
        assert_eq!(
            ResizeEdge::N.cursor_interaction(),
            mouse::Interaction::ResizingVertically
        );
        assert_eq!(
            ResizeEdge::S.cursor_interaction(),
            mouse::Interaction::ResizingVertically
        );
        assert_eq!(
            ResizeEdge::E.cursor_interaction(),
            mouse::Interaction::ResizingHorizontally
        );
        assert_eq!(
            ResizeEdge::W.cursor_interaction(),
            mouse::Interaction::ResizingHorizontally
        );
        assert_eq!(
            ResizeEdge::NE.cursor_interaction(),
            mouse::Interaction::ResizingDiagonallyUp
        );
        assert_eq!(
            ResizeEdge::SW.cursor_interaction(),
            mouse::Interaction::ResizingDiagonallyUp
        );
        assert_eq!(
            ResizeEdge::NW.cursor_interaction(),
            mouse::Interaction::ResizingDiagonallyDown
        );
        assert_eq!(
            ResizeEdge::SE.cursor_interaction(),
            mouse::Interaction::ResizingDiagonallyDown
        );
    }

    #[test]
    fn resolved_rect_centers_by_default_and_fits_viewport_without_shrinking_when_roomy() {
        let win = DialogWindowState::new(500.0, 560.0).preferred_height(600.0);
        assert!(win.is_centered());

        // Spacious viewport (1400x900, avail height 868 >= 600): keeps full preferred height & centers
        let roomy = win.fit_viewport(Size::new(1400.0, 900.0));
        assert_eq!(roomy.width, 500.0);
        assert_eq!(roomy.height, 600.0);
        assert_eq!(roomy.x, 450.0);
        assert_eq!(roomy.y, 150.0);

        // Constrained viewport (700x500, avail height 468 < 600): shrinks to 468 and centers with 16px margin
        let tight = win.fit_viewport(Size::new(700.0, 500.0));
        assert_eq!(tight.width, 500.0);
        assert_eq!(tight.height, 468.0);
        assert_eq!(tight.x, 100.0);
        assert_eq!(tight.y, 16.0);

        // clamp_dialog_rect also preserves preferred height when viewport permits
        let clamped_roomy = clamp_dialog_rect(
            Rectangle::new(Point::new(300.0, 100.0), Size::new(500.0, 600.0)),
            280.0,
            180.0,
            1400.0,
            1200.0,
            Size::new(1400.0, 900.0),
            16.0,
        );
        assert_eq!(clamped_roomy.height, 600.0);
        assert_eq!(clamped_roomy.y, 100.0);

        // Tiny viewport with large margin stays symmetrically centered
        let tiny = clamp_dialog_rect(
            Rectangle::new(Point::new(0.0, 0.0), Size::new(220.0, 160.0)),
            120.0,
            100.0,
            800.0,
            800.0,
            Size::new(240.0, 180.0),
            24.0,
        );
        assert_eq!(tiny.width, 192.0);
        assert_eq!(tiny.height, 132.0);
        assert_eq!(tiny.x, 24.0);
        assert_eq!(tiny.y, 24.0);
    }
}
