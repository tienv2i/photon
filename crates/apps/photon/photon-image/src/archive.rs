//! Hybrid archive (`.cbz` / `.zip`) and `.pdf` page session backend.
//!
//! Indexes archive/PDF pages without eagerly extracting or rasterizing the entire container,
//! then materializes requested pages lazily into a per-session temp directory so Viewer,
//! OCR/translate plugins, and Editor can operate on real disk paths.

use std::cmp::Ordering;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use image::RgbaImage;
use zip::ZipArchive;

use crate::ImageError;

static SESSION_SEQ: AtomicU64 = AtomicU64::new(1);

const ARCHIVE_ROOT_DIR: &str = "photon-archives";

const ARCHIVE_IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "png", "webp", "gif", "bmp", "tif", "tiff", "avif", "ico", "pnm",
    "pbm", "pgm", "ppm", "qoi", "tga", "hdr", "exr", "ora", "kra", "svg", "svgz", "apng", "pam",
    "dds", "ff", "cur", "jxl", "psd", "heic", "heif", "hif", "jp2", "j2k", "jpx", "jpf",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    ZipCbz,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePageEntry {
    pub index: usize,
    pub label: String,
}

/// Maximum number of extracted/rasterized archive pages kept on disk per open session.
const MAX_MATERIALIZED_ARCHIVE_PAGES: usize = 32;

pub struct ArchiveSession {
    kind: ArchiveKind,
    source_path: PathBuf,
    temp_dir: PathBuf,
    pages: Vec<ArchivePageEntry>,
    entry_names: Vec<String>,
    materialized_pages: Vec<Option<PathBuf>>,
    materialized_lru: Vec<usize>,
}

impl Drop for ArchiveSession {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

/// Returns `true` if `path` has a supported archive/document container extension (`.cbz`, `.zip`, `.pdf`).
pub fn is_archive_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("cbz" | "zip" | "pdf")
    )
}

impl ArchiveSession {
    /// Open a `.cbz`/`.zip` or `.pdf` container, indexing its pages without eager full extraction.
    pub fn open(path: &Path) -> Result<Self, ImageError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let kind = match ext.as_deref() {
            Some("cbz" | "zip") => ArchiveKind::ZipCbz,
            Some("pdf") => ArchiveKind::Pdf,
            _ => {
                return Err(ImageError::Decode {
                    path: path.display().to_string(),
                    message: "unsupported archive/container format".to_owned(),
                });
            }
        };

        let root = std::env::temp_dir().join(ARCHIVE_ROOT_DIR);
        cleanup_stale_sessions(&root);
        let seq = SESSION_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
        let temp_dir = root.join(format!("{}-{seq}", std::process::id()));
        fs::create_dir_all(&temp_dir).map_err(|source| ImageError::Io {
            path: temp_dir.display().to_string(),
            source,
        })?;

        let res = match kind {
            ArchiveKind::ZipCbz => open_zip_index(path),
            ArchiveKind::Pdf => open_pdf_index(path),
        };

        let (pages, entry_names) = match res {
            Ok(v) => v,
            Err(err) => {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(err);
            }
        };

        let materialized_pages = vec![None; pages.len()];
        Ok(Self {
            kind,
            source_path: path.to_path_buf(),
            temp_dir,
            pages,
            entry_names,
            materialized_pages,
            materialized_lru: Vec::new(),
        })
    }

    pub fn kind(&self) -> ArchiveKind {
        self.kind
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn pages(&self) -> &[ArchivePageEntry] {
        &self.pages
    }

    /// Lazily extract or rasterize page `index` into the session temp dir and return its disk path.
    pub fn materialize_page(&mut self, index: usize) -> Result<PathBuf, ImageError> {
        if index >= self.pages.len() {
            return Err(ImageError::Decode {
                path: self.source_path.display().to_string(),
                message: format!("archive page index {index} out of bounds"),
            });
        }
        if let Some(Some(cached)) = self.materialized_pages.get(index)
            && cached.exists()
        {
            let out = cached.clone();
            self.touch_materialized_lru(index);
            return Ok(out);
        }

        let path = match self.kind {
            ArchiveKind::ZipCbz => self.materialize_zip_page(index)?,
            ArchiveKind::Pdf => self.materialize_pdf_page(index)?,
        };
        self.materialized_pages[index] = Some(path.clone());
        self.touch_materialized_lru(index);
        self.evict_excess_materialized_pages();
        Ok(path)
    }

    fn touch_materialized_lru(&mut self, index: usize) {
        if let Some(pos) = self.materialized_lru.iter().position(|&i| i == index) {
            self.materialized_lru.remove(pos);
        }
        self.materialized_lru.push(index);
    }

    fn evict_excess_materialized_pages(&mut self) {
        while self.materialized_lru.len() > MAX_MATERIALIZED_ARCHIVE_PAGES {
            let old_idx = self.materialized_lru.remove(0);
            if let Some(old_path) = self
                .materialized_pages
                .get_mut(old_idx)
                .and_then(Option::take)
            {
                let _ = fs::remove_file(old_path);
            }
        }
    }

    /// Render or decode a thumbnail for page `index` scaled to `max_edge`.
    pub fn page_thumbnail(&mut self, index: usize, max_edge: u32) -> Result<RgbaImage, ImageError> {
        if index >= self.pages.len() {
            return Err(ImageError::Decode {
                path: self.source_path.display().to_string(),
                message: format!("archive page index {index} out of bounds"),
            });
        }
        match self.kind {
            ArchiveKind::ZipCbz => {
                let page_path = self.materialize_page(index)?;
                crate::generate_thumbnail(&page_path, max_edge)
            }
            ArchiveKind::Pdf => {
                let page_num = index + 1;
                let edge = max_edge.max(16);
                let prefix = self.temp_dir.join(format!("thumb-{page_num:04}-{edge}"));
                let png_path = self
                    .temp_dir
                    .join(format!("thumb-{page_num:04}-{edge}.png"));
                if !png_path.exists() {
                    run_pdftoppm_page(
                        &self.source_path,
                        page_num,
                        &["-scale-to", &edge.to_string()],
                        &prefix,
                    )?;
                    if !png_path.exists() {
                        return Err(ImageError::Decode {
                            path: self.source_path.display().to_string(),
                            message: format!(
                                "pdftoppm did not produce thumbnail for page {page_num}"
                            ),
                        });
                    }
                }
                crate::decode_rgba(&png_path)
            }
        }
    }

    fn materialize_zip_page(&self, index: usize) -> Result<PathBuf, ImageError> {
        let entry_name = &self.entry_names[index];
        let safe_name = sanitize_zip_filename(index, entry_name);
        let dest = self.temp_dir.join(safe_name);

        let file = File::open(&self.source_path).map_err(|source| ImageError::Io {
            path: self.source_path.display().to_string(),
            source,
        })?;
        let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
            path: self.source_path.display().to_string(),
            message: err.to_string(),
        })?;
        let mut entry = archive
            .by_name(entry_name)
            .map_err(|err| ImageError::Decode {
                path: self.source_path.display().to_string(),
                message: err.to_string(),
            })?;

        let mut out = File::create(&dest).map_err(|source| ImageError::Io {
            path: dest.display().to_string(),
            source,
        })?;
        io::copy(&mut entry, &mut out).map_err(|source| ImageError::Io {
            path: dest.display().to_string(),
            source,
        })?;
        Ok(dest)
    }

    fn materialize_pdf_page(&self, index: usize) -> Result<PathBuf, ImageError> {
        let page_num = index + 1;
        let prefix = self.temp_dir.join(format!("page-{page_num:04}"));
        let png_path = self.temp_dir.join(format!("page-{page_num:04}.png"));
        run_pdftoppm_page(&self.source_path, page_num, &["-r", "150"], &prefix)?;
        if !png_path.exists() {
            return Err(ImageError::Decode {
                path: self.source_path.display().to_string(),
                message: format!("pdftoppm did not produce output for page {page_num}"),
            });
        }
        Ok(png_path)
    }
}

fn run_pdftoppm_page(
    pdf_path: &Path,
    page_num: usize,
    extra_args: &[&str],
    out_prefix: &Path,
) -> Result<(), ImageError> {
    let page_s = page_num.to_string();
    let out = ProcessCommand::new("pdftoppm")
        .arg("-png")
        .args(extra_args)
        .args(["-f", &page_s, "-l", &page_s, "-singlefile"])
        .arg(pdf_path)
        .arg(out_prefix)
        .output()
        .map_err(|err| ImageError::Decode {
            path: pdf_path.display().to_string(),
            message: format!("cannot run pdftoppm: {err}"),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(ImageError::Decode {
            path: pdf_path.display().to_string(),
            message: if stderr.is_empty() {
                format!("pdftoppm failed on page {page_num}")
            } else {
                format!("pdftoppm failed on page {page_num}: {stderr}")
            },
        });
    }
    Ok(())
}

fn open_zip_index(path: &Path) -> Result<(Vec<ArchivePageEntry>, Vec<String>), ImageError> {
    let file = File::open(path).map_err(|source| ImageError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|err| ImageError::Decode {
        path: path.display().to_string(),
        message: err.to_string(),
    })?;

    let mut names = Vec::new();
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name();
        if is_ignored_zip_entry(name) || !is_supported_archive_image(name) {
            continue;
        }
        names.push(name.to_owned());
    }

    names.sort_by(|a, b| natural_cmp(a, b));
    let pages = names
        .iter()
        .enumerate()
        .map(|(index, name)| ArchivePageEntry {
            index,
            label: name.clone(),
        })
        .collect();
    Ok((pages, names))
}

fn open_pdf_index(path: &Path) -> Result<(Vec<ArchivePageEntry>, Vec<String>), ImageError> {
    let out = ProcessCommand::new("pdfinfo")
        .arg(path)
        .output()
        .map_err(|err| ImageError::Decode {
            path: path.display().to_string(),
            message: format!("cannot run pdfinfo: {err}"),
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(ImageError::Decode {
            path: path.display().to_string(),
            message: if stderr.is_empty() {
                "pdfinfo failed".to_owned()
            } else {
                format!("pdfinfo failed: {stderr}")
            },
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let page_count = parse_pdfinfo_page_count(&stdout).ok_or_else(|| ImageError::Decode {
        path: path.display().to_string(),
        message: "pdfinfo output missing Pages count".to_owned(),
    })?;
    let pages = (0..page_count)
        .map(|index| ArchivePageEntry {
            index,
            label: format!("page-{:04}.png", index + 1),
        })
        .collect();
    Ok((pages, Vec::new()))
}

/// Parse `Pages: <N>` from `pdfinfo` stdout.
pub fn parse_pdfinfo_page_count(stdout: &str) -> Option<usize> {
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Pages:")
            && let Ok(n) = rest.trim().parse::<usize>()
        {
            return Some(n);
        }
    }
    None
}

fn is_ignored_zip_entry(name: &str) -> bool {
    if name.ends_with('/') {
        return true;
    }
    name.split(['/', '\\'])
        .any(|comp| comp == "__MACOSX" || comp.starts_with("._") || comp.starts_with('.'))
}

fn is_supported_archive_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ARCHIVE_IMAGE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Sanitize a ZIP entry path into a flat filename safe against zip-slip traversal.
pub fn sanitize_zip_filename(index: usize, entry_name: &str) -> String {
    let base = entry_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("page")
        .trim();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    let final_name = if cleaned.is_empty() { "page" } else { cleaned };
    format!("{index:05}_{final_name}")
}

/// Natural human sort comparison (`page2.jpg < page10.jpg`).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ia = a.as_bytes();
    let mut ib = b.as_bytes();
    while !ia.is_empty() && !ib.is_empty() {
        let da = ia[0].is_ascii_digit();
        let db = ib[0].is_ascii_digit();
        if da && db {
            let na_len = ia
                .iter()
                .position(|c| !c.is_ascii_digit())
                .unwrap_or(ia.len());
            let nb_len = ib
                .iter()
                .position(|c| !c.is_ascii_digit())
                .unwrap_or(ib.len());
            let (na, rest_a) = ia.split_at(na_len);
            let (nb, rest_b) = ib.split_at(nb_len);
            let trim_a = trim_leading_zeros(na);
            let trim_b = trim_leading_zeros(nb);
            match trim_a
                .len()
                .cmp(&trim_b.len())
                .then_with(|| trim_a.cmp(trim_b))
            {
                Ordering::Equal => {
                    // Tie-break on zero-padding length, then continue.
                    if na.len() != nb.len() {
                        return na.len().cmp(&nb.len());
                    }
                    ia = rest_a;
                    ib = rest_b;
                }
                ord => return ord,
            }
        } else {
            let ca = ia[0].to_ascii_lowercase();
            let cb = ib[0].to_ascii_lowercase();
            match ca.cmp(&cb) {
                Ordering::Equal => {
                    if ia[0] != ib[0] {
                        return ia[0].cmp(&ib[0]);
                    }
                    ia = &ia[1..];
                    ib = &ib[1..];
                }
                ord => return ord,
            }
        }
    }
    ia.len().cmp(&ib.len())
}

fn trim_leading_zeros(digits: &[u8]) -> &[u8] {
    let first_nz = digits
        .iter()
        .position(|&b| b != b'0')
        .unwrap_or(digits.len());
    &digits[first_nz..]
}

fn cleanup_stale_sessions(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let my_pid = std::process::id();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((pid_s, _)) = name.split_once('-') else {
            continue;
        };
        let Ok(pid) = pid_s.parse::<u32>() else {
            continue;
        };
        if pid == my_pid {
            continue;
        }
        #[cfg(target_os = "linux")]
        if !Path::new(&format!("/proc/{pid}")).exists() {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ExtendedColorType, ImageEncoder};
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    #[test]
    fn natural_sort_orders_numeric_segments_correctly() {
        let mut items = vec!["page10.jpg", "page2.jpg", "page1.jpg", "page02.jpg"];
        items.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            items,
            vec!["page1.jpg", "page2.jpg", "page02.jpg", "page10.jpg"]
        );
    }

    #[test]
    fn sanitize_zip_filename_blocks_traversal() {
        let safe = sanitize_zip_filename(3, "../../etc/passwd");
        assert!(!safe.contains('/'));
        assert!(!safe.contains(".."));
        assert_eq!(safe, "00003_passwd");
    }

    #[test]
    fn parse_pdfinfo_extracts_page_count() {
        let sample = "Title: Demo\nAuthor: Test\nPages:          42\nEncrypted:      no\n";
        assert_eq!(parse_pdfinfo_page_count(sample), Some(42));
        assert_eq!(parse_pdfinfo_page_count("Title: None\n"), None);
    }

    #[test]
    fn zip_cbz_session_indexes_materializes_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("photon-archive-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let cbz_path = dir.join("manga.cbz");

        let mut png1 = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png1)
            .write_image(&[255, 0, 0, 255], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
        let mut png2 = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png2)
            .write_image(
                &[
                    0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255,
                ],
                2,
                2,
                ExtendedColorType::Rgba8,
            )
            .unwrap();

        {
            let f = File::create(&cbz_path).unwrap();
            let mut zw = ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zw.start_file("__MACOSX/._page1.png", opts).unwrap();
            zw.write_all(b"junk").unwrap();
            zw.start_file("notes.txt", opts).unwrap();
            zw.write_all(b"readme").unwrap();
            zw.start_file("ch1/page10.png", opts).unwrap();
            zw.write_all(&png2).unwrap();
            zw.start_file("ch1/page2.png", opts).unwrap();
            zw.write_all(&png1).unwrap();
            zw.finish().unwrap();
        }

        let temp_dir_path;
        {
            let mut session = ArchiveSession::open(&cbz_path).unwrap();
            temp_dir_path = session.temp_dir().to_path_buf();
            assert_eq!(session.kind(), ArchiveKind::ZipCbz);
            assert_eq!(session.page_count(), 2);
            assert_eq!(session.pages()[0].label, "ch1/page2.png");
            assert_eq!(session.pages()[1].label, "ch1/page10.png");

            let p0 = session.materialize_page(0).unwrap();
            assert!(p0.is_file());
            assert_eq!(crate::decode_rgba(&p0).unwrap().dimensions(), (1, 1));
            // Second call hits cached path.
            assert_eq!(session.materialize_page(0).unwrap(), p0);

            let thumb = session.page_thumbnail(1, 16).unwrap();
            assert_eq!(thumb.dimensions(), (2, 2));
        }
        assert!(!temp_dir_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
