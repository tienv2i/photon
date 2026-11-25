//! Dedicated Batch processing modal dialogs (`Batch Resize`, `Batch Crop`, `Batch Convert`)
//! with Print Units (`px` / `cm` / `in` / `%`), DPI selection, source selection, and
//! output/overwrite guardrails.

use iced::widget::{checkbox, column, container, row, rule, slider, text, text_input};
use iced::{Alignment, Element, Fill, Length};
use photon_i18n::tr;
use photon_protocol::{
    BatchCropUnit, BatchImageFormat, BatchOperation, BatchOutput, BatchSource, CanvasAnchor,
    Command, ResizeFilter, ResizeMode, UiStateSnapshot,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::dialog::{ManagedDialogSpec, OverlayMode, dialog_style_for_theme};
use photon_ui::editor::{
    DimensionRatioSpec, LengthUnit, anchor_grid_3x3, dimension_ratio_grid, px_to_unit, unit_to_px,
};
use photon_ui::icons::Icon;
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, card_style, hairline_rule_style,
    input_style, slider_style,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip_pos};

use crate::app::{DesktopDialogId, Message, PhotonGui, default_window_state_for};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchDialogKind {
    Resize,
    Crop,
    Convert,
}

impl BatchDialogKind {
    pub const fn dialog_id(self) -> DesktopDialogId {
        match self {
            Self::Resize => DesktopDialogId::BatchResize,
            Self::Crop => DesktopDialogId::BatchCrop,
            Self::Convert => DesktopDialogId::BatchConvert,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BatchDialogMsg {
    UseSelection(bool),
    BesideOriginals(bool),
    OutputFolderChanged(String),
    PickOutputFolder,
    OutputFolderPicked(Option<String>),
    SuffixChanged(String),
    OverwriteToggled(bool),
    ResizeWidthChanged(String),
    ResizeHeightChanged(String),
    ResizeRatioChanged(String),
    ResizeUnitChanged(LengthUnit),
    ResizeDpiChanged(f32),
    ResizeToggleLockWidth,
    ResizeToggleLockHeight,
    ResizeToggleLockRatio,
    ResizeApplyRatioPreset(f32),
    ResizeModeChanged(ResizeMode),
    ResizeFilterChanged(ResizeFilter),
    CropXChanged(String),
    CropYChanged(String),
    CropWChanged(String),
    CropHChanged(String),
    CropUnitChanged(BatchCropUnit),
    CropAnchorChanged(Option<CanvasAnchor>),
    CropRatioChanged(Option<f64>),
    ConvertFormatChanged(BatchImageFormat),
    ConvertQualityChanged(u8),
    Confirm,
}

#[derive(Debug, Clone)]
pub struct BatchDialogState {
    pub kind: BatchDialogKind,
    pub use_selection: bool,
    pub beside_originals: bool,
    pub output_folder: String,
    pub suffix: String,
    pub overwrite: bool,
    pub resize_width_str: String,
    pub resize_height_str: String,
    pub resize_ratio_str: String,
    pub resize_unit: LengthUnit,
    pub resize_dpi: f32,
    pub resize_lock_width: bool,
    pub resize_lock_height: bool,
    pub resize_lock_ratio: bool,
    pub resize_mode: ResizeMode,
    pub resize_filter: ResizeFilter,
    pub resize_ref_px: (u32, u32),
    pub crop_x_str: String,
    pub crop_y_str: String,
    pub crop_w_str: String,
    pub crop_h_str: String,
    pub crop_unit: BatchCropUnit,
    pub crop_anchor: Option<CanvasAnchor>,
    pub crop_ratio: Option<f64>,
    pub convert_format: BatchImageFormat,
    pub convert_quality: u8,
    pub error: Option<String>,
}

impl BatchDialogState {
    pub fn new(
        kind: BatchDialogKind,
        has_selection_images: bool,
        current_folder: Option<&str>,
        ref_dims: Option<(u32, u32)>,
    ) -> Self {
        let (ref_w, ref_h) = ref_dims
            .filter(|(w, h)| *w > 0 && *h > 0)
            .unwrap_or((1920, 1080));
        let default_suffix = match kind {
            BatchDialogKind::Resize => "_resized",
            BatchDialogKind::Crop => "_cropped",
            BatchDialogKind::Convert => "",
        };
        Self {
            kind,
            use_selection: has_selection_images,
            beside_originals: true,
            output_folder: current_folder.unwrap_or_default().to_owned(),
            suffix: default_suffix.to_owned(),
            overwrite: false,
            resize_width_str: ref_w.to_string(),
            resize_height_str: ref_h.to_string(),
            resize_ratio_str: format_ratio(ref_w as f32 / ref_h as f32),
            resize_unit: LengthUnit::Px,
            resize_dpi: 96.0,
            resize_lock_width: false,
            resize_lock_height: false,
            resize_lock_ratio: true,
            resize_mode: ResizeMode::Stretch,
            resize_filter: ResizeFilter::Lanczos3,
            resize_ref_px: (ref_w, ref_h),
            crop_x_str: "0".to_owned(),
            crop_y_str: "0".to_owned(),
            crop_w_str: "80".to_owned(),
            crop_h_str: "80".to_owned(),
            crop_unit: BatchCropUnit::Percent,
            crop_anchor: Some(CanvasAnchor::Center),
            crop_ratio: None,
            convert_format: BatchImageFormat::Webp,
            convert_quality: 90,
            error: None,
        }
    }

    pub fn apply_msg(&mut self, msg: BatchDialogMsg) {
        self.error = None;
        match msg {
            BatchDialogMsg::UseSelection(v) => self.use_selection = v,
            BatchDialogMsg::BesideOriginals(v) => self.beside_originals = v,
            BatchDialogMsg::OutputFolderChanged(v) => self.output_folder = v,
            BatchDialogMsg::OutputFolderPicked(Some(folder)) => {
                self.output_folder = folder;
                self.beside_originals = false;
            }
            BatchDialogMsg::PickOutputFolder | BatchDialogMsg::OutputFolderPicked(None) => {}
            BatchDialogMsg::SuffixChanged(v) => self.suffix = v,
            BatchDialogMsg::OverwriteToggled(v) => self.overwrite = v,
            BatchDialogMsg::ResizeWidthChanged(val) => {
                if self.resize_lock_width {
                    return;
                }
                self.resize_width_str = val.clone();
                if self.resize_lock_ratio
                    && let Some(w_val) = parse_positive_f32(&val)
                {
                    let w_px = unit_to_px(
                        w_val,
                        self.resize_unit,
                        self.resize_dpi,
                        self.resize_ref_px.0,
                    );
                    if let Some(ratio) = parse_ratio(&self.resize_ratio_str)
                        && !self.resize_lock_height
                    {
                        let h_px = ((w_px as f32) / ratio).round().max(1.0) as u32;
                        let h_unit = px_to_unit(
                            h_px,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.1,
                        );
                        self.resize_height_str = format_unit_value(h_unit, self.resize_unit);
                    } else if let Some(h_val) = parse_positive_f32(&self.resize_height_str) {
                        let h_px = unit_to_px(
                            h_val,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.1,
                        );
                        self.resize_ratio_str = format_ratio(w_px as f32 / h_px.max(1) as f32);
                    }
                }
            }
            BatchDialogMsg::ResizeHeightChanged(val) => {
                if self.resize_lock_height {
                    return;
                }
                self.resize_height_str = val.clone();
                if self.resize_lock_ratio
                    && let Some(h_val) = parse_positive_f32(&val)
                {
                    let h_px = unit_to_px(
                        h_val,
                        self.resize_unit,
                        self.resize_dpi,
                        self.resize_ref_px.1,
                    );
                    if let Some(ratio) = parse_ratio(&self.resize_ratio_str)
                        && !self.resize_lock_width
                    {
                        let w_px = ((h_px as f32) * ratio).round().max(1.0) as u32;
                        let w_unit = px_to_unit(
                            w_px,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.0,
                        );
                        self.resize_width_str = format_unit_value(w_unit, self.resize_unit);
                    } else if let Some(w_val) = parse_positive_f32(&self.resize_width_str) {
                        let w_px = unit_to_px(
                            w_val,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.0,
                        );
                        self.resize_ratio_str = format_ratio(w_px as f32 / h_px.max(1) as f32);
                    }
                }
            }
            BatchDialogMsg::ResizeRatioChanged(val) => {
                if self.resize_lock_ratio {
                    return;
                }
                self.resize_ratio_str = val.clone();
                if let Some(ratio) = parse_ratio(&val) {
                    self.apply_aspect_ratio(ratio);
                }
            }
            BatchDialogMsg::ResizeUnitChanged(next_unit) => {
                if self.resize_unit == next_unit {
                    return;
                }
                let (w_px, h_px) = self.current_resize_px();
                self.resize_unit = next_unit;
                self.resize_width_str = format_unit_value(
                    px_to_unit(w_px, next_unit, self.resize_dpi, self.resize_ref_px.0),
                    next_unit,
                );
                self.resize_height_str = format_unit_value(
                    px_to_unit(h_px, next_unit, self.resize_dpi, self.resize_ref_px.1),
                    next_unit,
                );
            }
            BatchDialogMsg::ResizeDpiChanged(next_dpi) => {
                let (w_px, h_px) = self.current_resize_px();
                self.resize_dpi = next_dpi.clamp(1.0, 2400.0);
                if matches!(self.resize_unit, LengthUnit::Cm | LengthUnit::Inch) {
                    self.resize_width_str = format_unit_value(
                        px_to_unit(
                            w_px,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.0,
                        ),
                        self.resize_unit,
                    );
                    self.resize_height_str = format_unit_value(
                        px_to_unit(
                            h_px,
                            self.resize_unit,
                            self.resize_dpi,
                            self.resize_ref_px.1,
                        ),
                        self.resize_unit,
                    );
                }
            }
            BatchDialogMsg::ResizeToggleLockWidth => {
                self.resize_lock_width = !self.resize_lock_width;
            }
            BatchDialogMsg::ResizeToggleLockHeight => {
                self.resize_lock_height = !self.resize_lock_height;
            }
            BatchDialogMsg::ResizeToggleLockRatio => {
                self.resize_lock_ratio = !self.resize_lock_ratio;
                if self.resize_lock_ratio {
                    let (w_px, h_px) = self.current_resize_px();
                    self.resize_ratio_str = format_ratio(w_px as f32 / h_px.max(1) as f32);
                }
            }
            BatchDialogMsg::ResizeApplyRatioPreset(ratio) => {
                self.resize_lock_ratio = true;
                self.resize_ratio_str = format_ratio(ratio);
                self.apply_aspect_ratio(ratio);
            }
            BatchDialogMsg::ResizeModeChanged(mode) => self.resize_mode = mode,
            BatchDialogMsg::ResizeFilterChanged(filter) => self.resize_filter = filter,
            BatchDialogMsg::CropXChanged(v) => self.crop_x_str = v,
            BatchDialogMsg::CropYChanged(v) => self.crop_y_str = v,
            BatchDialogMsg::CropWChanged(v) => self.crop_w_str = v,
            BatchDialogMsg::CropHChanged(v) => self.crop_h_str = v,
            BatchDialogMsg::CropUnitChanged(unit) => self.crop_unit = unit,
            BatchDialogMsg::CropAnchorChanged(anchor) => self.crop_anchor = anchor,
            BatchDialogMsg::CropRatioChanged(ratio) => self.crop_ratio = ratio,
            BatchDialogMsg::ConvertFormatChanged(fmt) => self.convert_format = fmt,
            BatchDialogMsg::ConvertQualityChanged(q) => self.convert_quality = q.clamp(1, 100),
            BatchDialogMsg::Confirm => {}
        }
    }

    fn current_resize_px(&self) -> (u32, u32) {
        let w = parse_positive_f32(&self.resize_width_str)
            .map(|v| unit_to_px(v, self.resize_unit, self.resize_dpi, self.resize_ref_px.0))
            .unwrap_or(self.resize_ref_px.0);
        let h = parse_positive_f32(&self.resize_height_str)
            .map(|v| unit_to_px(v, self.resize_unit, self.resize_dpi, self.resize_ref_px.1))
            .unwrap_or(self.resize_ref_px.1);
        (w.clamp(1, 32_768), h.clamp(1, 32_768))
    }

    fn apply_aspect_ratio(&mut self, ratio: f32) {
        if ratio <= 0.0 || !ratio.is_finite() {
            return;
        }
        let (w_px, h_px) = self.current_resize_px();
        if !self.resize_lock_height {
            let next_h = ((w_px as f32) / ratio).round().clamp(1.0, 32_768.0) as u32;
            self.resize_height_str = format_unit_value(
                px_to_unit(
                    next_h,
                    self.resize_unit,
                    self.resize_dpi,
                    self.resize_ref_px.1,
                ),
                self.resize_unit,
            );
        } else if !self.resize_lock_width {
            let next_w = ((h_px as f32) * ratio).round().clamp(1.0, 32_768.0) as u32;
            self.resize_width_str = format_unit_value(
                px_to_unit(
                    next_w,
                    self.resize_unit,
                    self.resize_dpi,
                    self.resize_ref_px.0,
                ),
                self.resize_unit,
            );
        }
    }

    /// Build `Command::BatchRun` or return a localized i18n error key / message.
    pub fn build_command(&self, snapshot: &UiStateSnapshot) -> Result<Command, String> {
        let lang = snapshot.meta.language;
        let selected_images: Vec<String> = snapshot
            .entries
            .iter()
            .filter(|e| e.is_image && snapshot.meta.selection.contains(&e.path))
            .map(|e| e.path.clone())
            .collect();
        let source = if self.use_selection {
            if selected_images.is_empty() {
                return Err(tr(lang, "batch-no-images"));
            }
            BatchSource::Selection {
                paths: selected_images,
            }
        } else {
            let folder = snapshot
                .meta
                .current_folder
                .clone()
                .ok_or_else(|| tr(lang, "label-no-folder"))?;
            BatchSource::Folder { path: folder }
        };

        let folder = if self.beside_originals {
            None
        } else {
            let trimmed = self.output_folder.trim();
            if trimmed.is_empty() {
                return Err(tr(lang, "dialog-error-title"));
            }
            Some(trimmed.to_owned())
        };
        let output = BatchOutput {
            folder,
            suffix: self.suffix.trim().to_owned(),
            overwrite: self.overwrite,
        };

        let operation = match self.kind {
            BatchDialogKind::Resize => {
                let w_val = parse_positive_f32(&self.resize_width_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                let h_val = parse_positive_f32(&self.resize_height_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                let width = unit_to_px(
                    w_val,
                    self.resize_unit,
                    self.resize_dpi,
                    self.resize_ref_px.0,
                );
                let height = unit_to_px(
                    h_val,
                    self.resize_unit,
                    self.resize_dpi,
                    self.resize_ref_px.1,
                );
                if !(1..=32_768).contains(&width) || !(1..=32_768).contains(&height) {
                    return Err(tr(lang, "dialog-error-title"));
                }
                BatchOperation::Resize {
                    width,
                    height,
                    mode: self.resize_mode,
                    filter: self.resize_filter,
                    fill: None,
                    fill_alpha: 255,
                }
            }
            BatchDialogKind::Crop => {
                let x = parse_nonneg_f64(&self.crop_x_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                let y = parse_nonneg_f64(&self.crop_y_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                let width = parse_positive_f64(&self.crop_w_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                let height = parse_positive_f64(&self.crop_h_str)
                    .ok_or_else(|| tr(lang, "dialog-error-title"))?;
                BatchOperation::Crop {
                    x,
                    y,
                    width,
                    height,
                    unit: self.crop_unit,
                    anchor: self.crop_anchor,
                    ratio: self.crop_ratio,
                }
            }
            BatchDialogKind::Convert => {
                let supports_quality = matches!(
                    self.convert_format,
                    BatchImageFormat::Jpeg | BatchImageFormat::Webp
                );
                BatchOperation::Convert {
                    format: self.convert_format,
                    quality: supports_quality.then_some(self.convert_quality),
                }
            }
        };

        Ok(Command::BatchRun {
            operation,
            source,
            output,
        })
    }
}

pub fn build_batch_resize_dialog<'a>(
    state: &'a PhotonGui,
    batch: &'a BatchDialogState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    build_batch_dialog_for_kind(BatchDialogKind::Resize, state, batch, theme)
}

pub fn build_batch_crop_dialog<'a>(
    state: &'a PhotonGui,
    batch: &'a BatchDialogState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    build_batch_dialog_for_kind(BatchDialogKind::Crop, state, batch, theme)
}

pub fn build_batch_convert_dialog<'a>(
    state: &'a PhotonGui,
    batch: &'a BatchDialogState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    build_batch_dialog_for_kind(BatchDialogKind::Convert, state, batch, theme)
}

fn build_batch_dialog_for_kind<'a>(
    kind: BatchDialogKind,
    state: &'a PhotonGui,
    batch: &'a BatchDialogState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, DesktopDialogId, Message> {
    let lang = state.snapshot.meta.language;
    let selected_img_count = state
        .snapshot
        .entries
        .iter()
        .filter(|e| e.is_image && state.snapshot.meta.selection.contains(&e.path))
        .count();
    let folder_img_count = state.snapshot.entries.iter().filter(|e| e.is_image).count();

    let source_section = container(
        column![
            text(tr(lang, "batch-source"))
                .size(TEXT_SIZE_CONTROL)
                .style(|theme| {
                    let t = PaletteTokens::for_theme(theme);
                    text::Style {
                        color: Some(t.text_muted),
                    }
                }),
            row![
                button::labeled(
                    format!(
                        "{} ({selected_img_count})",
                        tr(lang, "batch-source-selection")
                    ),
                    None,
                    ButtonVariant::Tab {
                        active: batch.use_selection,
                    },
                    (selected_img_count > 0)
                        .then_some(Message::BatchDialog(BatchDialogMsg::UseSelection(true))),
                ),
                button::labeled(
                    format!("{} ({folder_img_count})", tr(lang, "batch-source-folder")),
                    None,
                    ButtonVariant::Tab {
                        active: !batch.use_selection,
                    },
                    Some(Message::BatchDialog(BatchDialogMsg::UseSelection(false))),
                ),
            ]
            .spacing(6),
        ]
        .spacing(6),
    )
    .padding(10)
    .width(Fill)
    .style(card_style);

    let op_section: Element<'a, Message> = match kind {
        BatchDialogKind::Resize => build_batch_resize_section(lang, batch),
        BatchDialogKind::Crop => build_batch_crop_section(lang, batch),
        BatchDialogKind::Convert => build_batch_convert_section(lang, batch),
    };

    let mut output_col = column![
        text(tr(lang, "batch-output"))
            .size(TEXT_SIZE_CONTROL)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        row![
            button::labeled(
                tr(lang, "batch-output-beside"),
                None,
                ButtonVariant::Tab {
                    active: batch.beside_originals,
                },
                Some(Message::BatchDialog(BatchDialogMsg::BesideOriginals(true))),
            ),
            button::labeled(
                tr(lang, "batch-output-folder"),
                None,
                ButtonVariant::Tab {
                    active: !batch.beside_originals,
                },
                Some(Message::BatchDialog(BatchDialogMsg::BesideOriginals(false))),
            ),
        ]
        .spacing(6),
    ]
    .spacing(8);

    if !batch.beside_originals {
        output_col = output_col.push(
            row![
                text_input("/path/to/output", &batch.output_folder)
                    .on_input(|v| Message::BatchDialog(BatchDialogMsg::OutputFolderChanged(v)))
                    .padding([6, 8])
                    .size(TEXT_SIZE_CONTROL)
                    .style(input_style)
                    .width(Fill),
                with_tooltip_pos(
                    button::icon_only(
                        Icon::FolderOpen,
                        ButtonVariant::Secondary,
                        Some(Message::BatchDialog(BatchDialogMsg::PickOutputFolder)),
                    ),
                    tr(lang, "batch-output-folder"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    output_col = output_col.push(
        row![
            column![
                text(tr(lang, "batch-suffix"))
                    .size(TEXT_SIZE_CAPTION)
                    .style(|theme| {
                        let t = PaletteTokens::for_theme(theme);
                        text::Style {
                            color: Some(t.text_muted),
                        }
                    }),
                text_input("_edited", &batch.suffix)
                    .on_input(|v| Message::BatchDialog(BatchDialogMsg::SuffixChanged(v)))
                    .padding([6, 8])
                    .size(TEXT_SIZE_CONTROL)
                    .style(input_style),
            ]
            .spacing(3)
            .width(Length::Fixed(160.0)),
            container(
                checkbox(batch.overwrite)
                    .label(tr(lang, "batch-overwrite"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(|v| Message::BatchDialog(BatchDialogMsg::OverwriteToggled(v))),
            )
            .padding([16, 0]),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    );

    let output_section = container(output_col)
        .padding(10)
        .width(Fill)
        .style(card_style);

    let mut body_col = column![source_section, op_section, output_section].spacing(10);

    if let Some(err) = &batch.error {
        body_col = body_col.push(text(err.clone()).size(TEXT_SIZE_CONTROL).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.danger),
            }
        }));
    }

    let can_run = if batch.use_selection {
        selected_img_count > 0
    } else {
        folder_img_count > 0
    };

    let footer = row![
        with_tooltip_pos(
            button::labeled(
                tr(lang, "dialog-cancel"),
                None,
                ButtonVariant::Ghost,
                Some(Message::CloseDialog),
            ),
            tr(lang, "tooltip-dialog-close"),
            TooltipPosition::Top,
        ),
        iced::widget::space::horizontal(),
        with_tooltip_pos(
            button::labeled(
                tr(lang, "batch-run"),
                Some(Icon::Check),
                ButtonVariant::Primary,
                can_run.then_some(Message::BatchDialog(BatchDialogMsg::Confirm)),
            ),
            tr(lang, "batch-run"),
            TooltipPosition::Top,
        ),
    ]
    .align_y(Alignment::Center);

    let id = kind.dialog_id();
    let title = match kind {
        BatchDialogKind::Resize => tr(lang, "batch-resize-title"),
        BatchDialogKind::Crop => tr(lang, "batch-crop-title"),
        BatchDialogKind::Convert => tr(lang, "batch-convert-title"),
    };

    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(Message::CloseDialog),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );

    ManagedDialogSpec {
        id,
        open: true,
        title,
        defaults: default_window_state_for(id),
        resizable: true,
        draggable: true,
        overlay: OverlayMode::default(),
        body: body_col.into(),
        footer: Some(footer.into()),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(Message::DialogEvent),
        on_close: Message::CloseDialog,
        on_noop: Message::Noop,
    }
}

fn build_batch_resize_section<'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, Message> {
    let grid = dimension_ratio_grid(DimensionRatioSpec {
        width_label: tr(lang, "label-width"),
        height_label: tr(lang, "label-height"),
        ratio_label: tr(lang, "label-presets"),
        unit_label: tr(lang, "label-unit"),
        dpi_label: tr(lang, "label-dpi"),
        width_str: &batch.resize_width_str,
        height_str: &batch.resize_height_str,
        ratio_str: &batch.resize_ratio_str,
        unit: batch.resize_unit,
        dpi: batch.resize_dpi,
        lock_width: batch.resize_lock_width,
        lock_height: batch.resize_lock_height,
        lock_ratio: batch.resize_lock_ratio,
        on_width_change: Box::new(|v| Message::BatchDialog(BatchDialogMsg::ResizeWidthChanged(v))),
        on_height_change: Box::new(|v| {
            Message::BatchDialog(BatchDialogMsg::ResizeHeightChanged(v))
        }),
        on_ratio_change: Box::new(|v| Message::BatchDialog(BatchDialogMsg::ResizeRatioChanged(v))),
        on_unit_change: Box::new(|u| Message::BatchDialog(BatchDialogMsg::ResizeUnitChanged(u))),
        on_dpi_change: Box::new(|d| Message::BatchDialog(BatchDialogMsg::ResizeDpiChanged(d))),
        on_toggle_lock_width: Message::BatchDialog(BatchDialogMsg::ResizeToggleLockWidth),
        on_toggle_lock_height: Message::BatchDialog(BatchDialogMsg::ResizeToggleLockHeight),
        on_toggle_lock_ratio: Message::BatchDialog(BatchDialogMsg::ResizeToggleLockRatio),
        ratio_presets: vec![
            (
                "1:1".to_owned(),
                Message::BatchDialog(BatchDialogMsg::ResizeApplyRatioPreset(1.0)),
            ),
            (
                "4:3".to_owned(),
                Message::BatchDialog(BatchDialogMsg::ResizeApplyRatioPreset(4.0 / 3.0)),
            ),
            (
                "3:2".to_owned(),
                Message::BatchDialog(BatchDialogMsg::ResizeApplyRatioPreset(1.5)),
            ),
            (
                "16:9".to_owned(),
                Message::BatchDialog(BatchDialogMsg::ResizeApplyRatioPreset(16.0 / 9.0)),
            ),
        ],
    });

    let mode_row = row![
        button::labeled(
            tr(lang, "mode-stretch"),
            None,
            ButtonVariant::Tab {
                active: batch.resize_mode == ResizeMode::Stretch,
            },
            Some(Message::BatchDialog(BatchDialogMsg::ResizeModeChanged(
                ResizeMode::Stretch,
            ))),
        ),
        button::labeled(
            tr(lang, "mode-cover"),
            None,
            ButtonVariant::Tab {
                active: batch.resize_mode == ResizeMode::Cover,
            },
            Some(Message::BatchDialog(BatchDialogMsg::ResizeModeChanged(
                ResizeMode::Cover,
            ))),
        ),
        button::labeled(
            tr(lang, "mode-pad"),
            None,
            ButtonVariant::Tab {
                active: batch.resize_mode == ResizeMode::Pad,
            },
            Some(Message::BatchDialog(BatchDialogMsg::ResizeModeChanged(
                ResizeMode::Pad,
            ))),
        ),
    ]
    .spacing(4);

    let filter_row = row![
        filter_tab("Lanczos3", ResizeFilter::Lanczos3, batch.resize_filter),
        filter_tab("CatmullRom", ResizeFilter::CatmullRom, batch.resize_filter),
        filter_tab("Triangle", ResizeFilter::Triangle, batch.resize_filter),
        filter_tab("Nearest", ResizeFilter::Nearest, batch.resize_filter),
    ]
    .spacing(4);

    container(
        column![
            grid,
            rule::horizontal(1).style(hairline_rule_style),
            row![
                column![
                    text(tr(lang, "label-resize-mode")).size(TEXT_SIZE_CAPTION),
                    mode_row
                ]
                .spacing(4)
                .width(Fill),
            ],
            column![
                text(tr(lang, "label-filter")).size(TEXT_SIZE_CAPTION),
                filter_row
            ]
            .spacing(4),
        ]
        .spacing(8),
    )
    .padding(10)
    .width(Fill)
    .style(card_style)
    .into()
}

fn filter_tab<'a>(
    label: &'static str,
    filter: ResizeFilter,
    active_filter: ResizeFilter,
) -> Element<'a, Message> {
    button::labeled(
        label,
        None,
        ButtonVariant::Tab {
            active: filter == active_filter,
        },
        Some(Message::BatchDialog(BatchDialogMsg::ResizeFilterChanged(
            filter,
        ))),
    )
    .into()
}

fn build_batch_crop_section<'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, Message> {
    let unit_row = row![
        button::labeled(
            "%",
            None,
            ButtonVariant::Tab {
                active: batch.crop_unit == BatchCropUnit::Percent,
            },
            Some(Message::BatchDialog(BatchDialogMsg::CropUnitChanged(
                BatchCropUnit::Percent,
            ))),
        ),
        button::labeled(
            "px",
            None,
            ButtonVariant::Tab {
                active: batch.crop_unit == BatchCropUnit::Pixels,
            },
            Some(Message::BatchDialog(BatchDialogMsg::CropUnitChanged(
                BatchCropUnit::Pixels,
            ))),
        ),
    ]
    .spacing(4);

    let dims_row = row![
        labeled_small_input("W", &batch.crop_w_str, |v| {
            Message::BatchDialog(BatchDialogMsg::CropWChanged(v))
        }),
        labeled_small_input("H", &batch.crop_h_str, |v| {
            Message::BatchDialog(BatchDialogMsg::CropHChanged(v))
        }),
    ]
    .spacing(8);

    let xy_row = row![
        labeled_small_input("X", &batch.crop_x_str, |v| {
            Message::BatchDialog(BatchDialogMsg::CropXChanged(v))
        }),
        labeled_small_input("Y", &batch.crop_y_str, |v| {
            Message::BatchDialog(BatchDialogMsg::CropYChanged(v))
        }),
    ]
    .spacing(8);

    let anchor_idx = batch.crop_anchor.map(CanvasAnchor::index).unwrap_or(4);
    let anchor_grid = anchor_grid_3x3(anchor_idx, |idx| {
        Message::BatchDialog(BatchDialogMsg::CropAnchorChanged(Some(
            CanvasAnchor::from_index(idx),
        )))
    });

    let anchor_box = column![
        row![
            text(tr(lang, "label-anchor")).size(TEXT_SIZE_CAPTION),
            iced::widget::space::horizontal(),
            button::labeled(
                "X/Y",
                None,
                ButtonVariant::Tab {
                    active: batch.crop_anchor.is_none(),
                },
                Some(Message::BatchDialog(BatchDialogMsg::CropAnchorChanged(
                    None
                ))),
            ),
        ]
        .align_y(Alignment::Center),
        anchor_grid,
    ]
    .spacing(6);

    let ratio_row = row![
        crop_ratio_chip("Free", None, batch.crop_ratio),
        crop_ratio_chip("1:1", Some(1.0), batch.crop_ratio),
        crop_ratio_chip("4:3", Some(4.0 / 3.0), batch.crop_ratio),
        crop_ratio_chip("3:2", Some(1.5), batch.crop_ratio),
        crop_ratio_chip("16:9", Some(16.0 / 9.0), batch.crop_ratio),
    ]
    .spacing(4);

    let mut left_col = column![
        row![
            text(tr(lang, "label-unit")).size(TEXT_SIZE_CAPTION),
            iced::widget::space::horizontal(),
            unit_row,
        ]
        .align_y(Alignment::Center),
        dims_row,
    ]
    .spacing(8)
    .width(Fill);

    if batch.crop_anchor.is_none() {
        left_col = left_col.push(xy_row);
    }
    left_col = left_col.push(
        column![
            text(tr(lang, "label-presets")).size(TEXT_SIZE_CAPTION),
            ratio_row
        ]
        .spacing(4),
    );

    container(
        row![left_col, anchor_box]
            .spacing(12)
            .align_y(Alignment::Start),
    )
    .padding(10)
    .width(Fill)
    .style(card_style)
    .into()
}

fn crop_ratio_chip<'a>(
    label: &'static str,
    ratio: Option<f64>,
    current: Option<f64>,
) -> Element<'a, Message> {
    let active = match (ratio, current) {
        (None, None) => true,
        (Some(a), Some(b)) => (a - b).abs() < 0.01,
        _ => false,
    };
    button::labeled(
        label,
        None,
        ButtonVariant::Tab { active },
        Some(Message::BatchDialog(BatchDialogMsg::CropRatioChanged(
            ratio,
        ))),
    )
    .into()
}

fn build_batch_convert_section<'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, Message> {
    let mut row1 = row![].spacing(4);
    let mut row2 = row![].spacing(4);
    for (idx, fmt) in BatchImageFormat::ALL.iter().copied().enumerate() {
        let btn = button::labeled(
            fmt.label().to_ascii_uppercase(),
            None,
            ButtonVariant::Tab {
                active: batch.convert_format == fmt,
            },
            Some(Message::BatchDialog(BatchDialogMsg::ConvertFormatChanged(
                fmt,
            ))),
        );
        if idx < 4 {
            row1 = row1.push(btn);
        } else {
            row2 = row2.push(btn);
        }
    }

    let mut col = column![
        text(tr(lang, "label-format"))
            .size(TEXT_SIZE_CONTROL)
            .style(|theme| {
                let t = PaletteTokens::for_theme(theme);
                text::Style {
                    color: Some(t.text_muted),
                }
            }),
        row1,
        row2,
    ]
    .spacing(6);

    if matches!(
        batch.convert_format,
        BatchImageFormat::Jpeg | BatchImageFormat::Webp
    ) {
        col = col.push(
            column![
                row![
                    text(tr(lang, "label-quality")).size(TEXT_SIZE_CONTROL),
                    iced::widget::space::horizontal(),
                    text(format!("{}%", batch.convert_quality)).size(TEXT_SIZE_CONTROL),
                ],
                slider(1..=100, batch.convert_quality, |q| {
                    Message::BatchDialog(BatchDialogMsg::ConvertQualityChanged(q))
                })
                .style(slider_style),
            ]
            .spacing(4),
        );
    }

    container(col)
        .padding(10)
        .width(Fill)
        .style(card_style)
        .into()
}

fn labeled_small_input<'a>(
    label: &'static str,
    val: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        text(label).size(TEXT_SIZE_CAPTION).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        }),
        text_input("", val)
            .on_input(on_input)
            .padding([5, 8])
            .size(TEXT_SIZE_CONTROL)
            .style(input_style),
    ]
    .spacing(2)
    .width(Fill)
    .into()
}

fn parse_positive_f32(raw: &str) -> Option<f32> {
    raw.trim()
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn parse_positive_f64(raw: &str) -> Option<f64> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn parse_nonneg_f64(raw: &str) -> Option<f64> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}

fn parse_ratio(raw: &str) -> Option<f32> {
    let trimmed = raw.trim();
    if let Some((a, b)) = trimmed.split_once(':') {
        let num = a.trim().parse::<f32>().ok()?;
        let den = b.trim().parse::<f32>().ok()?;
        if num > 0.0 && den > 0.0 && num.is_finite() && den.is_finite() {
            return Some(num / den);
        }
        return None;
    }
    trimmed
        .parse::<f32>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

fn format_ratio(ratio: f32) -> String {
    if (ratio - 1.0).abs() < 0.01 {
        "1:1".to_owned()
    } else if (ratio - 4.0 / 3.0).abs() < 0.015 {
        "4:3".to_owned()
    } else if (ratio - 1.5).abs() < 0.015 {
        "3:2".to_owned()
    } else if (ratio - 16.0 / 9.0).abs() < 0.015 {
        "16:9".to_owned()
    } else {
        format!("{ratio:.3}")
    }
}

fn format_unit_value(val: f32, unit: LengthUnit) -> String {
    match unit {
        LengthUnit::Px => format!("{}", val.round().max(1.0) as u32),
        LengthUnit::Percent => format!("{val:.1}"),
        LengthUnit::Cm | LengthUnit::Inch => format!("{val:.2}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photon_protocol::{
        Entry, Language, LayoutState, Theme, TranslateSettings, UiStateMeta, UiText, Workspace,
    };

    fn test_snapshot() -> UiStateSnapshot {
        UiStateSnapshot {
            meta: UiStateMeta {
                revision: 1,
                listing_epoch: 1,
                tree_revision: 1,
                workspace: Workspace::Browser,
                theme: Theme::Dark,
                language: Language::English,
                current_folder: Some("/tmp/photos".to_owned()),
                root_folder: Some("/tmp/photos".to_owned()),
                breadcrumbs: Vec::new(),
                selection: vec!["/tmp/photos/a.png".to_owned()],
                selection_info: None,
                layout: LayoutState::default(),
                remember_last_folder: true,
                hardware_acceleration: true,
                viewer_index: None,
                viewer_path: None,
                viewer_count: 0,
                editor: None,
                status: UiText::new("ready"),
                safe_mode: true,
                active_tasks: Vec::new(),
                favorites: Vec::new(),
                batch: None,
                translation: None,
                translate_settings: TranslateSettings::default(),
            },
            entries: vec![Entry {
                name: "a.png".to_owned(),
                path: "/tmp/photos/a.png".to_owned(),
                is_dir: false,
                is_image: true,
                size: 1024,
                modified_ms: 0,
                kind: "PNG".to_owned(),
            }],
            viewer_images: Vec::new(),
        }
    }

    #[test]
    fn batch_resize_locks_aspect_and_converts_units() {
        let mut st = BatchDialogState::new(
            BatchDialogKind::Resize,
            true,
            Some("/tmp/photos"),
            Some((1920, 1080)),
        );
        st.apply_msg(BatchDialogMsg::ResizeWidthChanged("960".to_owned()));
        assert_eq!(st.resize_height_str, "540");

        st.apply_msg(BatchDialogMsg::ResizeUnitChanged(LengthUnit::Inch));
        assert_eq!(st.resize_width_str, "10.00");
        assert_eq!(st.current_resize_px(), (960, 540));
    }

    #[test]
    fn batch_build_command_builds_valid_resize_crop_convert() {
        let snap = test_snapshot();
        let st = BatchDialogState::new(
            BatchDialogKind::Resize,
            true,
            Some("/tmp/photos"),
            Some((800, 600)),
        );
        let cmd = st.build_command(&snap).expect("valid batch resize");
        assert!(matches!(
            cmd,
            Command::BatchRun {
                operation: BatchOperation::Resize {
                    width: 800,
                    height: 600,
                    ..
                },
                source: BatchSource::Selection { .. },
                ..
            }
        ));
    }
}
