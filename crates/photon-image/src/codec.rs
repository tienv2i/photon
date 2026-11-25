//! Image decoding, header probing, thumbnailing, atomic save with `.bak` backup,
//! and delivery export with optional downscaling, JPEG quality, and metadata splicing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, RgbaImage};

use crate::ImageError;

pub const DEFAULT_JPEG_QUALITY: u8 = 92;

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
            _ => return None,
        })
    }

    pub const CHOICES: &'static str = ".png .jpg .webp .tiff .gif .bmp .tga .qoi";
}

/// Fast header-only dimensions lookup without decoding full pixel buffers.
pub fn probe_dimensions(path: &Path) -> Option<(u32, u32)> {
    ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// Decode an image into RGBA8 pixels on CPU.
pub fn decode_rgba(path: &Path) -> Result<RgbaImage, ImageError> {
    let reader = ImageReader::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let reader = reader
        .with_guessed_format()
        .map_err(|source| ImageError::Io {
            path: path.display().to_string(),
            source,
        })?;
    let dyn_img = reader.decode().map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    Ok(dyn_img.to_rgba8())
}

/// Generate a thumbnail RGBA8 buffer scaled proportionally to fit `max_edge`.
pub fn generate_thumbnail(path: &Path, max_edge: u32) -> Result<RgbaImage, ImageError> {
    let rgba = decode_rgba(path)?;
    let (w, h) = rgba.dimensions();
    if w <= max_edge && h <= max_edge {
        return Ok(rgba);
    }
    let dyn_img = DynamicImage::ImageRgba8(rgba);
    Ok(dyn_img.thumbnail(max_edge, max_edge).to_rgba8())
}

/// Encode `pixels` into `format` using optional JPEG `quality` (`10..=100`).
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
            let mut cursor = std::io::Cursor::new(bytes);
            DynamicImage::ImageRgba8(pixels.clone())
                .write_to(&mut cursor, ImageFormat::Gif)
                .map_err(|e| map_err(e.to_string()))?;
            bytes = cursor.into_inner();
        }
        SaveFormat::Webp => {
            let mut cursor = std::io::Cursor::new(bytes);
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
        other => {
            let mut cursor = std::io::Cursor::new(bytes);
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
                SaveFormat::Jpeg | SaveFormat::Gif | SaveFormat::Webp => unreachable!(),
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
