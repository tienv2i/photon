//! Native OS file/folder/save dialogs powered by `rfd`.

use std::path::PathBuf;

use rfd::AsyncFileDialog;

/// Open the OS native "Open With..." application chooser dialog for `path`.
pub async fn open_with_dialog(path: PathBuf) {
    #[cfg(target_os = "linux")]
    {
        if try_portal_open_with(&path).await {
            return;
        }
        let _ = std::process::Command::new("xdg-open")
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn pick_screen_color() -> Option<[u8; 3]> {
    None
}

/// Pick a pixel color anywhere on screen using the XDG Desktop Portal (`PickColor`).
#[cfg(target_os = "linux")]
pub async fn pick_screen_color() -> Option<[u8; 3]> {
    use iced::futures::StreamExt;
    use std::collections::HashMap;

    let conn = zbus::Connection::session().await.ok()?;
    let unique = conn
        .unique_name()?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!(
        "photon_pick_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let expected_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .ok()?
        .member("Response")
        .ok()?
        .path(expected_path.as_str())
        .ok()?
        .build();
    let mut stream = zbus::MessageStream::for_match_rule(rule, &conn, Some(4))
        .await
        .ok()?;

    let mut options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    options.insert("handle_token", zbus::zvariant::Value::from(token.as_str()));
    conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.Screenshot"),
        "PickColor",
        &("", options),
    )
    .await
    .ok()?;

    let msg = stream.next().await?.ok()?;
    let (code, results): (u32, HashMap<String, zbus::zvariant::OwnedValue>) =
        msg.body().deserialize().ok()?;
    if code != 0 {
        return None;
    }
    let val = results.get("color")?.try_clone().ok()?;
    let (r, g, b): (f64, f64, f64) = val.try_into().ok()?;
    Some([
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    ])
}

#[cfg(target_os = "linux")]
async fn try_portal_open_with(path: &std::path::Path) -> bool {
    use std::collections::HashMap;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(conn) = zbus::Connection::session().await else {
        return false;
    };
    let fd = zbus::zvariant::OwnedFd::from(std::os::fd::OwnedFd::from(file));
    let mut options: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    options.insert("ask", zbus::zvariant::Value::from(true));
    conn.call_method(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        Some("org.freedesktop.portal.OpenURI"),
        "OpenFile",
        &("", fd, options),
    )
    .await
    .is_ok()
}

/// File-extension filter passed to native Open/Save dialogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter<'a> {
    pub name: &'a str,
    pub extensions: &'a [&'a str],
}

/// Default raster & layered image formats supported by Photon.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "avif", "ico",
    "icns", "pnm", "pbm", "pgm", "ppm", "qoi", "tga", "hdr", "exr", "ora", "kra", "svg", "svgz",
    "apng", "pam", "dds", "ff", "cur", "jxl", "psd", "heic", "heif", "hif", "jp2", "j2k", "jpx",
    "jpf", "dng", "cr2", "cr3", "nef", "arw", "raf", "orf", "rw2", "ari", "crm", "crw", "dcr",
    "dcs", "erf", "iiq", "kdc", "mef", "mos", "mrw", "nrw", "ori", "pef", "raw", "rwl", "srw",
    "3fr", "fff", "x3f", "qtk",
];

/// Supported archive/document container extensions for open dialogs.
pub const ARCHIVE_EXTENSIONS: &[&str] = &["cbz", "zip", "pdf"];

/// Open a native OS directory picker dialog.
pub async fn pick_folder(title: &str, starting_dir: Option<PathBuf>) -> Option<PathBuf> {
    build_dialog(title, starting_dir, &[])
        .pick_folder()
        .await
        .map(|h| h.path().to_path_buf())
}

/// Open a native OS single-file picker dialog with optional extension filters.
pub async fn open_file(
    title: &str,
    starting_dir: Option<PathBuf>,
    filters: &[FileFilter<'_>],
) -> Option<PathBuf> {
    build_dialog(title, starting_dir, filters)
        .pick_file()
        .await
        .map(|h| h.path().to_path_buf())
}

/// Open a native OS multi-file picker dialog with optional extension filters.
pub async fn open_files(
    title: &str,
    starting_dir: Option<PathBuf>,
    filters: &[FileFilter<'_>],
) -> Vec<PathBuf> {
    build_dialog(title, starting_dir, filters)
        .pick_files()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.path().to_path_buf())
        .collect()
}

/// Open a native OS Save-As dialog with optional starting directory, default filename, and filters.
pub async fn save_file(
    title: &str,
    starting_dir: Option<PathBuf>,
    default_name: Option<&str>,
    filters: &[FileFilter<'_>],
) -> Option<PathBuf> {
    let mut dialog = build_dialog(title, starting_dir, filters);
    if let Some(name) = default_name
        && !name.trim().is_empty()
    {
        dialog = dialog.set_file_name(name);
    }
    dialog.save_file().await.map(|h| h.path().to_path_buf())
}

/// Convenience helper: pick one or more raster image files.
pub async fn open_images(title: &str, starting_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let filters = [FileFilter {
        name: "Images",
        extensions: IMAGE_EXTENSIONS,
    }];
    open_files(title, starting_dir, &filters).await
}

/// Convenience helper: Save-As dialog pre-configured for raster & layered image exports.
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

/// Convenience helper: Save-As dialog prioritizing OpenRaster (`.ora`) multi-layer format.
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

fn build_dialog(
    title: &str,
    starting_dir: Option<PathBuf>,
    filters: &[FileFilter<'_>],
) -> AsyncFileDialog {
    let mut dialog = AsyncFileDialog::new().set_title(title);
    if let Some(dir) = starting_dir
        && dir.is_dir()
    {
        dialog = dialog.set_directory(dir);
    }
    for f in filters {
        if !f.extensions.is_empty() {
            dialog = dialog.add_filter(f.name, f.extensions);
        }
    }
    dialog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_filter_contains_common_formats() {
        assert!(IMAGE_EXTENSIONS.contains(&"png"));
        assert!(IMAGE_EXTENSIONS.contains(&"jpg"));
        assert!(IMAGE_EXTENSIONS.contains(&"webp"));
        assert!(IMAGE_EXTENSIONS.contains(&"ora"));
        assert!(IMAGE_EXTENSIONS.contains(&"psd"));
        assert!(IMAGE_EXTENSIONS.contains(&"jxl"));
        assert!(IMAGE_EXTENSIONS.contains(&"svg"));
        assert!(IMAGE_EXTENSIONS.contains(&"jp2"));
        assert!(IMAGE_EXTENSIONS.contains(&"j2k"));
        assert!(IMAGE_EXTENSIONS.contains(&"jpx"));
        assert!(IMAGE_EXTENSIONS.contains(&"jpf"));
        assert!(IMAGE_EXTENSIONS.contains(&"heic"));
        assert!(IMAGE_EXTENSIONS.contains(&"heif"));
        assert!(IMAGE_EXTENSIONS.contains(&"hif"));
        assert!(IMAGE_EXTENSIONS.contains(&"icns"));
    }

    #[test]
    fn build_dialog_handles_missing_dir_and_filters() {
        let _d = build_dialog(
            "Open",
            Some(PathBuf::from("/nonexistent-photon-dir")),
            &[FileFilter {
                name: "Images",
                extensions: IMAGE_EXTENSIONS,
            }],
        );
    }
}
