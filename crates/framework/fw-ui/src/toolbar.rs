//! Top toolbar, segmented workspace switcher, and toolbar button-group primitives.

use iced::widget::{button as iced_button, column, container, row, rule};
use iced::{Alignment, Border, Color, Element, Length};

use crate::button::{self, ButtonVariant, TOOLBAR_CONTROL_HEIGHT};
use crate::icons::{self, Icon};
use crate::theme::{PaletteTokens, RADIUS_MD, RADIUS_SM, hairline_rule_style, surface_panel_style};
use crate::tooltip::{TooltipPosition, with_optional_tooltip};

pub struct WorkspaceTab<M> {
    pub label: String,
    pub icon: Icon,
    pub active: bool,
    pub tooltip: Option<String>,
    pub on_select: M,
}

impl<M> WorkspaceTab<M> {
    pub fn new(label: impl Into<String>, icon: Icon, active: bool, on_select: M) -> Self {
        Self {
            label: label.into(),
            icon,
            active,
            tooltip: None,
            on_select,
        }
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

/// Responsive layout mode for the workspace switcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSwitcherMode {
    Full,
    CompactIcons,
    Dropdown,
}

/// Canonical responsive breakpoints for [`WorkspaceSwitcherMode`] based on window width.
pub fn workspace_switcher_mode(window_width: f32) -> WorkspaceSwitcherMode {
    if window_width >= 1120.0 {
        WorkspaceSwitcherMode::Full
    } else if window_width >= 820.0 {
        WorkspaceSwitcherMode::CompactIcons
    } else {
        WorkspaceSwitcherMode::Dropdown
    }
}

/// Segmented Browser / Viewer / Editor workspace switcher with uniform control height
/// and optional tooltips.
pub fn workspace_switcher<'a, M: Clone + 'a>(tabs: Vec<WorkspaceTab<M>>) -> Element<'a, M> {
    workspace_switcher_compact(tabs, false)
}

/// Responsive workspace switcher supporting full segmented tabs, compact icon-only tabs,
/// and a single-button dropdown trigger.
pub fn workspace_switcher_responsive<'a, M: Clone + 'a>(
    tabs: Vec<WorkspaceTab<M>>,
    mode: WorkspaceSwitcherMode,
    menu_open: bool,
    on_toggle_menu: Option<M>,
) -> Element<'a, M> {
    match mode {
        WorkspaceSwitcherMode::Full => workspace_switcher_compact(tabs, false),
        WorkspaceSwitcherMode::CompactIcons => workspace_switcher_compact(tabs, true),
        WorkspaceSwitcherMode::Dropdown => {
            let active_tab = tabs.iter().find(|t| t.active).or_else(|| tabs.first());
            let icon_color = menu_open.then_some(Color::WHITE);
            let mut icons_row = row![].spacing(4).align_y(Alignment::Center);
            if let Some(tab) = active_tab {
                icons_row = icons_row.push(icons::render(tab.icon, 16.0, icon_color));
            }
            icons_row = icons_row.push(icons::render(Icon::ChevronDown, 12.0, icon_color));

            let content = container(icons_row)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);
            let mut b = iced_button(content)
                .height(Length::Fixed(TOOLBAR_CONTROL_HEIGHT))
                .padding([0, 8])
                .style(button::style_for(ButtonVariant::Tab { active: menu_open }));
            if let Some(msg) = on_toggle_menu {
                b = b.on_press(msg);
            }
            let tip =
                active_tab.and_then(|tab| tab.tooltip.clone().or_else(|| Some(tab.label.clone())));
            workspace_switcher_pill(with_optional_tooltip(b, tip, TooltipPosition::Bottom))
        }
    }
}

/// Segmented Browser / Viewer / Editor workspace switcher with optional compact icon-only mode.
///
/// When `compact == true`, each tab hides its text label (`padding: [0, 8]`,
/// `TOOLBAR_CONTROL_HEIGHT`) while preserving its localized tooltip.
pub fn workspace_switcher_compact<'a, M: Clone + 'a>(
    tabs: Vec<WorkspaceTab<M>>,
    compact: bool,
) -> Element<'a, M> {
    let mut r = row![].spacing(4).align_y(Alignment::Center);
    for tab in tabs {
        let variant = ButtonVariant::Tab { active: tab.active };
        let on_select = (!tab.active).then_some(tab.on_select);
        if compact {
            let icon_color = tab.active.then_some(Color::WHITE);
            let content = container(icons::render(tab.icon, 16.0, icon_color))
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center);
            let mut b = iced_button(content)
                .height(Length::Fixed(TOOLBAR_CONTROL_HEIGHT))
                .padding([0, 8])
                .style(button::style_for(variant));
            if let Some(msg) = on_select {
                b = b.on_press(msg);
            }
            let tip = tab.tooltip.or(Some(tab.label));
            r = r.push(with_optional_tooltip(b, tip, TooltipPosition::Bottom));
        } else {
            r = r.push(button::toolbar_labeled(
                tab.label,
                Some(tab.icon),
                variant,
                on_select,
                tab.tooltip,
            ));
        }
    }
    workspace_switcher_pill(r.into())
}

fn workspace_switcher_pill<'a, M: 'a>(content: Element<'a, M>) -> Element<'a, M> {
    container(content)
        .padding(3)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: RADIUS_MD.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

/// Compact pill wrapper for a segmented group of toolbar icon buttons.
pub fn toolbar_button_group<'a, M: 'a>(items: Vec<Element<'a, M>>) -> Element<'a, M> {
    let mut r = row![].spacing(2).align_y(Alignment::Center);
    for item in items {
        r = r.push(item);
    }
    container(r)
        .padding(2)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                border: Border {
                    color: t.border,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        })
        .into()
}

pub fn toolbar<'a, M: 'a>(
    left: impl Into<Element<'a, M>>,
    right: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    column![
        container(
            row![left.into(), iced::widget::space::horizontal(), right.into(),]
                .spacing(12)
                .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .style(surface_panel_style),
        rule::horizontal(1).style(hairline_rule_style),
    ]
    .into()
}

pub fn toolbar_split<'a, M: 'a>(
    left: impl Into<Element<'a, M>>,
    center: impl Into<Element<'a, M>>,
    right: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    column![
        container(
            row![
                left.into(),
                iced::widget::space::horizontal(),
                center.into(),
                iced::widget::space::horizontal(),
                right.into(),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(Length::Fill)
        .style(surface_panel_style),
        rule::horizontal(1).style(hairline_rule_style),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_switcher_builds_regular_and_compact_variants() {
        let make_tabs = || {
            vec![
                WorkspaceTab::new("Browser", Icon::Browser, true, 1u8)
                    .tooltip("Browser Workspace (Ctrl+1)"),
                WorkspaceTab::new("Viewer", Icon::Viewer, false, 2u8)
                    .tooltip("Viewer Workspace (Ctrl+2)"),
                WorkspaceTab::new("Editor", Icon::Editor, false, 3u8),
            ]
        };

        let _legacy: Element<'_, u8> = workspace_switcher(make_tabs());
        let _full: Element<'_, u8> = workspace_switcher_compact(make_tabs(), false);
        let _compact: Element<'_, u8> = workspace_switcher_compact(make_tabs(), true);
    }

    #[test]
    fn workspace_switcher_responsive_builds_all_modes() {
        let make_tabs = || {
            vec![
                WorkspaceTab::new("Browser", Icon::Browser, true, 1u8)
                    .tooltip("Browser Workspace (Ctrl+1)"),
                WorkspaceTab::new("Viewer", Icon::Viewer, false, 2u8)
                    .tooltip("Viewer Workspace (Ctrl+2)"),
                WorkspaceTab::new("Editor", Icon::Editor, false, 3u8),
            ]
        };

        let _full: Element<'_, u8> = workspace_switcher_responsive(
            make_tabs(),
            WorkspaceSwitcherMode::Full,
            false,
            Some(99),
        );
        let _compact: Element<'_, u8> = workspace_switcher_responsive(
            make_tabs(),
            WorkspaceSwitcherMode::CompactIcons,
            false,
            Some(99),
        );
        let _dropdown_closed: Element<'_, u8> = workspace_switcher_responsive(
            make_tabs(),
            WorkspaceSwitcherMode::Dropdown,
            false,
            Some(99),
        );
        let _dropdown_open: Element<'_, u8> =
            workspace_switcher_responsive(make_tabs(), WorkspaceSwitcherMode::Dropdown, true, None);
        let _dropdown_empty: Element<'_, u8> = workspace_switcher_responsive(
            Vec::<WorkspaceTab<u8>>::new(),
            WorkspaceSwitcherMode::Dropdown,
            false,
            Some(99),
        );
    }
}
