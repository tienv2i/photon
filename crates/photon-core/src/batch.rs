//! Non-blocking queued & cancellable batch image processor (`resize`, `crop`, `convert`).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use photon_image::codec::{SaveFormat, backup_path, encode_with, write_atomically};
use photon_image::{decode_rgba, pixels, validate_dimensions};
use photon_protocol::{
    BatchCropUnit, BatchImageFormat, BatchOperation, BatchOutput, BatchState, CanvasAnchor,
    ProtocolError, ResizeMode, UiText,
};

use crate::file_ops::normalize_path;

pub const MAX_BATCH_FILES: usize = 500;

#[derive(Debug, Clone)]
pub struct BatchPlan {
    pub job_id: String,
    pub operation: BatchOperation,
    pub inputs: Vec<PathBuf>,
    pub outputs: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum BatchEvent {
    Progress {
        done: usize,
        failed: usize,
        current: String,
    },
    Finished {
        done: usize,
        failed: usize,
        cancelled: bool,
        message: UiText,
    },
}

impl BatchPlan {
    pub fn build(
        job_id: String,
        operation: BatchOperation,
        inputs: Vec<PathBuf>,
        output: BatchOutput,
    ) -> Result<Self, ProtocolError> {
        if inputs.is_empty() {
            return Err(ProtocolError::line(
                "no_images",
                UiText::new("batch_no_images"),
            ));
        }
        if inputs.len() > MAX_BATCH_FILES {
            return Err(ProtocolError::line(
                "too_many_files",
                UiText::with("batch_too_many", [("max", MAX_BATCH_FILES.to_string())]),
            ));
        }
        validate_operation(&operation)?;
        validate_policy(&operation, &inputs, &output)?;

        let mut outputs = Vec::with_capacity(inputs.len());
        for src in &inputs {
            outputs.push(destination(src, &operation, &output)?);
        }

        let mut seen = HashSet::new();
        for p in &outputs {
            if !seen.insert(p.clone()) {
                return Err(ProtocolError::line(
                    "duplicate_output",
                    UiText::with(
                        "batch_duplicate_output",
                        [("path", p.display().to_string())],
                    ),
                ));
            }
        }

        Ok(Self {
            job_id,
            operation,
            inputs,
            outputs,
        })
    }

    pub fn initial_state(&self) -> BatchState {
        BatchState {
            job_id: self.job_id.clone(),
            operation: self.operation.name().to_owned(),
            total: self.inputs.len(),
            done: 0,
            failed: 0,
            running: true,
            cancelled: false,
            current: self
                .inputs
                .first()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }

    pub fn run(self, cancel: Arc<AtomicBool>, mut on_event: impl FnMut(BatchEvent)) {
        let total = self.inputs.len();
        let mut done = 0usize;
        let mut failed = 0usize;

        for (src, dst) in self.inputs.iter().zip(&self.outputs) {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            let current = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| src.to_string_lossy().into_owned());
            on_event(BatchEvent::Progress {
                done,
                failed,
                current: current.clone(),
            });
            match process_one(src, dst, &self.operation) {
                Ok(()) => {
                    done += 1;
                }
                Err(err) => {
                    failed += 1;
                    tracing::warn!(
                        target: "photon::batch",
                        source = %src.display(),
                        destination = %dst.display(),
                        error = %err,
                        "batch item failed"
                    );
                }
            }
        }

        let cancelled = cancel.load(Ordering::SeqCst) && done + failed < total;
        let message = if cancelled {
            UiText::with(
                "batch_cancelled",
                [("done", done.to_string()), ("total", total.to_string())],
            )
        } else {
            UiText::with(
                "batch_done",
                [("done", done.to_string()), ("total", total.to_string())],
            )
        };
        on_event(BatchEvent::Finished {
            done,
            failed,
            cancelled,
            message,
        });
    }
}

fn process_one(src: &Path, dst: &Path, operation: &BatchOperation) -> Result<(), String> {
    let image = decode_rgba(src).map_err(|e| e.to_string())?;
    let (pixels_out, quality) = match operation {
        BatchOperation::Resize {
            width,
            height,
            mode,
            filter,
            fill,
            fill_alpha,
        } => (
            pixels::resize_with_mode(
                &image,
                *width,
                *height,
                *filter,
                *mode,
                fill.unwrap_or([0, 0, 0]),
                *fill_alpha,
            ),
            None,
        ),
        BatchOperation::Crop {
            x,
            y,
            width,
            height,
            unit,
            anchor,
            ratio,
        } => {
            let (cx, cy, cw, ch) = crop_rect_full(
                image.width(),
                image.height(),
                *x,
                *y,
                *width,
                *height,
                *unit,
                *anchor,
                *ratio,
            );
            (
                pixels::grow_crop(&image, cx as i32, cy as i32, cw, ch, [0, 0, 0], 255),
                None,
            )
        }
        BatchOperation::Convert { quality, .. } => (image, *quality),
    };

    let fmt = SaveFormat::from_path(dst)
        .ok_or_else(|| format!("unsupported output format: {}", dst.display()))?;
    let bytes = encode_with(&pixels_out, fmt, quality, dst).map_err(|e| e.to_string())?;
    if dst == src && dst.is_file() {
        let backup = backup_path(dst);
        fs::copy(dst, &backup).map_err(|e| format!("backup failed {}: {e}", backup.display()))?;
    }
    write_atomically(dst, &bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn anchor_offset(container: u32, size: u32, start: bool, end: bool) -> u32 {
    let slack = container.saturating_sub(size);
    if start {
        0
    } else if end {
        slack
    } else {
        slack / 2
    }
}

#[allow(clippy::too_many_arguments)]
pub fn crop_rect_full(
    image_width: u32,
    image_height: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    unit: BatchCropUnit,
    anchor: Option<CanvasAnchor>,
    ratio: Option<f64>,
) -> (u32, u32, u32, u32) {
    let is_pixels = unit == BatchCropUnit::Pixels;
    let to_px = |value: f64, length: u32| -> u32 {
        if is_pixels {
            value.round().clamp(0.0, f64::from(length)) as u32
        } else {
            (f64::from(length) * value / 100.0)
                .round()
                .clamp(0.0, f64::from(length)) as u32
        }
    };

    let (crop_w, crop_h) = if let Some(aspect) = ratio.filter(|r| r.is_finite() && *r > 0.0) {
        let max_w = if width > 0.0 {
            to_px(width, image_width).max(1).min(image_width)
        } else {
            image_width
        };
        let max_h = if height > 0.0 {
            to_px(height, image_height).max(1).min(image_height)
        } else {
            image_height
        };
        let box_w = f64::from(max_w);
        let box_h = f64::from(max_h);
        if box_w / box_h > aspect {
            let w = (box_h * aspect).round().clamp(1.0, box_w) as u32;
            (w, max_h)
        } else {
            let h = (box_w / aspect).round().clamp(1.0, box_h) as u32;
            (max_w, h)
        }
    } else {
        (
            to_px(width, image_width).max(1).min(image_width),
            to_px(height, image_height).max(1).min(image_height),
        )
    };

    if let Some(a) = anchor {
        let h_start = matches!(
            a,
            CanvasAnchor::TopLeft | CanvasAnchor::Left | CanvasAnchor::BottomLeft
        );
        let h_end = matches!(
            a,
            CanvasAnchor::TopRight | CanvasAnchor::Right | CanvasAnchor::BottomRight
        );
        let v_start = matches!(
            a,
            CanvasAnchor::TopLeft | CanvasAnchor::Top | CanvasAnchor::TopRight
        );
        let v_end = matches!(
            a,
            CanvasAnchor::BottomLeft | CanvasAnchor::Bottom | CanvasAnchor::BottomRight
        );
        let left = anchor_offset(image_width, crop_w, h_start, h_end);
        let top = anchor_offset(image_height, crop_h, v_start, v_end);
        return (left, top, crop_w, crop_h);
    }

    let left = to_px(x, image_width).min(image_width.saturating_sub(1));
    let top = to_px(y, image_height).min(image_height.saturating_sub(1));
    let right = (left + crop_w).min(image_width);
    let bottom = (top + crop_h).min(image_height);
    (left, top, (right - left).max(1), (bottom - top).max(1))
}

fn format_for_batch(fmt: BatchImageFormat) -> SaveFormat {
    match fmt {
        BatchImageFormat::Png => SaveFormat::Png,
        BatchImageFormat::Jpeg => SaveFormat::Jpeg,
        BatchImageFormat::Webp => SaveFormat::Webp,
        BatchImageFormat::Tiff => SaveFormat::Tiff,
        BatchImageFormat::Gif => SaveFormat::Gif,
        BatchImageFormat::Bmp => SaveFormat::Bmp,
        BatchImageFormat::Tga => SaveFormat::Tga,
        BatchImageFormat::Qoi => SaveFormat::Qoi,
    }
}

fn output_extension(source: &Path, operation: &BatchOperation) -> Result<String, ProtocolError> {
    match operation {
        BatchOperation::Convert { format, .. } => Ok(format.extension().to_owned()),
        _ => source
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| {
                ProtocolError::new(
                    "missing_extension",
                    format!("missing extension: {}", source.display()),
                )
            }),
    }
}

fn destination(
    source: &Path,
    operation: &BatchOperation,
    output: &BatchOutput,
) -> Result<PathBuf, ProtocolError> {
    if output.overwrite {
        if let BatchOperation::Convert { format, .. } = operation {
            let wanted = format_for_batch(*format);
            if SaveFormat::from_path(source) != Some(wanted) {
                return Err(ProtocolError::line(
                    "convert_overwrite",
                    UiText::with(
                        "batch_convert_overwrite",
                        [("path", source.display().to_string())],
                    ),
                ));
            }
        }
        return Ok(source.to_path_buf());
    }

    let folder = match &output.folder {
        Some(f) => normalize_path(Path::new(f)),
        None => source
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    let stem = source.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
        ProtocolError::new(
            "missing_name",
            format!("bad filename: {}", source.display()),
        )
    })?;
    let ext = output_extension(source, operation)?;
    let path = folder.join(format!("{stem}{}.{ext}", output.suffix));
    if path == source {
        return Err(ProtocolError::line(
            "output_is_source",
            UiText::with(
                "batch_output_is_source",
                [("path", source.display().to_string())],
            ),
        ));
    }
    Ok(path)
}

fn validate_operation(operation: &BatchOperation) -> Result<(), ProtocolError> {
    match operation {
        BatchOperation::Resize {
            width,
            height,
            mode,
            fill,
            ..
        } => {
            validate_dimensions(*width, *height)
                .map_err(|e| ProtocolError::new("invalid_resize", e.to_string()))?;
            if *mode == ResizeMode::Pad && fill.is_none() {
                return Err(ProtocolError::new(
                    "invalid_resize",
                    "pad mode requires a fill color",
                ));
            }
            Ok(())
        }
        BatchOperation::Crop {
            x,
            y,
            width,
            height,
            unit,
            anchor,
            ratio,
        } => {
            let finite = [x, y, width, height].iter().all(|v| v.is_finite());
            let valid_ratio = ratio.is_none_or(|r| r.is_finite() && r > 0.0);
            if !finite || !valid_ratio {
                return Err(ProtocolError::new(
                    "invalid_crop",
                    "invalid crop parameters",
                ));
            }
            let is_pixels = *unit == BatchCropUnit::Pixels;
            let has_anchor = anchor.is_some();
            if ratio.is_some() {
                let ok = if is_pixels {
                    *width >= 0.0 && *height >= 0.0 && (has_anchor || (*x >= 0.0 && *y >= 0.0))
                } else {
                    *width >= 0.0
                        && *height >= 0.0
                        && *width <= 100.05
                        && *height <= 100.05
                        && (has_anchor || (*x >= -0.05 && *y >= -0.05 && *x < 100.0 && *y < 100.0))
                };
                return if ok {
                    Ok(())
                } else {
                    Err(ProtocolError::new("invalid_crop", "crop outside bounds"))
                };
            }
            if is_pixels {
                if *width < 1.0 || *height < 1.0 || (!has_anchor && (*x < 0.0 || *y < 0.0)) {
                    return Err(ProtocolError::new("invalid_crop", "crop outside bounds"));
                }
                return Ok(());
            }
            let inside = if has_anchor {
                *width > 0.0 && *height > 0.0 && *width <= 100.05 && *height <= 100.05
            } else {
                *x >= -0.05
                    && *y >= -0.05
                    && *width > 0.0
                    && *height > 0.0
                    && *x + *width <= 100.05
                    && *y + *height <= 100.05
            };
            if !inside {
                return Err(ProtocolError::new("invalid_crop", "crop outside bounds"));
            }
            if (has_anchor || (x.abs() < f64::EPSILON && y.abs() < f64::EPSILON))
                && (*width - 100.0).abs() < f64::EPSILON
                && (*height - 100.0).abs() < f64::EPSILON
            {
                return Err(ProtocolError::new(
                    "crop_noop",
                    "crop covers the entire image",
                ));
            }
            Ok(())
        }
        BatchOperation::Convert { quality, .. } => {
            if quality.is_some_and(|q| q == 0 || q > 100) {
                return Err(ProtocolError::new(
                    "invalid_quality",
                    "quality must be between 1 and 100",
                ));
            }
            Ok(())
        }
    }
}

fn validate_policy(
    operation: &BatchOperation,
    inputs: &[PathBuf],
    output: &BatchOutput,
) -> Result<(), ProtocolError> {
    if output.suffix.contains('/') || output.suffix.contains('\\') || output.suffix.contains('\0') {
        return Err(ProtocolError::new(
            "invalid_suffix",
            "suffix cannot contain path separators",
        ));
    }
    if output.overwrite && output.folder.is_some() {
        return Err(ProtocolError::new(
            "overwrite_with_folder",
            "cannot combine overwrite with an output folder",
        ));
    }
    if let Some(folder) = &output.folder {
        let p = normalize_path(Path::new(folder));
        if !p.is_dir() {
            return Err(ProtocolError::new(
                "missing_folder",
                format!("output folder does not exist: {}", p.display()),
            ));
        }
    }
    if output.overwrite
        && let BatchOperation::Convert { format, .. } = operation
    {
        let wanted = format_for_batch(*format);
        if let Some(src) = inputs
            .iter()
            .find(|s| SaveFormat::from_path(s) != Some(wanted))
        {
            return Err(ProtocolError::line(
                "convert_overwrite",
                UiText::with(
                    "batch_convert_overwrite",
                    [("path", src.display().to_string())],
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use photon_protocol::ResizeFilter;

    #[test]
    fn crop_rect_full_handles_percent_pixels_anchor_and_ratio() {
        assert_eq!(
            crop_rect_full(
                1000,
                800,
                0.0,
                0.0,
                50.0,
                50.0,
                BatchCropUnit::Percent,
                Some(CanvasAnchor::BottomRight),
                None,
            ),
            (500, 400, 500, 400)
        );
        assert_eq!(
            crop_rect_full(
                1000,
                600,
                0.0,
                0.0,
                100.0,
                100.0,
                BatchCropUnit::Percent,
                Some(CanvasAnchor::Center),
                Some(1.0),
            ),
            (200, 0, 600, 600)
        );
    }

    #[test]
    fn batch_plan_validates_duplicates_overwrite_and_runs_with_cancel() {
        let dir = std::env::temp_dir().join(format!("photon-batch-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let p1 = dir.join("a.png");
        let p2 = dir.join("b.png");
        RgbaImage::from_pixel(40, 20, Rgba([10, 20, 30, 255]))
            .save(&p1)
            .unwrap();
        RgbaImage::from_pixel(40, 20, Rgba([40, 50, 60, 255]))
            .save(&p2)
            .unwrap();

        // Empty suffix without overwrite or output folder is rejected
        assert!(
            BatchPlan::build(
                "j0".into(),
                BatchOperation::Resize {
                    width: 20,
                    height: 10,
                    mode: ResizeMode::Stretch,
                    filter: ResizeFilter::Triangle,
                    fill: None,
                    fill_alpha: 255,
                },
                vec![p1.clone()],
                BatchOutput::default(),
            )
            .is_err()
        );

        // Run resize with suffix
        let plan = BatchPlan::build(
            "j1".into(),
            BatchOperation::Resize {
                width: 20,
                height: 10,
                mode: ResizeMode::Stretch,
                filter: ResizeFilter::Triangle,
                fill: None,
                fill_alpha: 255,
            },
            vec![p1.clone(), p2.clone()],
            BatchOutput {
                folder: None,
                suffix: "_small".into(),
                overwrite: false,
            },
        )
        .unwrap();

        let cancel = Arc::new(AtomicBool::new(false));
        let mut events = Vec::new();
        plan.run(cancel, |ev| events.push(ev));
        assert!(matches!(
            events.last(),
            Some(BatchEvent::Finished {
                done: 2,
                failed: 0,
                cancelled: false,
                ..
            })
        ));
        assert_eq!(
            photon_image::probe_dimensions(&dir.join("a_small.png")),
            Some((20, 10))
        );

        // Cancel stops before processing all items
        let plan2 = BatchPlan::build(
            "j2".into(),
            BatchOperation::Convert {
                format: BatchImageFormat::Jpeg,
                quality: Some(85),
            },
            vec![p1, p2],
            BatchOutput::default(),
        )
        .unwrap();
        let cancel2 = Arc::new(AtomicBool::new(true));
        let mut events2 = Vec::new();
        plan2.run(cancel2, |ev| events2.push(ev));
        assert!(matches!(
            events2.last(),
            Some(BatchEvent::Finished {
                done: 0,
                cancelled: true,
                ..
            })
        ));

        let _ = fs::remove_dir_all(&dir);
    }
}
