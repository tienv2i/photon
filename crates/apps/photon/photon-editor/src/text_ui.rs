//! Non-destructive Text Tool (`T`) and Text Layer inspector options.

use fw_panels::section_card;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::editor::checker_swatch;
use fw_ui::icons::Icon;
use fw_ui::theme::{TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, hairline_rule_style, input_style};
use fw_ui::typography::text;
use iced::widget::{checkbox, column, row, rule, text_input};
use iced::{Alignment, Element, Fill};
use photon_i18n::tr;
use photon_protocol::{Command, EditorState, Language, TextAlign, TextLayerData};

use crate::{
    ColorPickerTarget, EditorMessage, EditorViewState, rgb_to_color, slider_stepper_control,
};

/// Build Tool Options card for `EditorTool::Text` (`T`) and active Text Layers.
pub fn build_text_tool_options<'a>(
    state: EditorViewState<'a>,
    ed: &'a EditorState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let is_active_text_layer = ed.active_text_data.is_some();
    let data = ed
        .active_text_data
        .as_ref()
        .unwrap_or(&state.editor_ui.text_draft)
        .clone();
    let next_idx = ed.layers.len() + 1;

    let emit_change = move |next: TextLayerData| {
        if is_active_text_layer {
            EditorMessage::EditorAddOrUpdateTextLayer(next)
        } else {
            EditorMessage::EditorSetTextDraft(next)
        }
    };

    let d_text = data.clone();
    let d_font = data.clone();
    let d_size = data.clone();
    let d_lh = data.clone();
    let d_bold = data.clone();
    let d_italic = data.clone();
    let d_x = data.clone();
    let d_y = data.clone();

    let font_presets = ["sans-serif", "serif", "monospace"];
    let mut font_row = row![].spacing(4);
    for preset in font_presets {
        let d_p = data.clone();
        let active = data.font_family.eq_ignore_ascii_case(preset);
        font_row = font_row.push(
            button::labeled(
                preset.to_owned(),
                None,
                ButtonVariant::Tab { active },
                Some({
                    let mut next = d_p;
                    next.font_family = preset.to_owned();
                    emit_change(next)
                }),
            )
            .width(Fill),
        );
    }

    let align_btn = |label: String, align: TextAlign, base: TextLayerData| {
        let mut next = base;
        let active = next.align == align;
        next.align = align;
        button::labeled(
            label,
            None,
            ButtonVariant::Tab { active },
            Some(emit_change(next)),
        )
        .width(Fill)
    };

    let max_x = (ed.width as f32).max(200.0);
    let max_y = (ed.height as f32).max(200.0);
    let text_rgb = data.color;

    let mut body = column![
        column![
            text(tr(lang, "label-text-content")).size(TEXT_SIZE_CAPTION),
            text_input(&tr(lang, "placeholder-text-content"), &data.text)
                .on_input(move |val| {
                    let mut next = d_text.clone();
                    next.text = val;
                    emit_change(next)
                })
                .size(TEXT_SIZE_CONTROL)
                .padding([6, 8])
                .style(input_style),
        ]
        .spacing(4),
        column![
            text(tr(lang, "label-font-family")).size(TEXT_SIZE_CAPTION),
            font_row,
            text_input("sans-serif", &data.font_family)
                .on_input(move |val| {
                    let mut next = d_font.clone();
                    next.font_family = val;
                    emit_change(next)
                })
                .size(TEXT_SIZE_CONTROL)
                .padding([5, 8])
                .style(input_style),
        ]
        .spacing(4),
        slider_stepper_control(
            tr(lang, "label-font-size"),
            data.font_size.clamp(6.0, 240.0),
            6.0..=240.0,
            1.0_f32,
            format!("{:.0} px", data.font_size),
            move |v| {
                let mut next = d_size.clone();
                next.font_size = v;
                emit_change(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-line-height"),
            data.line_height.clamp(0.6, 3.0),
            0.6..=3.0,
            0.05_f32,
            format!("{:.2}×", data.line_height),
            move |v| {
                let mut next = d_lh.clone();
                next.line_height = v;
                emit_change(next)
            },
        ),
        row![
            text(tr(lang, "label-text-color")).size(TEXT_SIZE_CAPTION),
            checker_swatch(
                Some(rgb_to_color(text_rgb)),
                28.0,
                20.0,
                Some(EditorMessage::OpenColorPicker(
                    ColorPickerTarget::TextLayerColor,
                )),
            ),
            iced::widget::space::horizontal(),
            checkbox(data.bold)
                .label(tr(lang, "label-text-bold"))
                .text_size(TEXT_SIZE_CONTROL)
                .on_toggle(move |b| {
                    let mut next = d_bold.clone();
                    next.bold = b;
                    emit_change(next)
                }),
            checkbox(data.italic)
                .label(tr(lang, "label-text-italic"))
                .text_size(TEXT_SIZE_CONTROL)
                .on_toggle(move |i| {
                    let mut next = d_italic.clone();
                    next.italic = i;
                    emit_change(next)
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        row![
            align_btn(tr(lang, "align-left"), TextAlign::Left, data.clone()),
            align_btn(tr(lang, "align-center"), TextAlign::Center, data.clone()),
            align_btn(tr(lang, "align-right"), TextAlign::Right, data.clone()),
        ]
        .spacing(4),
        slider_stepper_control(
            tr(lang, "label-transform-x"),
            data.x.clamp(-max_x, max_x),
            -max_x..=max_x,
            1.0_f32,
            format!("{:.0} px", data.x),
            move |v| {
                let mut next = d_x.clone();
                next.x = v;
                emit_change(next)
            },
        ),
        slider_stepper_control(
            tr(lang, "label-transform-y"),
            data.y.clamp(-max_y, max_y),
            -max_y..=max_y,
            1.0_f32,
            format!("{:.0} px", data.y),
            move |v| {
                let mut next = d_y.clone();
                next.y = v;
                emit_change(next)
            },
        ),
        rule::horizontal(1).style(hairline_rule_style),
    ]
    .spacing(8);

    let mut actions = row![
        button::labeled(
            tr(lang, "action-layer-add-text"),
            Some(Icon::Text),
            if is_active_text_layer {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Primary
            },
            Some(EditorMessage::Dispatch(Command::EditorAddTextLayer {
                name: format!("Text {next_idx}"),
                data: data.clone(),
            })),
        )
        .width(Fill),
    ]
    .spacing(4);

    if is_active_text_layer {
        actions = actions.push(
            button::labeled(
                tr(lang, "action-layer-rasterize"),
                Some(Icon::Layers),
                ButtonVariant::Secondary,
                Some(EditorMessage::Dispatch(Command::EditorRasterizeLayer {
                    id: None,
                })),
            )
            .width(Fill),
        );
    }

    body = body.push(actions);

    section_card(tr(lang, "tool-text"), body)
}
