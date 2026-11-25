//! Pure-Rust Fluent (`.ftl`) localization embedded at compile time.
//!
//! `Core` emits `UiText { code, args }`; this crate translates keys and `UiText`
//! values for `en-US` and `vi-VN` with compile-time embedding and automated
//! parity tests across locales.

use std::collections::{BTreeMap, HashMap};

use fluent_templates::{Loader, fluent_bundle::FluentValue, static_loader};
use photon_protocol::{Language, UiText};
use unic_langid::{LanguageIdentifier, langid};

pub const EN_US: LanguageIdentifier = langid!("en-US");
pub const VI_VN: LanguageIdentifier = langid!("vi-VN");

static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en-US",
    };
}

pub fn resolve_lang_id(language: Language) -> LanguageIdentifier {
    match language {
        Language::English => EN_US,
        Language::Vietnamese => VI_VN,
        Language::System => detect_system_language(),
    }
}

fn detect_system_language() -> LanguageIdentifier {
    for var in ["PHOTON_UI_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let lower = val.to_ascii_lowercase();
            if lower.starts_with("vi") {
                return VI_VN;
            }
            if lower.starts_with("en") {
                return EN_US;
            }
        }
    }
    EN_US
}

pub fn tr(language: Language, key: &str) -> String {
    let lang_id = resolve_lang_id(language);
    LOCALES.lookup(&lang_id, key)
}

pub fn tr_args(language: Language, key: &str, args: &BTreeMap<String, String>) -> String {
    let lang_id = resolve_lang_id(language);
    let fluent_args: HashMap<std::borrow::Cow<'static, str>, FluentValue<'_>> = args
        .iter()
        .map(|(k, v)| {
            let val = if let Ok(num) = v.parse::<i64>() {
                FluentValue::from(num)
            } else {
                FluentValue::from(v.as_str())
            };
            (std::borrow::Cow::Owned(k.clone()), val)
        })
        .collect();
    LOCALES.lookup_with_args(&lang_id, key, &fluent_args)
}

pub fn tr_ui_text(language: Language, ui: &UiText) -> String {
    if ui.is_empty() {
        return String::new();
    }
    // Convert snake_case Core codes (e.g. `folder_contents`) to kebab-case Fluent IDs (`folder-contents`).
    let key = ui.code.replace('_', "-");
    let translated = if ui.args.is_empty() {
        tr(language, &key)
    } else {
        tr_args(language, &key, &ui.args)
    };
    if translated.contains("Unknown localization") || translated == key {
        ui.english()
    } else {
        translated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn parse_ftl_keys_and_vars(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut map = BTreeMap::new();
        let mut current_key: Option<String> = None;
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if !line.starts_with(' ')
                && !line.starts_with('\t')
                && let Some((key, rest)) = line.split_once('=')
            {
                let key = key.trim().to_owned();
                let vars = extract_vars(rest);
                map.insert(key.clone(), vars);
                current_key = Some(key);
            } else if let Some(key) = &current_key {
                let vars = extract_vars(line);
                if let Some(existing) = map.get_mut(key) {
                    existing.extend(vars);
                }
            }
        }
        map
    }

    fn extract_vars(text: &str) -> BTreeSet<String> {
        let mut vars = BTreeSet::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == b'$' {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if end > start {
                    vars.insert(text[start..end].to_owned());
                }
                i = end;
            } else {
                i += 1;
            }
        }
        vars
    }

    #[test]
    fn en_and_vi_catalogues_match_keys_and_placeholders() {
        let en_src = include_str!("../locales/en-US/main.ftl");
        let vi_src = include_str!("../locales/vi-VN/main.ftl");
        let en = parse_ftl_keys_and_vars(en_src);
        let vi = parse_ftl_keys_and_vars(vi_src);

        let en_keys: BTreeSet<_> = en.keys().cloned().collect();
        let vi_keys: BTreeSet<_> = vi.keys().cloned().collect();

        let missing_in_vi: Vec<_> = en_keys.difference(&vi_keys).collect();
        let extra_in_vi: Vec<_> = vi_keys.difference(&en_keys).collect();
        assert!(
            missing_in_vi.is_empty(),
            "Missing in vi-VN: {missing_in_vi:?}"
        );
        assert!(extra_in_vi.is_empty(), "Extra in vi-VN: {extra_in_vi:?}");

        for (key, en_vars) in &en {
            let vi_vars = &vi[key];
            assert_eq!(
                en_vars, vi_vars,
                "Placeholder mismatch for key `{key}`: en={en_vars:?}, vi={vi_vars:?}"
            );
        }
    }

    #[test]
    fn ui_text_renders_in_english_and_vietnamese() {
        let ui = UiText::with("folder_contents", [("items", "12"), ("images", "8")]);
        let en = tr_ui_text(Language::English, &ui);
        let vi = tr_ui_text(Language::Vietnamese, &ui);
        assert!(
            en.contains("12") && en.contains("8") && en.contains("images"),
            "{en}"
        );
        assert!(
            vi.contains("12") && vi.contains("8") && vi.contains("ảnh"),
            "{vi}"
        );
    }

    #[test]
    fn editor_ui_text_codes_translate_without_missing_vars() {
        let cases = [
            UiText::with("op_canvas_size", [("width", "800"), ("height", "600")]),
            UiText::with("op_rotate_angle", [("degrees", "15.5")]),
            UiText::with("op_layer_add", [("name", "Layer 2")]),
            UiText::with("editor_exported", [("path", "/tmp/out.jpg")]),
            UiText::with("history_jumped", [("index", "3")]),
            UiText::new("op_adjust_hsv"),
            UiText::new("op_color_balance"),
            UiText::new("op_adjust_curves"),
            UiText::new("op_brush"),
            UiText::new("op_eraser"),
            UiText::new("op_layer_duplicate"),
            UiText::new("op_layer_delete"),
            UiText::new("op_layer_reorder"),
            UiText::new("op_layer_props"),
            UiText::new("op_layer_merge_down"),
            UiText::new("op_layer_flatten"),
            UiText::new("op_canvas_bg"),
            UiText::new("layer_locked"),
            UiText::new("layer_hidden"),
            UiText::new("layer_not_found"),
            UiText::new("cannot_delete_last_layer"),
            UiText::new("cannot_merge_bottom_layer"),
            UiText::new("op_liquify"),
            UiText::new("op_mask_add"),
            UiText::new("op_mask_delete"),
            UiText::new("op_mask_apply"),
            UiText::new("op_mask_invert"),
            UiText::new("translate_done"),
            UiText::new("translate_ocr_unavailable"),
            UiText::new("ai_feature_disabled"),
            UiText::new("batch_already_running"),
            UiText::new("batch_no_images"),
            UiText::with("trashed_items", [("count", "2")]),
            UiText::with("deleted_items", [("count", "2")]),
            UiText::with("copied_items", [("count", "3")]),
            UiText::with("moved_items", [("count", "1")]),
            UiText::with("renamed_item", [("name", "photo.jpg")]),
            UiText::with("favorite_added", [("name", "Pictures")]),
            UiText::with("favorite_removed", [("name", "Pictures")]),
            UiText::with("batch_done", [("done", "5"), ("total", "5")]),
            UiText::with("batch_cancelled", [("done", "2"), ("total", "5")]),
            UiText::with("batch_too_many", [("max", "500")]),
            UiText::with("batch_duplicate_output", [("path", "/tmp/a.png")]),
            UiText::with("batch_output_is_source", [("path", "/tmp/a.png")]),
            UiText::with("batch_convert_overwrite", [("path", "/tmp/a.png")]),
        ];
        for lang in [Language::English, Language::Vietnamese] {
            for ui in &cases {
                let rendered = tr_ui_text(lang, ui);
                assert!(!rendered.is_empty(), "empty translation for {:?}", ui.code);
                assert!(
                    !rendered.contains('$') && !rendered.contains("Unknown localization"),
                    "broken fluent rendering for {:?} in {:?}: {rendered}",
                    ui.code,
                    lang
                );
            }
        }
    }
}
