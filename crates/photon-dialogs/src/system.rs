//! Native OS file/folder/save dialogs powered by `rfd`.

use std::path::PathBuf;

use rfd::AsyncFileDialog;

/// File-extension filter passed to native Open/Save dialogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter<'a> {
    pub name: &'a str,
    pub extensions: &'a [&'a str],
}

/// Default raster image formats supported by Photon.
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff", "tif", "ico", "qoi", "tga",
];

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

/// Convenience helper: Save-As dialog pre-configured for raster image exports.
pub async fn save_image_as(
    title: &str,
    starting_dir: Option<PathBuf>,
    default_name: Option<&str>,
) -> Option<PathBuf> {
    let filters = [FileFilter {
        name: "Images",
        extensions: &["png", "jpg", "jpeg", "webp", "bmp", "tiff"],
    }];
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
