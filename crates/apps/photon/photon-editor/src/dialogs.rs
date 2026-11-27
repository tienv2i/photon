use crate::*;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::dialog::DialogSpec;
use fw_ui::editor::checker_swatch;
use fw_ui::icons::Icon;
use fw_ui::theme::{PaletteTokens, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, input_style};
use fw_ui::typography::text;
use iced::widget::{canvas, checkbox, column, row, text_input};
use iced::{Alignment, Color, Fill, Length, Point, Rectangle, Size};
use photon_i18n::tr;
use photon_protocol::Language;

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = f32::from(r) / 255.0;
    let gf = f32::from(g) / 255.0;
    let bf = f32::from(b) / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if (max - rf).abs() <= f32::EPSILON {
        60.0 * (((gf - bf) / d).rem_euclid(6.0))
    } else if (max - gf).abs() <= f32::EPSILON {
        60.0 * (((bf - rf) / d) + 2.0)
    } else {
        60.0 * (((rf - gf) / d) + 4.0)
    };
    let s = if max <= f32::EPSILON { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (rf, gf, bf) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    [
        ((rf + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((gf + m).clamp(0.0, 1.0) * 255.0).round() as u8,
        ((bf + m).clamp(0.0, 1.0) * 255.0).round() as u8,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WheelDragTarget {
    Ring,
    Square,
}

#[derive(Debug, Clone, Copy, Default)]
struct ColorWheelState {
    dragging: Option<WheelDragTarget>,
}

struct ColorWheelProgram {
    hue: f32,
    sat: f32,
    val: f32,
}

impl ColorWheelProgram {
    fn geometry(bounds: Rectangle) -> (Point, f32, f32, f32) {
        let size = bounds.width.min(bounds.height).max(40.0);
        let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);
        let outer_r = size * 0.48;
        let inner_r = size * 0.35;
        let sq_half = inner_r * std::f32::consts::FRAC_1_SQRT_2 * 0.88;
        (center, outer_r, inner_r, sq_half)
    }

    fn emit_from_cursor(
        &self,
        pos: Point,
        bounds: Rectangle,
        target: WheelDragTarget,
    ) -> EditorMessage {
        let (center, _, _, sq_half) = Self::geometry(bounds);
        match target {
            WheelDragTarget::Ring => {
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let h = dy.atan2(dx).to_degrees().rem_euclid(360.0);
                let [nr, ng, nb] = hsv_to_rgb(h, self.sat, self.val);
                EditorMessage::ColorPickerSetRgb(nr, ng, nb)
            }
            WheelDragTarget::Square => {
                let s = ((pos.x - (center.x - sq_half)) / (sq_half * 2.0)).clamp(0.0, 1.0);
                let v = (1.0 - (pos.y - (center.y - sq_half)) / (sq_half * 2.0)).clamp(0.0, 1.0);
                let [nr, ng, nb] = hsv_to_rgb(self.hue, s, v);
                EditorMessage::ColorPickerSetRgb(nr, ng, nb)
            }
        }
    }
}

impl canvas::Program<EditorMessage> for ColorWheelProgram {
    type State = ColorWheelState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<EditorMessage>> {
        match event {
            canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                let pos = cursor.position_in(bounds)?;
                let (center, outer_r, inner_r, sq_half) = Self::geometry(bounds);
                let dx = pos.x - center.x;
                let dy = pos.y - center.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist >= inner_r - 2.0 && dist <= outer_r + 4.0 {
                    state.dragging = Some(WheelDragTarget::Ring);
                    return Some(
                        canvas::Action::publish(self.emit_from_cursor(
                            pos,
                            bounds,
                            WheelDragTarget::Ring,
                        ))
                        .and_capture(),
                    );
                }
                if dx.abs() <= sq_half + 4.0 && dy.abs() <= sq_half + 4.0 {
                    state.dragging = Some(WheelDragTarget::Square);
                    return Some(
                        canvas::Action::publish(self.emit_from_cursor(
                            pos,
                            bounds,
                            WheelDragTarget::Square,
                        ))
                        .and_capture(),
                    );
                }
                None
            }
            canvas::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                let target = state.dragging?;
                let pos = cursor.position_from(bounds.position())?;
                Some(
                    canvas::Action::publish(self.emit_from_cursor(pos, bounds, target))
                        .and_capture(),
                )
            }
            canvas::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                if state.dragging.take().is_some() {
                    Some(canvas::Action::capture())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (center, outer_r, inner_r, sq_half) = Self::geometry(bounds);

        // Hue ring (90 segments for smooth wheel).
        let segs = 90_u32;
        for i in 0..segs {
            let a0 = (i as f32 / segs as f32) * std::f32::consts::TAU;
            let a1 = ((i as f32 + 1.15) / segs as f32) * std::f32::consts::TAU;
            let [cr, cg, cb] = hsv_to_rgb((i as f32 / segs as f32) * 360.0, 1.0, 1.0);
            let p0 = Point::new(center.x + a0.cos() * inner_r, center.y + a0.sin() * inner_r);
            let p1 = Point::new(center.x + a0.cos() * outer_r, center.y + a0.sin() * outer_r);
            let p2 = Point::new(center.x + a1.cos() * outer_r, center.y + a1.sin() * outer_r);
            let p3 = Point::new(center.x + a1.cos() * inner_r, center.y + a1.sin() * inner_r);
            let path = canvas::Path::new(|b| {
                b.move_to(p0);
                b.line_to(p1);
                b.line_to(p2);
                b.line_to(p3);
                b.close();
            });
            frame.fill(&path, Color::from_rgb8(cr, cg, cb));
        }

        // Inner SV square (18x18 grid).
        let steps = 18_u32;
        let sq_left = center.x - sq_half;
        let sq_top = center.y - sq_half;
        let cell = (sq_half * 2.0) / steps as f32;
        for row in 0..steps {
            let v = 1.0 - (row as f32 + 0.5) / steps as f32;
            for col in 0..steps {
                let s = (col as f32 + 0.5) / steps as f32;
                let [cr, cg, cb] = hsv_to_rgb(self.hue, s, v);
                frame.fill_rectangle(
                    Point::new(sq_left + col as f32 * cell, sq_top + row as f32 * cell),
                    Size::new(cell + 0.6, cell + 0.6),
                    Color::from_rgb8(cr, cg, cb),
                );
            }
        }
        frame.stroke(
            &canvas::Path::rectangle(
                Point::new(sq_left, sq_top),
                Size::new(sq_half * 2.0, sq_half * 2.0),
            ),
            canvas::Stroke::default()
                .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.25))
                .with_width(1.0),
        );

        // Hue indicator on ring.
        let hue_rad = self.hue.to_radians();
        let mid_r = (inner_r + outer_r) * 0.5;
        let h_pt = Point::new(
            center.x + hue_rad.cos() * mid_r,
            center.y + hue_rad.sin() * mid_r,
        );
        frame.stroke(
            &canvas::Path::circle(h_pt, 4.5),
            canvas::Stroke::default()
                .with_color(Color::WHITE)
                .with_width(2.0),
        );

        // SV indicator in square.
        let sv_pt = Point::new(
            sq_left + self.sat.clamp(0.0, 1.0) * sq_half * 2.0,
            sq_top + (1.0 - self.val.clamp(0.0, 1.0)) * sq_half * 2.0,
        );
        frame.stroke(
            &canvas::Path::circle(sv_pt, 5.0),
            canvas::Stroke::default()
                .with_color(if self.val > 0.55 && self.sat < 0.45 {
                    Color::BLACK
                } else {
                    Color::WHITE
                })
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_color_picker_dialog<'a>(
    lang: Language,
    target: ColorPickerTarget,
    r: u8,
    g: u8,
    b: u8,
    hex: &'a str,
    transparent: bool,
) -> DialogSpec<'a, EditorMessage> {
    let allow_transparent = !matches!(
        target,
        ColorPickerTarget::Foreground
            | ColorPickerTarget::ViewportBackground
            | ColorPickerTarget::ViewerViewportBg
    );
    let preview_color = if transparent && allow_transparent {
        None
    } else {
        Some(Color::from_rgb8(r, g, b))
    };

    const PRESETS: [[u8; 3]; 8] = [
        [0, 0, 0],
        [255, 255, 255],
        [239, 68, 68],
        [249, 115, 22],
        [234, 179, 8],
        [34, 197, 94],
        [59, 130, 246],
        [168, 85, 247],
    ];
    let mut preset_row = row![].spacing(6);
    for p in PRESETS {
        preset_row = preset_row.push(checker_swatch(
            Some(rgb_to_color(p)),
            22.0,
            22.0,
            Some(EditorMessage::ColorPickerSetRgb(p[0], p[1], p[2])),
        ));
    }

    let (hue, sat, val) = rgb_to_hsv(r, g, b);
    let wheel = iced::widget::canvas(ColorWheelProgram { hue, sat, val })
        .width(Length::Fixed(164.0))
        .height(Length::Fixed(164.0));

    let right_controls = column![
        row![
            checker_swatch(preview_color, 40.0, 40.0, None),
            column![
                text("Hex").size(TEXT_SIZE_CAPTION),
                text_input("#000000", hex)
                    .on_input(EditorMessage::ColorPickerHexChanged)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([5, 8])
                    .style(input_style),
            ]
            .spacing(3)
            .width(Fill),
            button::icon_only_tipped(
                Icon::Eyedropper,
                ButtonVariant::Secondary,
                Some(EditorMessage::ColorPickerPickScreen),
                tr(lang, "tool-eyedropper"),
            ),
        ]
        .spacing(8)
        .align_y(Alignment::End),
        slider_stepper_control(
            "R",
            f32::from(r),
            0.0..=255.0,
            1.0_f32,
            r.to_string(),
            move |v| EditorMessage::ColorPickerSetRgb(v.round() as u8, g, b),
        ),
        slider_stepper_control(
            "G",
            f32::from(g),
            0.0..=255.0,
            1.0_f32,
            g.to_string(),
            move |v| EditorMessage::ColorPickerSetRgb(r, v.round() as u8, b),
        ),
        slider_stepper_control(
            "B",
            f32::from(b),
            0.0..=255.0,
            1.0_f32,
            b.to_string(),
            move |v| EditorMessage::ColorPickerSetRgb(r, g, v.round() as u8),
        ),
    ]
    .spacing(6)
    .width(Fill);

    let mut body = column![
        row![wheel, right_controls]
            .spacing(14)
            .align_y(Alignment::Center),
        preset_row,
    ]
    .spacing(10);

    if allow_transparent {
        body = body.push(
            checkbox(transparent)
                .label(tr(lang, "label-transparent"))
                .text_size(TEXT_SIZE_CONTROL)
                .on_toggle(EditorMessage::ColorPickerToggleTransparent),
        );
    }

    let title = match target {
        ColorPickerTarget::Foreground => tr(lang, "label-foreground"),
        ColorPickerTarget::Background => tr(lang, "label-background"),
        ColorPickerTarget::ViewportBackground => tr(lang, "label-viewport-bg"),
        ColorPickerTarget::ViewerViewportBg => tr(lang, "menu-viewer-backdrop"),
        ColorPickerTarget::CanvasBackground
        | ColorPickerTarget::CropFill
        | ColorPickerTarget::CanvasSizeFill
        | ColorPickerTarget::RotateFill => tr(lang, "label-canvas-bg"),
        ColorPickerTarget::GradientStop(_) => tr(lang, "label-gradient-stops"),
        ColorPickerTarget::VectorShapeFill => tr(lang, "shape-fill-fill"),
        ColorPickerTarget::VectorShapeStroke => tr(lang, "shape-fill-stroke"),
        ColorPickerTarget::TextLayerColor => tr(lang, "label-text-color"),
    };

    let mut footer = row![].spacing(8).align_y(Alignment::Center);
    if matches!(
        target,
        ColorPickerTarget::ViewportBackground | ColorPickerTarget::ViewerViewportBg
    ) {
        footer = footer.push(button::labeled(
            tr(lang, "action-reset"),
            Some(Icon::Reset),
            ButtonVariant::Ghost,
            Some(EditorMessage::ResetViewportBackground),
        ));
    }
    let footer = footer
        .push(iced::widget::space::horizontal())
        .push(button::labeled(
            tr(lang, "dialog-cancel"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::CloseDialog),
        ))
        .push(button::labeled(
            "OK",
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(EditorMessage::ConfirmColorPicker),
        ));

    DialogSpec {
        open: true,
        title,
        width: 380.0,
        body: body.into(),
        footer: footer.into(),
        on_close: EditorMessage::CloseDialog,
        on_noop: EditorMessage::Noop,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_export_dialog<'a>(
    lang: Language,
    path: &'a str,
    quality: u8,
    max_edge: &'a str,
    metadata: bool,
    error: Option<&'a str>,
) -> DialogSpec<'a, EditorMessage> {
    let format_chip = |ext: &str, label: &str| {
        let p = std::path::Path::new(path);
        let active = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(ext));
        let next_path = if path.trim().is_empty() {
            format!("export.{ext}")
        } else if p.extension().is_some() {
            p.with_extension(ext).to_string_lossy().into_owned()
        } else {
            format!("{}.{ext}", path.trim_end_matches('.'))
        };
        button::labeled(
            label.to_owned(),
            None,
            ButtonVariant::Tab { active },
            Some(EditorMessage::ExportPathChanged(next_path)),
        )
        .padding([3, 8])
    };

    let mut body = column![
        column![
            text(tr(lang, "label-export-file")).size(TEXT_SIZE_CONTROL),
            row![
                text_input("/path/to/export.jpg", path)
                    .on_input(EditorMessage::ExportPathChanged)
                    .size(TEXT_SIZE_CONTROL)
                    .padding([7, 10])
                    .style(input_style)
                    .width(Fill),
                button::labeled(
                    "…",
                    Some(Icon::FolderOpen),
                    ButtonVariant::Secondary,
                    Some(EditorMessage::PickExportDialog),
                ),
            ]
            .spacing(6),
            row![
                format_chip("png", "PNG"),
                format_chip("jpg", "JPG"),
                format_chip("webp", "WebP"),
                format_chip("ora", "ORA"),
                format_chip("avif", "AVIF"),
                format_chip("tiff", "TIFF"),
            ]
            .spacing(4),
        ]
        .spacing(4),
        slider_stepper_control(
            tr(lang, "label-quality"),
            f32::from(quality),
            10.0..=100.0,
            1.0_f32,
            format!("{quality}%"),
            |v| EditorMessage::ExportQualityChanged(v.round() as u8),
        ),
        column![
            text(tr(lang, "label-max-edge")).size(TEXT_SIZE_CONTROL),
            text_input(&tr(lang, "label-original-size"), max_edge)
                .on_input(EditorMessage::ExportMaxEdgeChanged)
                .size(TEXT_SIZE_CONTROL)
                .padding([7, 10])
                .style(input_style),
        ]
        .spacing(4),
        checkbox(metadata)
            .label(tr(lang, "label-keep-metadata"))
            .text_size(TEXT_SIZE_CONTROL)
            .on_toggle(EditorMessage::ExportMetadataToggled),
    ]
    .spacing(10);

    if let Some(err) = error {
        body = body.push(text(err.to_owned()).size(12).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.danger),
            }
        }));
    }

    let footer = row![
        iced::widget::space::horizontal(),
        button::labeled(
            tr(lang, "dialog-cancel"),
            None,
            ButtonVariant::Secondary,
            Some(EditorMessage::CloseDialog),
        ),
        button::labeled(
            tr(lang, "action-export"),
            Some(Icon::Export),
            ButtonVariant::Primary,
            Some(EditorMessage::ConfirmExport),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    DialogSpec {
        open: true,
        title: tr(lang, "dialog-export-title"),
        width: 460.0,
        body: body.into(),
        footer: footer.into(),
        on_close: EditorMessage::CloseDialog,
        on_noop: EditorMessage::Noop,
    }
}
