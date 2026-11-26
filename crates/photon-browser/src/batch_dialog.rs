//! Dedicated Batch processing modal dialogs (`Batch Resize`, `Batch Crop`, `Batch Convert`)
//! with Print Units (`px` / `cm` / `in` / `%`), DPI selection, source selection, and
//! output/overwrite guardrails.

use iced::widget::{
    button as iced_button, checkbox, column, container, pick_list, row, rule, slider, text_input,
};
use iced::{Alignment, Border, Element, Fill, Length};
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
    PaletteTokens, RADIUS_SM, TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, card_style,
    hairline_rule_style, input_style, pick_list_menu_style, pick_list_style, slider_style,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip_pos};
use photon_ui::typography::text;

use photon_dialogs::{DialogEvent, DialogWindowState};

const SECTION_PADDING: u16 = 12;
const SECTION_GAP: u32 = 10;
const ROW_SPACING: u32 = 8;
const CONTROL_GAP: u32 = 8;
const FIELD_LABEL_GAP: u32 = 4;
const INPUT_PADDING: [u16; 2] = [6, 8];
const PICKER_PADDING: [u16; 2] = [6, 10];
const CONTROL_ROW_HEIGHT: f32 = 28.0;
const SUFFIX_FIELD_WIDTH: f32 = 180.0;
const FORMAT_PICKER_WIDTH: f32 = 180.0;
const ANCHOR_COLUMN_WIDTH: f32 = 108.0;

fn section_title_text<'a, M: 'a>(label: String) -> Element<'a, M> {
    text(label)
        .size(TEXT_SIZE_CONTROL)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        })
        .into()
}

fn field_label_text<'a, M: 'a>(label: impl Into<String>) -> Element<'a, M> {
    text(label.into())
        .size(TEXT_SIZE_CAPTION)
        .style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.text_muted),
            }
        })
        .into()
}

#[derive(Debug, Clone)]
pub enum BatchDialogAction<Id> {
    Msg(BatchDialogMsg),
    Close,
    DialogEvent(DialogEvent<Id>),
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchDialogKind {
    Resize,
    Crop,
    Convert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchInputMode {
    Selection,
    CurrentFolder,
    CustomFolder,
}

#[derive(Debug, Clone)]
pub enum BatchDialogMsg {
    InputModeChanged(BatchInputMode),
    InputFolderChanged(String),
    PickInputFolder,
    InputFolderPicked(Option<String>),
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
    pub input_mode: BatchInputMode,
    pub input_folder: String,
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
            input_mode: if has_selection_images {
                BatchInputMode::Selection
            } else {
                BatchInputMode::CurrentFolder
            },
            input_folder: current_folder.unwrap_or_default().to_owned(),
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
            BatchDialogMsg::InputModeChanged(m) => self.input_mode = m,
            BatchDialogMsg::InputFolderChanged(v) => self.input_folder = v,
            BatchDialogMsg::InputFolderPicked(Some(folder)) => {
                self.input_folder = folder;
                self.input_mode = BatchInputMode::CustomFolder;
            }
            BatchDialogMsg::PickInputFolder | BatchDialogMsg::InputFolderPicked(None) => {}
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
        let source = match self.input_mode {
            BatchInputMode::Selection => {
                if selected_images.is_empty() {
                    return Err(tr(lang, "batch-no-images"));
                }
                BatchSource::Selection {
                    paths: selected_images,
                }
            }
            BatchInputMode::CurrentFolder => {
                let folder = snapshot
                    .meta
                    .current_folder
                    .clone()
                    .ok_or_else(|| tr(lang, "label-no-folder"))?;
                BatchSource::Folder { path: folder }
            }
            BatchInputMode::CustomFolder => {
                let trimmed = self.input_folder.trim();
                if trimmed.is_empty() {
                    return Err(tr(lang, "label-no-folder"));
                }
                BatchSource::Folder {
                    path: trimmed.to_owned(),
                }
            }
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

pub fn build_batch_resize_dialog<'a, Id: Copy + Eq + Ord + std::hash::Hash + 'a>(
    snapshot: &'a UiStateSnapshot,
    batch: &'a BatchDialogState,
    id: Id,
    defaults: DialogWindowState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, Id, BatchDialogAction<Id>> {
    build_batch_dialog_for_kind(
        BatchDialogKind::Resize,
        snapshot,
        batch,
        id,
        defaults,
        theme,
    )
}

pub fn build_batch_crop_dialog<'a, Id: Copy + Eq + Ord + std::hash::Hash + 'a>(
    snapshot: &'a UiStateSnapshot,
    batch: &'a BatchDialogState,
    id: Id,
    defaults: DialogWindowState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, Id, BatchDialogAction<Id>> {
    build_batch_dialog_for_kind(BatchDialogKind::Crop, snapshot, batch, id, defaults, theme)
}

pub fn build_batch_convert_dialog<'a, Id: Copy + Eq + Ord + std::hash::Hash + 'a>(
    snapshot: &'a UiStateSnapshot,
    batch: &'a BatchDialogState,
    id: Id,
    defaults: DialogWindowState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, Id, BatchDialogAction<Id>> {
    build_batch_dialog_for_kind(
        BatchDialogKind::Convert,
        snapshot,
        batch,
        id,
        defaults,
        theme,
    )
}

fn build_batch_dialog_for_kind<'a, Id: Copy + Eq + Ord + std::hash::Hash + 'a>(
    kind: BatchDialogKind,
    snapshot: &'a UiStateSnapshot,
    batch: &'a BatchDialogState,
    id: Id,
    defaults: DialogWindowState,
    theme: &iced::Theme,
) -> ManagedDialogSpec<'a, Id, BatchDialogAction<Id>> {
    let lang = snapshot.meta.language;
    let selected_img_count = snapshot
        .entries
        .iter()
        .filter(|e| e.is_image && snapshot.meta.selection.contains(&e.path))
        .count();
    let folder_img_count = snapshot.entries.iter().filter(|e| e.is_image).count();

    let mut source_col = column![
        section_title_text(tr(lang, "batch-source")),
        row![
            button::labeled(
                format!(
                    "{} ({selected_img_count})",
                    tr(lang, "batch-source-selection")
                ),
                None,
                ButtonVariant::Tab {
                    active: batch.input_mode == BatchInputMode::Selection,
                },
                (selected_img_count > 0).then_some(BatchDialogAction::Msg(
                    BatchDialogMsg::InputModeChanged(BatchInputMode::Selection)
                )),
            ),
            button::labeled(
                format!("{} ({folder_img_count})", tr(lang, "batch-source-folder")),
                None,
                ButtonVariant::Tab {
                    active: batch.input_mode == BatchInputMode::CurrentFolder,
                },
                Some(BatchDialogAction::Msg(BatchDialogMsg::InputModeChanged(
                    BatchInputMode::CurrentFolder
                ))),
            ),
            button::labeled(
                tr(lang, "batch-source-custom"),
                None,
                ButtonVariant::Tab {
                    active: batch.input_mode == BatchInputMode::CustomFolder,
                },
                Some(BatchDialogAction::Msg(BatchDialogMsg::InputModeChanged(
                    BatchInputMode::CustomFolder
                ))),
            ),
        ]
        .spacing(CONTROL_GAP)
        .align_y(Alignment::Center),
    ]
    .spacing(ROW_SPACING);

    if batch.input_mode == BatchInputMode::CustomFolder {
        source_col = source_col.push(
            row![
                text_input("/path/to/input", &batch.input_folder)
                    .on_input(|v| BatchDialogAction::Msg(BatchDialogMsg::InputFolderChanged(v)))
                    .padding(INPUT_PADDING)
                    .size(TEXT_SIZE_CONTROL)
                    .style(input_style)
                    .width(Fill),
                with_tooltip_pos(
                    button::icon_only(
                        Icon::FolderOpen,
                        ButtonVariant::Secondary,
                        Some(BatchDialogAction::Msg(BatchDialogMsg::PickInputFolder)),
                    ),
                    tr(lang, "batch-source-custom"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(CONTROL_GAP)
            .align_y(Alignment::Center),
        );
    }

    let source_section = container(source_col)
        .padding(SECTION_PADDING)
        .width(Fill)
        .style(card_style);

    let op_section: Element<'a, BatchDialogAction<Id>> = match kind {
        BatchDialogKind::Resize => build_batch_resize_section(lang, batch),
        BatchDialogKind::Crop => build_batch_crop_section(lang, batch),
        BatchDialogKind::Convert => build_batch_convert_section(lang, batch),
    };

    let mut output_col = column![
        section_title_text(tr(lang, "batch-output")),
        row![
            button::labeled(
                tr(lang, "batch-output-beside"),
                None,
                ButtonVariant::Tab {
                    active: batch.beside_originals,
                },
                Some(BatchDialogAction::Msg(BatchDialogMsg::BesideOriginals(
                    true
                ))),
            ),
            button::labeled(
                tr(lang, "batch-output-folder"),
                None,
                ButtonVariant::Tab {
                    active: !batch.beside_originals,
                },
                Some(BatchDialogAction::Msg(BatchDialogMsg::BesideOriginals(
                    false
                ))),
            ),
        ]
        .spacing(CONTROL_GAP)
        .align_y(Alignment::Center),
    ]
    .spacing(ROW_SPACING);

    if !batch.beside_originals {
        output_col = output_col.push(
            row![
                text_input("/path/to/output", &batch.output_folder)
                    .on_input(|v| BatchDialogAction::Msg(BatchDialogMsg::OutputFolderChanged(v)))
                    .padding(INPUT_PADDING)
                    .size(TEXT_SIZE_CONTROL)
                    .style(input_style)
                    .width(Fill),
                with_tooltip_pos(
                    button::icon_only(
                        Icon::FolderOpen,
                        ButtonVariant::Secondary,
                        Some(BatchDialogAction::Msg(BatchDialogMsg::PickOutputFolder)),
                    ),
                    tr(lang, "batch-output-folder"),
                    TooltipPosition::Top,
                ),
            ]
            .spacing(CONTROL_GAP)
            .align_y(Alignment::Center),
        );
    }

    output_col = output_col.push(
        row![
            column![
                field_label_text(tr(lang, "batch-suffix")),
                text_input("_edited", &batch.suffix)
                    .on_input(|v| BatchDialogAction::Msg(BatchDialogMsg::SuffixChanged(v)))
                    .padding(INPUT_PADDING)
                    .size(TEXT_SIZE_CONTROL)
                    .style(input_style),
            ]
            .spacing(FIELD_LABEL_GAP)
            .width(Length::Fixed(SUFFIX_FIELD_WIDTH)),
            container(
                checkbox(batch.overwrite)
                    .label(tr(lang, "batch-overwrite"))
                    .text_size(TEXT_SIZE_CONTROL)
                    .on_toggle(|v| BatchDialogAction::Msg(BatchDialogMsg::OverwriteToggled(v))),
            )
            .height(Length::Fixed(CONTROL_ROW_HEIGHT))
            .align_y(Alignment::Center),
        ]
        .spacing(12)
        .align_y(Alignment::End),
    );

    let output_section = container(output_col)
        .padding(SECTION_PADDING)
        .width(Fill)
        .style(card_style);

    let mut body_col = column![source_section, op_section, output_section]
        .spacing(SECTION_GAP)
        .width(Fill);

    if let Some(err) = &batch.error {
        body_col = body_col.push(text(err.clone()).size(TEXT_SIZE_CONTROL).style(|theme| {
            let t = PaletteTokens::for_theme(theme);
            text::Style {
                color: Some(t.danger),
            }
        }));
    }

    let can_run = match batch.input_mode {
        BatchInputMode::Selection => selected_img_count > 0,
        BatchInputMode::CurrentFolder => folder_img_count > 0,
        BatchInputMode::CustomFolder => !batch.input_folder.trim().is_empty(),
    };

    let footer = row![
        with_tooltip_pos(
            button::labeled(
                tr(lang, "dialog-cancel"),
                None,
                ButtonVariant::Ghost,
                Some(BatchDialogAction::Close),
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
                can_run.then_some(BatchDialogAction::Msg(BatchDialogMsg::Confirm)),
            ),
            tr(lang, "batch-run"),
            TooltipPosition::Top,
        ),
    ]
    .spacing(CONTROL_GAP)
    .width(Fill)
    .align_y(Alignment::Center);

    let title = match kind {
        BatchDialogKind::Resize => tr(lang, "batch-resize-title"),
        BatchDialogKind::Crop => tr(lang, "batch-crop-title"),
        BatchDialogKind::Convert => tr(lang, "batch-convert-title"),
    };

    let close_btn = with_tooltip_pos(
        button::icon_only(
            Icon::Close,
            ButtonVariant::Ghost,
            Some(BatchDialogAction::Close),
        ),
        tr(lang, "tooltip-dialog-close"),
        TooltipPosition::Left,
    );

    ManagedDialogSpec {
        id,
        open: true,
        title,
        defaults,
        resizable: true,
        draggable: true,
        overlay: OverlayMode::default(),
        body: body_col.into(),
        footer: Some(footer.into()),
        close_button: Some(close_btn),
        style: dialog_style_for_theme(theme),
        on_event: Box::new(BatchDialogAction::DialogEvent),
        on_close: BatchDialogAction::Close,
        on_noop: BatchDialogAction::Noop,
    }
}

fn build_batch_resize_section<'a, Id: Clone + 'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, BatchDialogAction<Id>> {
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
        on_width_change: Box::new(|v| {
            BatchDialogAction::Msg(BatchDialogMsg::ResizeWidthChanged(v))
        }),
        on_height_change: Box::new(|v| {
            BatchDialogAction::Msg(BatchDialogMsg::ResizeHeightChanged(v))
        }),
        on_ratio_change: Box::new(|v| {
            BatchDialogAction::Msg(BatchDialogMsg::ResizeRatioChanged(v))
        }),
        on_unit_change: Box::new(|u| BatchDialogAction::Msg(BatchDialogMsg::ResizeUnitChanged(u))),
        on_dpi_change: Box::new(|d| BatchDialogAction::Msg(BatchDialogMsg::ResizeDpiChanged(d))),
        on_toggle_lock_width: BatchDialogAction::Msg(BatchDialogMsg::ResizeToggleLockWidth),
        on_toggle_lock_height: BatchDialogAction::Msg(BatchDialogMsg::ResizeToggleLockHeight),
        on_toggle_lock_ratio: BatchDialogAction::Msg(BatchDialogMsg::ResizeToggleLockRatio),
        ratio_presets: vec![
            (
                "1:1".to_owned(),
                BatchDialogAction::Msg(BatchDialogMsg::ResizeApplyRatioPreset(1.0)),
            ),
            (
                "4:3".to_owned(),
                BatchDialogAction::Msg(BatchDialogMsg::ResizeApplyRatioPreset(4.0 / 3.0)),
            ),
            (
                "3:2".to_owned(),
                BatchDialogAction::Msg(BatchDialogMsg::ResizeApplyRatioPreset(1.5)),
            ),
            (
                "16:9".to_owned(),
                BatchDialogAction::Msg(BatchDialogMsg::ResizeApplyRatioPreset(16.0 / 9.0)),
            ),
        ],
    });

    #[derive(Clone, PartialEq, Eq)]
    struct LabeledChoice<T: Copy + PartialEq> {
        value: T,
        label: String,
    }
    impl<T: Copy + PartialEq> std::fmt::Display for LabeledChoice<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.label)
        }
    }

    let mode_options = vec![
        LabeledChoice {
            value: ResizeMode::Stretch,
            label: tr(lang, "mode-stretch"),
        },
        LabeledChoice {
            value: ResizeMode::Cover,
            label: tr(lang, "mode-cover"),
        },
        LabeledChoice {
            value: ResizeMode::Pad,
            label: tr(lang, "mode-pad"),
        },
    ];
    let selected_mode = mode_options
        .iter()
        .find(|o| o.value == batch.resize_mode)
        .cloned();
    let mode_picker = pick_list(mode_options, selected_mode, |o| {
        BatchDialogAction::Msg(BatchDialogMsg::ResizeModeChanged(o.value))
    })
    .text_size(TEXT_SIZE_CONTROL)
    .padding(PICKER_PADDING)
    .width(Fill)
    .style(pick_list_style)
    .menu_style(pick_list_menu_style);

    let filter_options = vec![
        LabeledChoice {
            value: ResizeFilter::Lanczos3,
            label: "Lanczos3".to_owned(),
        },
        LabeledChoice {
            value: ResizeFilter::CatmullRom,
            label: "CatmullRom".to_owned(),
        },
        LabeledChoice {
            value: ResizeFilter::Triangle,
            label: "Triangle".to_owned(),
        },
        LabeledChoice {
            value: ResizeFilter::Nearest,
            label: "Nearest".to_owned(),
        },
    ];
    let selected_filter = filter_options
        .iter()
        .find(|o| o.value == batch.resize_filter)
        .cloned();
    let filter_picker = pick_list(filter_options, selected_filter, |o| {
        BatchDialogAction::Msg(BatchDialogMsg::ResizeFilterChanged(o.value))
    })
    .text_size(TEXT_SIZE_CONTROL)
    .padding(PICKER_PADDING)
    .width(Fill)
    .style(pick_list_style)
    .menu_style(pick_list_menu_style);

    container(
        column![
            grid,
            rule::horizontal(1).style(hairline_rule_style),
            row![
                column![field_label_text(tr(lang, "label-resize-mode")), mode_picker]
                    .spacing(FIELD_LABEL_GAP)
                    .width(Fill),
                column![field_label_text(tr(lang, "label-filter")), filter_picker]
                    .spacing(FIELD_LABEL_GAP)
                    .width(Fill),
            ]
            .spacing(CONTROL_GAP),
        ]
        .spacing(ROW_SPACING),
    )
    .padding(SECTION_PADDING)
    .width(Fill)
    .style(card_style)
    .into()
}

fn build_batch_crop_section<'a, Id: Clone + 'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, BatchDialogAction<Id>> {
    let unit_row = row![
        button::labeled(
            "%",
            None,
            ButtonVariant::Tab {
                active: batch.crop_unit == BatchCropUnit::Percent,
            },
            Some(BatchDialogAction::Msg(BatchDialogMsg::CropUnitChanged(
                BatchCropUnit::Percent,
            ))),
        ),
        button::labeled(
            "px",
            None,
            ButtonVariant::Tab {
                active: batch.crop_unit == BatchCropUnit::Pixels,
            },
            Some(BatchDialogAction::Msg(BatchDialogMsg::CropUnitChanged(
                BatchCropUnit::Pixels,
            ))),
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let dims_row = row![
        labeled_small_input("W", &batch.crop_w_str, |v| {
            BatchDialogAction::Msg(BatchDialogMsg::CropWChanged(v))
        }),
        labeled_small_input("H", &batch.crop_h_str, |v| {
            BatchDialogAction::Msg(BatchDialogMsg::CropHChanged(v))
        }),
    ]
    .spacing(CONTROL_GAP);

    let xy_row = row![
        labeled_small_input("X", &batch.crop_x_str, |v| {
            BatchDialogAction::Msg(BatchDialogMsg::CropXChanged(v))
        }),
        labeled_small_input("Y", &batch.crop_y_str, |v| {
            BatchDialogAction::Msg(BatchDialogMsg::CropYChanged(v))
        }),
    ]
    .spacing(CONTROL_GAP);

    let anchor_idx = batch.crop_anchor.map(CanvasAnchor::index).unwrap_or(4);
    let anchor_grid = anchor_grid_3x3(anchor_idx, |idx| {
        BatchDialogAction::Msg(BatchDialogMsg::CropAnchorChanged(Some(
            CanvasAnchor::from_index(idx),
        )))
    });

    let anchor_box = column![
        row![
            field_label_text(tr(lang, "label-anchor")),
            iced::widget::space::horizontal(),
            button::labeled(
                "X/Y",
                None,
                ButtonVariant::Tab {
                    active: batch.crop_anchor.is_none(),
                },
                Some(BatchDialogAction::Msg(BatchDialogMsg::CropAnchorChanged(
                    None
                ))),
            ),
        ]
        .width(Fill)
        .align_y(Alignment::Center),
        anchor_grid,
    ]
    .spacing(ROW_SPACING)
    .width(Length::Fixed(ANCHOR_COLUMN_WIDTH));

    #[derive(Clone, Copy)]
    struct RatioPreset {
        label: &'static str,
        ratio: Option<f64>,
    }
    impl PartialEq for RatioPreset {
        fn eq(&self, other: &Self) -> bool {
            self.label == other.label
        }
    }
    impl Eq for RatioPreset {}
    impl std::fmt::Display for RatioPreset {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.label)
        }
    }

    let ratio_options = [
        RatioPreset {
            label: "Free",
            ratio: None,
        },
        RatioPreset {
            label: "1:1",
            ratio: Some(1.0),
        },
        RatioPreset {
            label: "4:3",
            ratio: Some(4.0 / 3.0),
        },
        RatioPreset {
            label: "3:2",
            ratio: Some(1.5),
        },
        RatioPreset {
            label: "16:9",
            ratio: Some(16.0 / 9.0),
        },
    ];
    let selected_ratio = ratio_options
        .iter()
        .find(|o| match (o.ratio, batch.crop_ratio) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() < 0.01,
            _ => false,
        })
        .copied();
    let ratio_row = pick_list(ratio_options, selected_ratio, |o| {
        BatchDialogAction::Msg(BatchDialogMsg::CropRatioChanged(o.ratio))
    })
    .text_size(TEXT_SIZE_CONTROL)
    .padding(PICKER_PADDING)
    .width(Fill)
    .style(pick_list_style)
    .menu_style(pick_list_menu_style);

    let mut left_col = column![
        row![
            field_label_text(tr(lang, "label-unit")),
            iced::widget::space::horizontal(),
            unit_row,
        ]
        .width(Fill)
        .align_y(Alignment::Center),
        dims_row,
    ]
    .spacing(ROW_SPACING)
    .width(Fill);

    if batch.crop_anchor.is_none() {
        left_col = left_col.push(xy_row);
    }
    left_col = left_col.push(
        column![field_label_text(tr(lang, "label-presets")), ratio_row]
            .spacing(FIELD_LABEL_GAP)
            .width(Fill),
    );

    container(
        row![left_col, anchor_box]
            .spacing(12)
            .align_y(Alignment::Start),
    )
    .padding(SECTION_PADDING)
    .width(Fill)
    .style(card_style)
    .into()
}

fn build_batch_convert_section<'a, Id: Clone + 'a>(
    lang: photon_protocol::Language,
    batch: &'a BatchDialogState,
) -> Element<'a, BatchDialogAction<Id>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FormatChoice(BatchImageFormat);
    impl std::fmt::Display for FormatChoice {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0.label().to_ascii_uppercase())
        }
    }

    let format_options: Vec<FormatChoice> = BatchImageFormat::ALL
        .iter()
        .copied()
        .map(FormatChoice)
        .collect();
    let format_picker = pick_list(
        format_options,
        Some(FormatChoice(batch.convert_format)),
        |c| BatchDialogAction::Msg(BatchDialogMsg::ConvertFormatChanged(c.0)),
    )
    .text_size(TEXT_SIZE_CONTROL)
    .padding(PICKER_PADDING)
    .width(Length::Fixed(FORMAT_PICKER_WIDTH))
    .style(pick_list_style)
    .menu_style(pick_list_menu_style);

    let mut col =
        column![section_title_text(tr(lang, "label-format")), format_picker,].spacing(ROW_SPACING);

    if matches!(
        batch.convert_format,
        BatchImageFormat::Jpeg | BatchImageFormat::Webp
    ) {
        let dec_q = batch.convert_quality.saturating_sub(1).max(1);
        let inc_q = (batch.convert_quality + 1).min(100);
        let step_btn = |sym: &'static str, target: u8| {
            iced_button(text(sym).size(TEXT_SIZE_CAPTION))
                .padding([2, 6])
                .style(|theme, status| {
                    let t = PaletteTokens::for_theme(theme);
                    let bg = if matches!(status, iced_button::Status::Hovered) {
                        t.bg_hover
                    } else {
                        t.bg_surface
                    };
                    iced_button::Style {
                        background: Some(bg.into()),
                        text_color: t.text_secondary,
                        border: Border {
                            color: t.border,
                            width: 1.0,
                            radius: RADIUS_SM.into(),
                        },
                        ..Default::default()
                    }
                })
                .on_press(BatchDialogAction::Msg(
                    BatchDialogMsg::ConvertQualityChanged(target),
                ))
        };
        let value_badge =
            container(text(format!("{}%", batch.convert_quality)).size(TEXT_SIZE_CAPTION))
                .padding([2, 8])
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
                });
        col = col.push(
            column![
                row![
                    field_label_text(tr(lang, "label-quality")),
                    iced::widget::space::horizontal(),
                    row![step_btn("<", dec_q), value_badge, step_btn(">", inc_q)]
                        .spacing(2)
                        .align_y(Alignment::Center),
                ]
                .width(Fill)
                .align_y(Alignment::Center),
                slider(1..=100, batch.convert_quality, |q| {
                    BatchDialogAction::Msg(BatchDialogMsg::ConvertQualityChanged(q))
                })
                .width(Fill)
                .style(slider_style),
            ]
            .spacing(FIELD_LABEL_GAP)
            .width(Fill),
        );
    }

    container(col)
        .padding(SECTION_PADDING)
        .width(Fill)
        .style(card_style)
        .into()
}

fn labeled_small_input<'a, M: Clone + 'a>(
    label: &'static str,
    val: &'a str,
    on_input: impl Fn(String) -> M + 'a,
) -> Element<'a, M> {
    column![
        field_label_text(label),
        text_input("", val)
            .on_input(on_input)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE_CONTROL)
            .style(input_style),
    ]
    .spacing(FIELD_LABEL_GAP)
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
                save_ui_state_on_exit: true,
                hardware_acceleration: true,
                backup_method: Default::default(),
                viewer_index: None,
                viewer_path: None,
                viewer_count: 0,
                editor: None,
                editor_tabs: Vec::new(),
                active_editor_tab_id: None,
                focus_request_seq: 0,
                status: UiText::new("ready"),
                safe_mode: true,
                active_tasks: Vec::new(),
                favorites: Vec::new(),
                batch: None,
                translation: None,
                viewer_saved_translation: None,
                translate_settings: TranslateSettings::default(),
                plugins: Vec::new(),
                ai_settings: Default::default(),
                ai_models: Vec::new(),
                quick_actions: Vec::new(),
                ai_task: None,
                editor_caption: None,
                has_pixel_clipboard: false,
                viewer_archive: None,
                editor_unsupported: None,
                editor_source_is_virtual: false,
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
