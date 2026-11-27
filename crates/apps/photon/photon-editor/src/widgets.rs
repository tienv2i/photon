use crate::*;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::editor::tool_rail_button_tipped;
use fw_ui::icons::Icon;
use fw_ui::theme::{PaletteTokens, TEXT_SIZE_CAPTION, slider_style};
use fw_ui::typography::text;
use iced::widget::container as iced_container;
use iced::widget::{column, container, row, slider};
use iced::{Alignment, Element, Fill, Length};
use photon_i18n::tr;
use photon_protocol::{ApplyTarget, BlendMode, Command, Language};

pub(crate) fn toolbar_divider<'a, M: 'a>() -> Element<'a, M> {
    iced_container(iced::widget::rule::vertical(1).style(fw_ui::theme::hairline_rule_style))
        .height(Length::Fixed(18.0))
        .into()
}

pub(crate) fn subtool_icon_row<'a>(
    lang: Language,
    active: EditorTool,
    tools: &[EditorTool],
) -> Element<'a, EditorMessage> {
    row(tools.iter().copied().map(|tool| {
        tool_rail_button_tipped(
            tool.icon(),
            active == tool,
            true,
            Some(EditorMessage::EditorSelectTool(tool)),
            editor_tool_label(lang, tool),
        )
    }))
    .spacing(4)
    .into()
}

/// Two-line slider with stepper control:
/// - Line 1: `<Label>` on the left (`Length::Fill`), `< [value] >` stepper on the right.
/// - Line 2: full-width slider bar.
pub fn slider_stepper_control<'a, M: Clone + 'a>(
    label: impl Into<String>,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    value_text: impl Into<String>,
    on_change: impl Fn(f32) -> M + 'a,
) -> Element<'a, M> {
    let min = *range.start();
    let max = *range.end();
    let eps = (step * 0.25).max(1e-5);
    let can_dec = value > min + eps;
    let can_inc = value < max - eps;
    let dec_msg = can_dec.then(|| on_change((value - step).clamp(min, max)));
    let inc_msg = can_inc.then(|| on_change((value + step).clamp(min, max)));

    let value_pill = container(
        text(value_text.into())
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_primary),
            }),
    )
    .width(Length::Fixed(58.0))
    .padding([2, 4])
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(|theme| {
        let t = PaletteTokens::for_theme(theme);
        container::Style {
            background: Some(t.bg_elevated.into()),
            border: iced::Border {
                color: t.border,
                width: 1.0,
                radius: fw_ui::theme::RADIUS_SM.into(),
            },
            ..Default::default()
        }
    });

    let stepper = row![
        button::compact_icon(Icon::Prev, ButtonVariant::Ghost, dec_msg),
        value_pill,
        button::compact_icon(Icon::Next, ButtonVariant::Ghost, inc_msg),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    let header = row![
        text(label.into())
            .size(TEXT_SIZE_CAPTION)
            .width(Fill)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_secondary),
            }),
        stepper,
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let bar = slider(range, value, on_change)
        .step(step)
        .width(Fill)
        .style(slider_style);

    column![header, bar].spacing(3).width(Fill).into()
}

pub fn apply_target_control<'a, M: Clone + 'a>(
    lang: Language,
    target: ApplyTarget,
    on_select: impl Fn(ApplyTarget) -> M + 'a,
) -> Element<'a, M> {
    column![
        text(tr(lang, "label-apply-target"))
            .size(TEXT_SIZE_CAPTION)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_secondary),
            }),
        row![
            button::labeled(
                tr(lang, "target-active-layer"),
                None,
                ButtonVariant::Tab {
                    active: target == ApplyTarget::ActiveLayer,
                },
                Some(on_select(ApplyTarget::ActiveLayer)),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "target-whole-image"),
                None,
                ButtonVariant::Tab {
                    active: target == ApplyTarget::WholeImage,
                },
                Some(on_select(ApplyTarget::WholeImage)),
            )
            .width(Fill),
            button::labeled(
                tr(lang, "target-new-layer"),
                None,
                ButtonVariant::Tab {
                    active: target == ApplyTarget::NewLayer,
                },
                Some(on_select(ApplyTarget::NewLayer)),
            )
            .width(Fill),
        ]
        .spacing(4),
    ]
    .spacing(4)
    .width(Fill)
    .into()
}

pub(crate) fn build_apply_target_row<'a>(
    target: ApplyTarget,
    lang: Language,
) -> Element<'a, EditorMessage> {
    apply_target_control(lang, target, EditorMessage::EditorSetApplyTarget)
}

pub(crate) fn build_ai_task_banner<'a>(
    state: EditorViewState<'a>,
    lang: Language,
) -> Option<Element<'a, EditorMessage>> {
    let task = state.snapshot.meta.ai_task.as_ref().filter(|t| t.running)?;
    Some(
        row![
            text(format!("{}% · {}", task.percent, task.stage))
                .size(11)
                .width(Fill),
            button::labeled(
                tr(lang, "action-cancel"),
                Some(Icon::Close),
                ButtonVariant::Danger,
                Some(EditorMessage::Dispatch(Command::AiCancel)),
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
    )
}

pub(crate) fn blend_mode_label(b: BlendMode, lang: Language) -> String {
    match b {
        BlendMode::Normal => tr(lang, "blend-normal"),
        BlendMode::Multiply => tr(lang, "blend-multiply"),
        BlendMode::Screen => tr(lang, "blend-screen"),
        BlendMode::Overlay => tr(lang, "blend-overlay"),
        BlendMode::Darken => tr(lang, "blend-darken"),
        BlendMode::Lighten => tr(lang, "blend-lighten"),
        BlendMode::Difference => tr(lang, "blend-difference"),
    }
}
