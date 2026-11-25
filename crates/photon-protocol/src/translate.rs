//! Viewer OCR & translation settings, blocks, and results.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_libre_url() -> String {
    "http://localhost:5000".to_owned()
}

fn default_src_lang() -> String {
    "eng".to_owned()
}

fn default_tgt_lang() -> String {
    "vi".to_owned()
}

/// Translation backend provider used after Tesseract OCR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TranslateProvider {
    #[default]
    None,
    GoogleWeb,
    LibreTranslate,
}

/// Persisted OCR / translation configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslateSettings {
    #[serde(default)]
    pub provider: TranslateProvider,
    #[serde(default = "default_libre_url")]
    pub libre_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_src_lang")]
    pub default_source_lang: String,
    #[serde(default = "default_tgt_lang")]
    pub default_target_lang: String,
}

impl Default for TranslateSettings {
    fn default() -> Self {
        Self {
            provider: TranslateProvider::None,
            libre_endpoint: default_libre_url(),
            api_key: None,
            default_source_lang: default_src_lang(),
            default_target_lang: default_tgt_lang(),
        }
    }
}

/// One recognized / translated text box in document pixel coordinates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslationBlock {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub original: String,
    pub translated: String,
}

/// Complete OCR / translation output for an image path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub path: String,
    pub source_lang: String,
    pub target_lang: String,
    pub original_text: String,
    pub translated_text: String,
    #[serde(default)]
    pub blocks: Vec<TranslationBlock>,
}
