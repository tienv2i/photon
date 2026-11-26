//! Persisted UI theme, language, panel visibility, and layout preferences.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "vi")]
    Vietnamese,
}

impl Language {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en",
            Self::Vietnamese => "vi",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    #[default]
    Dark,
}

impl Theme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

pub const LAYOUT_PANEL_KEYS: [&str; 15] = [
    "browser.sidebar",
    "browser.quick_access",
    "browser.explorer",
    "browser.inspector",
    "browser.preview",
    "browser.info",
    "viewer.sidebar",
    "viewer.inspector",
    "viewer.adjustments",
    "viewer.info",
    "viewer.filmstrip",
    "editor.tools",
    "editor.inspector",
    "editor.history",
    "editor.layers",
];

/// Default visibility for a dock panel key.
pub fn default_panel_visible(key: &str) -> bool {
    !matches!(
        key,
        "browser.inspector"
            | "browser.preview"
            | "browser.info"
            | "viewer.inspector"
            | "viewer.adjustments"
            | "viewer.info"
            | "viewer.filmstrip"
    )
}

/// Stackable panel sections in the Browser left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserLeftPanel {
    QuickAccess,
    Explorer,
}

impl BrowserLeftPanel {
    pub const ALL: [Self; 2] = [Self::QuickAccess, Self::Explorer];
}

fn default_browser_left_panel_order() -> Vec<BrowserLeftPanel> {
    BrowserLeftPanel::ALL.to_vec()
}

/// Stackable panel sections in the Browser right sidebar (`Preview` and `Info`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRightPanel {
    Preview,
    Info,
}

impl BrowserRightPanel {
    pub const ALL: [Self; 2] = [Self::Preview, Self::Info];
}

fn default_browser_right_panel_order() -> Vec<BrowserRightPanel> {
    BrowserRightPanel::ALL.to_vec()
}

/// Stackable panel sections in the Viewer right sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewerRightPanel {
    ViewAdjustment,
    Info,
}

impl ViewerRightPanel {
    pub const ALL: [Self; 2] = [Self::ViewAdjustment, Self::Info];
}

fn default_viewer_right_panel_order() -> Vec<ViewerRightPanel> {
    ViewerRightPanel::ALL.to_vec()
}

/// Stackable panel sections in the Editor right sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditorRightPanel {
    ToolOptionsHistory,
    Layers,
}

impl EditorRightPanel {
    pub const ALL: [Self; 2] = [Self::ToolOptionsHistory, Self::Layers];
}

/// Active tab inside the Editor's combined Tool Options + History panel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EditorToolTab {
    #[default]
    Options,
    History,
}

fn default_editor_right_panel_order() -> Vec<EditorRightPanel> {
    EditorRightPanel::ALL.to_vec()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserViewMode {
    #[default]
    Grid,
    List,
    Details,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSortField {
    #[default]
    Name,
    Modified,
    Size,
    Kind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserThumbSize {
    Xs,
    S,
    #[default]
    M,
    L,
    Xl,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewerViewMode {
    #[default]
    Single,
    Double,
    #[serde(alias = "overview")]
    Webtoon,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebtoonFitMode {
    #[default]
    Auto,
    Width,
    Height,
    Original,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SystemOpenWindowMode {
    #[default]
    FitImage,
    Fullscreen,
    Remembered,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppLaunchWindowMode {
    #[default]
    Maximized,
    Remembered,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanvasAntiAliasingMode {
    Crisp,
    #[default]
    Smooth,
    HighQuality,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewportDenoiseMode {
    #[default]
    Off,
    Light,
    Medium,
    Strong,
}

fn default_editor_right_width() -> u32 {
    300
}

fn default_editor_right_split_pct() -> u16 {
    58
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct LayoutState {
    pub left_width: u32,
    pub right_width: u32,
    #[serde(default = "default_editor_right_width")]
    pub editor_right_width: u32,
    #[serde(default = "default_editor_right_split_pct")]
    pub editor_right_split_pct: u16,
    #[serde(default = "default_browser_left_panel_order")]
    pub browser_left_panel_order: Vec<BrowserLeftPanel>,
    #[serde(default = "default_browser_right_panel_order")]
    pub browser_right_panel_order: Vec<BrowserRightPanel>,
    #[serde(default = "default_viewer_right_panel_order")]
    pub viewer_right_panel_order: Vec<ViewerRightPanel>,
    #[serde(default = "default_editor_right_panel_order")]
    pub editor_right_panel_order: Vec<EditorRightPanel>,
    #[serde(default)]
    pub editor_tool_tab: EditorToolTab,
    pub window_width: u32,
    pub window_height: u32,
    #[serde(default)]
    pub app_launch_window_mode: AppLaunchWindowMode,
    #[serde(default = "crate::default_true")]
    pub window_maximized: bool,
    #[serde(default)]
    pub canvas_anti_aliasing: CanvasAntiAliasingMode,
    #[serde(default)]
    pub viewport_denoise: ViewportDenoiseMode,
    pub browser_view_mode: BrowserViewMode,
    #[serde(default)]
    pub browser_sort_field: BrowserSortField,
    #[serde(default)]
    pub browser_sort_order: SortOrder,
    #[serde(default)]
    pub browser_thumb_size: BrowserThumbSize,
    pub viewer_view_mode: ViewerViewMode,
    pub system_open_window_mode: SystemOpenWindowMode,
    pub manga_rtl: bool,
    pub manga_cover_separate: bool,
    pub manga_page_gap: u16,
    pub webtoon_fit_mode: WebtoonFitMode,
    #[serde(default)]
    pub viewer_viewport_background: Option<[u8; 3]>,
    pub panels: BTreeMap<String, bool>,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            left_width: 240,
            right_width: 290,
            editor_right_width: default_editor_right_width(),
            editor_right_split_pct: default_editor_right_split_pct(),
            browser_left_panel_order: default_browser_left_panel_order(),
            browser_right_panel_order: default_browser_right_panel_order(),
            viewer_right_panel_order: default_viewer_right_panel_order(),
            editor_right_panel_order: default_editor_right_panel_order(),
            editor_tool_tab: EditorToolTab::default(),
            window_width: 1240,
            window_height: 800,
            app_launch_window_mode: AppLaunchWindowMode::default(),
            window_maximized: true,
            canvas_anti_aliasing: CanvasAntiAliasingMode::Smooth,
            viewport_denoise: ViewportDenoiseMode::Off,
            browser_view_mode: BrowserViewMode::Grid,
            browser_sort_field: BrowserSortField::Name,
            browser_sort_order: SortOrder::Asc,
            browser_thumb_size: BrowserThumbSize::M,
            viewer_view_mode: ViewerViewMode::Single,
            system_open_window_mode: SystemOpenWindowMode::FitImage,
            manga_rtl: false,
            manga_cover_separate: true,
            manga_page_gap: 8,
            webtoon_fit_mode: WebtoonFitMode::Auto,
            viewer_viewport_background: None,
            panels: LAYOUT_PANEL_KEYS
                .iter()
                .map(|key| ((*key).to_owned(), default_panel_visible(key)))
                .collect(),
        }
    }
}

impl LayoutState {
    pub fn sanitized(mut self) -> Self {
        self.left_width = self.left_width.clamp(160, 520);
        self.right_width = self.right_width.clamp(200, 600);
        self.editor_right_width = self.editor_right_width.clamp(220, 560);
        self.editor_right_split_pct = self.editor_right_split_pct.clamp(20, 80);
        self.browser_left_panel_order =
            sanitize_panel_order(&self.browser_left_panel_order, &BrowserLeftPanel::ALL);
        self.browser_right_panel_order =
            sanitize_panel_order(&self.browser_right_panel_order, &BrowserRightPanel::ALL);
        self.viewer_right_panel_order =
            sanitize_panel_order(&self.viewer_right_panel_order, &ViewerRightPanel::ALL);
        self.editor_right_panel_order =
            sanitize_panel_order(&self.editor_right_panel_order, &EditorRightPanel::ALL);
        self.window_width = self.window_width.clamp(680, 3840);
        self.window_height = self.window_height.clamp(500, 2160);
        self.manga_page_gap = self.manga_page_gap.min(64);
        let mut panels = BTreeMap::new();
        for key in LAYOUT_PANEL_KEYS {
            panels.insert(
                key.to_owned(),
                self.panels
                    .get(key)
                    .copied()
                    .unwrap_or_else(|| default_panel_visible(key)),
            );
        }
        self.panels = panels;
        self
    }

    pub fn panel(&self, key: &str) -> bool {
        self.panels
            .get(key)
            .copied()
            .unwrap_or_else(|| default_panel_visible(key))
    }

    pub fn with_panel(mut self, key: &str, visible: bool) -> Self {
        self.panels.insert(key.to_owned(), visible);
        self
    }

    /// Returns true if Settings-configured default fields match between `self` and `other`.
    pub fn settings_fields_eq(&self, other: &Self) -> bool {
        self.app_launch_window_mode == other.app_launch_window_mode
            && self.system_open_window_mode == other.system_open_window_mode
            && self.viewer_view_mode == other.viewer_view_mode
            && self.canvas_anti_aliasing == other.canvas_anti_aliasing
            && self.viewport_denoise == other.viewport_denoise
            && self.manga_rtl == other.manga_rtl
            && self.manga_cover_separate == other.manga_cover_separate
            && self.manga_page_gap == other.manga_page_gap
            && self.webtoon_fit_mode == other.webtoon_fit_mode
    }

    /// Copies Settings-configured default fields from `settings` into `self`.
    pub fn apply_settings_from(&mut self, settings: &Self) {
        self.app_launch_window_mode = settings.app_launch_window_mode;
        self.system_open_window_mode = settings.system_open_window_mode;
        self.viewer_view_mode = settings.viewer_view_mode;
        self.canvas_anti_aliasing = settings.canvas_anti_aliasing;
        self.viewport_denoise = settings.viewport_denoise;
        self.manga_rtl = settings.manga_rtl;
        self.manga_cover_separate = settings.manga_cover_separate;
        self.manga_page_gap = settings.manga_page_gap;
        self.webtoon_fit_mode = settings.webtoon_fit_mode;
    }

    /// Resets session UI state to defaults while preserving Settings-configured defaults.
    pub fn reset_ui_state_preserving_settings(&self) -> Self {
        let mut reset = Self::default();
        reset.apply_settings_from(self);
        reset
    }
}

fn sanitize_panel_order<P: Copy + PartialEq>(order: &[P], all: &[P]) -> Vec<P> {
    let mut out = Vec::with_capacity(all.len());
    for &panel in order {
        if !out.contains(&panel) {
            out.push(panel);
        }
    }
    for &panel in all {
        if !out.contains(&panel) {
            out.push(panel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_panels_hide_filmstrip_and_inspectors() {
        let layout = LayoutState::default();
        assert!(layout.panel("browser.quick_access"));
        assert!(layout.panel("browser.explorer"));
        assert!(!layout.panel("viewer.adjustments"));
        assert!(!layout.panel("viewer.info"));
        assert!(!layout.panel("viewer.filmstrip"));
        assert!(!layout.panel("browser.inspector"));
        assert!(!layout.panel("viewer.inspector"));
        assert!(layout.panel("browser.sidebar"));
        assert!(layout.panel("editor.tools"));
        assert!(layout.panel("editor.inspector"));
        assert!(layout.panel("editor.history"));
        assert!(layout.panel("editor.layers"));
        assert_eq!(
            layout.browser_left_panel_order,
            vec![BrowserLeftPanel::QuickAccess, BrowserLeftPanel::Explorer]
        );
        assert_eq!(
            layout.viewer_right_panel_order,
            vec![ViewerRightPanel::ViewAdjustment, ViewerRightPanel::Info]
        );
        assert_eq!(
            layout.editor_right_panel_order,
            vec![
                EditorRightPanel::ToolOptionsHistory,
                EditorRightPanel::Layers
            ]
        );
        assert_eq!(layout.editor_tool_tab, EditorToolTab::Options);
    }

    #[test]
    fn legacy_layout_json_deserializes_with_editor_sidebar_defaults() {
        let raw = r#"{"leftWidth":260,"panels":{"browser.sidebar":true}}"#;
        let parsed: LayoutState = serde_json::from_str(raw).unwrap();
        let clean = parsed.sanitized();
        assert_eq!(clean.left_width, 260);
        assert!(clean.panel("browser.quick_access"));
        assert!(clean.panel("browser.explorer"));
        assert!(!clean.panel("viewer.adjustments"));
        assert!(!clean.panel("viewer.info"));
        assert!(!clean.panel("browser.inspector"));
        assert!(!clean.panel("viewer.filmstrip"));
        assert_eq!(
            clean.browser_left_panel_order,
            vec![BrowserLeftPanel::QuickAccess, BrowserLeftPanel::Explorer]
        );
        assert_eq!(
            clean.viewer_right_panel_order,
            vec![ViewerRightPanel::ViewAdjustment, ViewerRightPanel::Info]
        );
        assert_eq!(
            clean.editor_right_panel_order,
            vec![
                EditorRightPanel::ToolOptionsHistory,
                EditorRightPanel::Layers
            ]
        );
        assert_eq!(clean.editor_tool_tab, EditorToolTab::Options);
    }

    #[test]
    fn sanitized_deduplicates_and_completes_panel_orders() {
        let layout = LayoutState {
            browser_left_panel_order: vec![BrowserLeftPanel::Explorer, BrowserLeftPanel::Explorer],
            viewer_right_panel_order: vec![ViewerRightPanel::Info],
            editor_right_panel_order: vec![EditorRightPanel::Layers, EditorRightPanel::Layers],
            editor_tool_tab: EditorToolTab::History,
            ..Default::default()
        }
        .sanitized();
        assert_eq!(
            layout.browser_left_panel_order,
            vec![BrowserLeftPanel::Explorer, BrowserLeftPanel::QuickAccess]
        );
        assert_eq!(
            layout.viewer_right_panel_order,
            vec![ViewerRightPanel::Info, ViewerRightPanel::ViewAdjustment]
        );
        assert_eq!(
            layout.editor_right_panel_order,
            vec![
                EditorRightPanel::Layers,
                EditorRightPanel::ToolOptionsHistory
            ]
        );
        assert_eq!(layout.editor_tool_tab, EditorToolTab::History);
    }

    #[test]
    fn panel_visibility_defaults_and_overrides_roundtrip() {
        assert!(default_panel_visible("browser.quick_access"));
        assert!(default_panel_visible("browser.explorer"));
        assert!(!default_panel_visible("viewer.adjustments"));
        assert!(!default_panel_visible("viewer.info"));
        assert!(!default_panel_visible("browser.inspector"));
        assert!(!default_panel_visible("viewer.filmstrip"));

        let layout = LayoutState::default()
            .with_panel("browser.quick_access", false)
            .with_panel("viewer.adjustments", true)
            .with_panel("viewer.info", true)
            .sanitized();
        assert!(!layout.panel("browser.quick_access"));
        assert!(layout.panel("browser.explorer"));
        assert!(layout.panel("viewer.adjustments"));
        assert!(layout.panel("viewer.info"));
    }

    #[test]
    fn startup_and_viewport_defaults_are_backward_compatible() {
        let layout = LayoutState::default();
        assert_eq!(
            layout.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );
        assert!(layout.window_maximized);
        assert_eq!(layout.canvas_anti_aliasing, CanvasAntiAliasingMode::Smooth);
        assert_eq!(layout.viewport_denoise, ViewportDenoiseMode::Off);

        let parsed: LayoutState = serde_json::from_str(r#"{"windowWidth":1000}"#).unwrap();
        assert_eq!(
            parsed.app_launch_window_mode,
            AppLaunchWindowMode::Maximized
        );
        assert!(parsed.window_maximized);
        assert_eq!(parsed.canvas_anti_aliasing, CanvasAntiAliasingMode::Smooth);
        assert_eq!(parsed.viewport_denoise, ViewportDenoiseMode::Off);
    }
}
