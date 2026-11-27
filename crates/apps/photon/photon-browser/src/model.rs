use std::path::Path;

use comp_file_browser::FileEntryLike;
pub use comp_file_browser::{
    compile_filter_regex as compile_browser_filter_regex, entry_name_matches_filter, format_bytes,
    format_modified_ms,
};
use fw_ui::icons::Icon;
use photon_protocol::{BrowserSortField, BrowserThumbSize, Entry, SortOrder};

use crate::state::BrowserViewState;

struct EntryAdapter<'a>(&'a Entry);

impl FileEntryLike for EntryAdapter<'_> {
    fn name(&self) -> &str {
        &self.0.name
    }

    fn is_dir(&self) -> bool {
        self.0.is_dir
    }

    fn size(&self) -> u64 {
        self.0.size
    }

    fn modified_ms(&self) -> u64 {
        self.0.modified_ms
    }

    fn kind(&self) -> &str {
        &self.0.kind
    }
}

fn to_comp_thumb_size(size: BrowserThumbSize) -> comp_file_browser::ThumbSize {
    match size {
        BrowserThumbSize::Xs => comp_file_browser::ThumbSize::Xs,
        BrowserThumbSize::S => comp_file_browser::ThumbSize::S,
        BrowserThumbSize::M => comp_file_browser::ThumbSize::M,
        BrowserThumbSize::L => comp_file_browser::ThumbSize::L,
        BrowserThumbSize::Xl => comp_file_browser::ThumbSize::Xl,
    }
}

fn from_comp_thumb_size(size: comp_file_browser::ThumbSize) -> BrowserThumbSize {
    match size {
        comp_file_browser::ThumbSize::Xs => BrowserThumbSize::Xs,
        comp_file_browser::ThumbSize::S => BrowserThumbSize::S,
        comp_file_browser::ThumbSize::M => BrowserThumbSize::M,
        comp_file_browser::ThumbSize::L => BrowserThumbSize::L,
        comp_file_browser::ThumbSize::Xl => BrowserThumbSize::Xl,
    }
}

pub fn entry_kind_label(entry: &Entry) -> String {
    comp_file_browser::entry_kind_label(&EntryAdapter(entry))
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
    comp_file_browser::thumb_card_min_width(to_comp_thumb_size(size))
}

pub(crate) fn thumb_size_label(size: BrowserThumbSize) -> &'static str {
    comp_file_browser::thumb_size_label(to_comp_thumb_size(size))
}

pub(crate) fn step_thumb_size(size: BrowserThumbSize, delta: isize) -> Option<BrowserThumbSize> {
    comp_file_browser::step_thumb_size(to_comp_thumb_size(size), delta).map(from_comp_thumb_size)
}

pub(crate) fn is_browser_filter_regex_invalid(state: BrowserViewState<'_>) -> bool {
    state.browser_filter_regex
        && !state.browser_filter.trim().is_empty()
        && compile_browser_filter_regex(state.browser_filter, state.browser_filter_case_sensitive)
            .is_err()
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
        if ordered == std::cmp::Ordering::Equal {
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
