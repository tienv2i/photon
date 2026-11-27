//! Standalone showcase gallery for `photon-ui` components (`cargo run -p photon-ui --example gallery`).

use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Fill, Theme};
use photon_ui::button::{self, ButtonVariant, IconButton};
use photon_ui::dialog::{DialogSpec, modal_dialog};
use photon_ui::editor::{LayerActionButton, LayerRowSpec, layer_action_bar, layer_row};
use photon_ui::icons::Icon;
use photon_ui::menu::{MenuItem, menu_panel, with_top_right_menu};
use photon_ui::panel::{
    DraggablePanelHeaderSpec, DropIndicator, PanelTabItem, dock_panel, draggable_panel_header,
    kv_row, panel_tab_bar, section_card, status_bar,
};
use photon_ui::theme::app_shell_style;
use photon_ui::toolbar::{WorkspaceTab, toolbar, workspace_switcher};
use photon_ui::tooltip::{TooltipPosition, with_tooltip};

#[derive(Default)]
struct Gallery {
    dark: bool,
    active_tab: usize,
    inspector_tab: usize,
    layers_collapsed: bool,
    menu_open: bool,
    dialog_open: bool,
    last_action: String,
}

#[derive(Debug, Clone)]
enum Msg {
    SelectTab(usize),
    SelectInspectorTab(usize),
    ToggleLayersCollapse,
    ToggleTheme,
    ToggleMenu,
    CloseMenu,
    OpenDialog,
    CloseDialog,
    Noop,
    Action(&'static str),
}

fn update(state: &mut Gallery, msg: Msg) {
    match msg {
        Msg::SelectTab(i) => state.active_tab = i,
        Msg::SelectInspectorTab(i) => state.inspector_tab = i,
        Msg::ToggleLayersCollapse => state.layers_collapsed = !state.layers_collapsed,
        Msg::ToggleTheme => state.dark = !state.dark,
        Msg::ToggleMenu => state.menu_open = !state.menu_open,
        Msg::CloseMenu => state.menu_open = false,
        Msg::OpenDialog => state.dialog_open = true,
        Msg::CloseDialog => state.dialog_open = false,
        Msg::Noop => {}
        Msg::Action(name) => {
            state.menu_open = false;
            state.last_action = name.to_owned();
        }
    }
}

fn view(state: &Gallery) -> Element<'_, Msg> {
    let switcher = workspace_switcher(vec![
        WorkspaceTab::new(
            "Browser",
            Icon::Browser,
            state.active_tab == 0,
            Msg::SelectTab(0),
        )
        .tooltip("Browser workspace (Ctrl+1)"),
        WorkspaceTab::new(
            "Viewer",
            Icon::Viewer,
            state.active_tab == 1,
            Msg::SelectTab(1),
        )
        .tooltip("Viewer workspace (Ctrl+2)"),
        WorkspaceTab::new(
            "Editor",
            Icon::Editor,
            state.active_tab == 2,
            Msg::SelectTab(2),
        )
        .tooltip("Editor workspace (Ctrl+3)"),
    ]);

    let right_bar = row![
        with_tooltip(
            button::toolbar_labeled(
                "Open Dialog",
                Some(Icon::Settings),
                ButtonVariant::Secondary,
                Some(Msg::OpenDialog),
                None,
            ),
            "Open modal dialog preview",
        ),
        IconButton::new(if state.dark { Icon::Sun } else { Icon::Moon })
            .toolbar()
            .tooltip("Toggle light/dark theme")
            .tooltip_position(TooltipPosition::Bottom)
            .on_press(Msg::ToggleTheme)
            .into_element(),
        button::toolbar_labeled(
            "Menu",
            Some(Icon::Globe),
            ButtonVariant::Secondary,
            Some(Msg::ToggleMenu),
            Some("Language options".into()),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let inspector_tabs = panel_tab_bar(
        vec![
            PanelTabItem::new(
                "options",
                "Tool Options",
                state.inspector_tab == 0,
                Msg::SelectInspectorTab(0),
            )
            .icon(Icon::Sliders)
            .tooltip("Active tool options"),
            PanelTabItem::new(
                "history",
                "History",
                state.inspector_tab == 1,
                Msg::SelectInspectorTab(1),
            )
            .icon(Icon::History)
            .tooltip("Edit history"),
        ],
        false,
    );

    let layer_header = draggable_panel_header(DraggablePanelHeaderSpec {
        title: "Layers".into(),
        icon: Some(Icon::Layers),
        badge: Some("2".into()),
        collapsed: state.layers_collapsed,
        dragging: false,
        drop_indicator: DropIndicator::None,
        drag_tooltip: Some("Drag to reorder panel".into()),
        collapse_tooltip: Some("Collapse/expand Layers".into()),
        on_toggle_collapse: Msg::ToggleLayersCollapse,
        on_drag_start: Msg::Action("DragStart"),
        on_drag_hover: None,
        on_drag_end: None,
    });

    let layers_demo = column![
        layer_header,
        layer_row(LayerRowSpec {
            name: "Foreground".into(),
            subtitle: Some("1920×1080 · Normal 100%".into()),
            kind_badge: None,
            active: true,
            visible: true,
            locked: false,
            clipped_to_below: false,
            lock_alpha: false,
            has_mask: true,
            mask_enabled: true,
            mask_linked: true,
            editing_mask: false,
            thumbnail: None,
            mask_thumbnail: None,
            renaming: None,
            on_select: Msg::Action("SelectLayer1"),
            on_toggle_visible: Msg::Action("ToggleVis1"),
            on_toggle_lock: Msg::Action("ToggleLock1"),
            on_toggle_mask_edit: Some(Msg::Action("MaskEdit1")),
            on_toggle_mask_link: Some(Msg::Action("MaskLink1")),
            on_toggle_mask: Some(Msg::Action("MaskDelete1")),
            on_start_rename: Some(Msg::Action("Rename1")),
            on_rename_input: None,
            on_rename_submit: None,
            on_right_press: None,
            visibility_tooltip: Some("Toggle visibility".into()),
            lock_tooltip: Some("Toggle lock".into()),
            mask_tooltip: Some("Edit layer mask".into()),
            mask_link_tooltip: Some("Toggle mask link".into()),
            mask_toggle_tooltip: Some("Add/remove mask".into()),
            rename_tooltip: Some("Rename layer".into()),
        }),
        layer_action_bar(vec![
            LayerActionButton::new(Icon::Plus, Some(Msg::Action("LayerAdd")))
                .tooltip("Add empty layer"),
            LayerActionButton::new(Icon::Duplicate, Some(Msg::Action("LayerDup")))
                .tooltip("Duplicate layer"),
            LayerActionButton::new(Icon::Trash, None).tooltip("Delete layer"),
        ]),
    ]
    .spacing(6);

    let center = container(
        column![
            section_card(
                "Buttons",
                row![
                    button::labeled(
                        "Primary",
                        Some(Icon::Save),
                        ButtonVariant::Primary,
                        Some(Msg::Action("Primary"))
                    ),
                    button::labeled(
                        "Secondary",
                        Some(Icon::Refresh),
                        ButtonVariant::Secondary,
                        Some(Msg::Action("Secondary"))
                    ),
                    button::labeled(
                        "Ghost",
                        Some(Icon::Info),
                        ButtonVariant::Ghost,
                        Some(Msg::Action("Ghost"))
                    ),
                    button::labeled(
                        "Danger",
                        Some(Icon::Reset),
                        ButtonVariant::Danger,
                        Some(Msg::Action("Danger"))
                    ),
                ]
                .spacing(8)
            ),
            section_card(
                "Editor Right-Sidebar Primitives",
                column![inspector_tabs, layers_demo].spacing(10)
            ),
            section_card(
                "Inspector Rows",
                column![
                    kv_row(
                        "Last Action",
                        if state.last_action.is_empty() {
                            "None".into()
                        } else {
                            state.last_action.clone()
                        }
                    ),
                    kv_row("Theme", if state.dark { "Dark" } else { "Light" }),
                ]
                .spacing(6)
            ),
        ]
        .spacing(12)
        .padding(16),
    )
    .width(Fill)
    .height(Fill);

    let sidebar = dock_panel(
        "Sidebar",
        Some(Icon::Folder),
        220.0,
        column![
            button::list_row(
                Icon::FolderOpen,
                "Pictures",
                Some("128".into()),
                true,
                Some(Msg::Action("Pictures"))
            ),
            button::list_row(
                Icon::Image,
                "sample.png",
                Some("2.4 MB".into()),
                false,
                Some(Msg::Action("sample.png"))
            ),
        ]
        .spacing(4),
    );

    let shell = container(
        column![
            toolbar(switcher, right_bar),
            row![sidebar, center].height(Fill),
            status_bar("Gallery Ready", "photon-ui v0.1.0"),
        ]
        .height(Fill),
    )
    .width(Fill)
    .height(Fill)
    .style(app_shell_style);

    let with_menu = with_top_right_menu(
        shell,
        state.menu_open,
        menu_panel(
            vec![
                MenuItem::new("English", Msg::Action("English"))
                    .icon(Icon::Globe)
                    .checked(true),
                MenuItem::new("Tiếng Việt", Msg::Action("Tiếng Việt")).icon(Icon::Globe),
            ],
            190.0,
        ),
        Msg::CloseMenu,
    );

    modal_dialog(
        with_menu,
        DialogSpec {
            open: state.dialog_open,
            title: "Settings".into(),
            width: 420.0,
            body: text("Shared DialogFrame from photon-ui").size(13).into(),
            footer: row![
                iced::widget::space::horizontal(),
                button::labeled(
                    "Cancel",
                    None,
                    ButtonVariant::Secondary,
                    Some(Msg::CloseDialog)
                ),
                button::labeled(
                    "Confirm",
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(Msg::CloseDialog)
                ),
            ]
            .spacing(8)
            .into(),
            on_close: Msg::CloseDialog,
            on_noop: Msg::Noop,
        },
    )
}

fn main() -> iced::Result {
    iced::application(
        || {
            (
                Gallery {
                    dark: true,
                    ..Default::default()
                },
                iced::Task::none(),
            )
        },
        update,
        view,
    )
    .theme(|s: &Gallery| if s.dark { Theme::Dark } else { Theme::Light })
    .run()
}
