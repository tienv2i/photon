//! Protocol types for non-destructive Text Layers (`TextLayerData`, `TextAlign`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn default_font_family() -> String {
    "Sans Serif".to_owned()
}

fn default_font_size() -> f32 {
    32.0
}

fn default_text_color() -> [u8; 3] {
    [255, 255, 255]
}

fn default_line_height() -> f32 {
    1.25
}

/// Horizontal text alignment for a [`TextLayerData`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// Non-destructive editable text specification stored on a text [`Layer`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextLayerData {
    pub text: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_text_color")]
    pub color: [u8; 3],
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub box_width: Option<f32>,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

impl Default for TextLayerData {
    fn default() -> Self {
        Self {
            text: "Text".to_owned(),
            font_family: default_font_family(),
            font_size: default_font_size(),
            color: default_text_color(),
            align: TextAlign::Left,
            line_height: default_line_height(),
            bold: false,
            italic: false,
            box_width: None,
            x: 0.0,
            y: 0.0,
        }
    }
}

impl TextLayerData {
    pub fn new(text: impl Into<String>, x: f32, y: f32) -> Self {
        Self {
            text: text.into(),
            x,
            y,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_layer_data_serde_roundtrip_and_defaults() {
        let data = TextLayerData {
            text: "Hello Photon".to_owned(),
            font_family: "Fira Sans".to_owned(),
            font_size: 42.0,
            color: [250, 210, 60],
            align: TextAlign::Center,
            line_height: 1.4,
            bold: true,
            italic: true,
            box_width: Some(320.0),
            x: 24.0,
            y: 48.0,
        };
        let json = serde_json::to_string(&data).expect("serialize TextLayerData");
        let parsed: TextLayerData = serde_json::from_str(&json).expect("deserialize TextLayerData");
        assert_eq!(parsed, data);

        let minimal: TextLayerData =
            serde_json::from_str(r#"{"text":"Minimal"}"#).expect("minimal TextLayerData");
        assert_eq!(minimal.text, "Minimal");
        assert_eq!(minimal.font_size, 32.0);
        assert_eq!(minimal.align, TextAlign::Left);
        assert!((minimal.line_height - 1.25).abs() < 1e-5);
    }
}
