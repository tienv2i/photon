use std::cmp::Ordering;
use std::path::Path;

use photon_protocol::{BrowserSortField, BrowserThumbSize, Entry, SortOrder};
use photon_ui::icons::Icon;
use regex_automata::meta::Regex;
use regex_automata::util::syntax::Config as RegexSyntaxConfig;

use crate::state::BrowserViewState;

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

pub fn entry_kind_label(entry: &Entry) -> String {
    if !entry.kind.is_empty() {
        return entry.kind.clone();
    }
    if entry.is_dir {
        return "Folder".to_owned();
    }
    Path::new(&entry.name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "File".to_owned())
}

pub(crate) fn is_archive_path_str(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(photon_protocol::is_archive_extension)
}

pub(crate) fn entry_icon(entry: &Entry) -> Icon {
    if entry.is_dir {
        Icon::Folder
    } else if entry.is_image {
        Icon::Image
    } else if is_archive_path_str(&entry.path) {
        Icon::BookOpen
    } else {
        Icon::Info
    }
}

pub fn thumb_card_min_width(size: BrowserThumbSize) -> f32 {
    match size {
        BrowserThumbSize::Xs => 96.0,
        BrowserThumbSize::S => 125.0,
        BrowserThumbSize::M => 155.0,
        BrowserThumbSize::L => 200.0,
        BrowserThumbSize::Xl => 250.0,
    }
}

pub(crate) fn thumb_size_label(size: BrowserThumbSize) -> &'static str {
    match size {
        BrowserThumbSize::Xs => "XS",
        BrowserThumbSize::S => "S",
        BrowserThumbSize::M => "M",
        BrowserThumbSize::L => "L",
        BrowserThumbSize::Xl => "XL",
    }
}

pub(crate) fn step_thumb_size(size: BrowserThumbSize, delta: isize) -> Option<BrowserThumbSize> {
    const ORDER: [BrowserThumbSize; 5] = [
        BrowserThumbSize::Xs,
        BrowserThumbSize::S,
        BrowserThumbSize::M,
        BrowserThumbSize::L,
        BrowserThumbSize::Xl,
    ];
    let idx = ORDER.iter().position(|s| *s == size)? as isize + delta;
    if (0..ORDER.len() as isize).contains(&idx) {
        Some(ORDER[idx as usize])
    } else {
        None
    }
}

fn compile_browser_filter_regex(query: &str, case_sensitive: bool) -> Result<Option<Regex>, ()> {
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

pub(crate) fn is_browser_filter_regex_invalid(state: BrowserViewState<'_>) -> bool {
    state.browser_filter_regex
        && !state.browser_filter.trim().is_empty()
        && compile_browser_filter_regex(state.browser_filter, state.browser_filter_case_sensitive)
            .is_err()
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

pub(crate) fn sorted_filtered_entries(state: BrowserViewState<'_>) -> Vec<&Entry> {
    let compiled_regex = if state.browser_filter_regex {
        match compile_browser_filter_regex(
            state.browser_filter,
            state.browser_filter_case_sensitive,
        ) {
            Ok(opt) => opt,
            Err(()) => return Vec::new(),
        }
    } else {
        None
    };
    let mut items: Vec<&Entry> = state
        .snapshot
        .entries
        .iter()
        .filter(|e| {
            entry_name_matches_filter(
                &e.name,
                state.browser_filter,
                state.browser_filter_case_sensitive,
                state.browser_filter_regex,
                compiled_regex.as_ref(),
            )
        })
        .collect();

    let field = state.snapshot.meta.layout.browser_sort_field;
    let order = state.snapshot.meta.layout.browser_sort_order;
    items.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir);
        }
        let cmp = match field {
            BrowserSortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            BrowserSortField::Modified => a.modified_ms.cmp(&b.modified_ms),
            BrowserSortField::Size => a.size.cmp(&b.size),
            BrowserSortField::Kind => entry_kind_label(a).cmp(&entry_kind_label(b)),
        };
        let ordered = match order {
            SortOrder::Asc => cmp,
            SortOrder::Desc => cmp.reverse(),
        };
        if ordered == Ordering::Equal {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            ordered
        }
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_and_modified_produce_expected_strings() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MB");
        assert_eq!(format_modified_ms(0), "—");
        assert_eq!(format_modified_ms(1_700_000_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn thumb_stepper_respects_bounds() {
        assert_eq!(step_thumb_size(BrowserThumbSize::Xs, -1), None);
        assert_eq!(
            step_thumb_size(BrowserThumbSize::Xs, 1),
            Some(BrowserThumbSize::S)
        );
        assert_eq!(step_thumb_size(BrowserThumbSize::Xl, 1), None);
    }

    #[test]
    fn browser_filter_supports_case_sensitivity_and_regex() {
        assert!(entry_name_matches_filter(
            "Photo_01.PNG",
            "photo",
            false,
            false,
            None
        ));
        assert!(!entry_name_matches_filter(
            "Photo_01.PNG",
            "photo",
            true,
            false,
            None
        ));
        assert!(entry_name_matches_filter(
            "Photo_01.PNG",
            "Photo",
            true,
            false,
            None
        ));

        let re_ci = compile_browser_filter_regex(r"^img_\d+\.png$", false)
            .unwrap()
            .unwrap();
        assert!(entry_name_matches_filter(
            "IMG_042.png",
            r"^img_\d+\.png$",
            false,
            true,
            Some(&re_ci)
        ));

        let re_cs = compile_browser_filter_regex(r"^IMG_\d+\.png$", true)
            .unwrap()
            .unwrap();
        assert!(entry_name_matches_filter(
            "IMG_042.png",
            r"^IMG_\d+\.png$",
            true,
            true,
            Some(&re_cs)
        ));
        assert!(!entry_name_matches_filter(
            "img_042.png",
            r"^IMG_\d+\.png$",
            true,
            true,
            Some(&re_cs)
        ));

        assert!(compile_browser_filter_regex("[unclosed", false).is_err());
    }
}
