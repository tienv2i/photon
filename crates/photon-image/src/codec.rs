//! Image decoding, header probing, thumbnailing, atomic save with `.bak` backup,
//! and delivery export with optional downscaling, JPEG quality, and metadata splicing.

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::read::GzDecoder;
use image::codecs::pnm::{PnmEncoder, PnmSubtype, SampleEncoding};
use image::imageops::{self, FilterType};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, RgbaImage};
use photon_protocol::BlendMode;

use crate::ImageError;
use crate::layer::{Layer, LayerStack};
use crate::selection::SelectionMask;

pub const DEFAULT_JPEG_QUALITY: u8 = 92;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFormat {
    Png,
    Jpeg,
    Webp,
    Tiff,
    Gif,
    Bmp,
    Tga,
    Qoi,
    Ora,
    Avif,
    Exr,
    Hdr,
    Ico,
    Pnm,
    Farbfeld,
}

impl SaveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" | "jpe" | "jfif" => Self::Jpeg,
            "webp" => Self::Webp,
            "tif" | "tiff" => Self::Tiff,
            "gif" => Self::Gif,
            "bmp" => Self::Bmp,
            "tga" => Self::Tga,
            "qoi" => Self::Qoi,
            "ora" => Self::Ora,
            "avif" => Self::Avif,
            "exr" => Self::Exr,
            "hdr" => Self::Hdr,
            "ico" => Self::Ico,
            "pnm" | "ppm" | "pgm" | "pbm" | "pam" => Self::Pnm,
            "ff" => Self::Farbfeld,
            _ => return None,
        })
    }

    pub const CHOICES: &'static str =
        ".png .jpg .webp .tiff .gif .bmp .tga .qoi .ora .avif .exr .hdr .ico .pnm .ff";
}

fn lower_ext(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

fn is_raw_ext(ext: &str) -> bool {
    matches!(
        ext,
        "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2"
    )
}

/// Fast header-only dimensions lookup without decoding full pixel buffers.
pub fn probe_dimensions(path: &Path) -> Option<(u32, u32)> {
    if let Some(ext) = lower_ext(path) {
        match ext.as_str() {
            "ora" => return crate::ora::probe_ora_dimensions(path),
            "svg" | "svgz" => return probe_svg_dimensions(path, ext == "svgz"),
            "jxl" => {
                let img = jxl_oxide::JxlImage::builder().open(path).ok()?;
                let (w, h) = (img.width(), img.height());
                return (w > 0 && h > 0).then_some((w, h));
            }
            "psd" => return probe_psd_dimensions(path),
            "cur" => return probe_cur_dimensions(path),
            "apng" => {
                if let Ok(file) = fs::File::open(path) {
                    let reader =
                        ImageReader::with_format(std::io::BufReader::new(file), ImageFormat::Png);
                    return reader.into_dimensions().ok();
                }
            }
            "heic" | "heif" | "avif" => {
                if let Ok(reader) = ImageReader::open(path)
                    && let Ok(guessed) = reader.with_guessed_format()
                    && let Ok(dims) = guessed.into_dimensions()
                {
                    return Some(dims);
                }
                return probe_via_cli(path);
            }
            e if is_raw_ext(e) => return probe_raw_dimensions(path),
            _ => {}
        }
    }

    if let Ok(reader) = ImageReader::open(path)
        && let Ok(guessed) = reader.with_guessed_format()
        && let Ok(dims) = guessed.into_dimensions()
    {
        return Some(dims);
    }

    let fmt = lower_ext(path).and_then(|e| ImageFormat::from_extension(&e))?;
    let file = fs::File::open(path).ok()?;
    ImageReader::with_format(std::io::BufReader::new(file), fmt)
        .into_dimensions()
        .ok()
}

/// Decode an image into RGBA8 pixels on CPU.
pub fn decode_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    if let Some(ext) = lower_ext(path) {
        match ext.as_str() {
            "ora" => return crate::ora::decode_ora_merged(path),
            "svg" | "svgz" => return decode_svg_rgba(path, ext == "svgz"),
            "jxl" => return decode_jxl_rgba(path),
            "psd" => return decode_psd_rgba(path),
            "cur" => return decode_cur_rgba(path),
            "apng" => {
                let file = fs::File::open(path).map_err(|source| ImageError::Io {
                    path: path.display().to_string(),
                    source,
                })?;
                let dyn_img =
                    ImageReader::with_format(std::io::BufReader::new(file), ImageFormat::Png)
                        .decode()
                        .map_err(|err| ImageError::Decode {
                            path: path.display().to_string(),
                            message: err.to_string(),
                        })?;
                return Ok(dyn_img.to_rgba8());
            }
            "heic" | "heif" => return decode_via_cli(path, &["heif-convert", "ffmpeg", "magick"]),
            "avif" => {
                if let Ok(reader) = ImageReader::open(path)
                    && let Ok(guessed) = reader.with_guessed_format()
                    && let Ok(dyn_img) = guessed.decode()
                {
                    return Ok(dyn_img.to_rgba8());
                }
                return decode_via_cli(path, &["avifdec", "ffmpeg", "magick"]);
            }
            e if is_raw_ext(e) => return decode_raw_rgba(path),
            _ => {}
        }
    }

    let reader = ImageReader::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let guessed = reader
        .with_guessed_format()
        .map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
    if guessed.format().is_some() {
        let dyn_img = guessed.decode().map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
        return Ok(dyn_img.to_rgba8());
    }

    if let Some(fmt) = lower_ext(path).and_then(|e| ImageFormat::from_extension(&e)) {
        let file = fs::File::open(path).map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let dyn_img = ImageReader::with_format(std::io::BufReader::new(file), fmt)
            .decode()
            .map_err(|err| ImageError::Decode {
                path: path.display().to_string(),
                message: err.to_string(),
            })?;
        return Ok(dyn_img.to_rgba8());
    }

    Err(ImageError::Decode {
        path: path.display().to_string(),
        message: "unrecognized image format".to_owned(),
    })
}

/// Generate a thumbnail RGBA8 buffer scaled proportionally to fit `max_edge`.
pub fn generate_thumbnail(path: &Path, max_edge: u32) -> Result<RgbaImage, ImageError> {
    if lower_ext(path).as_deref() == Some("ora") {
        return crate::ora::ora_thumbnail(path, max_edge);
    }
    let rgba = decode_rgba(path)?;
    let (w, h) = rgba.dimensions();
    if w <= max_edge && h <= max_edge {
        return Ok(rgba);
    }
    let dyn_img = DynamicImage::ImageRgba8(rgba);
    Ok(dyn_img.thumbnail(max_edge, max_edge).to_rgba8())
}

fn read_svg_bytes(path: &Path, compressed: bool) -> Result<Vec<u8>, ImageError> {
    let raw = fs::read(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    if !compressed {
        return Ok(raw);
    }
    let mut decoder = GzDecoder::new(Cursor::new(raw));
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: format!("invalid svgz gzip stream: {err}"),
        })?;
    Ok(out)
}

fn probe_svg_dimensions(path: &Path, compressed: bool) -> Option<(u32, u32)> {
    let bytes = read_svg_bytes(path, compressed).ok()?;
    let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default()).ok()?;
    let sz = tree.size();
    let w = sz
        .width()
        .ceil()
        .clamp(1.0, crate::MAX_IMAGE_DIMENSION as f32) as u32;
    let h = sz
        .height()
        .ceil()
        .clamp(1.0, crate::MAX_IMAGE_DIMENSION as f32) as u32;
    Some((w, h))
}

fn decode_svg_rgba(path: &Path, compressed: bool) -> Result<RgbaImage, ImageError> {
    let bytes = read_svg_bytes(path, compressed)?;
    let tree = usvg::Tree::from_data(&bytes, &usvg::Options::default()).map_err(|err| {
        ImageError::Decode {
            path: path.display().to_string(),
            message: err.to_string(),
        }
    })?;
    let sz = tree.size();
    let w = sz
        .width()
        .ceil()
        .clamp(1.0, crate::MAX_IMAGE_DIMENSION as f32) as u32;
    let h = sz
        .height()
        .ceil()
        .clamp(1.0, crate::MAX_IMAGE_DIMENSION as f32) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "failed to allocate SVG pixmap".to_owned(),
    })?;
    let sx = w as f32 / sz.width().max(1e-3);
    let sy = h as f32 / sz.height().max(1e-3);
    let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny_skia stores premultiplied RGBA; convert back to straight-alpha RGBA8.
    let mut raw = pixmap.take();
    for px in raw.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a > 0 && a < 255 {
            let af = f32::from(a) / 255.0;
            px[0] = (f32::from(px[0]) / af).round().clamp(0.0, 255.0) as u8;
            px[1] = (f32::from(px[1]) / af).round().clamp(0.0, 255.0) as u8;
            px[2] = (f32::from(px[2]) / af).round().clamp(0.0, 255.0) as u8;
        }
    }
    RgbaImage::from_raw(w, h, raw).ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "SVG pixmap size mismatch".to_owned(),
    })
}

fn decode_jxl_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    let image = jxl_oxide::JxlImage::builder()
        .open(path)
        .map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
    let w = image.width();
    let h = image.height();
    crate::validate_dimensions(w, h)?;
    let render = image.render_frame(0).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let mut stream = render.stream();
    let ch = stream.channels() as usize;
    let total_pixels = (w as usize).saturating_mul(h as usize);
    let mut samples = vec![0u8; total_pixels.saturating_mul(ch)];
    stream.write_to_buffer(&mut samples);

    let mut rgba = vec![0u8; total_pixels * 4];
    match ch {
        1 => {
            for (i, &l) in samples.iter().enumerate() {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[l, l, l, 255]);
            }
        }
        2 => {
            for (i, px) in samples.as_chunks::<2>().0.iter().enumerate() {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        3 => {
            for (i, px) in samples.as_chunks::<3>().0.iter().enumerate() {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        _ => {
            for (i, px) in samples.chunks_exact(ch).enumerate() {
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[px[0], px[1], px[2], px[3]]);
            }
        }
    }

    RgbaImage::from_raw(w, h, rgba).ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "JXL buffer size mismatch".to_owned(),
    })
}

fn probe_psd_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut file = fs::File::open(path).ok()?;
    let mut hdr = [0u8; 26];
    file.read_exact(&mut hdr).ok()?;
    if &hdr[0..4] != b"8BPS" {
        return None;
    }
    let h = u32::from_be_bytes([hdr[14], hdr[15], hdr[16], hdr[17]]);
    let w = u32::from_be_bytes([hdr[18], hdr[19], hdr[20], hdr[21]]);
    (w > 0 && h > 0).then_some((w, h))
}

fn decode_psd_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    let bytes = fs::read(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let psd = psd::Psd::from_bytes(&bytes).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let (w, h) = (psd.width(), psd.height());
    crate::validate_dimensions(w, h)?;
    let rgba = psd.rgba();
    RgbaImage::from_raw(w, h, rgba).ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "PSD RGBA size mismatch".to_owned(),
    })
}

/// Decode a multi-layer `.psd` into a Photon [`LayerStack`].
pub fn decode_psd_stack(path: &Path) -> Result<LayerStack, ImageError> {
    let bytes = fs::read(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let psd = psd::Psd::from_bytes(&bytes).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let (w, h) = (psd.width(), psd.height());
    crate::validate_dimensions(w, h)?;

    let mut layers = Vec::new();
    let mut next_id = 1u64;
    let expected_len = (w as usize).saturating_mul(h as usize).saturating_mul(4);

    for psd_layer in psd.layers() {
        if psd_layer.layer_bottom() < psd_layer.layer_top()
            || psd_layer.layer_right() < psd_layer.layer_left()
        {
            continue;
        }
        let rgba_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| psd_layer.rgba()));
        let Ok(rgba) = rgba_res else {
            continue;
        };
        if rgba.len() != expected_len {
            continue;
        }
        let Some(img) = RgbaImage::from_raw(w, h, rgba) else {
            continue;
        };
        let name = if psd_layer.name().trim().is_empty() {
            format!("Layer {next_id}")
        } else {
            psd_layer.name().to_owned()
        };
        let mut layer = Layer::new(next_id, name, Arc::new(img));
        layer.visible = psd_layer.visible();
        layer.opacity = (f32::from(psd_layer.opacity()) / 255.0).clamp(0.0, 1.0);
        layer.blend_mode = psd_blend_to_photon(&format!("{:?}", psd_layer.blend_mode()));
        layers.push(layer);
        next_id += 1;
    }

    if layers.is_empty() {
        let rgba = psd.rgba();
        let img = RgbaImage::from_raw(w, h, rgba).ok_or_else(|| ImageError::Decode {
            path: path.display().to_string(),
            message: "PSD RGBA size mismatch".to_owned(),
        })?;
        return Ok(LayerStack::from_single(Arc::new(img)));
    }

    let active_layer_id = layers.last().map(|l| l.id).unwrap_or(1);
    Ok(LayerStack {
        canvas_width: w,
        canvas_height: h,
        layers,
        active_layer_id,
        next_id,
        background: None,
        selection: SelectionMask::empty(w, h),
    })
}

fn psd_blend_to_photon(mode: &str) -> BlendMode {
    match mode {
        "Multiply" => BlendMode::Multiply,
        "Screen" => BlendMode::Screen,
        "Overlay" => BlendMode::Overlay,
        "Darken" => BlendMode::Darken,
        "Lighten" => BlendMode::Lighten,
        "Difference" => BlendMode::Difference,
        _ => BlendMode::Normal,
    }
}

fn probe_cur_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut bytes = fs::read(path).ok()?;
    if bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 2 && bytes[3] == 0 {
        bytes[2] = 1;
    }
    ImageReader::with_format(Cursor::new(bytes), ImageFormat::Ico)
        .into_dimensions()
        .ok()
}

fn decode_cur_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    let mut bytes = fs::read(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    if bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 2 && bytes[3] == 0 {
        bytes[2] = 1;
    }
    let dyn_img = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Ico)
        .decode()
        .map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
    Ok(dyn_img.to_rgba8())
}

fn probe_raw_dimensions(path: &Path) -> Option<(u32, u32)> {
    let src = rawler::rawsource::RawSource::new(path).ok()?;
    let raw = rawler::decode_dummy(&src).ok()?;
    let (w, h) = (raw.width as u32, raw.height as u32);
    (w > 0 && h > 0).then_some((w, h))
}

fn decode_raw_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    let raw = rawler::decode_file(path).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let developer = rawler::imgop::develop::RawDevelop::default();
    let intermediate = developer
        .develop_intermediate(&raw)
        .map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
    let dyn_img = intermediate
        .to_dynamic_image()
        .ok_or_else(|| ImageError::Decode {
            path: path.display().to_string(),
            message: "failed to convert developed RAW image".to_owned(),
        })?;
    Ok(dyn_img.to_rgba8())
}

fn probe_via_cli(path: &Path) -> Option<(u32, u32)> {
    if let Ok(out) = ProcessCommand::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
        ])
        .arg(path)
        .output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some((w_s, h_s)) = text.trim().split_once('x')
            && let (Ok(w), Ok(h)) = (w_s.parse::<u32>(), h_s.parse::<u32>())
            && w > 0
            && h > 0
        {
            return Some((w, h));
        }
    }
    if let Ok(out) = ProcessCommand::new("magick")
        .args(["identify", "-format", "%wx%h"])
        .arg(path)
        .output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some((w_s, h_s)) = text.trim().split_once('x')
            && let (Ok(w), Ok(h)) = (w_s.parse::<u32>(), h_s.parse::<u32>())
            && w > 0
            && h > 0
        {
            return Some((w, h));
        }
    }
    None
}

fn decode_via_cli(path: &Path, tools: &[&str]) -> Result<RgbaImage, ImageError> {
    let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_png =
        std::env::temp_dir().join(format!("photon-cli-dec-{}-{seq}.png", std::process::id()));

    for &tool in tools {
        let _ = fs::remove_file(&tmp_png);
        let status = match tool {
            "heif-convert" => ProcessCommand::new("heif-convert")
                .arg(path)
                .arg(&tmp_png)
                .output()
                .ok()
                .map(|o| o.status.success()),
            "avifdec" => ProcessCommand::new("avifdec")
                .arg(path)
                .arg(&tmp_png)
                .output()
                .ok()
                .map(|o| o.status.success()),
            "ffmpeg" => ProcessCommand::new("ffmpeg")
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(path)
                .args(["-frames:v", "1"])
                .arg(&tmp_png)
                .output()
                .ok()
                .map(|o| o.status.success()),
            "magick" => ProcessCommand::new("magick")
                .arg(path)
                .arg(&tmp_png)
                .output()
                .ok()
                .map(|o| o.status.success()),
            _ => None,
        };
        if status == Some(true) && tmp_png.exists() {
            let res = ImageReader::open(&tmp_png)
                .map_err(|source| ImageError::Io {
                    path: path.display().to_string(),
                    source,
                })
                .and_then(|r| {
                    r.with_guessed_format().map_err(|source| ImageError::Io {
                        path: path.display().to_string(),
                        source,
                    })
                })
                .and_then(|r| {
                    r.decode().map_err(|err| ImageError::Decode {
                        path: path.display().to_string(),
                        message: err.to_string(),
                    })
                })
                .map(|d| d.to_rgba8());
            let _ = fs::remove_file(&tmp_png);
            if let Ok(rgba) = res {
                return Ok(rgba);
            }
        }
    }
    let _ = fs::remove_file(&tmp_png);
    Err(ImageError::Decode {
        path: path.display().to_string(),
        message: format!(
            "cannot decode format without runtime helper ({})",
            tools.join(", ")
        ),
    })
}

/// Encode `pixels` into `format` using optional JPEG/AVIF `quality` (`10..=100`).
pub fn encode_with(
    pixels: &RgbaImage,
    format: SaveFormat,
    quality: Option<u8>,
    dest: &Path,
) -> Result<Vec<u8>, ImageError> {
    let q = quality
        .map(|v| v.clamp(10, 100))
        .unwrap_or(DEFAULT_JPEG_QUALITY);
    let mut bytes = Vec::new();
    let map_err = |msg: String| ImageError::Encode {
        path: dest.display().to_string(),
        message: msg,
    };

    match format {
        SaveFormat::Ora => {
            let stack = LayerStack::from_single(Arc::new(pixels.clone()));
            return crate::ora::encode_ora_bytes(&stack);
        }
        SaveFormat::Jpeg => {
            let rgb = image::RgbImage::from_fn(pixels.width(), pixels.height(), |x, y| {
                let p = pixels.get_pixel(x, y).0;
                let a = u32::from(p[3]);
                let over = |c: u8| -> u8 { ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8 };
                image::Rgb([over(p[0]), over(p[1]), over(p[2])])
            });
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, q)
                .encode_image(&rgb)
                .map_err(|e| map_err(e.to_string()))?;
        }
        SaveFormat::Gif => {
            let mut cursor = Cursor::new(bytes);
            DynamicImage::ImageRgba8(pixels.clone())
                .write_to(&mut cursor, ImageFormat::Gif)
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Webp => {
            let mut cursor = Cursor::new(bytes);
            image::codecs::webp::WebPEncoder::new_lossless(&mut cursor)
                .write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Avif => {
            let mut cursor = Cursor::new(bytes);
            image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut cursor, 6, q)
                .write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Exr => {
            let mut cursor = Cursor::new(bytes);
            DynamicImage::ImageRgba8(pixels.clone())
                .to_rgba32f()
                .write_to(&mut cursor, ImageFormat::OpenExr)
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Hdr => {
            let mut cursor = Cursor::new(bytes);
            DynamicImage::ImageRgba8(pixels.clone())
                .to_rgb32f()
                .write_to(&mut cursor, ImageFormat::Hdr)
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Ico => {
            let resized;
            let src_ico = if pixels.width() > 256 || pixels.height() > 256 {
                resized = DynamicImage::ImageRgba8(pixels.clone())
                    .thumbnail(256, 256)
                    .to_rgba8();
                &resized
            } else {
                pixels
            };
            let mut cursor = Cursor::new(bytes);
            image::codecs::ico::IcoEncoder::new(&mut cursor)
                .write_image(
                    src_ico.as_raw(),
                    src_ico.width(),
                    src_ico.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Pnm => {
            let ext = lower_ext(dest);
            let mut cursor = Cursor::new(bytes);
            match ext.as_deref() {
                Some("pam") => {
                    PnmEncoder::new(&mut cursor)
                        .with_subtype(PnmSubtype::ArbitraryMap)
                        .write_image(
                            pixels.as_raw(),
                            pixels.width(),
                            pixels.height(),
                            ExtendedColorType::Rgba8,
                        )
                        .map_err(|e| map_err(e.to_string()))?;
                }
                Some("pgm") => {
                    let luma = DynamicImage::ImageRgba8(pixels.clone()).to_luma8();
                    PnmEncoder::new(&mut cursor)
                        .with_subtype(PnmSubtype::Graymap(SampleEncoding::Binary))
                        .write_image(
                            luma.as_raw(),
                            luma.width(),
                            luma.height(),
                            ExtendedColorType::L8,
                        )
                        .map_err(|e| map_err(e.to_string()))?;
                }
                Some("pbm") => {
                    let mut luma = DynamicImage::ImageRgba8(pixels.clone()).to_luma8();
                    for p in luma.pixels_mut() {
                        p.0[0] = if p.0[0] >= 128 { 255 } else { 0 };
                    }
                    PnmEncoder::new(&mut cursor)
                        .with_subtype(PnmSubtype::Bitmap(SampleEncoding::Binary))
                        .write_image(
                            luma.as_raw(),
                            luma.width(),
                            luma.height(),
                            ExtendedColorType::L8,
                        )
                        .map_err(|e| map_err(e.to_string()))?;
                }
                _ => {
                    let rgb = DynamicImage::ImageRgba8(pixels.clone()).to_rgb8();
                    PnmEncoder::new(&mut cursor)
                        .with_subtype(PnmSubtype::Pixmap(SampleEncoding::Binary))
                        .write_image(
                            rgb.as_raw(),
                            rgb.width(),
                            rgb.height(),
                            ExtendedColorType::Rgb8,
                        )
                        .map_err(|e| map_err(e.to_string()))?;
                }
            }
            bytes = cursor.into_inner();
        }
        SaveFormat::Farbfeld => {
            let mut cursor = Cursor::new(bytes);
            DynamicImage::ImageRgba8(pixels.clone())
                .to_rgba16()
                .write_to(&mut cursor, ImageFormat::Farbfeld)
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        other => {
            let mut cursor = Cursor::new(bytes);
            let res = match other {
                SaveFormat::Png => image::codecs::png::PngEncoder::new(&mut cursor).write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                ),
                SaveFormat::Tiff => image::codecs::tiff::TiffEncoder::new(&mut cursor).write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                ),
                SaveFormat::Bmp => image::codecs::bmp::BmpEncoder::new(&mut cursor).write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                ),
                SaveFormat::Tga => image::codecs::tga::TgaEncoder::new(&mut cursor).write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                ),
                SaveFormat::Qoi => image::codecs::qoi::QoiEncoder::new(&mut cursor).write_image(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    ExtendedColorType::Rgba8,
                ),
                SaveFormat::Ora
                | SaveFormat::Jpeg
                | SaveFormat::Gif
                | SaveFormat::Webp
                | SaveFormat::Avif
                | SaveFormat::Exr
                | SaveFormat::Hdr
                | SaveFormat::Ico
                | SaveFormat::Pnm
                | SaveFormat::Farbfeld => unreachable!(),
            };
            res.map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
    }
    Ok(bytes)
}

/// Backup path `<path>.bak` alongside the overwritten file.
pub fn backup_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_owned());
    name.push_str(".bak");
    target.with_file_name(name)
}

/// Atomically write `bytes` to `path` via a `.photon-tmp` sibling file and rename.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ImageError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_owned());
    let tmp = parent.join(format!(".{name}.{}.photon-tmp", std::process::id()));
    let write_res = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(source) = write_res {
        let _ = fs::remove_file(&tmp);
        return Err(ImageError::Io {
            path: path.display().to_string(),
            source,
        });
    }
    Ok(())
}

/// Save `pixels` to `dest`, copying existing file bytes to `<dest>.bak` before overwriting.
pub fn save_to(pixels: &RgbaImage, dest: &Path) -> Result<PathBuf, ImageError> {
    let format = SaveFormat::from_path(dest).ok_or_else(|| ImageError::Encode {
        path: dest.display().to_string(),
        message: format!("unsupported format; use one of {}", SaveFormat::CHOICES),
    })?;
    let bytes = encode_with(pixels, format, None, dest)?;
    if dest.exists() {
        let backup = backup_path(dest);
        fs::copy(dest, &backup).map_err(|source| ImageError::Io {
            path: backup.display().to_string(),
            source,
        })?;
    }
    write_atomically(dest, &bytes)?;
    Ok(dest.to_path_buf())
}

/// Compute proportionally scaled dimensions under a `max_edge` limit (never upscales).
pub fn export_dimensions(source_w: u32, source_h: u32, max_edge: u32) -> Option<(u32, u32)> {
    let longer = source_w.max(source_h);
    if max_edge == 0 || max_edge >= longer {
        return None;
    }
    let scale = |side: u32| -> u32 {
        let scaled =
            (u64::from(side) * u64::from(max_edge) + u64::from(longer) / 2) / u64::from(longer);
        scaled.clamp(1, u64::from(u32::MAX)) as u32
    };
    Some((scale(source_w), scale(source_h)))
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

/// Export a delivery copy of `pixels` to `dest`, refusing to overwrite `source`.
pub fn export_to(
    source: &Path,
    pixels: &RgbaImage,
    dest: &Path,
    quality: Option<u8>,
    metadata: bool,
    max_edge: Option<u32>,
) -> Result<PathBuf, ImageError> {
    if paths_equal(source, dest) {
        return Err(ImageError::InvalidEdit(
            "export destination cannot be the same as the source file".to_owned(),
        ));
    }
    let format = SaveFormat::from_path(dest).ok_or_else(|| ImageError::Encode {
        path: dest.display().to_string(),
        message: format!("unsupported format; use one of {}", SaveFormat::CHOICES),
    })?;

    let target = max_edge.and_then(|edge| export_dimensions(pixels.width(), pixels.height(), edge));
    let scaled;
    let (to_encode, resized) = match target {
        Some((w, h)) => {
            scaled = imageops::resize(pixels, w, h, FilterType::Triangle);
            (&scaled, true)
        }
        None => (pixels, false),
    };

    let mut bytes = encode_with(to_encode, format, quality, dest)?;
    if metadata
        && format == SaveFormat::Jpeg
        && SaveFormat::from_path(source) == Some(SaveFormat::Jpeg)
        && let Ok(original) = fs::read(source)
    {
        bytes = splice_jpeg_metadata_segments(&original, bytes, !resized);
    }
    write_atomically(dest, &bytes)?;
    Ok(dest.to_path_buf())
}

/// Extract JPEG APP1 (EXIF/XMP), APP2 (ICC), and APP13 (IPTC) segments.
pub fn jpeg_metadata_segments(bytes: &[u8]) -> Vec<(u8, &[u8])> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            break;
        }
        let mut marker_at = pos;
        while marker_at + 1 < bytes.len() && bytes[marker_at + 1] == 0xFF {
            marker_at += 1;
        }
        let marker = bytes[marker_at + 1];
        let len_at = marker_at + 2;
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            pos = len_at;
            continue;
        }
        if marker == 0xDA || len_at + 2 > bytes.len() {
            break;
        }
        let length = usize::from(u16::from_be_bytes([bytes[len_at], bytes[len_at + 1]]));
        let seg_end = len_at + length;
        if length < 2 || seg_end > bytes.len() {
            break;
        }
        if matches!(marker, 0xE1 | 0xE2 | 0xED) {
            segments.push((marker, &bytes[pos..seg_end]));
        }
        pos = seg_end;
    }
    segments
}

/// Splice JPEG metadata segments from `original` right after `encoded`'s SOI marker.
pub fn splice_jpeg_metadata_segments(
    original: &[u8],
    encoded: Vec<u8>,
    keep_exif: bool,
) -> Vec<u8> {
    let segments: Vec<(u8, &[u8])> = jpeg_metadata_segments(original)
        .into_iter()
        .filter(|(marker, _)| keep_exif || *marker != 0xE1)
        .collect();
    if segments.is_empty() || encoded.len() < 2 || encoded[0] != 0xFF || encoded[1] != 0xD8 {
        return encoded;
    }
    let extra: usize = segments.iter().map(|(_, s)| s.len()).sum();
    let mut out = Vec::with_capacity(encoded.len() + extra);
    out.extend_from_slice(&encoded[..2]);
    for (_, seg) in segments {
        out.extend_from_slice(seg);
    }
    out.extend_from_slice(&encoded[2..]);
    out
}
