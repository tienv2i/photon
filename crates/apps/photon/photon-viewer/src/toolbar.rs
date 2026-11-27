use fw_ui::button::{self, ButtonVariant, TOOLBAR_CONTROL_HEIGHT};
use fw_ui::icons::Icon;
use fw_ui::theme::{TEXT_SIZE_CONTROL, hairline_rule_style};
use fw_ui::toolbar::{
    WorkspaceSwitcherMode, WorkspaceTab, toolbar, toolbar_button_group,
    workspace_switcher_responsive,
};
use fw_ui::tooltip::{TooltipPosition, with_tooltip_pos};
use fw_ui::typography::text;
use iced::widget::{button as iced_button, container, row, rule};
use iced::{Alignment, Element, Length};
use photon_canvas::CanvasEvent;
use photon_i18n::tr;
use photon_protocol::{Command, ViewerViewMode, Workspace};

use crate::types::{ViewerMessage, ViewerViewState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerToolbarSwitcherKind {
    MainThreeTab(WorkspaceSwitcherMode),
    StandaloneTwoButton { show_labels: bool },
}

pub fn viewer_toolbar_switcher_kind(
    _is_main_window: bool,
    window_width: f32,
) -> ViewerToolbarSwitcherKind {
    ViewerToolbarSwitcherKind::StandaloneTwoButton {
        show_labels: window_width >= 1120.0,
    }
}

/// Width below which the four Viewer utility toggles would collapse.
/// Kept for API compatibility; utility toggles stay expanded on the toolbar.
pub const VIEWER_UTILITY_COLLAPSE_WIDTH: f32 = 720.0;

pub fn viewer_utility_collapsed(_window_width: f32) -> bool {
    false
}

/// Right-edge offset (from the window's right edge) of the collapsed utility
/// button, used to anchor its dropdown menu underneath it.
pub fn viewer_utilities_menu_right_offset(window_width: f32) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels button
    let switcher_w = match viewer_toolbar_switcher_kind(false, window_width) {
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Full) => 252.0,
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::CompactIcons) => 112.0,
        ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Dropdown) => 56.0,
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true } => 164.0,
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false } => 72.0,
    };
    let view_mode_w = if window_width >= 1120.0 {
        138.0 // Single / Double / Manhwa + Reader Options
    } else {
        36.0 // Reader Options only
    };
    // utilities + spacing + divider + spacing + switcher + spacing + view mode + spacing
    utilities_w + 17.0 + switcher_w + 8.0 + view_mode_w + 8.0
}

pub fn viewer_reader_options_right_offset(state: ViewerViewState<'_>) -> f32 {
    let utilities_w = 12.0 + 32.0; // right padding + single Panels&Settings button
    let switcher_w =
        match viewer_toolbar_switcher_kind(state.is_main_window, state.window_size.width) {
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Full) => 252.0,
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::CompactIcons) => 112.0,
            ViewerToolbarSwitcherKind::MainThreeTab(WorkspaceSwitcherMode::Dropdown) => 56.0,
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true } => 164.0,
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false } => 72.0,
        };
    // utilities_w + spacing(8) + divider(9) + switcher_w + spacing(8)
    utilities_w + 17.0 + switcher_w + 8.0
}

/// Top toolbar for the Viewer workspace:
/// Left: Navigation + Zoom/Fit/1:1 + Rotate + Fullscreen.
/// Right: View Mode Switcher (`Single`/`Double`/`Manhwa`) + Reader Options + Workspace Switcher + Panels & Settings.
pub fn build_viewer_toolbar<'a>(
    state: ViewerViewState<'a>,
    right_utilities: Element<'a, ViewerMessage>,
) -> Element<'a, ViewerMessage> {
    let lang = state.snapshot.meta.language;
    let meta = &state.snapshot.meta;
    let has_nav = meta.viewer_count > 1;
    let has_image = meta.viewer_path.is_some();
    let viewer_mode = state.viewer_mode;
    let is_webtoon = viewer_mode == ViewerViewMode::Webtoon;
    let is_double = viewer_mode == ViewerViewMode::Double;

    // Navigation (Prev / Next)
    let prev_msg = has_nav.then_some({
        if is_double {
            ViewerMessage::DualStepSpread(-1)
        } else {
            ViewerMessage::Dispatch(Command::ViewerPrevious)
        }
    });
    let next_msg = has_nav.then_some({
        if is_double {
            ViewerMessage::DualStepSpread(1)
        } else {
            ViewerMessage::Dispatch(Command::ViewerNext)
        }
    });

    let nav_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::Prev,
            false,
            prev_msg,
            Some(tr(lang, "tooltip-viewer-prev")),
        ),
        button::toolbar_icon(
            Icon::Next,
            false,
            next_msg,
            Some(tr(lang, "tooltip-viewer-next")),
        ),
    ]);

    let mut actual_btn = iced_button(
        container(text(tr(lang, "action-actual-ratio")).size(TEXT_SIZE_CONTROL))
            .height(Length::Fill)
            .align_y(Alignment::Center),
    )
    .height(Length::Fixed(TOOLBAR_CONTROL_HEIGHT))
    .padding([0, 6])
    .style(button::style_for(ButtonVariant::Ghost));
    if let Some(msg) = (has_image && !is_webtoon).then_some(ViewerMessage::ViewerZoomActual) {
        actual_btn = actual_btn.on_press(msg);
    }
    let actual_ratio_btn = with_tooltip_pos(
        actual_btn,
        tr(lang, "tooltip-viewer-actual-size"),
        TooltipPosition::Bottom,
    );

    let zoom_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::ZoomOut,
            false,
            has_image.then_some(ViewerMessage::ZoomStep(0.8)),
            Some(tr(lang, "tooltip-viewer-zoom-out")),
        ),
        button::toolbar_icon(
            Icon::ZoomIn,
            false,
            has_image.then_some(ViewerMessage::ZoomStep(1.25)),
            Some(tr(lang, "tooltip-viewer-zoom-in")),
        ),
        button::toolbar_icon(
            Icon::Fit,
            false,
            has_image.then_some(ViewerMessage::Canvas(CanvasEvent::ResetFit)),
            Some(tr(lang, "tooltip-viewer-fit")),
        ),
        actual_ratio_btn,
    ]);

    let mut left = row![
        button::toolbar_icon(
            Icon::FolderOpen,
            false,
            Some(ViewerMessage::PickImagesDialog),
            Some(tr(lang, "action-open-file-dialog")),
        ),
        nav_group,
        zoom_group,
    ]
    .spacing(3)
    .align_y(Alignment::Center);

    if !is_webtoon {
        let rotate_group = toolbar_button_group(vec![
            button::toolbar_icon(
                Icon::RotateLeft,
                false,
                has_image.then_some(ViewerMessage::ViewerRotatePreview(-1)),
                Some(tr(lang, "tooltip-viewer-rotate-left")),
            ),
            button::toolbar_icon(
                Icon::RotateRight,
                false,
                has_image.then_some(ViewerMessage::ViewerRotatePreview(1)),
                Some(tr(lang, "tooltip-viewer-rotate-right")),
            ),
        ]);
        left = left.push(rotate_group);
    }

    // Right side: View Mode Switcher (Single | Double | Webtoon | Reader Options) + Workspace Switcher + Panels & Settings
    let reader_options_btn = button::toolbar_icon(
        Icon::BookOpen,
        state.reader_options_open,
        Some(ViewerMessage::ToggleReaderOptions),
        Some(tr(lang, "action-reader-options")),
    );
    let view_mode_group = if state.window_size.width >= 1120.0 {
        toolbar_button_group(vec![
            button::toolbar_icon(
                Icon::ViewSingle,
                viewer_mode == ViewerViewMode::Single,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Single)),
                Some(format!("{} (S)", tr(lang, "action-viewer-single"))),
            ),
            button::toolbar_icon(
                Icon::ViewDouble,
                viewer_mode == ViewerViewMode::Double,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Double)),
                Some(format!("{} (D)", tr(lang, "action-viewer-double"))),
            ),
            button::toolbar_icon(
                Icon::ViewManhwa,
                viewer_mode == ViewerViewMode::Webtoon,
                Some(ViewerMessage::SetViewerViewMode(ViewerViewMode::Webtoon)),
                Some(format!("{} (M)", tr(lang, "action-viewer-manhwa"))),
            ),
            reader_options_btn,
        ])
    } else {
        toolbar_button_group(vec![reader_options_btn])
    };

    let switcher = match viewer_toolbar_switcher_kind(state.is_main_window, state.window_size.width)
    {
        ViewerToolbarSwitcherKind::MainThreeTab(mode) => workspace_switcher_responsive(
            vec![
                WorkspaceTab::new(
                    tr(lang, "workspace-browser"),
                    Icon::Browser,
                    false,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Browser,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-browser")),
                WorkspaceTab::new(
                    tr(lang, "workspace-viewer"),
                    Icon::Viewer,
                    true,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Viewer,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-viewer")),
                WorkspaceTab::new(
                    tr(lang, "workspace-editor"),
                    Icon::Editor,
                    false,
                    ViewerMessage::Dispatch(Command::SetWorkspace {
                        workspace: Workspace::Editor,
                    }),
                )
                .tooltip(tr(lang, "tooltip-workspace-editor")),
            ],
            mode,
            state.workspace_switcher_open,
            Some(ViewerMessage::ToggleWorkspaceSwitcher),
        ),
        ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels } => {
            let browser_msg = Some(ViewerMessage::OpenBrowserWindow);
            let editor_msg = has_image.then_some(ViewerMessage::ViewerAdjOpenInEditor);
            let browser_tip = Some(tr(lang, "tooltip-workspace-browser"));
            let editor_tip = Some(tr(lang, "tooltip-workspace-editor"));
            if show_labels {
                toolbar_button_group(vec![
                    button::toolbar_labeled(
                        tr(lang, "workspace-browser"),
                        Some(Icon::Browser),
                        ButtonVariant::Ghost,
                        browser_msg,
                        browser_tip,
                    ),
                    button::toolbar_labeled(
                        tr(lang, "workspace-editor"),
                        Some(Icon::Editor),
                        ButtonVariant::Ghost,
                        editor_msg,
                        editor_tip,
                    ),
                ])
            } else {
                toolbar_button_group(vec![
                    button::toolbar_icon(Icon::Browser, false, browser_msg, browser_tip),
                    button::toolbar_icon(Icon::Editor, false, editor_msg, editor_tip),
                ])
            }
        }
    };
    // Utility toggles: Filmstrip → Fullscreen → View Adjustments → Info (always expanded).
    let utility_group = toolbar_button_group(vec![
        button::toolbar_icon(
            Icon::Filmstrip,
            state.viewer_filmstrip_open,
            Some(ViewerMessage::ToggleFilmstrip),
            Some(tr(lang, "tooltip-viewer-filmstrip")),
        ),
        button::toolbar_icon(
            if state.is_fullscreen {
                Icon::FullscreenExit
            } else {
                Icon::Fullscreen
            },
            state.is_fullscreen,
            Some(ViewerMessage::ToggleFullscreen),
            Some(tr(lang, "tooltip-viewer-fullscreen")),
        ),
        button::toolbar_icon(
            Icon::Palette,
            state.viewer_adjustments_open,
            Some(ViewerMessage::ToggleViewerColorPopover),
            Some(tr(lang, "panel-view-adjustments")),
        ),
        button::toolbar_icon(
            Icon::Info,
            state.viewer_info_open,
            Some(ViewerMessage::ToggleViewerInspector),
            Some(tr(lang, "action-info")),
        ),
    ]);

    let right: Element<'a, ViewerMessage> = row![
        utility_group,
        view_mode_group,
        switcher,
        container(rule::vertical(1).style(hairline_rule_style)).height(Length::Fixed(18.0)),
        right_utilities
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into();

    toolbar(left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_vs_main_toolbar_switcher_uses_two_button_pill_for_standalone() {
        assert_eq!(
            viewer_toolbar_switcher_kind(true, 1280.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(true, 900.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(false, 1120.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: true }
        );
        assert_eq!(
            viewer_toolbar_switcher_kind(false, 1119.0),
            ViewerToolbarSwitcherKind::StandaloneTwoButton { show_labels: false }
        );
    }

    #[test]
    fn utility_toggles_stay_expanded_at_all_widths() {
        assert!(!viewer_utility_collapsed(VIEWER_UTILITY_COLLAPSE_WIDTH));
        assert!(!viewer_utility_collapsed(1280.0));
        assert!(!viewer_utility_collapsed(
            VIEWER_UTILITY_COLLAPSE_WIDTH - 1.0
        ));
        assert!(!viewer_utility_collapsed(680.0));
    }

    #[test]
    fn collapsed_utilities_menu_anchors_left_of_view_mode_group() {
        // At narrow widths: utilities(44) + divider gap(17) + switcher(72)
        // + spacing(8) + reader options(36) + spacing(8).
        assert_eq!(viewer_utilities_menu_right_offset(680.0), 185.0);
        assert!(viewer_utilities_menu_right_offset(680.0) > 0.0);
    }
}
