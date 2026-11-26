use crate::layers_history::BlendChoice;
use crate::widgets::blend_mode_label;
use crate::*;
use iced::mouse;
use iced::widget::overlay::menu as iced_menu;
use iced::widget::{canvas, checkbox, column, pick_list, row};
use iced::{Alignment, Border, Color, Element, Fill, Length, Point, Rectangle, Shadow, Size};
use photon_i18n::tr;
use photon_panels::section_card;
use photon_protocol::{
    BlendMode, GradientColorMode, GradientKind, GradientRepeat, GradientStop, Language,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::editor::checker_swatch;
use photon_ui::icons::Icon;
use photon_ui::theme::{PaletteTokens, RADIUS_SM};
use photon_ui::typography::text;

#[derive(Default)]
struct GradientBarState {
    dragging_stop: Option<usize>,
}

struct GradientBarProgram {
    raw_stops: Vec<GradientStop>,
    selected_stop: usize,
    reverse: bool,
}

impl GradientBarProgram {
    fn sample_color(&self, t_in: f64) -> Color {
        let mut ramp = self.raw_stops.clone();
        ramp.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        if ramp.is_empty() {
            return Color::TRANSPARENT;
        }
        let t = if self.reverse { 1.0 - t_in } else { t_in }.clamp(0.0, 1.0);
        if t <= ramp[0].pos {
            let s = ramp[0];
            return Color::from_rgba8(s.color[0], s.color[1], s.color[2], s.alpha as f32);
        }
        let last = ramp.len() - 1;
        if t >= ramp[last].pos {
            let s = ramp[last];
            return Color::from_rgba8(s.color[0], s.color[1], s.color[2], s.alpha as f32);
        }
        for i in 0..last {
            let s0 = ramp[i];
            let s1 = ramp[i + 1];
            if t >= s0.pos && t <= s1.pos {
                let f = ((t - s0.pos) / (s1.pos - s0.pos).max(1e-6)).clamp(0.0, 1.0);
                let inv = 1.0 - f;
                let a = (s0.alpha * inv + s1.alpha * f).clamp(0.0, 1.0);
                let denom = a.max(1e-5);
                let r = (f64::from(s0.color[0]) * s0.alpha * inv
                    + f64::from(s1.color[0]) * s1.alpha * f)
                    / denom;
                let g = (f64::from(s0.color[1]) * s0.alpha * inv
                    + f64::from(s1.color[1]) * s1.alpha * f)
                    / denom;
                let b = (f64::from(s0.color[2]) * s0.alpha * inv
                    + f64::from(s1.color[2]) * s1.alpha * f)
                    / denom;
                return Color::from_rgba8(
                    r.round().clamp(0.0, 255.0) as u8,
                    g.round().clamp(0.0, 255.0) as u8,
                    b.round().clamp(0.0, 255.0) as u8,
                    a as f32,
                );
            }
        }
        Color::TRANSPARENT
    }
}

impl canvas::Program<EditorMessage> for GradientBarProgram {
    type State = GradientBarState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &iced::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<EditorMessage>> {
        let w = bounds.width.max(1.0);
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let bar_w = (w - 12.0).max(1.0);
                let rel_x = (pos.x - 6.0).clamp(0.0, bar_w);
                if pos.y >= 20.0 {
                    for (idx, s) in self.raw_stops.iter().enumerate() {
                        let sx = (s.pos as f32).clamp(0.0, 1.0) * bar_w;
                        if (rel_x - sx).abs() <= 9.0 {
                            state.dragging_stop = Some(idx);
                            return Some(
                                canvas::Action::publish(EditorMessage::EditorSelectGradientStop(
                                    idx,
                                ))
                                .and_capture(),
                            );
                        }
                    }
                }
                let norm = f64::from((rel_x / bar_w).clamp(0.0, 1.0));
                Some(
                    canvas::Action::publish(EditorMessage::EditorAddGradientStop(norm))
                        .and_capture(),
                )
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let idx = state.dragging_stop?;
                let pos = cursor.position()?;
                let bar_w = (w - 12.0).max(1.0);
                let norm = f64::from(((pos.x - bounds.x - 6.0) / bar_w).clamp(0.0, 1.0));
                Some(
                    canvas::Action::publish(EditorMessage::EditorSetGradientStopPos(idx, norm))
                        .and_capture(),
                )
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging_stop.is_some() =>
            {
                state.dragging_stop = None;
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let pal = PaletteTokens::for_theme(theme);
        let bar_h = 20.0_f32;
        let bar_w = (bounds.width - 12.0).max(1.0);
        let sq = 5.0_f32;
        let cols = (bar_w / sq).ceil() as usize;
        let rows = (bar_h / sq).ceil() as usize;
        for r in 0..rows {
            for c in 0..cols {
                let col = if (r + c) % 2 == 0 {
                    Color::from_rgb8(210, 210, 210)
                } else {
                    Color::from_rgb8(165, 165, 165)
                };
                frame.fill_rectangle(
                    Point::new(6.0 + c as f32 * sq, r as f32 * sq),
                    Size::new(sq.min(bar_w - c as f32 * sq), sq.min(bar_h - r as f32 * sq)),
                    col,
                );
            }
        }
        let slices = (bar_w as usize).clamp(32, 128);
        let step_w = bar_w / slices as f32;
        for i in 0..slices {
            let t = (i as f64 + 0.5) / slices as f64;
            let c = self.sample_color(t);
            frame.fill_rectangle(
                Point::new(6.0 + i as f32 * step_w, 0.0),
                Size::new(step_w + 0.6, bar_h),
                c,
            );
        }
        frame.stroke(
            &canvas::Path::rectangle(Point::new(6.0, 0.0), Size::new(bar_w, bar_h)),
            canvas::Stroke::default()
                .with_color(pal.border_strong)
                .with_width(1.0),
        );
        for (idx, s) in self.raw_stops.iter().enumerate() {
            let cx = 6.0 + (s.pos as f32).clamp(0.0, 1.0) * bar_w;
            let cy = bar_h + 8.0;
            let circle = canvas::Path::circle(Point::new(cx, cy), 5.0);
            frame.fill(
                &circle,
                Color::from_rgb8(s.color[0], s.color[1], s.color[2]),
            );
            frame.stroke(
                &circle,
                canvas::Stroke::default()
                    .with_color(if idx == self.selected_stop {
                        pal.accent
                    } else {
                        pal.border_strong
                    })
                    .with_width(if idx == self.selected_stop { 2.0 } else { 1.0 }),
            );
        }
        vec![frame.into_geometry()]
    }
}

pub(crate) fn build_gradient_tool_options<'a>(
    ui: &'a EditorUiState,
    lang: Language,
) -> Element<'a, EditorMessage> {
    let presets = [
        ("fg_to_bg", "gradient-preset-fg-bg"),
        ("fg_to_transparent", "gradient-preset-fg-clear"),
        ("rainbow", "gradient-preset-rainbow"),
        ("sunset", "gradient-preset-sunset"),
        ("ocean", "gradient-preset-ocean"),
        ("fire", "gradient-preset-fire"),
        ("chrome", "gradient-preset-chrome"),
        ("vignette", "gradient-preset-vignette"),
    ];
    #[derive(Clone, PartialEq, Eq)]
    struct PresetChoice {
        id: &'static str,
        label: String,
    }
    impl std::fmt::Display for PresetChoice {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.label)
        }
    }
    let preset_options: Vec<PresetChoice> = presets
        .into_iter()
        .map(|(id, key)| PresetChoice {
            id,
            label: tr(lang, key),
        })
        .collect();
    let selected_preset = preset_options
        .iter()
        .find(|p| p.id == ui.gradient_preset_id)
        .cloned();
    let preset_picker = pick_list(preset_options, selected_preset, |choice: PresetChoice| {
        EditorMessage::EditorApplyGradientPreset(choice.id.to_owned())
    })
    .placeholder(tr(lang, "label-custom"))
    .text_size(11)
    .padding([4, 8])
    .width(Fill)
    .style(photon_ui::theme::pick_list_style)
    .menu_style(photon_ui::theme::pick_list_menu_style);

    let raw_stops = ui.raw_gradient_stops();
    let sel_idx = ui
        .gradient_selected_stop
        .min(raw_stops.len().saturating_sub(1));
    let cur_stop = raw_stops.get(sel_idx).copied().unwrap_or(GradientStop {
        pos: 0.0,
        color: ui.fg_color,
        alpha: 1.0,
    });
    let can_delete_stop =
        ui.gradient_color_mode == GradientColorMode::Custom && ui.gradient_stops.len() > 2;

    let stop_bar: Element<'_, EditorMessage> = iced::widget::canvas(GradientBarProgram {
        raw_stops: raw_stops.clone(),
        selected_stop: sel_idx,
        reverse: ui.gradient_reverse,
    })
    .width(Fill)
    .height(Length::Fixed(34.0))
    .into();

    let stop_controls = row![
        checker_swatch(
            Some(rgb_to_color(cur_stop.color)),
            28.0,
            22.0,
            Some(EditorMessage::OpenColorPicker(
                ColorPickerTarget::GradientStop(sel_idx),
            )),
        ),
        button::labeled(
            if cur_stop.alpha < 0.5 {
                "α: 0%"
            } else {
                "α: 100%"
            },
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::EditorToggleGradientStopAlpha(sel_idx)),
        ),
        button::labeled(
            tr(lang, "action-stop-fg"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::EditorSetGradientStopFg(sel_idx)),
        ),
        button::labeled(
            tr(lang, "action-add-stop"),
            None,
            ButtonVariant::Ghost,
            Some(EditorMessage::EditorAddGradientStop(0.5)),
        ),
        button::compact_icon(
            Icon::Close,
            ButtonVariant::Ghost,
            can_delete_stop.then_some(EditorMessage::EditorRemoveSelectedGradientStop),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let shape_label = |k: GradientKind| match k {
        GradientKind::Linear => tr(lang, "gradient-linear"),
        GradientKind::Bilinear | GradientKind::Reflected => tr(lang, "gradient-bilinear"),
        GradientKind::Radial => tr(lang, "gradient-radial"),
        GradientKind::Square => tr(lang, "gradient-square"),
        GradientKind::Conical => tr(lang, "gradient-conical"),
        GradientKind::Spiral => tr(lang, "gradient-spiral"),
    };
    #[derive(Clone, PartialEq, Eq)]
    struct KindChoice {
        kind: GradientKind,
        label: String,
    }
    impl std::fmt::Display for KindChoice {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.label)
        }
    }
    let kind_options: Vec<KindChoice> = [
        GradientKind::Linear,
        GradientKind::Bilinear,
        GradientKind::Radial,
        GradientKind::Square,
        GradientKind::Conical,
        GradientKind::Spiral,
    ]
    .into_iter()
    .map(|kind| KindChoice {
        kind,
        label: shape_label(kind),
    })
    .collect();
    let current_kind = if ui.gradient_kind == GradientKind::Reflected {
        GradientKind::Bilinear
    } else {
        ui.gradient_kind
    };
    let selected_kind = kind_options
        .iter()
        .find(|c| c.kind == current_kind)
        .cloned();
    let kind_picker = pick_list(kind_options, selected_kind, |choice: KindChoice| {
        EditorMessage::EditorSetGradientKind(choice.kind)
    })
    .text_size(11)
    .padding([4, 8])
    .width(Fill)
    .style(photon_ui::theme::pick_list_style)
    .menu_style(photon_ui::theme::pick_list_menu_style);

    let repeat_row = row([
        (GradientRepeat::None, "gradient-repeat-none"),
        (GradientRepeat::Forwards, "gradient-repeat-forwards"),
        (GradientRepeat::Alternating, "gradient-repeat-alternating"),
    ]
    .into_iter()
    .map(|(r, key)| {
        button::labeled(
            tr(lang, key),
            None,
            ButtonVariant::Tab {
                active: ui.gradient_repeat == r,
            },
            Some(EditorMessage::EditorSetGradientRepeat(r)),
        )
        .width(Fill)
        .into()
    }))
    .spacing(3);

    let blend_options: Vec<BlendChoice> = BlendMode::ALL
        .into_iter()
        .map(|mode| BlendChoice {
            mode,
            label: blend_mode_label(mode, lang),
        })
        .collect();
    let selected_blend = blend_options
        .iter()
        .find(|c| c.mode == ui.gradient_blend_mode)
        .cloned();
    let blend_picker = pick_list(blend_options, selected_blend, |choice: BlendChoice| {
        EditorMessage::EditorSetGradientBlendMode(choice.mode)
    })
    .text_size(11)
    .padding([4, 8])
    .width(Fill)
    .style(|theme, status| {
        let t = PaletteTokens::for_theme(theme);
        let active = matches!(
            status,
            iced::widget::pick_list::Status::Hovered
                | iced::widget::pick_list::Status::Opened { .. }
        );
        iced::widget::pick_list::Style {
            text_color: t.text_primary,
            placeholder_color: t.text_muted,
            handle_color: t.text_secondary,
            background: if active { t.bg_hover } else { t.bg_surface }.into(),
            border: Border {
                color: if active { t.accent } else { t.border_strong },
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

    let mut body = column![
        row![
            text(tr(lang, "label-gradient-presets")).size(11),
            preset_picker,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text(tr(lang, "label-gradient-stops")).size(11),
        stop_bar,
        stop_controls,
        slider_stepper_control(
            tr(lang, "label-stop-pos"),
            cur_stop.pos as f32,
            0.0..=1.0,
            0.01_f32,
            format!("{:.0}%", cur_stop.pos * 100.0),
            move |v| EditorMessage::EditorSetGradientStopPos(sel_idx, f64::from(v)),
        ),
        row![text(tr(lang, "label-gradient-kind")).size(11), kind_picker,]
            .spacing(8)
            .align_y(Alignment::Center),
        text(tr(lang, "label-gradient-repeat")).size(11),
        repeat_row,
        row![
            text(tr(lang, "label-gradient-blend")).size(11),
            blend_picker,
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        slider_stepper_control(
            tr(lang, "label-opacity"),
            ui.gradient_opacity,
            0.05..=1.0,
            0.05_f32,
            format!("{:.0}%", ui.gradient_opacity * 100.0),
            EditorMessage::EditorSetGradientOpacity,
        ),
        row![
            checkbox(ui.gradient_reverse)
                .label(tr(lang, "label-gradient-reverse"))
                .on_toggle(|_| EditorMessage::EditorToggleGradientReverse),
            checkbox(ui.gradient_dither)
                .label(tr(lang, "label-gradient-dither"))
                .on_toggle(|_| EditorMessage::EditorToggleGradientDither),
        ]
        .spacing(8),
        checkbox(ui.gradient_interactive)
            .label(tr(lang, "label-gradient-interactive"))
            .on_toggle(|_| EditorMessage::EditorToggleGradientInteractive),
    ]
    .spacing(6);

    if ui.gradient_draft.is_some() {
        body = body.push(
            row![
                button::labeled(
                    tr(lang, "action-cancel"),
                    Some(Icon::Close),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::EditorCancelGradientDraft),
                )
                .width(Fill),
                button::labeled(
                    tr(lang, "action-apply"),
                    Some(Icon::Check),
                    ButtonVariant::Primary,
                    Some(EditorMessage::EditorApplyGradientDraft),
                )
                .width(Fill),
            ]
            .spacing(4),
        );
    }

    body = body.push(
        text(tr(lang, "hint-gradient-interactive"))
            .size(10)
            .style(|theme| text::Style {
                color: Some(PaletteTokens::for_theme(theme).text_muted),
            }),
    );

    section_card(tr(lang, ui.tool.label_key()), body)
}
