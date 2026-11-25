//! Themed dialog bridge over `photon-dialogs` plus modal presets and toast banners.

use iced::widget::{
    column, container, mouse_area, opaque, row, rule, scrollable, stack, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Length, Padding, Size, Theme};

pub use photon_dialogs::{
    ActiveGesture, CutoutStrips, DialogEvent, DialogManager, DialogStyle, DialogWindowState,
    DragOp, ManagedDialogSpec, OverlayMode, ResizeEdge, apply_overlay, clamp_dialog_rect,
    compute_cutout_strips, render_managed_dialog,
};

use crate::button::{self, ButtonVariant};
use crate::icons::{self, Icon};
use crate::theme::{
    PaletteTokens, RADIUS_LG, RADIUS_MD, TEXT_SIZE_CONTROL, TEXT_SIZE_DIALOG_TITLE,
    dialog_frame_style, hairline_rule_style, input_style, modal_backdrop_style, popover_style,
};

/// Build a [`DialogStyle`] from `photon-ui` [`PaletteTokens`].
pub fn dialog_style_for_tokens(t: PaletteTokens) -> DialogStyle {
    DialogStyle {
        surface_bg: t.bg_surface,
        header_bg: t.bg_elevated,
        border_color: t.border,
        border_strong: t.border_strong,
        accent: t.accent,
        text_primary: t.text_primary,
        text_muted: t.text_muted,
        scrim_color: Color::from_rgb8(0x06, 0x08, 0x0c),
        shadow_color: Color::from_rgba(0.0, 0.0, 0.0, 0.5),
        radius: RADIUS_LG,
    }
}

/// Build a [`DialogStyle`] matching an `iced::Theme`.
pub fn dialog_style_for_theme(theme: &Theme) -> DialogStyle {
    dialog_style_for_tokens(PaletteTokens::for_theme(theme))
}

/// Convenience builder for a themed, draggable, 8-way resizable managed dialog.
#[allow(clippy::too_many_arguments)]
pub fn managed_dialog<'a, Id, M>(
    base: impl Into<Element<'a, M>>,
    manager: &DialogManager<Id>,
    viewport: Size,
    theme: &Theme,
    id: Id,
    open: bool,
    title: impl Into<String>,
    defaults: DialogWindowState,
    resizable: bool,
    draggable: bool,
    overlay: OverlayMode,
    body: impl Into<Element<'a, M>>,
    footer: Option<Element<'a, M>>,
    on_event: impl Fn(DialogEvent<Id>) -> M + 'a,
    on_close: M,
    on_noop: M,
) -> Element<'a, M>
where
    Id: Ord + Clone + 'static,
    M: Clone + 'a,
{
    let close_btn: Element<'a, M> =
        button::icon_only(Icon::Close, ButtonVariant::Ghost, Some(on_close.clone())).into();

    render_managed_dialog(
        base,
        manager,
        viewport,
        ManagedDialogSpec {
            id,
            open,
            title: title.into(),
            defaults,
            resizable,
            draggable,
            overlay,
            body: body.into(),
            footer,
            close_button: Some(close_btn),
            style: dialog_style_for_theme(theme),
            on_event: Box::new(on_event),
            on_close,
            on_noop,
        },
    )
}

/// Render an ordered stack of managed dialogs over `base` (`dialogs` rendered bottom-to-top).
pub fn managed_dialog_stack<'a, Id, M>(
    base: impl Into<Element<'a, M>>,
    manager: &DialogManager<Id>,
    viewport: Size,
    dialogs: Vec<ManagedDialogSpec<'a, Id, M>>,
) -> Element<'a, M>
where
    Id: Ord + Clone + 'static,
    M: Clone + 'a,
{
    let mut layer: Element<'a, M> = base.into();
    for spec in dialogs {
        if spec.open {
            layer = render_managed_dialog(layer, manager, viewport, spec);
        }
    }
    layer
}

/// Legacy compatibility specification for centered non-resizable modal dialogs.
pub struct DialogSpec<'a, M> {
    pub open: bool,
    pub title: String,
    pub width: f32,
    pub body: Element<'a, M>,
    pub footer: Element<'a, M>,
    pub on_close: M,
    pub on_noop: M,
}

/// Render a centered modal dialog with `opaque(...)` backdrop locking and scrollable body.
pub fn modal_dialog<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    spec: DialogSpec<'a, M>,
) -> Element<'a, M> {
    if !spec.open {
        return base.into();
    }

    let header = container(
        row![
            text(spec.title).size(TEXT_SIZE_DIALOG_TITLE),
            iced::widget::space::horizontal(),
            button::icon_only(
                Icon::Close,
                ButtonVariant::Ghost,
                Some(spec.on_close.clone())
            ),
        ]
        .align_y(Alignment::Center),
    )
    .padding([12, 16]);

    let body_scroll = scrollable(container(spec.body).padding(16).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Shrink);

    let frame = container(
        column![
            header,
            rule::horizontal(1).style(hairline_rule_style),
            body_scroll,
            rule::horizontal(1).style(hairline_rule_style),
            container(spec.footer).padding([12, 16]),
        ]
        .width(Length::Fixed(spec.width)),
    )
    .max_height(680.0)
    .style(dialog_frame_style);

    let on_noop_scroll = spec.on_noop.clone();
    let protected_frame = opaque(
        mouse_area(frame)
            .on_press(spec.on_noop.clone())
            .on_right_press(spec.on_noop.clone())
            .on_scroll(move |_| on_noop_scroll.clone()),
    );

    let on_noop_bg = spec.on_noop.clone();
    let backdrop = opaque(
        mouse_area(
            container(iced::widget::space::horizontal())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(modal_backdrop_style),
        )
        .on_press(spec.on_close.clone())
        .on_right_press(spec.on_close)
        .on_scroll(move |_| on_noop_bg.clone()),
    );

    let centered = container(protected_frame)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    stack![base.into(), backdrop, centered]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Render an ordered stack of modal dialogs; the last open entry sits on top.
pub fn modal_stack<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    dialogs: Vec<DialogSpec<'a, M>>,
) -> Element<'a, M> {
    let mut layer: Element<'a, M> = base.into();
    for spec in dialogs {
        if spec.open {
            layer = modal_dialog(layer, spec);
        }
    }
    layer
}

pub struct ConfirmDialogSpec<M> {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub cancel_label: String,
    pub danger: bool,
    pub secondary_action: Option<(String, M)>,
    pub on_confirm: M,
    pub on_cancel: M,
    pub on_noop: M,
}

pub fn confirm_dialog_spec<'a, M: Clone + 'a>(spec: ConfirmDialogSpec<M>) -> DialogSpec<'a, M> {
    let body = text(spec.message)
        .size(TEXT_SIZE_CONTROL)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_secondary),
            }
        })
        .into();

    let mut footer_row = row![button::labeled(
        spec.cancel_label,
        None,
        ButtonVariant::Ghost,
        Some(spec.on_cancel.clone()),
    )]
    .spacing(8)
    .align_y(Alignment::Center);

    footer_row = footer_row.push(iced::widget::space::horizontal());

    let has_secondary = spec.secondary_action.is_some();
    if let Some((label, msg)) = spec.secondary_action {
        footer_row = footer_row.push(button::labeled(
            label,
            None,
            if spec.danger {
                ButtonVariant::Danger
            } else {
                ButtonVariant::Secondary
            },
            Some(msg),
        ));
    }

    footer_row = footer_row.push(button::labeled(
        spec.confirm_label,
        None,
        if spec.danger && !has_secondary {
            ButtonVariant::Danger
        } else {
            ButtonVariant::Primary
        },
        Some(spec.on_confirm),
    ));

    DialogSpec {
        open: true,
        title: spec.title,
        width: 420.0,
        body,
        footer: footer_row.into(),
        on_close: spec.on_cancel,
        on_noop: spec.on_noop,
    }
}

pub struct PromptDialogSpec<'a, M, F>
where
    F: Fn(String) -> M + 'a,
{
    pub title: String,
    pub label: Option<String>,
    pub placeholder: String,
    pub value: String,
    pub error: Option<String>,
    pub confirm_label: String,
    pub cancel_label: String,
    pub on_input: F,
    pub on_confirm: M,
    pub on_cancel: M,
    pub on_noop: M,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, M, F> PromptDialogSpec<'a, M, F>
where
    F: Fn(String) -> M + 'a,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        title: impl Into<String>,
        placeholder: impl Into<String>,
        value: impl Into<String>,
        confirm_label: impl Into<String>,
        cancel_label: impl Into<String>,
        on_input: F,
        on_confirm: M,
        on_cancel: M,
        on_noop: M,
    ) -> Self {
        Self {
            title: title.into(),
            label: None,
            placeholder: placeholder.into(),
            value: value.into(),
            error: None,
            confirm_label: confirm_label.into(),
            cancel_label: cancel_label.into(),
            on_input,
            on_confirm,
            on_cancel,
            on_noop,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }
}

pub fn prompt_dialog_spec<'a, M: Clone + 'a, F>(
    spec: PromptDialogSpec<'a, M, F>,
) -> DialogSpec<'a, M>
where
    F: Fn(String) -> M + 'a,
{
    let mut body_col = column![].spacing(8);
    if let Some(lbl) = spec.label {
        body_col = body_col.push(text(lbl).size(TEXT_SIZE_CONTROL).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_secondary),
            }
        }));
    }
    body_col = body_col.push(
        text_input(&spec.placeholder, &spec.value)
            .on_input(spec.on_input)
            .on_submit(spec.on_confirm.clone())
            .padding([8, 10])
            .size(TEXT_SIZE_CONTROL)
            .style(input_style)
            .width(Length::Fill),
    );
    if let Some(err) = spec.error {
        body_col = body_col.push(text(err).size(TEXT_SIZE_CONTROL).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.danger),
            }
        }));
    }

    let footer = row![
        button::labeled(
            spec.cancel_label,
            None,
            ButtonVariant::Ghost,
            Some(spec.on_cancel.clone()),
        ),
        iced::widget::space::horizontal(),
        button::labeled(
            spec.confirm_label,
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(spec.on_confirm),
        ),
    ]
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: spec.title,
        width: 400.0,
        body: body_col.into(),
        footer: footer.into(),
        on_close: spec.on_cancel,
        on_noop: spec.on_noop,
    }
}

pub fn alert_dialog_spec<'a, M: Clone + 'a>(
    title: impl Into<String>,
    message: impl Into<String>,
    close_label: impl Into<String>,
    on_close: M,
    on_noop: M,
) -> DialogSpec<'a, M> {
    let body = text(message.into())
        .size(TEXT_SIZE_CONTROL)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_secondary),
            }
        })
        .into();

    let footer = row![
        iced::widget::space::horizontal(),
        button::labeled(
            close_label.into(),
            None,
            ButtonVariant::Primary,
            Some(on_close.clone()),
        ),
    ]
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: title.into(),
        width: 400.0,
        body,
        footer: footer.into(),
        on_close,
        on_noop,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct ToastSpec<M> {
    pub message: String,
    pub kind: ToastKind,
    pub on_dismiss: M,
}

pub fn with_toast_banner<'a, M: Clone + 'a>(
    base: impl Into<Element<'a, M>>,
    toast: Option<ToastSpec<M>>,
) -> Element<'a, M> {
    let Some(t_spec) = toast else {
        return base.into();
    };
    let kind = t_spec.kind;
    let icon = match kind {
        ToastKind::Info => Icon::Info,
        ToastKind::Success => Icon::Check,
        ToastKind::Error => Icon::Close,
    };
    let banner = container(
        row![
            icons::render(icon, 14.0, None),
            text(t_spec.message).size(TEXT_SIZE_CONTROL),
            button::compact_icon(Icon::Close, ButtonVariant::Ghost, Some(t_spec.on_dismiss)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 12])
    .style(move |theme| {
        let mut st = popover_style(theme);
        let tokens = PaletteTokens::for_theme(theme);
        let border_color = match kind {
            ToastKind::Info => tokens.border_strong,
            ToastKind::Success => tokens.accent,
            ToastKind::Error => tokens.danger,
        };
        st.border = Border {
            color: border_color,
            width: 1.0,
            radius: RADIUS_MD.into(),
        };
        st
    });

    let overlay = container(opaque(banner))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(Padding {
            top: 12.0,
            right: 16.0,
            bottom: 42.0,
            left: 16.0,
        });

    stack![base.into(), overlay]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_style_bridges_dark_and_light_themes() {
        let dark = dialog_style_for_theme(&Theme::Dark);
        let light = dialog_style_for_theme(&Theme::Light);
        assert_ne!(dark.surface_bg, light.surface_bg);
        assert_ne!(dark.text_primary, light.text_primary);
        assert_eq!(dark.radius, RADIUS_LG);
    }
}
