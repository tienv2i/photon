//! Generic sidebar panel order, drag-and-drop state, and normalization helpers.

/// Visual drop-target placement indicator for draggable sidebar panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DropIndicator {
    #[default]
    None,
    Before,
    After,
}

/// Generic state tracker for drag-reorderable sidebar panels.
///
/// Supports arbitrary panel identifier enums `P: Copy + Eq` across workspaces
/// (e.g., Browser left sidebar, Viewer right sidebar, Editor right sidebar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarOrderState<P> {
    /// Current ordered list of panels from top to bottom.
    pub order: Vec<P>,
    /// Panel currently being dragged, if any.
    pub dragging: Option<P>,
    /// Panel currently hovered as a drop target, if any.
    pub drop_target: Option<P>,
}

impl<P: Copy + Eq> Default for SidebarOrderState<P> {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            dragging: None,
            drop_target: None,
        }
    }
}

impl<P: Copy + Eq> SidebarOrderState<P> {
    /// Create a normalized [`SidebarOrderState`] from a persisted order and canonical panel list.
    pub fn new(order: Vec<P>, all: &[P]) -> Self {
        let mut state = Self {
            order,
            dragging: None,
            drop_target: None,
        };
        state.normalize_order(all);
        state
    }

    /// Create a [`SidebarOrderState`] initialized to the canonical default order `all`.
    pub fn from_defaults(all: &[P]) -> Self {
        Self::new(all.to_vec(), all)
    }

    /// Current panel order slice.
    pub fn order(&self) -> &[P] {
        &self.order
    }

    /// Currently dragged panel, if a drag gesture is active.
    pub fn dragging(&self) -> Option<P> {
        self.dragging
    }

    /// Current hovered drop-target panel, if any.
    pub fn drop_target(&self) -> Option<P> {
        self.drop_target
    }

    /// Returns `true` while any panel drag gesture is active.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    /// Returns `true` if `panel` is the panel currently being dragged.
    pub fn is_dragging_panel(&self, panel: P) -> bool {
        self.dragging == Some(panel)
    }

    /// Deduplicate, remove unknown entries not in `all`, and append any missing panels from `all`.
    pub fn normalize_order(&mut self, all: &[P]) {
        self.order = normalize_panel_order(&self.order, all);
        if let Some(src) = self.dragging
            && !all.contains(&src)
        {
            self.dragging = None;
            self.drop_target = None;
        }
        if let Some(dst) = self.drop_target
            && !all.contains(&dst)
        {
            self.drop_target = None;
        }
    }

    /// Replace `order` and normalize against `all`.
    pub fn set_order(&mut self, order: Vec<P>, all: &[P]) {
        self.order = order;
        self.normalize_order(all);
    }

    /// Start dragging `panel`, initializing `drop_target` to `panel`.
    pub fn begin_drag(&mut self, panel: P) {
        self.dragging = Some(panel);
        self.drop_target = Some(panel);
    }

    /// Record `panel` as the hovered drop target while a drag is active.
    pub fn hover_drop(&mut self, panel: P) {
        if self.dragging.is_some() {
            self.drop_target = Some(panel);
        }
    }

    /// Commit the active drag onto `target` (or the last hovered `drop_target` if `target` is `None`).
    ///
    /// Returns `true` if `self.order` changed.
    pub fn commit_drop(&mut self, target: Option<P>) -> bool {
        let Some(source) = self.dragging.take() else {
            self.drop_target = None;
            return false;
        };
        let fallback_target = self.drop_target.take();
        let Some(dst) = target.or(fallback_target) else {
            return false;
        };
        if source == dst {
            return false;
        }
        reorder_panel_slice(&mut self.order, source, dst)
    }

    /// Cancel any active drag without modifying `order`.
    pub fn cancel_drag(&mut self) {
        self.dragging = None;
        self.drop_target = None;
    }

    /// Swap two panels by index when both are valid and distinct. Returns `true` if swapped.
    pub fn swap(&mut self, a: usize, b: usize) -> bool {
        if a < self.order.len() && b < self.order.len() && a != b {
            self.order.swap(a, b);
            true
        } else {
            false
        }
    }

    /// Swap the first two panels in `order` (useful for 2-panel sidebars). Returns `true` if swapped.
    pub fn swap_pair(&mut self) -> bool {
        self.swap(0, 1)
    }

    /// Swap two panels by identifier when both exist in `order` and are distinct.
    pub fn swap_panels(&mut self, a: P, b: P) -> bool {
        if a == b {
            return false;
        }
        let Some(ia) = self.order.iter().position(|p| *p == a) else {
            return false;
        };
        let Some(ib) = self.order.iter().position(|p| *p == b) else {
            return false;
        };
        self.order.swap(ia, ib);
        true
    }

    /// Compute the [`DropIndicator`] to display around `panel` for the active drag state.
    pub fn drop_indicator(&self, panel: P) -> DropIndicator {
        drop_indicator_for(&self.order, self.dragging, self.drop_target, panel)
    }
}

/// Normalize `order` so it contains every unique entry from `all` exactly once,
/// preserving valid existing orderings first.
pub fn normalize_panel_order<P: Copy + Eq>(order: &[P], all: &[P]) -> Vec<P> {
    let mut out = Vec::with_capacity(all.len());
    for &p in order {
        if all.contains(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    for &p in all {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Move `source` to `target`'s index in `order`. Returns `true` if `order` changed.
pub fn reorder_panel_slice<P: Copy + Eq>(order: &mut Vec<P>, source: P, target: P) -> bool {
    if source == target {
        return false;
    }
    let Some(src_idx) = order.iter().position(|p| *p == source) else {
        return false;
    };
    let Some(dst_idx) = order.iter().position(|p| *p == target) else {
        return false;
    };
    if src_idx == dst_idx {
        return false;
    }
    let item = order.remove(src_idx);
    order.insert(dst_idx, item);
    true
}

/// Compute [`DropIndicator`] (`Before`, `After`, or `None`) for `panel` given current drag state.
pub fn drop_indicator_for<P: Copy + Eq>(
    order: &[P],
    dragging: Option<P>,
    target: Option<P>,
    panel: P,
) -> DropIndicator {
    let (Some(src), Some(dst)) = (dragging, target) else {
        return DropIndicator::None;
    };
    if src == panel || dst != panel {
        return DropIndicator::None;
    }
    let Some(src_idx) = order.iter().position(|p| *p == src) else {
        return DropIndicator::None;
    };
    let Some(dst_idx) = order.iter().position(|p| *p == panel) else {
        return DropIndicator::None;
    };
    if src_idx < dst_idx {
        DropIndicator::After
    } else {
        DropIndicator::Before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestPanel {
        QuickAccess,
        Explorer,
        Details,
        Unknown,
    }

    const ALL_THREE: [TestPanel; 3] = [
        TestPanel::QuickAccess,
        TestPanel::Explorer,
        TestPanel::Details,
    ];

    #[test]
    fn normalize_deduplicates_filters_unknown_and_appends_missing() {
        let mut state = SidebarOrderState::new(
            vec![
                TestPanel::Details,
                TestPanel::Unknown,
                TestPanel::Details,
                TestPanel::QuickAccess,
            ],
            &ALL_THREE,
        );
        assert_eq!(
            state.order(),
            &[
                TestPanel::Details,
                TestPanel::QuickAccess,
                TestPanel::Explorer
            ]
        );

        state.set_order(vec![], &ALL_THREE);
        assert_eq!(state.order(), &ALL_THREE);
    }

    #[test]
    fn normalize_clears_invalid_drag_or_drop_target() {
        let mut state = SidebarOrderState::from_defaults(&ALL_THREE);
        state.begin_drag(TestPanel::Unknown);
        state.normalize_order(&ALL_THREE);
        assert_eq!(state.dragging(), None);
        assert_eq!(state.drop_target(), None);

        state.begin_drag(TestPanel::QuickAccess);
        state.hover_drop(TestPanel::Unknown);
        state.normalize_order(&ALL_THREE);
        assert_eq!(state.dragging(), Some(TestPanel::QuickAccess));
        assert_eq!(state.drop_target(), None);
    }

    #[test]
    fn drag_and_drop_reorder_forward_and_backward() {
        let mut state = SidebarOrderState::from_defaults(&ALL_THREE);

        // Drag first panel onto last -> [Explorer, Details, QuickAccess]
        state.begin_drag(TestPanel::QuickAccess);
        assert!(state.is_dragging());
        assert!(state.is_dragging_panel(TestPanel::QuickAccess));
        assert_eq!(
            state.drop_indicator(TestPanel::QuickAccess),
            DropIndicator::None
        );

        state.hover_drop(TestPanel::Details);
        assert_eq!(
            state.drop_indicator(TestPanel::Details),
            DropIndicator::After
        );
        assert_eq!(
            state.drop_indicator(TestPanel::Explorer),
            DropIndicator::None
        );

        assert!(state.commit_drop(None));
        assert!(!state.is_dragging());
        assert_eq!(state.drop_target(), None);
        assert_eq!(
            state.order(),
            &[
                TestPanel::Explorer,
                TestPanel::Details,
                TestPanel::QuickAccess
            ]
        );

        // Drag last panel onto first -> [QuickAccess, Explorer, Details]
        state.begin_drag(TestPanel::QuickAccess);
        state.hover_drop(TestPanel::Explorer);
        assert_eq!(
            state.drop_indicator(TestPanel::Explorer),
            DropIndicator::Before
        );
        assert!(state.commit_drop(Some(TestPanel::Explorer)));
        assert_eq!(state.order(), &ALL_THREE);
    }

    #[test]
    fn drag_and_drop_same_panel_or_no_drag_is_noop() {
        let mut state = SidebarOrderState::from_defaults(&ALL_THREE);

        // Hover / commit when not dragging does nothing
        state.hover_drop(TestPanel::Explorer);
        assert_eq!(state.drop_target(), None);
        assert!(!state.commit_drop(Some(TestPanel::Explorer)));

        // Drop onto self does nothing and clears drag state
        state.begin_drag(TestPanel::Explorer);
        assert!(!state.commit_drop(Some(TestPanel::Explorer)));
        assert!(!state.is_dragging());
        assert_eq!(state.drop_target(), None);
        assert_eq!(state.order(), &ALL_THREE);

        // Explicit drop target overrides stale hover target
        state.begin_drag(TestPanel::QuickAccess);
        state.hover_drop(TestPanel::Explorer);
        assert!(state.commit_drop(Some(TestPanel::Details)));
        assert_eq!(
            state.order(),
            &[
                TestPanel::Explorer,
                TestPanel::Details,
                TestPanel::QuickAccess
            ]
        );
    }

    #[test]
    fn cancel_drag_clears_state_without_reordering() {
        let mut state = SidebarOrderState::from_defaults(&ALL_THREE);
        state.begin_drag(TestPanel::QuickAccess);
        state.hover_drop(TestPanel::Details);
        state.cancel_drag();

        assert!(!state.is_dragging());
        assert_eq!(state.dragging(), None);
        assert_eq!(state.drop_target(), None);
        assert_eq!(
            state.drop_indicator(TestPanel::Details),
            DropIndicator::None
        );
        assert_eq!(state.order(), &ALL_THREE);
    }

    #[test]
    fn swap_and_swap_panels_and_swap_pair() {
        let mut state = SidebarOrderState::from_defaults(&ALL_THREE);

        assert!(state.swap(0, 2));
        assert_eq!(
            state.order(),
            &[
                TestPanel::Details,
                TestPanel::Explorer,
                TestPanel::QuickAccess
            ]
        );

        // Out-of-bounds or identical indices are no-ops
        assert!(!state.swap(0, 0));
        assert!(!state.swap(0, 99));

        assert!(state.swap_pair());
        assert_eq!(
            state.order(),
            &[
                TestPanel::Explorer,
                TestPanel::Details,
                TestPanel::QuickAccess
            ]
        );

        assert!(state.swap_panels(TestPanel::Explorer, TestPanel::QuickAccess));
        assert_eq!(
            state.order(),
            &[
                TestPanel::QuickAccess,
                TestPanel::Details,
                TestPanel::Explorer
            ]
        );

        assert!(!state.swap_panels(TestPanel::QuickAccess, TestPanel::QuickAccess));
        assert!(!state.swap_panels(TestPanel::QuickAccess, TestPanel::Unknown));
    }

    #[test]
    fn single_panel_sidebar_edge_cases() {
        let only_one = [TestPanel::Explorer];
        let mut state = SidebarOrderState::from_defaults(&only_one);
        assert!(!state.swap_pair());
        state.begin_drag(TestPanel::Explorer);
        assert!(!state.commit_drop(None));
        assert_eq!(state.order(), &only_one);
    }
}
