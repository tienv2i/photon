//! OpenRaster (`.ora`) multi-layer reader and writer (`zip` + `roxmltree` + `xmlwriter`).

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};
use photon_protocol::BlendMode;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::codec::{create_backup, write_atomically};
use crate::layer::{Layer, LayerStack, composite_stack};
use crate::selection::SelectionMask;
use crate::{ImageError, validate_dimensions};

const ORA_MIMETYPE: &[u8] = b"image/openraster";

/// Map Photon [`BlendMode`] to OpenRaster `composite-op` string.
pub fn blend_mode_to_ora(mode: BlendMode) -> &'static str {
    match mode {
        BlendMode::Normal => "svg:src-over",
        BlendMode::Multiply => "svg:multiply",
        BlendMode::Screen => "svg:screen",
        BlendMode::Overlay => "svg:overlay",
        BlendMode::Darken => "svg:darken",
        BlendMode::Lighten => "svg:lighten",
        BlendMode::Difference => "svg:difference",
    }
}

/// Map OpenRaster `composite-op` attribute to Photon [`BlendMode`].
pub fn ora_to_blend_mode(op: &str) -> BlendMode {
    match op.trim() {
        "svg:multiply" | "multiply" => BlendMode::Multiply,
        "svg:screen" | "screen" => BlendMode::Screen,
        "svg:overlay" | "overlay" => BlendMode::Overlay,
        "svg:darken" | "darken" => BlendMode::Darken,
        "svg:lighten" | "lighten" => BlendMode::Lighten,
        "svg:difference" | "difference" => BlendMode::Difference,
        _ => BlendMode::Normal,
    }
}

/// Fast header-only dimensions lookup from `stack.xml` inside an `.ora` archive.
pub fn probe_ora_dimensions(path: &Path) -> Option<(u32, u32)> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let xml = read_zip_string(&mut archive, "stack.xml").ok()?;
    let doc = roxmltree::Document::parse(&xml).ok()?;
    let image_node = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "image")?;
    let w = image_node.attribute("w")?.parse::<u32>().ok()?;
    let h = image_node.attribute("h")?.parse::<u32>().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

/// Read `Thumbnails/thumbnail.png` (falling back to `mergedimage.png`) scaled to `max_edge`.
pub fn ora_thumbnail(path: &Path, max_edge: u32) -> Result<RgbaImage, ImageError> {
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;

    let rgba = if let Ok(bytes) = read_zip_bytes(&mut archive, "Thumbnails/thumbnail.png") {
        decode_png_bytes(&bytes, path)?
    } else if let Ok(bytes) = read_zip_bytes(&mut archive, "mergedimage.png") {
        decode_png_bytes(&bytes, path)?
    } else {
        drop(archive);
        let stack = decode_ora_stack(path)?;
        (*composite_stack(&stack)).clone()
    };

    let (w, h) = rgba.dimensions();
    if max_edge == 0 || (w <= max_edge && h <= max_edge) {
        return Ok(rgba);
    }
    Ok(DynamicImage::ImageRgba8(rgba)
        .thumbnail(max_edge, max_edge)
        .to_rgba8())
}

/// Decode the flattened composite of an `.ora` file (`mergedimage.png` or composited stack).
pub fn decode_ora_merged(path: &Path) -> Result<RgbaImage, ImageError> {
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    if let Ok(bytes) = read_zip_bytes(&mut archive, "mergedimage.png")
        && let Ok(rgba) = decode_png_bytes(&bytes, path)
    {
        return Ok(rgba);
    }
    drop(archive);
    let stack = decode_ora_stack(path)?;
    Ok((*composite_stack(&stack)).clone())
}

/// Fast header-only dimensions lookup for `.kra` (Krita) archives.
pub fn probe_kra_dimensions(path: &Path) -> Option<(u32, u32)> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    for entry in ["mergedimage.png", "preview.png", "Thumbnails/thumbnail.png"] {
        if let Ok(bytes) = read_zip_bytes(&mut archive, entry)
            && let Ok(reader) = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()
            && let Ok((w, h)) = reader.into_dimensions()
            && w > 0
            && h > 0
        {
            return Some((w, h));
        }
    }
    None
}

/// Decode the flattened preview of a `.kra` archive (`mergedimage.png` / `preview.png`).
pub fn decode_kra_merged(path: &Path) -> Result<RgbaImage, ImageError> {
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    for entry in ["mergedimage.png", "preview.png", "Thumbnails/thumbnail.png"] {
        if let Ok(bytes) = read_zip_bytes(&mut archive, entry)
            && let Ok(reader) = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()
            && let Ok((w, h)) = reader.into_dimensions()
        {
            validate_dimensions(w, h)?;
            return image::load_from_memory(&bytes)
                .map(|d| d.to_rgba8())
                .map_err(|err| ImageError::Decode {
                    path: path.display().to_string(),
                    message: err.to_string(),
                });
        }
    }
    Err(ImageError::Decode {
        path: path.display().to_string(),
        message: "KRA archive does not contain mergedimage.png or preview.png".to_owned(),
    })
}

/// Read `Thumbnails/thumbnail.png` (falling back to `mergedimage.png` / `preview.png`) from `.kra`.
pub fn kra_thumbnail(path: &Path, max_edge: u32) -> Result<RgbaImage, ImageError> {
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;
    let mut decoded = None;
    for entry in ["Thumbnails/thumbnail.png", "mergedimage.png", "preview.png"] {
        if let Ok(bytes) = read_zip_bytes(&mut archive, entry)
            && let Ok(img) = image::load_from_memory(&bytes)
        {
            decoded = Some(img.to_rgba8());
            break;
        }
    }
    let rgba = decoded.ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "KRA archive does not contain a readable preview image".to_owned(),
    })?;
    let (w, h) = rgba.dimensions();
    if max_edge == 0 || (w <= max_edge && h <= max_edge) {
        return Ok(rgba);
    }
    Ok(DynamicImage::ImageRgba8(rgba)
        .thumbnail(max_edge, max_edge)
        .to_rgba8())
}

struct ParsedOraLayer {
    src: String,
    name: String,
    x: i32,
    y: i32,
    opacity: f32,
    visible: bool,
    blend_mode: BlendMode,
}

fn collect_stack_layers(
    node: roxmltree::Node<'_, '_>,
    parent_x: i32,
    parent_y: i32,
    parent_opacity: f32,
    parent_visible: bool,
    out: &mut Vec<ParsedOraLayer>,
) {
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "layer" => {
                let Some(src) = child.attribute("src") else {
                    continue;
                };
                let name = child
                    .attribute("name")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Layer")
                    .to_owned();
                let x = child
                    .attribute("x")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0)
                    + parent_x;
                let y = child
                    .attribute("y")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0)
                    + parent_y;
                let opacity = child
                    .attribute("opacity")
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0)
                    * parent_opacity;
                let visible = parent_visible
                    && child.attribute("visibility").unwrap_or("visible") != "hidden";
                let blend_mode = child
                    .attribute("composite-op")
                    .map(ora_to_blend_mode)
                    .unwrap_or(BlendMode::Normal);
                out.push(ParsedOraLayer {
                    src: src.to_owned(),
                    name,
                    x,
                    y,
                    opacity,
                    visible,
                    blend_mode,
                });
            }
            "stack" => {
                let sx = child
                    .attribute("x")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0)
                    + parent_x;
                let sy = child
                    .attribute("y")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0)
                    + parent_y;
                let sop = child
                    .attribute("opacity")
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0)
                    * parent_opacity;
                let svis = parent_visible
                    && child.attribute("visibility").unwrap_or("visible") != "hidden";
                collect_stack_layers(child, sx, sy, sop, svis, out);
            }
            _ => {}
        }
    }
}

/// Decode a multi-layer `.ora` archive into a Photon [`LayerStack`].
pub fn decode_ora_stack(path: &Path) -> Result<LayerStack, ImageError> {
    let path_str = path.display().to_string();
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path_str.clone(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path_str.clone(),
        message: err.to_string(),
    })?;

    let xml = read_zip_string(&mut archive, "stack.xml").map_err(|err| ImageError::Decode {
        path: path_str.clone(),
        message: format!("missing or unreadable stack.xml: {err}"),
    })?;
    let doc = roxmltree::Document::parse(&xml).map_err(|err| ImageError::Decode {
        path: path_str.clone(),
        message: format!("invalid stack.xml: {err}"),
    })?;

    let image_node = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "image")
        .ok_or_else(|| ImageError::Decode {
            path: path_str.clone(),
            message: "stack.xml missing <image> root".to_owned(),
        })?;
    let canvas_width = image_node
        .attribute("w")
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| ImageError::Decode {
            path: path_str.clone(),
            message: "stack.xml missing valid <image w>".to_owned(),
        })?;
    let canvas_height = image_node
        .attribute("h")
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(|| ImageError::Decode {
            path: path_str.clone(),
            message: "stack.xml missing valid <image h>".to_owned(),
        })?;
    validate_dimensions(canvas_width, canvas_height)?;

    let mut parsed_top_to_bottom = Vec::new();
    if let Some(root_stack) = image_node
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "stack")
    {
        collect_stack_layers(root_stack, 0, 0, 1.0, true, &mut parsed_top_to_bottom);
    }

    if parsed_top_to_bottom.is_empty() {
        if let Ok(merged_bytes) = read_zip_bytes(&mut archive, "mergedimage.png") {
            let rgba = decode_png_bytes(&merged_bytes, path)?;
            return Ok(LayerStack::from_single(Arc::new(rgba)));
        }
        return Err(ImageError::Decode {
            path: path_str,
            message: "ORA archive contains no layers".to_owned(),
        });
    }

    // OpenRaster <stack> stores topmost layer first; Photon LayerStack stores bottom-to-top.
    let mut layers = Vec::with_capacity(parsed_top_to_bottom.len());
    let mut next_id = 1u64;
    for parsed in parsed_top_to_bottom.into_iter().rev() {
        let png_bytes =
            read_zip_bytes(&mut archive, &parsed.src).map_err(|err| ImageError::Decode {
                path: path_str.clone(),
                message: format!("failed to read layer '{}': {err}", parsed.src),
            })?;
        let rgba = decode_png_bytes(&png_bytes, path)?;
        let mut layer = Layer::new(next_id, parsed.name, Arc::new(rgba));
        layer.offset_x = parsed.x;
        layer.offset_y = parsed.y;
        layer.opacity = parsed.opacity;
        layer.visible = parsed.visible;
        layer.blend_mode = parsed.blend_mode;
        layers.push(layer);
        next_id += 1;
    }

    let active_layer_id = layers.last().map(|l| l.id).unwrap_or(1);
    Ok(LayerStack {
        canvas_width,
        canvas_height,
        layers,
        active_layer_id,
        next_id,
        background: None,
        selection: SelectionMask::empty(canvas_width, canvas_height),
    })
}

/// Encode a [`LayerStack`] into OpenRaster (`.ora`) bytes in memory.
pub fn encode_ora_bytes(stack: &LayerStack) -> Result<Vec<u8>, ImageError> {
    let map_err = |msg: String| ImageError::Encode {
        path: "ora".to_owned(),
        message: msg,
    };

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    // Spec requirement: `mimetype` MUST be the first entry and MUST be Stored (uncompressed).
    writer
        .start_file("mimetype", stored)
        .map_err(|e| map_err(e.to_string()))?;
    writer
        .write_all(ORA_MIMETYPE)
        .map_err(|e| map_err(e.to_string()))?;

    let mut xml = xmlwriter::XmlWriter::new(xmlwriter::Options::default());
    xml.start_element("image");
    xml.write_attribute("version", "0.0.5");
    xml.write_attribute("w", &stack.canvas_width);
    xml.write_attribute("h", &stack.canvas_height);
    xml.start_element("stack");

    let mut layer_entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(stack.layers.len());
    // Write layers in top-to-bottom order inside stack.xml.
    for (idx, layer) in stack.layers.iter().rev().enumerate() {
        if layer.effect.is_some() {
            continue;
        }
        let entry_path = format!("data/layer_{idx}.png");
        let layer_img = materialize_layer_rgba_with_mask(layer);
        let png_bytes = encode_png_bytes(&layer_img).map_err(&map_err)?;

        xml.start_element("layer");
        xml.write_attribute("name", &layer.name);
        xml.write_attribute("src", &entry_path);
        xml.write_attribute("x", &layer.offset_x);
        xml.write_attribute("y", &layer.offset_y);
        xml.write_attribute("opacity", &format!("{:.4}", layer.opacity.clamp(0.0, 1.0)));
        xml.write_attribute(
            "visibility",
            if layer.visible { "visible" } else { "hidden" },
        );
        xml.write_attribute("composite-op", blend_mode_to_ora(layer.blend_mode));
        xml.end_element();

        layer_entries.push((entry_path, png_bytes));
    }

    xml.end_element(); // </stack>
    let xml_doc = xml.end_document();

    writer
        .start_file("stack.xml", deflated)
        .map_err(|e| map_err(e.to_string()))?;
    writer
        .write_all(xml_doc.as_bytes())
        .map_err(|e| map_err(e.to_string()))?;

    for (entry_path, png_bytes) in layer_entries {
        writer
            .start_file(&entry_path, deflated)
            .map_err(|e| map_err(e.to_string()))?;
        writer
            .write_all(&png_bytes)
            .map_err(|e| map_err(e.to_string()))?;
    }

    let merged = composite_stack(stack);
    let merged_png = encode_png_bytes(&merged).map_err(&map_err)?;
    writer
        .start_file("mergedimage.png", deflated)
        .map_err(|e| map_err(e.to_string()))?;
    writer
        .write_all(&merged_png)
        .map_err(|e| map_err(e.to_string()))?;

    let thumb_img = if merged.width() <= 256 && merged.height() <= 256 {
        (*merged).clone()
    } else {
        DynamicImage::ImageRgba8((*merged).clone())
            .thumbnail(256, 256)
            .to_rgba8()
    };
    let thumb_png = encode_png_bytes(&thumb_img).map_err(&map_err)?;
    writer
        .start_file("Thumbnails/thumbnail.png", deflated)
        .map_err(|e| map_err(e.to_string()))?;
    writer
        .write_all(&thumb_png)
        .map_err(|e| map_err(e.to_string()))?;

    let cursor = writer.finish().map_err(|e| map_err(e.to_string()))?;
    Ok(cursor.into_inner())
}

/// Write a [`LayerStack`] atomically as `.ora`, preserving `<dest>.bak` when overwriting.
pub fn encode_ora_stack(stack: &LayerStack, dest: &Path) -> Result<PathBuf, ImageError> {
    let bytes = encode_ora_bytes(stack)?;
    create_backup(dest)?;
    write_atomically(dest, &bytes)?;
    Ok(dest.to_path_buf())
}

fn materialize_layer_rgba_with_mask(layer: &Layer) -> RgbaImage {
    let (lw, lh) = layer.dimensions();
    if !layer.mask_enabled {
        return (*layer.pixels).clone();
    }
    let Some(mask) = layer
        .mask
        .as_deref()
        .filter(|m| m.len() == (lw as usize) * (lh as usize))
    else {
        return (*layer.pixels).clone();
    };
    let mut out = (*layer.pixels).clone();
    let mox = layer.mask_offset_x;
    let moy = layer.mask_offset_y;
    for y in 0..lh {
        let my = y as i32 - moy;
        for x in 0..lw {
            let mx = x as i32 - mox;
            let m = if mx >= 0 && my >= 0 && (mx as u32) < lw && (my as u32) < lh {
                u16::from(mask[(my as usize) * (lw as usize) + (mx as usize)])
            } else {
                0
            };
            let px = out.get_pixel_mut(x, y);
            let a = ((u16::from(px.0[3]) * m + 127) / 255) as u8;
            *px = Rgba([px.0[0], px.0[1], px.0[2], a]);
        }
    }
    out
}

fn encode_png_bytes(rgba: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    image::codecs::png::PngEncoder::new(&mut cursor)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

fn decode_png_bytes(bytes: &[u8], path: &Path) -> Result<RgbaImage, ImageError> {
    image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map(|d| d.to_rgba8())
        .map_err(|e| ImageError::Decode {
            path: path.display().to_string(),
            message: e.to_string(),
        })
}

fn read_zip_bytes(archive: &mut ZipArchive<File>, name: &str) -> std::io::Result<Vec<u8>> {
    let mut entry = archive.by_name(name)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

fn read_zip_string(archive: &mut ZipArchive<File>, name: &str) -> std::io::Result<String> {
    let mut entry = archive.by_name(name)?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}
