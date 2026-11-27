use crate::*;
use iced::widget::overlay::menu as iced_menu;
use iced::widget::{column, container, pick_list, row, scrollable};
use iced::{Alignment, Border, Color, Element, Fill, Shadow};
use photon_i18n::{tr, tr_ui_text};
use photon_protocol::{BlendMode, Command, EditorState, Language, LayerEffect};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::editor::{
    LayerActionButton, LayerRowSpec, checker_swatch, layer_action_bar, layer_row,
};
use photon_ui::icons::Icon;
use photon_ui::theme::{PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL};
use photon_ui::typography::text;
use photon_ui::{TooltipPosition, with_tooltip, with_tooltip_pos};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlendChoice {
    pub(crate) mode: BlendMode,
    pub(crate) label: String,
}

impl std::fmt::Display for BlendChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub(crate) fn build_layers_panel<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let ui = &state.editor_ui;
    let active_layer = ed
        .layers
        .iter()
        .find(|l| l.id == ed.active_layer_id)
        .or_else(|| ed.layers.first());

    let active_idx = ed
        .layers
        .iter()
        .position(|l| l.id == ed.active_layer_id)
        .unwrap_or(0);
    let active_unlocked = active_layer.is_some_and(|l| !l.locked);
    let below_unlocked = active_idx > 0
        && ed
            .layers
            .get(active_idx - 1)
            .is_some_and(|below| !below.locked);
    let can_move_up = active_idx + 1 < ed.layers.len();
    let can_move_down = active_idx > 0;
    let can_delete = ed.layers.len() > 1 && active_unlocked;
    let can_merge_down = active_idx > 0 && active_unlocked && below_unlocked;

    let blend_label = |b: BlendMode| -> String {
        match b {
            BlendMode::Normal => tr(lang, "blend-normal"),
            BlendMode::Multiply => tr(lang, "blend-multiply"),
            BlendMode::Screen => tr(lang, "blend-screen"),
            BlendMode::Overlay => tr(lang, "blend-overlay"),
            BlendMode::Darken => tr(lang, "blend-darken"),
            BlendMode::Lighten => tr(lang, "blend-lighten"),
            BlendMode::Difference => tr(lang, "blend-difference"),
        }
    };

    let mut props_col = column![].spacing(5);
    if let Some(layer) = active_layer {
        let layer_id = layer.id;
        let blend_options: Vec<BlendChoice> = BlendMode::ALL
            .into_iter()
            .map(|mode| BlendChoice {
                mode,
                label: blend_label(mode),
            })
            .collect();
        let selected_blend = blend_options
            .iter()
            .find(|c| c.mode == layer.blend_mode)
            .cloned();

        let blend_picker = pick_list(blend_options, selected_blend, move |choice: BlendChoice| {
            EditorMessage::Dispatch(Command::EditorLayerSetProps {
                id: layer_id,
                name: None,
                visible: None,
                locked: None,
                clipped_to_below: None,
                lock_alpha: None,
                opacity: None,
                blend_mode: Some(choice.mode),
                offset_x: None,
                offset_y: None,
            })
        })
        .width(Fill)
        .text_size(TEXT_SIZE_CONTROL)
        .padding([4, 8])
        .style(|theme, status| {
            let t = PaletteTokens::for_theme(theme);
            let active = matches!(
                status,
                pick_list::Status::Hovered | pick_list::Status::Opened { .. }
            );
            pick_list::Style {
                text_color: t.text_primary,
                placeholder_color: t.text_muted,
                handle_color: t.text_muted,
                background: if active { t.bg_hover } else { t.bg_app }.into(),
                border: Border {
                    color: if active { t.accent } else { t.border },
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
            }
        })
        .menu_style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            iced_menu::Style {
                background: t.bg_elevated.into(),
                border: Border {
                    color: t.border_strong,
                    width: 1.0,
                    radius: RADIUS_SM.into(),
                },
                text_color: t.text_primary,
                selected_text_color: Color::WHITE,
                selected_background: t.accent.into(),
                shadow: Shadow::default(),
            }
        });

        let blend_row = row![
            text(tr(lang, "label-blend-mode")).size(11).style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
            blend_picker,
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let opacity_row = slider_stepper_control(
            tr(lang, "label-opacity"),
            layer.opacity,
            0.0..=1.0,
            0.02_f32,
            format!("{:.0}%", layer.opacity * 100.0),
            move |v| {
                EditorMessage::Dispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: None,
                    locked: None,
                    clipped_to_below: None,
                    lock_alpha: None,
                    opacity: Some(v),
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                })
            },
        );

        let toggles_row = row![
            button::labeled_tipped(
                tr(lang, "action-layer-lock-alpha"),
                Some(if layer.lock_alpha {
                    Icon::Lock
                } else {
                    Icon::Unlock
                }),
                if layer.lock_alpha {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                },
                Some(EditorMessage::Dispatch(Command::EditorLayerSetProps {
                    id: layer_id,
                    name: None,
                    visible: None,
                    locked: None,
                    clipped_to_below: None,
                    lock_alpha: Some(!layer.lock_alpha),
                    opacity: None,
                    blend_mode: None,
                    offset_x: None,
                    offset_y: None,
                })),
                tr(lang, "tooltip-layer-lock-alpha"),
            ),
            button::labeled_tipped(
                tr(lang, "action-layer-clipping-mask-short"),
                Some(Icon::Layers),
                if layer.clipped_to_below {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                },
                Some(EditorMessage::Dispatch(
                    Command::EditorLayerToggleClippingMask { id: Some(layer_id) },
                )),
                tr(lang, "tooltip-layer-clipping-mask"),
            ),
        ]
        .spacing(6);

        props_col = props_col
            .push(blend_row)
            .push(opacity_row)
            .push(toggles_row);

        if let Some(effect) = &layer.effect {
            let effect_ctrl: Option<Element<'_, EditorMessage>> = match effect {
                LayerEffect::Blur { radius } => Some(slider_stepper_control(
                    tr(lang, "label-radius"),
                    *radius,
                    1.0..=24.0,
                    0.5_f32,
                    format!("{radius:.0} px"),
                    move |v| {
                        EditorMessage::Dispatch(Command::EditorLayerUpdateEffect {
                            id: layer_id,
                            effect: LayerEffect::Blur { radius: v },
                        })
                    },
                )),
                LayerEffect::Sharpen { amount } => Some(slider_stepper_control(
                    tr(lang, "label-strength"),
                    *amount,
                    0.0..=2.5,
                    0.1_f32,
                    format!("{amount:.1}"),
                    move |v| {
                        EditorMessage::Dispatch(Command::EditorLayerUpdateEffect {
                            id: layer_id,
                            effect: LayerEffect::Sharpen { amount: v },
                        })
                    },
                )),
                LayerEffect::Denoise {
                    strength,
                    detail_preserve,
                } => {
                    let str_val = *strength;
                    let det_val = *detail_preserve;
                    Some(
                        column![
                            slider_stepper_control(
                                tr(lang, "label-strength"),
                                str_val,
                                0.05..=1.0,
                                0.05_f32,
                                format!("{:.0}%", str_val * 100.0),
                                move |v| {
                                    EditorMessage::Dispatch(Command::EditorLayerUpdateEffect {
                                        id: layer_id,
                                        effect: LayerEffect::Denoise {
                                            strength: v,
                                            detail_preserve: det_val,
                                        },
                                    })
                                },
                            ),
                            slider_stepper_control(
                                tr(lang, "label-preserve-detail"),
                                det_val,
                                0.0..=1.0,
                                0.05_f32,
                                format!("{:.0}%", det_val * 100.0),
                                move |v| {
                                    EditorMessage::Dispatch(Command::EditorLayerUpdateEffect {
                                        id: layer_id,
                                        effect: LayerEffect::Denoise {
                                            strength: str_val,
                                            detail_preserve: v,
                                        },
                                    })
                                },
                            ),
                        ]
                        .spacing(4)
                        .into(),
                    )
                }
                LayerEffect::ColorLight(_)
                | LayerEffect::Hsv(_)
                | LayerEffect::ColorBalance(_)
                | LayerEffect::Curves(_) => {
                    let tab = match effect {
                        LayerEffect::ColorLight(_) => AdjustmentPopupTab::ColorAndLight,
                        LayerEffect::Hsv(_) => AdjustmentPopupTab::Hsv,
                        LayerEffect::ColorBalance(_) => AdjustmentPopupTab::ColorBalance,
                        LayerEffect::Curves(_) => AdjustmentPopupTab::Curves,
                        _ => unreachable!(),
                    };
                    Some(
                        button::labeled(
                            tr(lang, "action-edit-effect"),
                            Some(Icon::Sliders),
                            ButtonVariant::Secondary,
                            (!layer.locked).then_some(EditorMessage::EditorOpenAdjustPopup(tab)),
                        )
                        .padding([3, 8])
                        .width(Fill)
                        .into(),
                    )
                }
                _ => None,
            };
            if let Some(el) = effect_ctrl {
                props_col = props_col.push(el);
            }
        }

        // Photoshop-style single Layer Mask quick controls for active layer
        let mask_controls: Element<'_, EditorMessage> = if layer.has_mask {
            row![
                with_tooltip_pos(
                    button::labeled(
                        tr(
                            lang,
                            if layer.editing_mask {
                                "action-mask-edit-pixels"
                            } else {
                                "action-mask-edit"
                            }
                        ),
                        Some(Icon::Mask),
                        ButtonVariant::Tab {
                            active: layer.editing_mask,
                        },
                        Some(EditorMessage::Dispatch(
                            Command::EditorLayerMaskSetEditing {
                                id: layer_id,
                                editing: !layer.editing_mask,
                            }
                        )),
                    )
                    .padding([3, 6])
                    .width(Fill),
                    tr(lang, "tooltip-layer-mask"),
                    TooltipPosition::Top,
                ),
                button::compact_icon_tipped(
                    if layer.mask_enabled {
                        Icon::Eye
                    } else {
                        Icon::EyeOff
                    },
                    ButtonVariant::Tab {
                        active: layer.mask_enabled,
                    },
                    Some(EditorMessage::Dispatch(
                        Command::EditorLayerMaskSetEnabled {
                            id: layer_id,
                            enabled: !layer.mask_enabled,
                        }
                    )),
                    tr(
                        lang,
                        if layer.mask_enabled {
                            "action-mask-disable"
                        } else {
                            "action-mask-enable"
                        }
                    ),
                ),
                button::compact_icon_tipped(
                    if layer.mask_linked {
                        Icon::Lock
                    } else {
                        Icon::Unlock
                    },
                    ButtonVariant::Tab {
                        active: layer.mask_linked,
                    },
                    Some(EditorMessage::Dispatch(Command::EditorLayerMaskSetLinked {
                        id: layer_id,
                        linked: !layer.mask_linked,
                    })),
                    tr(
                        lang,
                        if layer.mask_linked {
                            "tooltip-mask-linked"
                        } else {
                            "tooltip-mask-unlinked"
                        }
                    ),
                ),
                button::compact_icon_tipped(
                    Icon::Swap,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(EditorMessage::Dispatch(
                        Command::EditorLayerMaskInvert { id: layer_id }
                    )),
                    tr(lang, "action-mask-invert"),
                ),
                button::compact_icon_tipped(
                    Icon::Check,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(EditorMessage::Dispatch(
                        Command::EditorLayerMaskApply { id: layer_id }
                    )),
                    tr(lang, "action-mask-apply"),
                ),
                button::compact_icon_tipped(
                    Icon::Trash,
                    ButtonVariant::Ghost,
                    (!layer.locked).then_some(EditorMessage::Dispatch(
                        Command::EditorLayerMaskDelete { id: layer_id }
                    )),
                    tr(lang, "action-mask-delete"),
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
        } else {
            with_tooltip_pos(
                button::labeled(
                    tr(lang, "action-mask-add"),
                    Some(Icon::Mask),
                    ButtonVariant::Secondary,
                    (!layer.locked).then_some(EditorMessage::Dispatch(
                        Command::EditorLayerMaskAdd {
                            id: layer_id,
                            reveal_all: true,
                        },
                    )),
                )
                .padding([3, 8])
                .width(Fill),
                tr(lang, "action-mask-add"),
                TooltipPosition::Top,
            )
        };
        props_col = props_col.push(mask_controls);
    }

    let mut list_col = column![].spacing(4).width(Fill);
    for layer in ed.layers.iter().rev() {
        let is_active = layer.id == ed.active_layer_id;
        let is_selected = is_active || ui.selected_layer_ids.contains(&layer.id);
        let is_renaming = ui.renaming_layer_id == Some(layer.id);
        let subtitle = if let Some(eff) = &layer.effect {
            let kind = tr(
                lang,
                if eff.is_filter() {
                    "layer-kind-filter"
                } else {
                    "layer-kind-adjustment"
                },
            );
            Some(format!("{kind} · {:.0}%", layer.opacity * 100.0))
        } else if layer.opacity < 0.999 || layer.blend_mode != BlendMode::Normal {
            Some(format!(
                "{} · {:.0}%",
                blend_label(layer.blend_mode),
                layer.opacity * 100.0
            ))
        } else {
            Some(format!("{}×{}", layer.width, layer.height))
        };

        let on_select = if is_active && layer.has_mask && layer.editing_mask {
            EditorMessage::Dispatch(Command::EditorLayerMaskSetEditing {
                id: layer.id,
                editing: false,
            })
        } else {
            EditorMessage::EditorLayerRowClicked(layer.id)
        };

        let row_el = layer_row(LayerRowSpec {
            name: layer.name.clone(),
            subtitle,
            kind_badge: layer.kind.badge(),
            active: is_selected,
            visible: layer.visible,
            locked: layer.locked,
            clipped_to_below: layer.clipped_to_below,
            lock_alpha: layer.lock_alpha,
            has_mask: layer.has_mask,
            mask_enabled: layer.mask_enabled,
            mask_linked: layer.mask_linked,
            editing_mask: layer.editing_mask,
            thumbnail: state.editor_layer_thumbs.get(&layer.id).cloned(),
            mask_thumbnail: state.editor_mask_thumbs.get(&layer.id).cloned(),
            renaming: is_renaming.then_some(ui.layer_rename_input.as_str()),
            on_select,
            on_toggle_visible: EditorMessage::Dispatch(Command::EditorLayerSetProps {
                id: layer.id,
                name: None,
                visible: Some(!layer.visible),
                locked: None,
                clipped_to_below: None,
                lock_alpha: None,
                opacity: None,
                blend_mode: None,
                offset_x: None,
                offset_y: None,
            }),
            on_toggle_lock: EditorMessage::Dispatch(Command::EditorLayerSetProps {
                id: layer.id,
                name: None,
                visible: None,
                locked: Some(!layer.locked),
                clipped_to_below: None,
                lock_alpha: None,
                opacity: None,
                blend_mode: None,
                offset_x: None,
                offset_y: None,
            }),
            on_toggle_mask_edit: layer.has_mask.then_some(EditorMessage::Dispatch(
                Command::EditorLayerMaskSetEditing {
                    id: layer.id,
                    editing: !(is_active && layer.editing_mask),
                },
            )),
            on_toggle_mask_link: layer.has_mask.then_some(EditorMessage::Dispatch(
                Command::EditorLayerMaskSetLinked {
                    id: layer.id,
                    linked: !layer.mask_linked,
                },
            )),
            on_toggle_mask: (!layer.locked).then_some({
                if layer.has_mask {
                    EditorMessage::Dispatch(Command::EditorLayerMaskDelete { id: layer.id })
                } else {
                    EditorMessage::Dispatch(Command::EditorLayerMaskAdd {
                        id: layer.id,
                        reveal_all: true,
                    })
                }
            }),
            on_start_rename: Some(EditorMessage::EditorStartLayerRename(
                layer.id,
                layer.name.clone(),
            )),
            on_rename_input: Some(Box::new(EditorMessage::EditorLayerRenameChanged)),
            on_rename_submit: Some(EditorMessage::EditorCommitLayerRename),
            on_right_press: Some(EditorMessage::OpenLayerContextMenu { layer_id: layer.id }),
            visibility_tooltip: Some(tr(lang, "tooltip-layer-visibility")),
            lock_tooltip: Some(tr(lang, "tooltip-layer-lock")),
            mask_tooltip: Some(tr(lang, "tooltip-layer-mask")),
            mask_link_tooltip: Some(tr(
                lang,
                if layer.mask_linked {
                    "tooltip-mask-linked"
                } else {
                    "tooltip-mask-unlinked"
                },
            )),
            mask_toggle_tooltip: Some(tr(
                lang,
                if layer.has_mask {
                    "action-mask-delete"
                } else {
                    "action-mask-add"
                },
            )),
            rename_tooltip: Some(tr(lang, "action-rename")),
        });

        if is_renaming {
            list_col = list_col.push(
                row![
                    container(row_el).width(Fill),
                    button::compact_icon_tipped(
                        Icon::Close,
                        ButtonVariant::Ghost,
                        Some(EditorMessage::EditorCancelLayerRename),
                        tr(lang, "action-cancel"),
                    ),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        } else {
            let drag_handle = iced::widget::mouse_area(
                container(text("⋮⋮").size(11).style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }))
                .padding([4, 2]),
            )
            .on_press(EditorMessage::EditorBeginDragLayer(layer.id))
            .interaction(iced::mouse::Interaction::Grab);
            let drop_row = iced::widget::mouse_area(
                row![drag_handle, container(row_el).width(Fill)]
                    .spacing(2)
                    .align_y(Alignment::Center),
            )
            .on_release(EditorMessage::EditorDropLayer(layer.id));
            list_col = list_col.push(drop_row);
        }
    }

    // Dedicated Canvas Background row below layers (KogaView LayersPanel.qml style)
    let is_bg_transparent = ed.canvas_background.is_none();
    let canvas_bg_row = container(
        row![
            button::compact_icon_tipped(
                if is_bg_transparent {
                    Icon::EyeOff
                } else {
                    Icon::Eye
                },
                ButtonVariant::Ghost,
                Some(EditorMessage::Dispatch(
                    Command::EditorSetCanvasBackground {
                        color: if is_bg_transparent {
                            Some([255, 255, 255])
                        } else {
                            None
                        },
                        transparent: !is_bg_transparent,
                    }
                )),
                tr(lang, "label-canvas-bg"),
            ),
            with_tooltip(
                checker_swatch(
                    ed.canvas_background.map(rgb_to_color),
                    22.0,
                    18.0,
                    Some(EditorMessage::OpenColorPicker(
                        ColorPickerTarget::CanvasBackground
                    )),
                ),
                tr(lang, "label-canvas-bg"),
            ),
            text(tr(lang, "label-canvas-bg"))
                .size(TEXT_SIZE_CONTROL)
                .width(Fill),
            text(
                ed.canvas_background
                    .map(format_hex)
                    .unwrap_or_else(|| tr(lang, "label-transparent")),
            )
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding([5, 6])
    .width(Fill)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_app.into()),
            border: iced::Border {
                color: t.border,
                width: 1.0,
                radius: photon_ui::theme::RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    let action_bar = layer_action_bar(vec![
        LayerActionButton::new(
            Icon::Plus,
            Some(EditorMessage::ToggleMenu(EditorMenuKind::AddLayer)),
        )
        .active(state.active_menu == Some(EditorMenuKind::AddLayer))
        .tooltip(tr(lang, "tooltip-layer-add")),
        LayerActionButton::new(
            Icon::Duplicate,
            if state.editor_ui.selected_layer_ids.len() >= 2 {
                Some(EditorMessage::Dispatch(
                    Command::EditorLayerDuplicateMultiple {
                        ids: state.editor_ui.selected_layer_ids.clone(),
                    },
                ))
            } else {
                active_layer
                    .map(|l| EditorMessage::Dispatch(Command::EditorLayerDuplicate { id: l.id }))
            },
        )
        .tooltip(tr(lang, "tooltip-layer-duplicate")),
        LayerActionButton::new(
            Icon::ArrowUp,
            active_layer.filter(|_| can_move_up).map(|l| {
                EditorMessage::Dispatch(Command::EditorLayerReorder {
                    id: l.id,
                    new_index: active_idx + 1,
                })
            }),
        )
        .tooltip(tr(lang, "tooltip-layer-up")),
        LayerActionButton::new(
            Icon::ArrowDown,
            active_layer.filter(|_| can_move_down).map(|l| {
                EditorMessage::Dispatch(Command::EditorLayerReorder {
                    id: l.id,
                    new_index: active_idx - 1,
                })
            }),
        )
        .tooltip(tr(lang, "tooltip-layer-down")),
        LayerActionButton::new(
            Icon::MergeDown,
            active_layer
                .filter(|_| can_merge_down)
                .map(|l| EditorMessage::Dispatch(Command::EditorLayerMergeDown { id: l.id })),
        )
        .tooltip(tr(lang, "tooltip-layer-merge-down")),
        LayerActionButton::new(
            Icon::Flatten,
            (ed.layers.len() > 1).then_some(EditorMessage::Dispatch(Command::EditorLayerFlatten)),
        )
        .tooltip(tr(lang, "tooltip-layer-flatten")),
        LayerActionButton::new(
            Icon::Trash,
            if state.editor_ui.selected_layer_ids.len() >= 2 {
                (state.editor_ui.selected_layer_ids.len() < ed.layers.len()).then(|| {
                    EditorMessage::Dispatch(Command::EditorLayerDeleteMultiple {
                        ids: state.editor_ui.selected_layer_ids.clone(),
                    })
                })
            } else {
                active_layer
                    .filter(|_| can_delete)
                    .map(|l| EditorMessage::Dispatch(Command::EditorLayerDelete { id: l.id }))
            },
        )
        .tooltip(tr(lang, "tooltip-layer-delete")),
    ]);

    column![
        props_col,
        scrollable(container(list_col).padding(iced::Padding {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 0.0,
        }))
        .height(Fill)
        .width(Fill),
        canvas_bg_row,
        action_bar,
    ]
    .spacing(8)
    .height(Fill)
    .width(Fill)
    .into()
}

pub(crate) fn build_history_panel<'a>(
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let mut col = column![].spacing(4);
    let at_original = ed.history.is_empty();

    col = col.push(
        button::labeled(
            format!("0. {}", tr(lang, "label-history-original")),
            Some(Icon::History),
            ButtonVariant::Tab {
                active: at_original,
            },
            Some(EditorMessage::Dispatch(Command::EditorHistoryJump {
                target_index: 0,
            })),
        )
        .width(Fill),
    );

    for (idx, op) in ed.history.iter().enumerate() {
        let is_current = idx + 1 == ed.history.len();
        col = col.push(
            button::labeled(
                format!("{}. {}", idx + 1, tr_ui_text(lang, op)),
                None,
                if is_current {
                    ButtonVariant::Tab { active: true }
                } else {
                    ButtonVariant::Ghost
                },
                Some(EditorMessage::Dispatch(Command::EditorHistoryJump {
                    target_index: idx + 1,
                })),
            )
            .width(Fill),
        );
    }

    for (r_idx, op) in ed.redo_history.iter().enumerate() {
        let target = ed.history.len() + r_idx + 1;
        col = col.push(
            button::labeled(
                format!("↷ {}. {}", target, tr_ui_text(lang, op)),
                None,
                ButtonVariant::Ghost,
                Some(EditorMessage::Dispatch(Command::EditorHistoryJump {
                    target_index: target,
                })),
            )
            .width(Fill),
        );
    }

    if ed.history_trimmed {
        col = col.push(
            text(tr(lang, "label-history-trimmed"))
                .size(11)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
        );
    }

    let reset_btn = with_tooltip_pos(
        button::labeled(
            tr(lang, "action-reset"),
            Some(Icon::Reset),
            ButtonVariant::Secondary,
            ed.modified
                .then_some(EditorMessage::Dispatch(Command::EditorReset)),
        )
        .width(Fill),
        tr(lang, "tooltip-history-reset"),
        TooltipPosition::Top,
    );

    column![col, reset_btn].spacing(8).width(Fill).into()
}
