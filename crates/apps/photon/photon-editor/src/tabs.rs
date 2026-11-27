//! Editor document tab strip and right-click tab context menu
//! (`Close`, `Close Others`, `Close to Left`, `Close to Right`, `Duplicate Image`).

use fw_ui::button::{self as ui_button, ButtonVariant};
use fw_ui::icons::{self, Icon};
use fw_ui::menu::{MenuEntry, MenuItem, estimate_menu_height_with_spacing, menu_panel_entries};
use fw_ui::theme::{PaletteTokens, RADIUS_SM};
use fw_ui::tooltip::{TooltipPosition, with_optional_tooltip, with_tooltip_pos};
use fw_ui::typography::text;
use iced::widget::{button, container, mouse_area, row, scrollable};
use iced::{Alignment, Border, Color, Element, Length};
use photon_i18n::tr;
use photon_protocol::{EditorTabSummary, Language};

use crate::{EditorMessage, EditorViewState};

/// Target scope for closing one or more Editor document tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabCloseScope {
    Single(u64),
    Others(u64),
    ToLeft(u64),
    ToRight(u64),
}

/// Compute the ordered list of tab IDs affected by [`TabCloseScope`].
#[must_use]
pub fn target_tab_ids(tabs: &[EditorTabSummary], scope: TabCloseScope) -> Vec<u64> {
    match scope {
        TabCloseScope::Single(id) => tabs.iter().filter(|t| t.id == id).map(|t| t.id).collect(),
        TabCloseScope::Others(id) => {
            if !tabs.iter().any(|t| t.id == id) {
                return Vec::new();
            }
            tabs.iter().filter(|t| t.id != id).map(|t| t.id).collect()
        }
        TabCloseScope::ToLeft(id) => {
            let Some(pos) = tabs.iter().position(|t| t.id == id) else {
                return Vec::new();
            };
            tabs[..pos].iter().map(|t| t.id).collect()
        }
        TabCloseScope::ToRight(id) => {
            let Some(pos) = tabs.iter().position(|t| t.id == id) else {
                return Vec::new();
            };
            tabs[pos + 1..].iter().map(|t| t.id).collect()
        }
    }
}

/// Whether `Close Others` is enabled for `tab_id`.
#[must_use]
pub fn can_close_others(tabs: &[EditorTabSummary], tab_id: u64) -> bool {
    !target_tab_ids(tabs, TabCloseScope::Others(tab_id)).is_empty()
}

/// Whether `Close to Left` is enabled for `tab_id`.
#[must_use]
pub fn can_close_left(tabs: &[EditorTabSummary], tab_id: u64) -> bool {
    !target_tab_ids(tabs, TabCloseScope::ToLeft(tab_id)).is_empty()
}

/// Whether `Close to Right` is enabled for `tab_id`.
#[must_use]
pub fn can_close_right(tabs: &[EditorTabSummary], tab_id: u64) -> bool {
    !target_tab_ids(tabs, TabCloseScope::ToRight(tab_id)).is_empty()
}

/// Render the Editor document tab strip with right-click context menu support on each tab.
pub fn build_editor_tab_bar<'a>(
    state: EditorViewState<'a>,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let mut tab_row = row![].spacing(4).align_y(Alignment::Center);

    for tab in &state.snapshot.meta.editor_tabs {
        let active = tab.active;
        let tip = format!("{} ({}×{})", tab.path, tab.width, tab.height);
        let mut label_row = row![].spacing(5).align_y(Alignment::Center);
        if tab.modified {
            label_row = label_row.push(text("•").size(12).style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.accent),
                }
            }));
        }
        label_row = label_row.push(
            text(tab.name.clone())
                .size(12)
                .wrapping(text::Wrapping::None),
        );

        let select_btn = button(
            container(label_row)
                .width(Length::Shrink)
                .max_width(168)
                .height(Length::Fill)
                .clip(true)
                .align_y(Alignment::Center),
        )
        .width(Length::Shrink)
        .height(Length::Fill)
        .padding([0, 10])
        .style(move |theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let bg = match status {
                button::Status::Hovered if !active => t.bg_hover,
                button::Status::Pressed if !active => t.bg_pressed,
                _ => Color::TRANSPARENT,
            };
            let text_color = if active || matches!(status, button::Status::Hovered) {
                t.text_primary
            } else {
                t.text_secondary
            };
            button::Style {
                background: Some(bg.into()),
                text_color,
                border: Border::default(),
                ..Default::default()
            }
        })
        .on_press(EditorMessage::SwitchTab(tab.id));

        let close_btn = button(
            container(icons::render(Icon::Close, 11.0, None))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(20.0))
        .padding(0)
        .style(ui_button::style_for(ButtonVariant::Ghost))
        .on_press(EditorMessage::RequestCloseTab(tab.id));

        let pill = container(
            row![select_btn, close_btn]
                .spacing(2)
                .align_y(Alignment::Center)
                .padding([0, 4]),
        )
        .width(Length::Shrink)
        .max_width(220)
        .height(Length::Fixed(28.0))
        .style(move |theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(if active { t.bg_selected } else { t.bg_elevated }.into()),
                border: Border {
                    color: if active { t.accent } else { t.border },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                ..Default::default()
            }
        });

        let interactive_pill =
            mouse_area(pill).on_right_press(EditorMessage::OpenTabContextMenu(tab.id));

        tab_row = tab_row.push(with_optional_tooltip(
            interactive_pill,
            Some(tip),
            TooltipPosition::Bottom,
        ));
    }

    let scroll = scrollable(tab_row)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new().width(4).scroller_width(4),
        ))
        .width(Length::Fill);

    let new_btn = with_tooltip_pos(
        ui_button::compact_icon(
            Icon::Plus,
            ButtonVariant::Ghost,
            Some(EditorMessage::OpenNewTabDialog),
        ),
        tr(lang, "tooltip-editor-new-tab"),
        TooltipPosition::Bottom,
    );

    let bar = row![scroll, new_btn].spacing(6).align_y(Alignment::Center);

    container(bar)
        .padding([4, 8])
        .width(Length::Fill)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            container::Style {
                background: Some(t.bg_app.into()),
                ..Default::default()
            }
        })
        .into()
}

/// Build the right-click context menu for an Editor document tab.
pub fn build_editor_tab_context_menu<'a>(
    state: EditorViewState<'a>,
    tab_id: u64,
) -> Option<(f32, Element<'a, EditorMessage>)> {
    let tabs = &state.snapshot.meta.editor_tabs;
    if !tabs.iter().any(|t| t.id == tab_id) {
        return None;
    }
    let lang = state.snapshot.meta.language;
    let others = target_tab_ids(tabs, TabCloseScope::Others(tab_id));
    let left = target_tab_ids(tabs, TabCloseScope::ToLeft(tab_id));
    let right = target_tab_ids(tabs, TabCloseScope::ToRight(tab_id));

    let entries = vec![
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-duplicate-image"),
                EditorMessage::DuplicateDocument(Some(tab_id)),
            )
            .icon(Icon::Duplicate),
        ),
        MenuEntry::Separator,
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-tab-close"),
                EditorMessage::RequestCloseTab(tab_id),
            )
            .icon(Icon::Close)
            .shortcut("Ctrl+W"),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-tab-close-others"),
                EditorMessage::RequestCloseTabs(others.clone()),
            )
            .enabled(!others.is_empty()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-tab-close-left"),
                EditorMessage::RequestCloseTabs(left.clone()),
            )
            .enabled(!left.is_empty()),
        ),
        MenuEntry::Item(
            MenuItem::new(
                tr(lang, "action-tab-close-right"),
                EditorMessage::RequestCloseTabs(right.clone()),
            )
            .enabled(!right.is_empty()),
        ),
    ];

    let est_h = estimate_menu_height_with_spacing(&entries);
    Some((est_h, menu_panel_entries(entries, 210.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tabs() -> Vec<EditorTabSummary> {
        (1..=4)
            .map(|id| EditorTabSummary {
                id,
                path: format!("/tmp/img{id}.png"),
                name: format!("img{id}.png"),
                width: 100,
                height: 100,
                modified: false,
                active: id == 2,
            })
            .collect()
    }

    #[test]
    fn target_tab_ids_computes_single_others_left_right() {
        let tabs = sample_tabs();
        assert_eq!(target_tab_ids(&tabs, TabCloseScope::Single(2)), vec![2]);
        assert_eq!(
            target_tab_ids(&tabs, TabCloseScope::Others(2)),
            vec![1, 3, 4]
        );
        assert_eq!(target_tab_ids(&tabs, TabCloseScope::ToLeft(2)), vec![1]);
        assert_eq!(target_tab_ids(&tabs, TabCloseScope::ToRight(2)), vec![3, 4]);
    }

    #[test]
    fn edge_tabs_disable_empty_left_or_right_close() {
        let tabs = sample_tabs();
        assert!(!can_close_left(&tabs, 1));
        assert!(can_close_right(&tabs, 1));
        assert!(can_close_left(&tabs, 4));
        assert!(!can_close_right(&tabs, 4));
        assert!(can_close_others(&tabs, 1));

        let single = vec![tabs[0].clone()];
        assert!(!can_close_others(&single, 1));
        assert!(!can_close_left(&single, 1));
        assert!(!can_close_right(&single, 1));
    }
}
