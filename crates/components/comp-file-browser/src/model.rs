//! Domain-agnostic file entry formatting, filtering, thumbnail sizing, and sorting.

use std::cmp::Ordering;
use std::path::Path;

use regex_automata::meta::Regex;
use regex_automata::util::syntax::Config as RegexSyntaxConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbSize {
    Xs,
    S,
    M,
    L,
    Xl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    Modified,
    Size,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Minimal adapter trait implemented by any app's directory entry type.
pub trait FileEntryLike {
    fn name(&self) -> &str;
    fn is_dir(&self) -> bool;
    fn size(&self) -> u64;
    fn modified_ms(&self) -> u64;
    fn kind(&self) -> &str;
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn format_modified_ms(ms: u64) -> String {
    if ms == 0 {
        return "—".to_owned();
    }
    let secs = (ms / 1000) as i64;
    let days_since_epoch = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let (year, month, day) = civil_from_days(days_since_epoch);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// Howard Hinnant's civil-from-days conversion (no external date crate required).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

pub fn entry_kind_label<E: FileEntryLike>(entry: &E) -> String {
    if !entry.kind().is_empty() {
        return entry.kind().to_owned();
    }
    if entry.is_dir() {
        return "Folder".to_owned();
    }
    Path::new(entry.name())
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "File".to_owned())
}

pub fn thumb_card_min_width(size: ThumbSize) -> f32 {
    match size {
        ThumbSize::Xs => 96.0,
        ThumbSize::S => 125.0,
        ThumbSize::M => 155.0,
        ThumbSize::L => 200.0,
        ThumbSize::Xl => 250.0,
    }
}

pub fn thumb_size_label(size: ThumbSize) -> &'static str {
    match size {
        ThumbSize::Xs => "XS",
        ThumbSize::S => "S",
        ThumbSize::M => "M",
        ThumbSize::L => "L",
        ThumbSize::Xl => "XL",
    }
}

pub fn step_thumb_size(size: ThumbSize, delta: isize) -> Option<ThumbSize> {
    const ORDER: [ThumbSize; 5] = [
        ThumbSize::Xs,
        ThumbSize::S,
        ThumbSize::M,
        ThumbSize::L,
        ThumbSize::Xl,
    ];
    let idx = ORDER.iter().position(|s| *s == size)? as isize + delta;
    if (0..ORDER.len() as isize).contains(&idx) {
        Some(ORDER[idx as usize])
    } else {
        None
    }
}

pub fn compile_filter_regex(query: &str, case_sensitive: bool) -> Result<Option<Regex>, ()> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(None);
    }
    Regex::builder()
        .syntax(RegexSyntaxConfig::new().case_insensitive(!case_sensitive))
        .build(q)
        .map(Some)
        .map_err(|_| ())
}

pub fn entry_name_matches_filter(
    name: &str,
    query: &str,
    case_sensitive: bool,
    use_regex: bool,
    compiled_regex: Option<&Regex>,
) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    if use_regex {
        if let Some(re) = compiled_regex {
            return re.is_match(name);
        }
        return false;
    }
    if case_sensitive {
        name.contains(q)
    } else {
        name.to_lowercase().contains(&q.to_lowercase())
    }
}

pub fn sort_file_entries<E: FileEntryLike>(items: &mut [&E], field: SortField, order: SortOrder) {
    items.sort_by(|a, b| {
        if a.is_dir() != b.is_dir() {
            return b.is_dir().cmp(&a.is_dir());
        }
        let cmp = match field {
            SortField::Name => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
            SortField::Modified => a.modified_ms().cmp(&b.modified_ms()),
            SortField::Size => a.size().cmp(&b.size()),
            SortField::Kind => entry_kind_label(*a).cmp(&entry_kind_label(*b)),
        };
        let ordered = match order {
            SortOrder::Asc => cmp,
            SortOrder::Desc => cmp.reverse(),
        };
        if ordered == Ordering::Equal {
            a.name().to_lowercase().cmp(&b.name().to_lowercase())
        } else {
            ordered
        }
    });
}
