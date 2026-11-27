//! Image-specific native OS file dialog helpers built atop `fw-dialogs`.

use std::path::PathBuf;

use fw_dialogs::{FileFilter, open_files, save_file};
use photon_protocol::IMAGE_EXTENSIONS;

/// Pick one or more raster/layered image files.
pub async fn open_images(title: &str, starting_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let filters = [FileFilter {
        name: "Images",
        extensions: IMAGE_EXTENSIONS,
    }];
    open_files(title, starting_dir, &filters).await
}

/// Save-As dialog pre-configured for raster & layered image exports.
pub async fn save_image_as(
    title: &str,
    starting_dir: Option<PathBuf>,
    default_name: Option<&str>,
) -> Option<PathBuf> {
    let filters = [
        FileFilter {
            name: "All Supported Images",
            extensions: &[
                "png", "jpg", "jpeg", "webp", "ora", "avif", "tiff", "tif", "gif", "bmp", "tga",
                "qoi", "exr", "hdr", "ico", "pnm", "ff",
            ],
        },
        FileFilter {
            name: "OpenRaster Layered Image (*.ora)",
            extensions: &["ora"],
        },
        FileFilter {
            name: "PNG Image (*.png)",
            extensions: &["png"],
        },
        FileFilter {
            name: "JPEG Image (*.jpg, *.jpeg)",
            extensions: &["jpg", "jpeg"],
        },
        FileFilter {
            name: "WebP Image (*.webp)",
            extensions: &["webp"],
        },
        FileFilter {
            name: "AVIF Image (*.avif)",
            extensions: &["avif"],
        },
        FileFilter {
            name: "TIFF Image (*.tiff, *.tif)",
            extensions: &["tiff", "tif"],
        },
        FileFilter {
            name: "GIF Image (*.gif)",
            extensions: &["gif"],
        },
        FileFilter {
            name: "BMP Image (*.bmp)",
            extensions: &["bmp"],
        },
        FileFilter {
            name: "QOI Image (*.qoi)",
            extensions: &["qoi"],
        },
        FileFilter {
            name: "TGA Image (*.tga)",
            extensions: &["tga"],
        },
        FileFilter {
            name: "OpenEXR Image (*.exr)",
            extensions: &["exr"],
        },
        FileFilter {
            name: "Radiance HDR (*.hdr)",
            extensions: &["hdr"],
        },
        FileFilter {
            name: "ICO Icon (*.ico)",
            extensions: &["ico"],
        },
        FileFilter {
            name: "Portable AnyMap (*.pnm)",
            extensions: &["pnm"],
        },
        FileFilter {
            name: "Farbfeld (*.ff)",
            extensions: &["ff"],
        },
    ];
    save_file(title, starting_dir, default_name, &filters).await
}

/// Save-As dialog prioritizing OpenRaster (`.ora`) multi-layer format.
pub async fn save_openraster_as(
    title: &str,
    starting_dir: Option<PathBuf>,
    default_name: Option<&str>,
) -> Option<PathBuf> {
    let filters = [
        FileFilter {
            name: "OpenRaster Layered Image (*.ora)",
            extensions: &["ora"],
        },
        FileFilter {
            name: "All Files",
            extensions: &["*"],
        },
    ];
    save_file(title, starting_dir, default_name, &filters).await
}
