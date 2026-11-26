//! `iced 0.14` daemon + multi-window GUI adapter (`photon`) using `photon-ui`.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{
    checkbox, column, container, image as iced_image, operation as widget_operation, pick_list,
    row, scrollable, text_input,
};
use iced::{
    Alignment, Color, Element, Fill, Length, Point, Rectangle, Size, Subscription, Task,
    Theme as IcedTheme, event, mouse, window,
};
use image::RgbaImage;
use photon_core::{CoreHandle, EditorTransformFrames};
use photon_i18n::{tr, tr_ui_text};
use photon_panels::{PanelTabItem, SidebarOrderState, panel_tab_bar};
use photon_protocol::{
    AiDevice, AiMaskStroke, AppLaunchWindowMode, ApplyTarget, BlendMode, BrowserLeftPanel,
    BrowserRightPanel, BrowserSortField, BrowserThumbSize, BrowserViewMode, CanvasAntiAliasingMode,
    ClientKind, ColorAdjustments, Command, CommandResponse, EditorRightPanel, EditorToolTab,
    Envelope, EventKind, FlipAxis, GradientColorMode, GradientRepeat, GradientStop, HealingMode,
    Language, LayerEffect, LayerTransformSpec, LiquifyMode, LiveStrokeKind, MoveDirection,
    PluginCapability, PluginInfo, PluginParamKind, PluginParamSpec, PluginParamValue, Query,
    ResizeFilter, ResizeMode, SelectionBooleanOp, SelectionModifyOp, SelectionShape, SortOrder,
    SystemOpenWindowMode, Theme, TransformEachSpec, TranslateProvider, TranslateSettings,
    UiStateSnapshot, UiText, VectorBlendSpec, VectorLayerData, VectorPathOperation,
    ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode, WebtoonFitMode, Workspace,
};
use photon_ui::button::{self, ButtonVariant};
use photon_ui::curve_editor::{CurveGraphEvent, curve_graph};
use photon_ui::dialog::{
    ConfirmDialogSpec, DialogEvent, DialogManager, DialogSpec, DialogWindowState,
    HeaderedManagedDialogSpec, ManagedDialogSpec, OverlayMode, PromptDialogSpec, ToastKind,
    ToastSpec, alert_dialog_spec, confirm_dialog_spec, dialog_style_for_theme,
    managed_dialog_stack_with_headers, prompt_dialog_spec, with_toast_banner,
};
use photon_ui::grid::{
    FILMSTRIP_VISIBLE_OVERSCAN, filmstrip_offset_for_index, filmstrip_visible_range,
};
use photon_ui::icons::Icon;
use photon_ui::menu::{
    CascadingSubmenu, MenuEntry, MenuIconButton, MenuItem, menu_panel_entries, with_anchored_menu,
    with_context_menu, with_context_menu_and_cascading_submenus, with_top_right_menu,
    with_top_right_menu_at,
};
use photon_ui::panel::{section_card, status_bar};
use photon_ui::theme::{
    PaletteTokens, TEXT_SIZE_CONTROL, app_shell_style, input_style, pick_list_menu_style,
    pick_list_style,
};
use photon_ui::tooltip::{TooltipPosition, with_tooltip_pos};
use photon_ui::typography::text;

use crate::inspector::{ViewerUiPresentation, build_ui_tree};
use photon_browser::batch_dialog::{
    BatchDialogAction, BatchDialogKind, BatchDialogMsg, BatchDialogState,
    build_batch_convert_dialog, build_batch_crop_dialog, build_batch_resize_dialog,
    build_batch_transform_dialog,
};
use photon_browser::{
    BrowserMenuKind, BrowserMessage, BrowserViewState, browser_filter_input_id,
    build_browser_context_menu_entries_with_offset, build_browser_help_menu_entries,
    build_browser_plugin_group_submenu, build_browser_plugins_menu_entries,
    build_browser_plugins_submenu_with_offset, build_browser_sort_menu, build_browser_toolbar,
    build_browser_view, build_browser_window_menu_entries, build_quick_access_context_menu,
};
use photon_canvas::{
    CanvasEvent, CpuImageFrame, CpuMaskFrame, TransformPreview, default_viewport_bg_rgb,
    dual_spread_geometry,
};
use photon_editor::panel_splitter::{DockDragState, DockSplitterKind};
use photon_editor::shortcuts::{
    EditorShortcutAction, ShortcutKey, ShortcutNamedKey, resolve_editor_shortcut,
};
use photon_editor::{
    AdjustmentPopupTab, AlphaRefineMode, ColorBalanceZone, ColorPickerTarget, CompareMode,
    CurvesChannel, DimGridAction, DimensionInputState, EditorMenuKind, EditorMessage, EditorTool,
    EditorToolGroup, EditorUiState, EditorViewState, SmartPasteFitMode, SmartPasteState,
    TabCloseScope, WatermarkTab, apply_target_control, build_color_picker_dialog,
    build_editor_add_layer_menu, build_editor_canvas_context_menu, build_editor_layer_context_menu,
    build_editor_plugins_menu_entries, build_editor_tab_context_menu, build_editor_toolbar,
    build_editor_vector_menu_entries, build_editor_view, build_export_dialog,
    build_export_vector_svg_dialog, build_tool_group_flyout_menu, build_transform_each_dialog,
    build_vector_blend_dialog, build_vector_shape_from_draw_commit, crop_uv_to_signed,
    default_curves, format_hex, parse_hex_rgb, rgb_to_color, slider_stepper_control,
};
use photon_protocol::DisplayFont;
use photon_ui::editor::{anchor_grid_3x3, checker_swatch};
use photon_viewer::{
    ViewerMessage, ViewerTranslateMode, ViewerTranslateState, ViewerViewState,
    build_viewer_context_menu_with_offset, build_viewer_plugin_group_submenu,
    build_viewer_plugins_submenu_with_offset, build_viewer_quality_menu_entries,
    build_viewer_toolbar, build_viewer_view, build_viewer_zoom_submenu, dual_spread_slots,
    dual_spread_slots_for, filmstrip_scroll_id, step_dual_spread, viewer_counter_summary,
    viewer_double_fit_geometries, viewer_reader_options_right_offset, viewer_status_summary,
    viewer_utilities_menu_right_offset, webtoon_index_at_scroll, webtoon_offset_for_index,
    webtoon_scroll_id,
};

mod dialogs;
mod keyboard;
mod media_loader;
mod menus;
mod message;
mod platform;
mod semantic_ui;
mod settings;
mod state;
mod update_canvas;
mod update_editor;
mod update_snapshot;
mod update_viewer_translate;

pub use message::Message;
#[allow(unused_imports)]
pub(crate) use state::ActiveContextMenu;
#[allow(unused_imports)]
pub use state::{
    BrowserClipboardPaste, DesktopDialogId, NewDocBackground, PhotonGui, SelectionModifyKind,
    ToastState,
};

#[allow(unused_imports)]
use dialogs::*;
#[allow(unused_imports)]
use keyboard::*;
#[allow(unused_imports)]
use media_loader::*;
#[allow(unused_imports)]
use menus::*;
#[allow(unused_imports)]
use message::*;
#[allow(unused_imports)]
use platform::*;
#[allow(unused_imports)]
use semantic_ui::*;
#[allow(unused_imports)]
use settings::*;
#[allow(unused_imports)]
use state::*;
#[allow(unused_imports)]
use update_canvas::*;
#[allow(unused_imports)]
use update_editor::*;
#[allow(unused_imports)]
use update_snapshot::*;
#[allow(unused_imports)]
use update_viewer_translate::*;

static CORE_HANDLE: OnceLock<CoreHandle> = OnceLock::new();
static GPU_BACKEND_NAME: OnceLock<String> = OnceLock::new();
static OPENED_WITH_FILES: OnceLock<bool> = OnceLock::new();
static STANDALONE_VIEWER: AtomicBool = AtomicBool::new(false);
static STANDALONE_EDITOR: AtomicBool = AtomicBool::new(false);

pub fn init_globals(
    handle: CoreHandle,
    gpu_backend: String,
    opened_with_files: bool,
    standalone_viewer: bool,
    standalone_editor: bool,
) {
    let _ = CORE_HANDLE.set(handle);
    let _ = GPU_BACKEND_NAME.set(gpu_backend);
    let _ = OPENED_WITH_FILES.set(opened_with_files);
    STANDALONE_VIEWER.store(standalone_viewer, Ordering::Relaxed);
    STANDALONE_EDITOR.store(standalone_editor, Ordering::Relaxed);
}

fn is_standalone_viewer() -> bool {
    STANDALONE_VIEWER.load(Ordering::Relaxed)
}

fn is_standalone_editor() -> bool {
    STANDALONE_EDITOR.load(Ordering::Relaxed)
}

pub(crate) fn window_app_id_for(standalone_viewer: bool, standalone_editor: bool) -> &'static str {
    if standalone_viewer {
        "io.github.tienv2i.Photon.Viewer"
    } else if standalone_editor {
        "io.github.tienv2i.Photon.Editor"
    } else {
        "io.github.tienv2i.Photon"
    }
}

fn window_app_id() -> &'static str {
    window_app_id_for(is_standalone_viewer(), is_standalone_editor())
}

pub(crate) fn window_start_policy(
    is_standalone_viewer: bool,
    opened_with_files: bool,
    app_mode: AppLaunchWindowMode,
    sys_mode: SystemOpenWindowMode,
    window_maximized: bool,
) -> (bool, bool) {
    if is_standalone_viewer {
        (
            opened_with_files && sys_mode == SystemOpenWindowMode::Fullscreen,
            sys_mode == SystemOpenWindowMode::Maximized
                || (sys_mode == SystemOpenWindowMode::Remembered && window_maximized),
        )
    } else {
        (
            false,
            app_mode == AppLaunchWindowMode::Maximized
                || (app_mode == AppLaunchWindowMode::Remembered && window_maximized),
        )
    }
}

pub fn boot() -> (PhotonGui, Task<Message>) {
    let core = CORE_HANDLE
        .get()
        .expect("CoreHandle must be initialized before iced::daemon")
        .clone();
    let snapshot = core.latest_snapshot();
    let folder_input = snapshot.meta.current_folder.clone().unwrap_or_default();
    let opened_with_files = OPENED_WITH_FILES.get().copied().unwrap_or(false);
    let remembered_size = Size::new(
        (snapshot.meta.layout.window_width as f32).clamp(680.0, 3840.0),
        (snapshot.meta.layout.window_height as f32).clamp(500.0, 2160.0),
    );
    let viewer_remembered_size = Size::new(
        (snapshot.meta.layout.viewer_window_width as f32).clamp(480.0, 3840.0),
        (snapshot.meta.layout.viewer_window_height as f32).clamp(360.0, 2160.0),
    );
    let viewer_fixed_size = Size::new(
        (snapshot.meta.layout.viewer_fixed_width as f32).clamp(480.0, 3840.0),
        (snapshot.meta.layout.viewer_fixed_height as f32).clamp(360.0, 2160.0),
    );
    let launch_mode = snapshot.meta.layout.system_open_window_mode;
    let (start_fullscreen, start_maximized) = window_start_policy(
        is_standalone_viewer(),
        opened_with_files,
        snapshot.meta.layout.app_launch_window_mode,
        launch_mode,
        if is_standalone_viewer() {
            snapshot.meta.layout.viewer_window_maximized
        } else {
            snapshot.meta.layout.window_maximized
        },
    );
    let mut suppress_resize_until = None;
    let init_size = if is_standalone_viewer() {
        match launch_mode {
            SystemOpenWindowMode::FitImage if opened_with_files => {
                let target_img = snapshot
                    .meta
                    .viewer_path
                    .as_deref()
                    .or_else(|| snapshot.meta.editor.as_ref().map(|e| e.path.as_str()));
                if let Some((iw, ih)) =
                    target_img.and_then(|p| photon_image::probe_dimensions(Path::new(p)))
                {
                    suppress_resize_until = Some(Instant::now() + Duration::from_millis(1200));
                    window_size_for_image(&snapshot, iw, ih)
                } else {
                    viewer_remembered_size
                }
            }
            SystemOpenWindowMode::FitImage => {
                suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
                viewer_remembered_size
            }
            SystemOpenWindowMode::FixedSize => {
                suppress_resize_until = Some(Instant::now() + Duration::from_millis(1200));
                viewer_fixed_size
            }
            SystemOpenWindowMode::Maximized | SystemOpenWindowMode::Fullscreen => {
                suppress_resize_until = Some(Instant::now() + Duration::from_millis(1200));
                viewer_remembered_size
            }
            SystemOpenWindowMode::Remembered => {
                suppress_resize_until = Some(
                    Instant::now()
                        + Duration::from_millis(if start_maximized { 1200 } else { 600 }),
                );
                viewer_remembered_size
            }
        }
    } else {
        // Ignore initial WM map/configure/maximize resize jitter so remembered size isn't overwritten.
        suppress_resize_until = Some(
            Instant::now()
                + Duration::from_millis(if start_maximized || start_fullscreen {
                    1200
                } else {
                    600
                }),
        );
        remembered_size
    };

    #[cfg(target_os = "linux")]
    let platform_specific = window::settings::PlatformSpecific {
        application_id: window_app_id().to_owned(),
        ..Default::default()
    };
    #[cfg(not(target_os = "linux"))]
    let platform_specific = window::settings::PlatformSpecific::default();

    let init_ui_scale = snapshot.meta.layout.ui_scale().max(0.5);
    let open_logical_size = Size::new(
        (init_size.width / init_ui_scale).max(400.0),
        (init_size.height / init_ui_scale).max(320.0),
    );
    let open_min_size = Size::new(
        (680.0 / init_ui_scale).max(360.0),
        (500.0 / init_ui_scale).max(280.0),
    );

    let (id, open_task) = window::open(window::Settings {
        size: open_logical_size,
        min_size: Some(open_min_size),
        platform_specific,
        ..Default::default()
    });

    let mut windows = BTreeMap::new();
    windows.insert(
        id,
        if is_standalone_viewer() {
            WindowRole::AuxiliaryViewer
        } else {
            WindowRole::Main
        },
    );

    let mut editor_ui = EditorUiState::default();
    if let Some(ed) = &snapshot.meta.editor {
        editor_ui.sync_document_dims(ed.width, ed.height);
    }
    let init_layout = &snapshot.meta.layout;
    let viewer_mode = init_layout.viewer_view_mode;
    let viewer_manga_rtl = init_layout.manga_rtl;
    let viewer_manga_cover_separate = init_layout.manga_cover_separate;
    let viewer_manga_page_gap = init_layout.manga_page_gap;
    let viewer_webtoon_fit_mode = init_layout.webtoon_fit_mode;
    let browser_preview_open =
        init_layout.panel("browser.preview") || init_layout.panel("browser.inspector");
    let browser_info_open =
        init_layout.panel("browser.info") || init_layout.panel("browser.inspector");
    let browser_inspector_open = browser_preview_open || browser_info_open;
    let browser_quick_access_open = init_layout.panel("browser.quick_access");
    let browser_explorer_open = init_layout.panel("browser.explorer");
    let browser_left_panels = SidebarOrderState::new(
        init_layout.browser_left_panel_order.clone(),
        &BrowserLeftPanel::ALL,
    );
    let browser_right_panels = SidebarOrderState::new(
        init_layout.browser_right_panel_order.clone(),
        &BrowserRightPanel::ALL,
    );
    let viewer_info_open =
        init_layout.panel("viewer.info") || init_layout.panel("viewer.inspector");
    let viewer_adjustments_open = init_layout.panel("viewer.adjustments");
    let viewer_right_panels = SidebarOrderState::new(
        init_layout.viewer_right_panel_order.clone(),
        &ViewerRightPanel::ALL,
    );
    let viewer_filmstrip_open = init_layout.panel("viewer.filmstrip");
    let editor_tool_tab = init_layout.editor_tool_tab;
    let editor_history_open = init_layout.panel("editor.history");
    let editor_layers_open = init_layout.panel("editor.layers");
    let editor_panels = SidebarOrderState::new(
        init_layout.editor_right_panel_order.clone(),
        &EditorRightPanel::ALL,
    );
    let editor_panel_order = editor_panels.order().to_vec();
    let last_core_layout = init_layout.clone();
    let viewer_viewport_background = init_layout.viewer_viewport_background;
    let initial_plugin_id = snapshot.meta.plugins.first().map(|p| p.id.clone());
    let initial_translate_draft = snapshot.meta.translate_settings.clone();

    let mut app = PhotonGui {
        core,
        snapshot,
        windows,
        main_window: Some(id),
        last_active_window: Some(id),
        active_dialog_window: None,
        window_size: open_logical_size,
        os_scale_factor: 1.0,
        resize_save_seq: 0,
        suppress_resize_until,
        folder_input,
        browser_filter: String::new(),
        browser_filter_bar_open: false,
        browser_filter_case_sensitive: false,
        browser_filter_regex: false,
        last_click: None,
        thumbnails: HashMap::new(),
        thumbnails_loading: BTreeSet::new(),
        browser_visible_range: Some((0, BROWSER_INITIAL_VISIBLE_ENTRIES)),
        editor_layer_thumbs: HashMap::new(),
        editor_mask_thumbs: HashMap::new(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        pending_adj: ColorAdjustments::default(),
        viewer_adj: ColorAdjustments::default(),
        transfer_adj_to_editor: None,
        viewer_frame: None,
        viewer_pair_frame: None,
        editor_frame: None,
        editor_original_frame: None,
        editor_selection_mask: None,
        editor_histogram: None,
        transform_preview: None,
        transform_layer_id: None,
        marching_ants_phase: 0.0,
        editor_ui,
        browser_file_clipboard: Vec::new(),
        last_synced_clipboard_png: None,
        pending_close_tabs: Vec::new(),
        new_doc_clipboard_dims: None,
        new_doc_clipboard_png: None,
        new_doc_create_from_clipboard: false,
        viewer_mode,
        viewer_manga_rtl,
        viewer_manga_cover_separate,
        viewer_manga_page_gap,
        viewer_webtoon_fit_mode,
        browser_inspector_open,
        browser_preview_open,
        browser_info_open,
        browser_quick_access_open,
        browser_explorer_open,
        browser_quick_access_collapsed: false,
        browser_explorer_collapsed: false,
        browser_preview_collapsed: false,
        browser_info_collapsed: false,
        browser_left_panels,
        browser_right_panels,
        viewer_info_open,
        viewer_adjustments_open,
        viewer_right_panels,
        viewer_filmstrip_open,
        viewer_filmstrip_offset: 0.0,
        viewer_color_popover_open: viewer_adjustments_open,
        dock_drag: DockDragState::default(),
        browser_left_width_drag: None,
        browser_right_width_drag: None,
        viewer_right_width_preview: None,
        viewer_right_split_preview: None,
        editor_tool_tab,
        editor_history_open,
        editor_layers_open,
        editor_history_collapsed: false,
        editor_layers_collapsed: false,
        editor_panels,
        editor_panel_order,
        dragging_editor_panel: None,
        editor_panel_drop_target: None,
        pending_leave_to_browser: false,
        pending_layout_commands: 0,
        last_core_layout,
        pending_screenshots: BTreeMap::new(),
        settings_open: false,
        settings_tab: SettingsTab::General,
        plugin_settings_tab: PluginSettingsTab::General,
        settings_collapsed: BTreeSet::new(),
        settings_confirm_factory_reset: false,
        plugins_search: String::new(),
        plugins_selected_id: initial_plugin_id,
        translate_draft: initial_translate_draft.clone(),
        dialog: None,
        dialog_manager: DialogManager::new(),
        toast: None,
        toast_seq: 0,
        path_editing: false,
        active_menu: None,
        editor_add_layer_anchor: None,
        context_menu: None,
        last_cursor: None,
        viewer_quarter_turns: 0,
        webtoon_zoom: 1.0,
        webtoon_scroll_y: 0.0,
        webtoon_visible_index: None,
        image_sizes: HashMap::new(),
        webtoon_pages: HashMap::new(),
        webtoon_loading: BTreeSet::new(),
        webtoon_order: VecDeque::new(),
        is_fullscreen: start_fullscreen,
        crop_uv: None,
        viewport_background: None,
        viewer_viewport_background,
        ctrl_held: false,
        shift_held: false,
        browser_selection_anchor: None,
        editor_tool_hold: None,
        editor_tool_hold_seq: 0,
        expanded_folders: BTreeSet::new(),
        folder_children: HashMap::new(),
        viewer_translate: ViewerTranslateState {
            source_lang: initial_translate_draft.default_source_lang,
            target_lang: initial_translate_draft.default_target_lang,
            ..Default::default()
        },
        translate_drag: TranslateDragTarget::None,
        pending_translate_queue: VecDeque::new(),
        active_translate_target: None,
        webtoon_hover: None,
        webtoon_marquee_drag: None,
        editor_ai_stroke_pts: Vec::new(),
        last_ai_clipboard: None,
        last_ai_error: None,
        editor_tab_view_states: HashMap::new(),
        quick_access_editing: false,
        canvas_size_draft: None,
        reopen_canvas_size_after_picker: false,
        new_doc_draft: None,
        reopen_new_doc_after_picker: false,
    };

    let size_tasks = probe_viewer_image_sizes(&mut app);
    let thumb_tasks = schedule_missing_thumbnails(&mut app);
    let webtoon_tasks = if app.viewer_mode == ViewerViewMode::Webtoon {
        let center = app.snapshot.meta.viewer_index.unwrap_or(0);
        schedule_webtoon_pages_near(&mut app, center)
    } else {
        Task::none()
    };
    let tree_load_tasks = sync_expanded_tree_tasks(&mut app, true);
    let load_task = reload_active_frames(
        &app.core,
        &app.snapshot,
        &app.image_sizes,
        &app.viewer_ui_presentation(),
    );
    let tree_task = sync_ui_tree_task(&app);
    let fs_task = if start_fullscreen {
        window::set_mode(id, window::Mode::Fullscreen)
    } else {
        Task::none()
    };
    let max_task = if start_maximized && !start_fullscreen {
        window::maximize(id, true)
    } else {
        Task::none()
    };
    (
        app,
        Task::batch([
            open_task.map(Message::WindowOpened),
            fs_task,
            max_task,
            load_task,
            tree_task,
            tree_load_tasks,
            size_tasks,
            thumb_tasks,
            webtoon_tasks,
        ]),
    )
}

pub fn title(state: &PhotonGui, window_id: window::Id) -> String {
    let lang = state.snapshot.meta.language;
    let base = tr(lang, "app-title");
    let meta = &state.snapshot.meta;
    let role = state
        .windows
        .get(&window_id)
        .copied()
        .unwrap_or(WindowRole::Main);
    let ws = match role {
        WindowRole::AuxiliaryViewer => Workspace::Viewer,
        WindowRole::Main => meta.workspace,
    };
    match ws {
        Workspace::Browser => {
            if let Some(folder) = meta.current_folder.as_deref() {
                let name = Path::new(folder)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(folder);
                format!("{name} — {base}")
            } else {
                format!("{base} — {}", tr(lang, "workspace-browser"))
            }
        }
        Workspace::Viewer => match state.viewer_mode {
            ViewerViewMode::Webtoon => {
                let count = meta.viewer_count;
                let cur = state
                    .webtoon_visible_index
                    .or(meta.viewer_index)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let title_left = meta
                    .current_folder
                    .as_deref()
                    .and_then(|f| Path::new(f).file_name())
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| tr(lang, "action-viewer-manhwa"));
                format!("{title_left} ({cur}/{count}) — {base}")
            }
            ViewerViewMode::Double => {
                let imgs = &state.snapshot.viewer_images;
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                let name_for = |idx: usize| {
                    imgs.get(idx)
                        .and_then(|p| Path::new(p).file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("-")
                };
                match (left_slot, right_slot) {
                    (Some((li, _)), Some((ri, _))) => {
                        let a = name_for(li);
                        let b = name_for(ri);
                        format!("{a} & {b} ({}·{}/{}) — {base}", li + 1, ri + 1, imgs.len())
                    }
                    (Some((li, _)), None) => {
                        format!("{} ({}/{}) — {base}", name_for(li), li + 1, imgs.len())
                    }
                    _ => format!("{base} — {}", tr(lang, "workspace-viewer")),
                }
            }
            ViewerViewMode::Single => {
                if let Some(name) = meta
                    .viewer_path
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .and_then(|n| n.to_str())
                {
                    let idx_part = if meta.viewer_count > 0 {
                        format!(
                            " ({}/{})",
                            meta.viewer_index.unwrap_or(0) + 1,
                            meta.viewer_count
                        )
                    } else {
                        String::new()
                    };
                    if let Some(frame) = &state.viewer_frame {
                        format!(
                            "{name} · {}×{}{idx_part} — {base}",
                            frame.width, frame.height
                        )
                    } else {
                        format!("{name}{idx_part} — {base}")
                    }
                } else {
                    format!("{base} — {}", tr(lang, "workspace-viewer"))
                }
            }
        },
        Workspace::Editor => {
            if let Some(doc) = &meta.editor {
                let dirty = if doc.modified { " *" } else { "" };
                format!(
                    "{}{} · {}×{} — {base}",
                    doc.name, dirty, doc.width, doc.height
                )
            } else {
                format!("{base} — {}", tr(lang, "workspace-editor"))
            }
        }
    }
}

pub fn theme(state: &PhotonGui, _window: window::Id) -> IcedTheme {
    match state.snapshot.meta.theme {
        Theme::Dark => IcedTheme::Dark,
        Theme::Light => IcedTheme::Light,
    }
}

pub fn iced_font_for(font: DisplayFont) -> iced::Font {
    match font {
        DisplayFont::System | DisplayFont::BundledSans => iced::Font::DEFAULT,
        DisplayFont::Inter => iced::Font::with_name("Inter"),
        DisplayFont::Cantarell => iced::Font::with_name("Cantarell"),
        DisplayFont::NotoSans => iced::Font::with_name("Noto Sans"),
        DisplayFont::DejaVuSans => iced::Font::with_name("DejaVu Sans"),
        DisplayFont::Serif => iced::Font {
            family: iced::font::Family::Serif,
            ..iced::Font::DEFAULT
        },
        DisplayFont::Monospace => iced::Font::MONOSPACE,
    }
}

/// Application UI scale multiplier passed to `iced::daemon::scale_factor`.
///
/// `iced_winit` multiplies the OS / Wayland `wp_fractional_scale_v1` window scale factor
/// by this value (`window.scale_factor() * app_scale_factor`). We return `ui_scale`
/// directly so Wayland fractional scaling (`1.25x`, `1.5x`) is respected and never inverted.
pub fn scale_factor(state: &PhotonGui, _window: window::Id) -> f32 {
    state.snapshot.meta.layout.ui_scale()
}

pub fn subscription(state: &PhotonGui) -> Subscription<Message> {
    let mut subs = vec![
        Subscription::run(state_watch_stream),
        Subscription::run(event_broadcast_stream),
        iced::keyboard::listen().map(Message::KeyEvent),
        event::listen_with(|ev, _status, win_id| match ev {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::WindowCursorMoved(position))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::WindowMouseUp)
            }
            iced::Event::Window(window::Event::Focused) => Some(Message::WindowFocused(win_id)),
            iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowUnfocused),
            iced::Event::Window(window::Event::Rescaled(scale)) => {
                Some(Message::WindowRescaled(win_id, scale))
            }
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => {
                Some(Message::ModifiersChanged(m))
            }
            _ => None,
        }),
        window::close_events().map(Message::WindowClosed),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ];
    if state.snapshot.meta.workspace == Workspace::Editor
        && state
            .snapshot
            .meta
            .editor
            .as_ref()
            .is_some_and(|ed| ed.selection.has_selection)
    {
        subs.push(iced::time::every(Duration::from_millis(60)).map(|_| Message::MarchingAntsTick));
    }
    Subscription::batch(subs)
}

fn state_watch_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(16, async |mut output| {
        use iced::futures::SinkExt;
        let Some(core) = CORE_HANDLE.get() else {
            return;
        };
        let mut rx = core.subscribe_state();
        while rx.changed().await.is_ok() {
            let snap = rx.borrow().clone();
            let _ = output.send(Message::StateUpdated(snap)).await;
        }
    })
}

fn event_broadcast_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, async |mut output| {
        use iced::futures::SinkExt;
        let Some(core) = CORE_HANDLE.get() else {
            return;
        };
        let mut rx = core.subscribe_events();
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let _ = output.send(Message::CoreEvent(ev)).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

pub fn update(state: &mut PhotonGui, message: Message) -> Task<Message> {
    match message {
        Message::Noop => Task::none(),
        Message::ToggleMenu(menu) => {
            state.context_menu = None;
            state.active_menu = if state.active_menu == Some(menu) {
                state.editor_add_layer_anchor = None;
                None
            } else {
                state.editor_add_layer_anchor = (menu == ActiveMenu::EditorAddLayer)
                    .then(|| state.last_cursor)
                    .flatten();
                Some(menu)
            };
            Task::none()
        }
        Message::HoverMenu(menu) => {
            if state.active_menu.is_some() && state.active_menu != Some(menu) {
                state.editor_add_layer_anchor = (menu == ActiveMenu::EditorAddLayer)
                    .then(|| state.last_cursor)
                    .flatten();
                state.active_menu = Some(menu);
            }
            Task::none()
        }
        Message::CloseMenu => {
            state.active_menu = None;
            state.editor_add_layer_anchor = None;
            Task::none()
        }
        Message::OpenShortcutsDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let lang = state.snapshot.meta.language;
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::Shortcuts,
                title: tr(lang, "action-shortcuts"),
                message: "Ctrl+1/2/3: Switch Workspace (Browser/Viewer)\nCtrl+O / Ctrl+Shift+O: Open Folder / Files · Ctrl+N: New Document · Ctrl+W: Close Tab\nCtrl+Z / Ctrl+Shift+Z: Undo / Redo · Ctrl+S / Ctrl+Shift+S: Save / Save As · Ctrl+Shift+E: Export\nCtrl+A / Ctrl+D / Ctrl+Shift+I: Select All / Deselect / Invert · Ctrl+J: Duplicate Layer · Ctrl+E: Merge Down · Ctrl+Shift+N: New Layer · Ctrl+T: Free Transform\nCtrl++ / Ctrl+- / Ctrl+0 / Ctrl+1: Zoom In / Out / Fit / 100% · F11: Fullscreen\nEditor Tools: V (Move), M (Marquee), L (Lasso), W (Magic Wand), C (Crop), I (Eyedropper), B (Brush), E (Eraser), G (Gradient/Bucket), P (Pen), A (Path), U (Shape), T (Text), J (Healing), H (Hand), Z (Zoom), X (Swap Colors), Q (Quick Mask), [ / ] (Brush Size)".to_owned(),
            });
            open_dialog_window(state, DesktopDialogId::Shortcuts);
            Task::none()
        }
        Message::ToggleBrowserShowHidden => commit_layout_with(state, |layout| {
            layout.browser_show_hidden = !layout.browser_show_hidden;
        }),
        Message::ToolGroupPressed(group) => {
            state.editor_tool_hold_seq = state.editor_tool_hold_seq.wrapping_add(1);
            let seq = state.editor_tool_hold_seq;
            state.editor_tool_hold = Some((group, seq));
            if group.tools().len() <= 1 {
                return Task::none();
            }
            Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(280)).await;
                    (group, seq)
                },
                |(g, s)| Message::ToolGroupHoldTimeout(g, s),
            )
        }
        Message::ToolGroupHoldTimeout(group, seq) => {
            if state.editor_tool_hold == Some((group, seq)) {
                state.editor_tool_hold = None;
                state.active_menu = Some(ActiveMenu::EditorToolFlyout(group));
            }
            Task::none()
        }
        Message::ToolGroupReleased(group) => {
            let was_holding = state
                .editor_tool_hold
                .take()
                .is_some_and(|(g, _)| g == group);
            if was_holding
                && !matches!(state.active_menu, Some(ActiveMenu::EditorToolFlyout(g)) if g == group)
            {
                let tool = group.active_tool(&state.editor_ui);
                return update(state, Message::EditorSelectTool(tool));
            }
            Task::none()
        }
        Message::OpenToolGroupFlyout(group) => {
            state.editor_tool_hold = None;
            if group.tools().len() > 1 {
                state.active_menu = Some(ActiveMenu::EditorToolFlyout(group));
            } else {
                let tool = group.active_tool(&state.editor_ui);
                return update(state, Message::EditorSelectTool(tool));
            }
            Task::none()
        }
        Message::CloseToolGroupFlyout => {
            if matches!(state.active_menu, Some(ActiveMenu::EditorToolFlyout(_))) {
                state.active_menu = None;
            }
            Task::none()
        }
        Message::OpenAboutDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let lang = state.snapshot.meta.language;
            let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
            state.dialog = Some(InAppDialog::Alert {
                id: DesktopDialogId::About,
                title: tr(lang, "action-about"),
                message: format!(
                    "{} v{}\nUnified Browser · Viewer · Editor\nGPU Backend: {backend}",
                    tr(lang, "app-title"),
                    env!("CARGO_PKG_VERSION")
                ),
            });
            open_dialog_window(state, DesktopDialogId::About);
            Task::none()
        }
        Message::ViewerZoomActual => {
            state.context_menu = None;
            if state.viewer_mode == ViewerViewMode::Webtoon {
                state.viewer_webtoon_fit_mode = WebtoonFitMode::Original;
                state.webtoon_zoom = 1.0;
                return sync_ui_tree_task(state);
            }
            if let Some(frame) = &state.viewer_frame {
                let (base_w, base_h) = if state.viewer_mode == ViewerViewMode::Double
                    && let Some(pair) = &state.viewer_pair_frame
                {
                    let (sw, sh, _, _) = dual_spread_geometry(
                        (frame.width, frame.height),
                        (pair.width, pair.height),
                        f32::from(state.viewer_manga_page_gap),
                    );
                    (sw, sh)
                } else {
                    (frame.width as f32, frame.height as f32)
                };
                let odd = (state.viewer_quarter_turns % 2) == 1;
                let (iw, ih) = if odd {
                    (base_h, base_w)
                } else {
                    (base_w, base_h)
                };
                let vp_w = (state.window_size.width - 40.0).max(120.0);
                let vp_h = (state.window_size.height - 130.0).max(120.0);
                let (actual_zoom, min_z, max_z) =
                    actual_zoom_for_bounds(iw.max(1.0) as u32, ih.max(1.0) as u32, vp_w, vp_h);
                state.zoom = actual_zoom.clamp(min_z, max_z);
                state.pan_x = 0.0;
                state.pan_y = 0.0;
            }
            sync_ui_tree_task(state)
        }
        Message::ViewerRotatePreview(delta) => {
            state.context_menu = None;
            let next = (i16::from(state.viewer_quarter_turns) + i16::from(delta)).rem_euclid(4);
            state.viewer_quarter_turns = next as u8;
            sync_ui_tree_task(state)
        }
        Message::ToggleFullscreen => {
            state.active_menu = None;
            state.context_menu = None;
            state.is_fullscreen = !state.is_fullscreen;
            if let Some(win_id) = state.main_window {
                let mode = if state.is_fullscreen {
                    window::Mode::Fullscreen
                } else {
                    window::Mode::Windowed
                };
                return Task::batch([window::set_mode(win_id, mode), sync_ui_tree_task(state)]);
            }
            sync_ui_tree_task(state)
        }
        Message::CloseFilmstrip => {
            state.context_menu = None;
            state.viewer_filmstrip_open = false;
            state.viewer_filmstrip_offset = 0.0;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::FilmstripScrolled { offset } => {
            let offset = offset.max(0.0);
            if (state.viewer_filmstrip_offset - offset).abs() < 0.5 {
                return Task::none();
            }
            state.viewer_filmstrip_offset = offset;
            schedule_missing_thumbnails(state)
        }
        Message::FilmstripWheelScroll(delta) => {
            let dx = match delta {
                iced::mouse::ScrollDelta::Lines { x, y } => {
                    if x.abs() > 0.001 {
                        -x * 64.0
                    } else {
                        -y * 64.0
                    }
                }
                iced::mouse::ScrollDelta::Pixels { x, y } => {
                    if x.abs() > 0.5 && x.abs() >= y.abs() {
                        -x
                    } else {
                        -y
                    }
                }
            };
            if dx.abs() < 0.1 {
                return Task::none();
            }
            let next = (state.viewer_filmstrip_offset + dx).max(0.0);
            state.viewer_filmstrip_offset = next;
            let scroll_task = widget_operation::scroll_to(
                filmstrip_scroll_id(),
                scrollable::AbsoluteOffset {
                    x: Some(next),
                    y: None,
                },
            );
            Task::batch([scroll_task, schedule_missing_thumbnails(state)])
        }
        Message::ToggleFilmstrip => {
            state.active_menu = None;
            state.context_menu = None;
            state.viewer_filmstrip_open = !state.viewer_filmstrip_open;
            let thumb_task = if state.viewer_filmstrip_open {
                schedule_missing_thumbnails(state)
            } else {
                state.viewer_filmstrip_offset = 0.0;
                Task::none()
            };
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([
                thumb_task,
                scroll_filmstrip_to_active(state),
                layout_task,
                sync_ui_tree_task(state),
            ])
        }
        Message::ToggleViewerInspector => {
            state.active_menu = None;
            state.context_menu = None;
            state.viewer_info_open = !state.viewer_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleBrowserInspector => {
            state.active_menu = None;
            state.context_menu = None;
            let next = !(state.browser_preview_open || state.browser_info_open);
            state.browser_preview_open = next;
            state.browser_info_open = next;
            state.browser_inspector_open = next;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BrowserOpenSelectedInEditor => {
            state.active_menu = None;
            state.context_menu = None;
            let selected = state.snapshot.meta.selection.iter().find(|path| {
                state
                    .snapshot
                    .entries
                    .iter()
                    .any(|e| &e.path == *path && e.is_image)
            });
            if let Some(path) = selected.cloned() {
                open_in_editor_or_forward(state, path)
            } else {
                Task::none()
            }
        }
        Message::BrowserOpenSelectedInViewer => {
            state.active_menu = None;
            state.context_menu = None;
            let selected = state.snapshot.meta.selection.iter().find(|path| {
                state.snapshot.entries.iter().any(|e| {
                    &e.path == *path
                        && (e.is_image
                            || Path::new(&e.path)
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .is_some_and(photon_protocol::is_archive_extension))
                })
            });
            if let Some(path) = selected.cloned() {
                open_or_focus_viewer_process(vec![path], state.snapshot.meta.viewer_single_instance)
            } else {
                Task::none()
            }
        }
        Message::SetUiScalePct(scale_pct) => {
            let clamped = scale_pct.clamp(
                photon_protocol::UI_SCALE_PCT_MIN,
                photon_protocol::UI_SCALE_PCT_MAX,
            );
            state.suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
            commit_layout_with(state, move |layout| {
                layout.ui_scale_pct = clamped;
            })
        }
        Message::SetFontScalePct(scale_pct) => {
            let clamped = scale_pct.clamp(
                photon_protocol::FONT_SCALE_PCT_MIN,
                photon_protocol::FONT_SCALE_PCT_MAX,
            );
            state.viewer_translate.font_scale = (clamped as f32) / 100.0;
            commit_layout_with(state, move |layout| {
                layout.font_scale_pct = clamped;
            })
        }
        Message::SetDisplayFont(font) => commit_layout_with(state, move |layout| {
            layout.display_font = font;
        }),
        Message::CopyViewerPath => {
            state.context_menu = None;
            if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                let toast_task = show_toast(
                    state,
                    tr(state.snapshot.meta.language, "toast-path-copied"),
                    ToastKind::Success,
                );
                return Task::batch([iced::clipboard::write(path), toast_task]);
            }
            Task::none()
        }
        Message::OpenSettings => {
            state.active_menu = None;
            state.context_menu = None;
            state.settings_open = true;
            state.settings_tab = SettingsTab::General;
            state.settings_confirm_factory_reset = false;
            state.translate_draft = state.snapshot.meta.translate_settings.clone();
            if state.plugins_selected_id.is_none() {
                state.plugins_selected_id =
                    state.snapshot.meta.plugins.first().map(|p| p.id.clone());
            }
            open_dialog_window(state, DesktopDialogId::Settings);
            Task::batch([
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: false }),
                sync_ui_tree_task(state),
            ])
        }
        Message::CloseSettings => {
            state.settings_open = false;
            state.settings_confirm_factory_reset = false;
            state.dialog_manager.close(&DesktopDialogId::Settings);
            if state.dialog.is_none() && state.editor_ui.adjust_popup_tab.is_none() {
                state.active_dialog_window = None;
            }
            Task::batch([
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: false }),
                sync_ui_tree_task(state),
            ])
        }
        Message::ModifiersChanged(m) => {
            state.ctrl_held = m.control();
            state.shift_held = m.shift();
            Task::none()
        }
        Message::ShutdownAfterSave => iced::exit(),
        Message::SaveCurrentUiLayout => {
            state.active_menu = None;
            let save_task =
                dispatch_to_core(state.core.clone(), Command::SaveUiState { force: true });
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-ui-layout-saved"),
                ToastKind::Success,
            );
            Task::batch([save_task, toast_task])
        }
        Message::ApplySafeDefaultsToDisk => {
            state.active_menu = None;
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: true,
                },
            )
        }
        Message::SetViewerViewportBackground(bg) => {
            state.active_menu = None;
            state.viewer_viewport_background = bg;
            commit_layout_with(state, |layout| {
                layout.viewer_viewport_background = bg;
            })
        }
        Message::SelectSettingsTab(tab) => {
            state.settings_tab = tab;
            state.settings_confirm_factory_reset = false;
            sync_ui_tree_task(state)
        }
        Message::SelectPluginSettingsTab(tab) => {
            state.plugin_settings_tab = tab;
            Task::none()
        }
        Message::ToggleSettingsSection(key) => {
            if !state.settings_collapsed.remove(key) {
                state.settings_collapsed.insert(key);
            }
            Task::none()
        }
        Message::RequestFactoryReset => {
            if !state.settings_confirm_factory_reset {
                state.settings_confirm_factory_reset = true;
                return Task::none();
            }
            state.settings_confirm_factory_reset = false;
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: true,
                },
            )
        }
        Message::PluginsSearchChanged(val) => {
            state.plugins_search = val;
            Task::none()
        }
        Message::PluginsSelect(id) => {
            state.plugins_selected_id = Some(id);
            Task::none()
        }
        Message::SetTranslateProvider(provider) => {
            state.translate_draft.provider = provider;
            dispatch_to_core(
                state.core.clone(),
                Command::SetTranslateSettings {
                    settings: state.translate_draft.clone(),
                },
            )
        }
        Message::TranslateEndpointChanged(val) => {
            state.translate_draft.libre_endpoint = val;
            Task::none()
        }
        Message::TranslateApiKeyChanged(val) => {
            let trimmed = val.trim().to_owned();
            state.translate_draft.api_key = if trimmed.is_empty() { None } else { Some(val) };
            Task::none()
        }
        Message::TranslateSourceLangChanged(val) => {
            state.translate_draft.default_source_lang = val;
            Task::none()
        }
        Message::TranslateTargetLangChanged(val) => {
            state.translate_draft.default_target_lang = val;
            Task::none()
        }
        Message::SaveTranslateSettings => dispatch_to_core(
            state.core.clone(),
            Command::SetTranslateSettings {
                settings: state.translate_draft.clone(),
            },
        ),
        Message::PickCustomModelFile(model_id) => {
            let title = tr(state.snapshot.meta.language, "action-local-file");
            Task::perform(
                async move { photon_dialogs::open_file(&title, None, &[]).await },
                move |picked| Message::CustomModelFilePicked(model_id, picked),
            )
        }
        Message::CustomModelFilePicked(model_id, Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::AiSetCustomModel {
                model_id,
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::CustomModelFilePicked(_, None) => Task::none(),
        Message::OpenPluginModels(plugin_id) => {
            state.active_menu = None;
            state.context_menu = None;
            state.plugin_settings_tab = PluginSettingsTab::General;
            state.dialog = Some(InAppDialog::PluginModels { plugin_id });
            open_dialog_window(state, DesktopDialogId::PluginModels);
            Task::none()
        }
        Message::OpenTranslateSettings => {
            state.active_menu = None;
            state.context_menu = None;
            state.translate_draft = state.snapshot.meta.translate_settings.clone();
            state.dialog = Some(InAppDialog::TranslateSettings);
            open_dialog_window(state, DesktopDialogId::TranslateSettings);
            Task::none()
        }
        Message::OpenTranslatePanel => {
            state.active_menu = None;
            state.context_menu = None;
            match state.snapshot.meta.workspace {
                Workspace::Viewer => {
                    handle_viewer_translate_message(state, ViewerMessage::ToggleTranslateOverlay)
                }
                Workspace::Browser => {
                    state.viewer_translate.active = true;
                    state.viewer_translate.active_page_path =
                        state.snapshot.meta.viewer_path.clone();
                    let layout_task = commit_layout_with(state, |_| {});
                    let ws_task = dispatch_to_core(
                        state.core.clone(),
                        Command::SetWorkspace {
                            workspace: Workspace::Viewer,
                        },
                    );
                    Task::batch([layout_task, ws_task, sync_ui_tree_task(state)])
                }
                Workspace::Editor => {
                    state.viewer_translate.active = true;
                    state.viewer_translate.active_page_path =
                        state.snapshot.meta.viewer_path.clone();
                    let layout_task = commit_layout_with(state, |_| {});
                    let ws_task = update(
                        state,
                        Message::RequestEditorLeave {
                            close_editor: false,
                        },
                    );
                    Task::batch([layout_task, ws_task, sync_ui_tree_task(state)])
                }
            }
        }
        Message::CopyText(value) => {
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-clipboard-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(value), toast_task])
        }
        Message::ViewerTranslate(vmsg) => handle_viewer_translate_message(state, vmsg),
        Message::EditorAi(emsg) => handle_editor_ai_message(state, emsg),
        Message::EditorVectorText(emsg) => handle_editor_vector_text_message(state, emsg),
        Message::ExportVectorSvgPathPicked(Some(picked)) => {
            if let Some(InAppDialog::ExportVectorSvg { path, error }) = &mut state.dialog {
                *path = picked.to_string_lossy().into_owned();
                *error = None;
            }
            Task::none()
        }
        Message::ExportVectorSvgPathPicked(None) => Task::none(),
        Message::LayoutCommandCompleted(res) => {
            state.pending_layout_commands = state.pending_layout_commands.saturating_sub(1);
            update(state, Message::CommandCompleted(res))
        }
        Message::KeyEvent(ev) => handle_key_event(state, ev),
        Message::WindowOpened(id) => {
            tracing::debug!(target: "photon::desktop", ?id, "window opened");
            state.last_active_window = Some(id);
            let mut tasks = vec![
                window::scale_factor(id).map(move |s| Message::WindowRescaled(id, s)),
                sync_ui_tree_task(state),
            ];
            if state.main_window == Some(id) && !state.is_fullscreen {
                let check_task = Task::batch([window::monitor_size(id).then(move |mon| {
                    window::is_maximized(id)
                        .map(move |maxed| Message::WindowMonitorSizeCheck(id, mon, maxed))
                })]);
                tasks.push(check_task);
            }
            Task::batch(tasks)
        }
        Message::WindowRescaled(id, scale) => {
            if state.main_window == Some(id) && scale.is_finite() && scale > 0.0 {
                state.os_scale_factor = scale.clamp(0.5, 4.0);
                state.suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
            }
            sync_ui_tree_task(state)
        }
        Message::WindowFocused(id) => {
            if state.windows.contains_key(&id) {
                state.last_active_window = Some(id);
            }
            Task::none()
        }
        Message::WakeWindow(win_id) => {
            crate::window_activation::request_platform_activation(std::process::id());
            Task::batch([
                window::minimize(win_id, false),
                window::request_user_attention(win_id, None),
                Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(30)).await;
                    },
                    move |_| Message::WakeWindowRaise(win_id),
                ),
            ])
        }
        Message::WakeWindowRaise(win_id) => Task::batch([
            window::gain_focus(win_id),
            window::set_level(win_id, window::Level::AlwaysOnTop),
            Task::perform(
                async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                },
                move |_| Message::WakeWindowSettle(win_id),
            ),
        ]),
        Message::WakeWindowSettle(win_id) => Task::batch([
            window::set_level(win_id, window::Level::Normal),
            window::gain_focus(win_id),
            window::request_user_attention(win_id, None),
        ]),
        Message::WindowMonitorSizeCheck(id, Some(mon), is_maximized) => {
            // On Wayland, `xdg_toplevel::configure` already constrains initial window bounds and
            // calling `window::resize` during initial `wp_fractional_scale_v1` negotiation can
            // race with `ScaleFactorChanged` and invert the window size by `1 / os_scale`.
            let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
            if !on_wayland && state.main_window == Some(id) && !state.is_fullscreen && !is_maximized
            {
                let ui_scale = state.snapshot.meta.layout.ui_scale().max(0.5);
                let min_w = 680.0 / ui_scale;
                let min_h = 500.0 / ui_scale;
                let max_w = (mon.width * 0.94).max(min_w).min(mon.width);
                let max_h = (mon.height * 0.90).max(min_h).min(mon.height);
                if state.window_size.width > max_w || state.window_size.height > max_h {
                    let clamped = Size::new(
                        state.window_size.width.min(max_w).max(min_w),
                        state.window_size.height.min(max_h).max(min_h),
                    );
                    state.window_size = clamped;
                    state.suppress_resize_until = Some(Instant::now() + Duration::from_millis(600));
                    return window::resize(id, clamped);
                }
            }
            Task::none()
        }
        Message::WindowMonitorSizeCheck(_, None, _) => Task::none(),
        Message::WindowClosed(id) => {
            state.windows.remove(&id);
            if state.last_active_window == Some(id) {
                state.last_active_window = state.windows.keys().next().copied();
            }
            if state.active_dialog_window == Some(id) {
                state.active_dialog_window = state.last_active_window;
            }
            if state.main_window == Some(id) {
                state.main_window = state.windows.keys().next().copied();
            }
            if state.windows.is_empty() {
                if !is_standalone_viewer()
                    && !is_standalone_editor()
                    && (state.snapshot.meta.save_ui_state_on_exit || state.ctrl_held)
                {
                    let core = state.core.clone();
                    let force = state.ctrl_held;
                    return Task::perform(
                        async move {
                            let env = Envelope::new(
                                format!("gui-exit-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::SaveUiState { force },
                            );
                            let _ = core.dispatch(env).await;
                        },
                        |_| Message::ShutdownAfterSave,
                    );
                }
                return iced::exit();
            }
            sync_ui_tree_task(state)
        }
        Message::WindowResized(id, size) => {
            if state.main_window == Some(id) {
                state.window_size = size;
                if is_standalone_viewer() {
                    let thumb_task = if state.viewer_filmstrip_open {
                        schedule_missing_thumbnails(state)
                    } else {
                        Task::none()
                    };
                    let should_persist_viewer = state.snapshot.meta.layout.system_open_window_mode
                        == SystemOpenWindowMode::Remembered
                        && !state
                            .suppress_resize_until
                            .is_some_and(|until| Instant::now() < until);
                    if !should_persist_viewer {
                        return Task::batch([thumb_task, sync_ui_tree_task(state)]);
                    }
                    state.suppress_resize_until = None;
                    let ui_scale = state.snapshot.meta.layout.ui_scale().max(0.5);
                    let w = ((size.width * ui_scale).round() as u32).clamp(480, 3840);
                    let h = ((size.height * ui_scale).round() as u32).clamp(360, 2160);
                    state.resize_save_seq = state.resize_save_seq.wrapping_add(1);
                    let seq = state.resize_save_seq;
                    let save_task = Task::perform(
                        async move {
                            tokio::time::sleep(Duration::from_millis(400)).await;
                            Message::PersistWindowSize(seq, w, h)
                        },
                        |m| m,
                    );
                    return Task::batch([thumb_task, sync_ui_tree_task(state), save_task]);
                }
                if state
                    .suppress_resize_until
                    .is_some_and(|until| Instant::now() < until)
                {
                    return sync_ui_tree_task(state);
                }
                state.suppress_resize_until = None;
                let ui_scale = state.snapshot.meta.layout.ui_scale().max(0.5);
                let w = ((size.width * ui_scale).round() as u32).clamp(680, 3840);
                let h = ((size.height * ui_scale).round() as u32).clamp(500, 2160);
                state.resize_save_seq = state.resize_save_seq.wrapping_add(1);
                let seq = state.resize_save_seq;
                let save_task = Task::perform(
                    async move {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        Message::PersistWindowSize(seq, w, h)
                    },
                    |m| m,
                );
                return Task::batch([sync_ui_tree_task(state), save_task]);
            }
            Task::none()
        }
        Message::PersistWindowSize(seq, w, h) => {
            if seq != state.resize_save_seq
                || state.is_fullscreen
                || state
                    .suppress_resize_until
                    .is_some_and(|until| Instant::now() < until)
            {
                return Task::none();
            }
            let Some(main_id) = state.main_window else {
                return Task::none();
            };
            window::is_maximized(main_id)
                .map(move |is_max| Message::PersistWindowSizeIfWindowed(seq, w, h, is_max))
        }
        Message::PersistWindowSizeIfWindowed(seq, w, h, is_maximized) => {
            if seq != state.resize_save_seq || state.is_fullscreen {
                return Task::none();
            }
            let layout = &state.snapshot.meta.layout;
            if is_standalone_viewer() {
                if layout.system_open_window_mode != SystemOpenWindowMode::Remembered {
                    return Task::none();
                }
                if is_maximized {
                    if layout.viewer_window_maximized {
                        return Task::none();
                    }
                    return commit_layout_with(state, |layout| {
                        layout.viewer_window_maximized = true;
                    });
                }
                if !layout.viewer_window_maximized
                    && layout.viewer_window_width == w
                    && layout.viewer_window_height == h
                {
                    return Task::none();
                }
                return commit_layout_with(state, |layout| {
                    layout.viewer_window_maximized = false;
                    layout.viewer_window_width = w;
                    layout.viewer_window_height = h;
                });
            }
            if is_maximized {
                if layout.window_maximized {
                    return Task::none();
                }
                return commit_layout_with(state, |layout| {
                    layout.window_maximized = true;
                });
            }
            if !layout.window_maximized && layout.window_width == w && layout.window_height == h {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.window_maximized = false;
                layout.window_width = w;
                layout.window_height = h;
            })
        }
        Message::StateUpdated(snap) => update_snapshot::handle_state_updated(state, snap),
        Message::CoreEvent(ev) => update_snapshot::handle_core_event(state, ev),
        Message::TriggerWindowScreenshot(win_id) => {
            window::screenshot(win_id).map(move |shot| Message::ScreenshotReady(win_id, shot))
        }
        Message::ScreenshotReady(win_id, shot) => {
            if let Some((request_id, out_path)) = state.pending_screenshots.remove(&win_id) {
                let core = state.core.clone();
                return Task::perform(
                    async move {
                        let w = shot.size.width;
                        let h = shot.size.height;
                        if let Some(parent) = Path::new(&out_path).parent()
                            && !parent.as_os_str().is_empty()
                        {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        if let Some(img) = RgbaImage::from_raw(w, h, shot.rgba.to_vec()) {
                            let _ = img.save(&out_path);
                            core.emit_event(EventKind::ScreenshotCaptured {
                                request_id,
                                path: out_path,
                                width: w,
                                height: h,
                            })
                            .await;
                        }
                    },
                    |_| Message::Noop,
                );
            }
            Task::none()
        }
        Message::Dispatch(cmd) => {
            state.active_menu = None;
            if is_standalone_viewer()
                && let Command::SetWorkspace { workspace } = cmd
                && workspace != Workspace::Viewer
            {
                return switch_workspace_from_standalone_viewer(state, workspace);
            }
            if is_standalone_editor()
                && let Command::SetWorkspace { workspace } = cmd
                && workspace != Workspace::Editor
            {
                return match workspace {
                    Workspace::Browser => forward_or_spawn_browser(browser_dir_for_state(state)),
                    Workspace::Viewer => open_editor_doc_in_viewer(state),
                    Workspace::Editor => Task::none(),
                };
            }
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::CommandCompleted(res) => update_snapshot::handle_command_completed(state, res),
        Message::PickFolderDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-open-folder");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                Message::FolderPicked,
            )
        }
        Message::FolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::OpenFolder {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::FolderPicked(None) => Task::none(),
        Message::PickFavoriteFolderDialog => {
            let title = tr(state.snapshot.meta.language, "action-pin-quick-access");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                Message::FavoriteFolderPicked,
            )
        }
        Message::FavoriteFolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::AddFavorite {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::FavoriteFolderPicked(None) => Task::none(),
        Message::PickImagesDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-open-file");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::open_images(&title, start).await },
                Message::ImagesPicked,
            )
        }
        Message::ImagesPicked(paths) if !paths.is_empty() => {
            let path_strings: Vec<String> = paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            if is_standalone_viewer() {
                if let Some(first) = path_strings.first().cloned() {
                    let core = state.core.clone();
                    return Task::perform(
                        async move {
                            let app_paths = photon_storage::AppPaths::current();
                            let canon = canonicalize_path_str(&first);
                            let _lock = photon_ipc::acquire_viewer_path_lock(
                                &app_paths,
                                &canon,
                                Duration::from_secs(2),
                            );
                            if let Some(matched) =
                                photon_ipc::find_live_viewer_for_path(&app_paths, &canon)
                            {
                                if matched.info.pid != std::process::id() {
                                    let env = Envelope::new(
                                        format!("gui-{}", std::process::id()),
                                        ClientKind::Gui,
                                        Command::FocusWindow,
                                    );
                                    let _ = photon_ipc::send_once(
                                        &matched.info.socket_path,
                                        &photon_ipc::IpcRequest::Dispatch { envelope: env },
                                    )
                                    .await;
                                    return None;
                                }
                            }
                            let env = Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::OpenFiles {
                                    paths: path_strings,
                                },
                            );
                            Some(core.dispatch(env).await)
                        },
                        |res| match res {
                            Some(r) => Message::CommandCompleted(r),
                            None => Message::Noop,
                        },
                    );
                }
                Task::none()
            } else {
                dispatch_to_core(
                    state.core.clone(),
                    Command::OpenFiles {
                        paths: path_strings,
                    },
                )
            }
        }
        Message::ImagesPicked(_) => Task::none(),
        Message::EditorOpenNewTabDialog => {
            state.active_menu = None;
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "tooltip-editor-new-tab");
            let start = state
                .snapshot
                .meta
                .editor
                .as_ref()
                .and_then(|d| Path::new(&d.path).parent().map(Path::to_path_buf))
                .or_else(|| {
                    state
                        .snapshot
                        .meta
                        .current_folder
                        .as_deref()
                        .map(PathBuf::from)
                });
            Task::perform(
                async move { photon_dialogs::open_images(&title, start).await },
                Message::EditorNewTabFilesPicked,
            )
        }
        Message::EditorNewTabFilesPicked(paths) if !paths.is_empty() => {
            let core = state.core.clone();
            Task::perform(
                async move {
                    let mut last_resp: Option<Result<CommandResponse, String>> = None;
                    for path in paths {
                        let env = Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::EditorOpen {
                                path: path.to_string_lossy().into_owned(),
                            },
                        );
                        last_resp = Some(core.dispatch(env).await);
                    }
                    last_resp
                },
                |res| match res {
                    Some(r) => Message::CommandCompleted(r),
                    None => Message::Noop,
                },
            )
        }
        Message::EditorNewTabFilesPicked(_) => Task::none(),
        Message::OpenInViewerWindow(path) => {
            open_or_focus_viewer_process(vec![path], state.snapshot.meta.viewer_single_instance)
        }
        Message::OpenBrowserWindow => forward_or_spawn_browser(browser_dir_for_state(state)),
        Message::OpenEditorViewerWindow => open_editor_doc_in_viewer(state),
        Message::PickSaveAsDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-save-as");
            let (start, default_name) = match &state.snapshot.meta.editor {
                Some(doc) => {
                    let p = Path::new(&doc.save_path);
                    (p.parent().map(Path::to_path_buf), Some(doc.name.clone()))
                }
                None => (None, None),
            };
            Task::perform(
                async move {
                    photon_dialogs::save_image_as(&title, start, default_name.as_deref()).await
                },
                Message::SaveAsPicked,
            )
        }
        Message::PickSaveOpenRasterDialog => {
            state.active_menu = None;
            let title = tr(state.snapshot.meta.language, "action-save-openraster");
            let (start, default_name) = match &state.snapshot.meta.editor {
                Some(doc) => {
                    let p = Path::new(&doc.save_path);
                    let stem = Path::new(&doc.name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("image");
                    (
                        p.parent().map(Path::to_path_buf),
                        Some(format!("{stem}.ora")),
                    )
                }
                None => (None, Some("image.ora".to_owned())),
            };
            Task::perform(
                async move {
                    photon_dialogs::save_openraster_as(&title, start, default_name.as_deref()).await
                },
                Message::SaveAsPicked,
            )
        }
        Message::SaveAsPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::EditorSaveAs {
                path: path.to_string_lossy().into_owned(),
            },
        ),
        Message::SaveAsPicked(None) => Task::none(),
        Message::OpenNewFolderDialog => {
            state.active_menu = None;
            let parent = state
                .snapshot
                .meta
                .current_folder
                .clone()
                .unwrap_or_else(|| {
                    photon_storage::paths::home_dir()
                        .to_string_lossy()
                        .into_owned()
                });
            state.dialog = Some(InAppDialog::NewFolder {
                parent,
                name: String::new(),
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::NewFolder);
            Task::none()
        }
        Message::NewFolderNameChanged(val) => {
            if let Some(InAppDialog::NewFolder { name, error, .. }) = &mut state.dialog {
                *name = val;
                *error = None;
            }
            Task::none()
        }
        Message::ConfirmNewFolder => {
            let Some(InAppDialog::NewFolder { parent, name, .. }) = state.dialog.clone() else {
                return Task::none();
            };
            let clean = name.trim().to_owned();
            if clean.is_empty() || clean.contains('/') || clean.contains('\\') {
                if let Some(InAppDialog::NewFolder { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::NewFolder);
            dispatch_to_core(
                state.core.clone(),
                Command::NewFolder {
                    parent,
                    name: clean,
                },
            )
        }
        Message::OpenSelectionModifyDialog(kind) => {
            state.active_menu = None;
            state.dialog = Some(InAppDialog::SelectionModify {
                kind,
                value: kind.default_value().to_owned(),
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::SelectionModify);
            Task::none()
        }
        Message::SelectionModifyValueChanged(val) => {
            if let Some(InAppDialog::SelectionModify { value, error, .. }) = &mut state.dialog {
                *value = val;
                *error = None;
            }
            Task::none()
        }
        Message::ConfirmSelectionModify => {
            let Some(InAppDialog::SelectionModify { kind, value, .. }) = state.dialog.clone()
            else {
                return Task::none();
            };
            match kind.parse_and_validate(&value, state.snapshot.meta.language) {
                Ok(op) => {
                    state.dialog = None;
                    state
                        .dialog_manager
                        .close(&DesktopDialogId::SelectionModify);
                    dispatch_to_core(state.core.clone(), Command::EditorSelectModify { op })
                }
                Err(msg) => {
                    if let Some(InAppDialog::SelectionModify { error, .. }) = &mut state.dialog {
                        *error = Some(msg);
                    }
                    Task::none()
                }
            }
        }
        Message::RequestResetLayout => {
            state.dialog = Some(InAppDialog::ConfirmResetLayout);
            open_dialog_window(state, DesktopDialogId::ConfirmResetLayout);
            Task::none()
        }
        Message::ConfirmResetLayout => {
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmResetLayout);
            dispatch_to_core(
                state.core.clone(),
                Command::ResetConfig {
                    reset_layout: true,
                    reset_settings: false,
                },
            )
        }
        Message::RequestEditorLeave { close_editor } => {
            state.active_menu = None;
            state.pending_leave_to_browser = false;
            if let Some(doc) = &state.snapshot.meta.editor {
                if doc.modified {
                    state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                        tab_id: Some(doc.id),
                        close_editor,
                    });
                    open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                    return Task::none();
                }
                if close_editor {
                    return dispatch_to_core(
                        state.core.clone(),
                        Command::EditorCloseTab {
                            tab_id: Some(doc.id),
                        },
                    );
                }
                let has_disk_file = (!doc.save_path.is_empty()
                    && Path::new(&doc.save_path).is_file())
                    || (!doc.path.is_empty() && Path::new(&doc.path).is_file());
                if !has_disk_file && state.snapshot.viewer_images.is_empty() {
                    return update(state, Message::OpenExportDialog);
                }
            }
            if !close_editor && is_standalone_editor() {
                return open_editor_doc_in_viewer(state);
            }
            dispatch_to_core(state.core.clone(), Command::WorkspaceBack)
        }
        Message::RequestEditorLeaveToBrowser => {
            state.active_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor
                && doc.modified
            {
                state.pending_leave_to_browser = true;
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                    tab_id: Some(doc.id),
                    close_editor: false,
                });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            state.pending_leave_to_browser = false;
            if is_standalone_editor() {
                return forward_or_spawn_browser(browser_dir_for_state(state));
            }
            dispatch_to_core(
                state.core.clone(),
                Command::SetWorkspace {
                    workspace: Workspace::Browser,
                },
            )
        }
        Message::EditorSwitchTab(tab_id) => {
            state.active_menu = None;
            state.context_menu = None;
            dispatch_to_core(state.core.clone(), Command::EditorSwitchTab { tab_id })
        }
        Message::EditorRequestCloseTab(id) => {
            state.active_menu = None;
            state.context_menu = None;
            state.pending_close_tabs.clear();
            let is_modified = state
                .snapshot
                .meta
                .editor_tabs
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.modified)
                .unwrap_or_else(|| {
                    state
                        .snapshot
                        .meta
                        .editor
                        .as_ref()
                        .is_some_and(|d| d.id == id && d.modified)
                });
            if is_modified {
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                    tab_id: Some(id),
                    close_editor: true,
                });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCloseTab { tab_id: Some(id) },
            )
        }
        Message::EditorRequestCloseTabs(ids) => {
            state.active_menu = None;
            state.context_menu = None;
            if ids.is_empty() {
                return Task::none();
            }
            let any_modified = ids.iter().any(|id| {
                state
                    .snapshot
                    .meta
                    .editor_tabs
                    .iter()
                    .find(|t| t.id == *id)
                    .is_some_and(|t| t.modified)
            });
            if any_modified {
                state.pending_close_tabs = ids;
                state.dialog = Some(InAppDialog::ConfirmUnsavedLeave {
                    tab_id: None,
                    close_editor: true,
                });
                open_dialog_window(state, DesktopDialogId::ConfirmUnsavedLeave);
                return Task::none();
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCloseTabs { tab_ids: ids },
            )
        }
        Message::EditorOpenTabContextMenu(tab_id) => {
            state.active_menu = None;
            let p = state.last_cursor.unwrap_or(Point::new(120.0, 72.0));
            state.context_menu = Some(ActiveContextMenu::EditorTab {
                x: p.x.max(8.0),
                y: p.y.max(48.0),
                tab_id,
            });
            sync_ui_tree_task(state)
        }
        Message::EditorDuplicateDocument(tab_id) => {
            state.active_menu = None;
            state.context_menu = None;
            state.editor_ui.smart_paste = None;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorDuplicateDocument { tab_id },
            )
        }
        Message::DiscardAndLeaveEditor(close_editor) => {
            let to_browser = std::mem::take(&mut state.pending_leave_to_browser);
            let batch_tabs = std::mem::take(&mut state.pending_close_tabs);
            let tab_id = match state.dialog.take() {
                Some(InAppDialog::ConfirmUnsavedLeave { tab_id, .. }) => tab_id,
                _ => None,
            };
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            if !batch_tabs.is_empty() {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::EditorCloseTabs {
                        tab_ids: batch_tabs,
                    },
                );
            }
            if !close_editor && is_standalone_editor() {
                if to_browser {
                    return forward_or_spawn_browser(browser_dir_for_state(state));
                }
                return open_editor_doc_in_viewer(state);
            }
            let core = state.core.clone();
            Task::perform(
                async move {
                    let close_cmd = match tab_id {
                        Some(tab_id) => Command::EditorCloseTab {
                            tab_id: Some(tab_id),
                        },
                        None => Command::EditorClose,
                    };
                    if close_editor {
                        return core
                            .dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                close_cmd,
                            ))
                            .await;
                    }
                    let nav_cmd = if to_browser {
                        Command::SetWorkspace {
                            workspace: Workspace::Browser,
                        }
                    } else {
                        Command::WorkspaceBack
                    };
                    let nav_res = core
                        .dispatch(Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            nav_cmd,
                        ))
                        .await;
                    let _ = core
                        .dispatch(Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            close_cmd,
                        ))
                        .await;
                    nav_res
                },
                Message::CommandCompleted,
            )
        }
        Message::SaveAndLeaveEditor(close_editor) => {
            let to_browser = std::mem::take(&mut state.pending_leave_to_browser);
            let tab_id = match state.dialog.take() {
                Some(InAppDialog::ConfirmUnsavedLeave { tab_id, .. }) => tab_id,
                _ => None,
            };
            let needs_switch = tab_id
                .zip(state.snapshot.meta.active_editor_tab_id)
                .is_some_and(|(target, active)| target != active);
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmUnsavedLeave);
            let standalone_leave_task = if !close_editor && is_standalone_editor() {
                Some(if to_browser {
                    forward_or_spawn_browser(browser_dir_for_state(state))
                } else {
                    open_editor_doc_in_viewer(state)
                })
            } else {
                None
            };
            let core = state.core.clone();
            let save_task = Task::perform(
                async move {
                    if needs_switch && let Some(tab_id) = tab_id {
                        let _ = core
                            .dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                Command::EditorSwitchTab { tab_id },
                            ))
                            .await;
                    }
                    let save_resp = core
                        .dispatch(Envelope::new(
                            format!("gui-{}", std::process::id()),
                            ClientKind::Gui,
                            Command::EditorSave,
                        ))
                        .await;
                    match save_resp {
                        Ok(resp) if resp.ok => {
                            if !close_editor && is_standalone_editor() {
                                return Ok(resp);
                            }
                            let leave_cmd = if close_editor {
                                match tab_id {
                                    Some(tab_id) => Command::EditorCloseTab {
                                        tab_id: Some(tab_id),
                                    },
                                    None => Command::EditorClose,
                                }
                            } else if to_browser {
                                Command::SetWorkspace {
                                    workspace: Workspace::Browser,
                                }
                            } else {
                                Command::WorkspaceBack
                            };
                            core.dispatch(Envelope::new(
                                format!("gui-{}", std::process::id()),
                                ClientKind::Gui,
                                leave_cmd,
                            ))
                            .await
                        }
                        other => other,
                    }
                },
                Message::CommandCompleted,
            );
            if let Some(ext_task) = standalone_leave_task {
                Task::batch([save_task, ext_task])
            } else {
                save_task
            }
        }
        Message::CloseDialog => {
            state.pending_leave_to_browser = false;
            state.pending_close_tabs.clear();
            state.new_doc_create_from_clipboard = false;
            if let Some(d) = state.dialog.take() {
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::ViewportBackground,
                    previous_viewport_bg,
                    ..
                } = &d
                {
                    state.viewport_background = *previous_viewport_bg;
                }
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::ViewerViewportBg,
                    previous_viewport_bg,
                    ..
                } = &d
                {
                    state.viewer_viewport_background = *previous_viewport_bg;
                }
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::CanvasSizeFill,
                    ..
                } = &d
                    && std::mem::take(&mut state.reopen_canvas_size_after_picker)
                    && let Some((dims, anchor, fill)) = state.canvas_size_draft.take()
                {
                    state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                    state.dialog = Some(InAppDialog::CanvasSize {
                        dims,
                        anchor,
                        fill,
                        error: None,
                    });
                    open_dialog_window(state, DesktopDialogId::CanvasSize);
                    return Task::none();
                }
                if let InAppDialog::ColorPicker {
                    target: ColorPickerTarget::CanvasBackground,
                    ..
                } = &d
                    && std::mem::take(&mut state.reopen_new_doc_after_picker)
                    && let Some((dims, custom_rgb)) = state.new_doc_draft.take()
                {
                    state.dialog_manager.close(&DesktopDialogId::ColorPicker);
                    state.dialog = Some(InAppDialog::NewDocument {
                        dims,
                        background: NewDocBackground::Custom(custom_rgb),
                        custom_rgb,
                        error: None,
                    });
                    open_dialog_window(state, DesktopDialogId::NewDocument);
                    return Task::none();
                }
                state.canvas_size_draft = None;
                state.reopen_canvas_size_after_picker = false;
                state.new_doc_draft = None;
                state.reopen_new_doc_after_picker = false;
                state.dialog_manager.close(&dialog_id_for_in_app(&d));
            }
            Task::none()
        }
        Message::OpenNewDocumentDialog => {
            state.active_menu = None;
            state.context_menu = None;
            state.new_doc_draft = None;
            state.reopen_new_doc_after_picker = false;
            state.new_doc_clipboard_dims = None;
            state.new_doc_clipboard_png = None;
            state.new_doc_create_from_clipboard = false;
            state.dialog = Some(InAppDialog::NewDocument {
                dims: DimensionInputState::from_dims(1920, 1080),
                background: NewDocBackground::White,
                custom_rgb: [255, 255, 255],
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::NewDocument);
            let core = state.core.clone();
            let has_internal = state.snapshot.meta.has_pixel_clipboard;
            Task::perform(
                async move {
                    let os_png = tokio::task::spawn_blocking(read_clipboard_png_bytes)
                        .await
                        .ok()
                        .flatten();
                    let png = if let Some(bytes) = os_png {
                        Some(bytes)
                    } else if has_internal {
                        match core.query(Query::EditorClipboardPng).await {
                            Ok(Ok(val)) => val
                                .get("pngBytes")
                                .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok()),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let dims = png.as_deref().and_then(probe_png_dimensions);
                    (dims, png)
                },
                |(dims, png)| Message::NewDocClipboardProbed { dims, png },
            )
        }
        Message::NewDocClipboardProbed { dims, png } => {
            state.new_doc_clipboard_dims = dims;
            state.new_doc_clipboard_png = png;
            Task::none()
        }
        Message::NewDocUseClipboardSize => {
            if let Some((w, h)) = state.new_doc_clipboard_dims
                && let Some(InAppDialog::NewDocument { dims, error, .. }) = &mut state.dialog
            {
                dims.sync_from_px(w, h);
                *error = None;
            }
            Task::none()
        }
        Message::NewDocToggleCreateFromClipboard(enabled) => {
            state.new_doc_create_from_clipboard = enabled;
            if enabled
                && let Some((w, h)) = state.new_doc_clipboard_dims
                && let Some(InAppDialog::NewDocument { dims, error, .. }) = &mut state.dialog
            {
                dims.sync_from_px(w, h);
                *error = None;
            }
            Task::none()
        }
        Message::NewDocCreateFromClipboard => {
            let png = state.new_doc_clipboard_png.clone();
            state.dialog = None;
            state.new_doc_draft = None;
            state.reopen_new_doc_after_picker = false;
            state.new_doc_create_from_clipboard = false;
            state.dialog_manager.close(&DesktopDialogId::NewDocument);
            if png.is_some() || state.snapshot.meta.has_pixel_clipboard {
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorNewFromClipboard { png_bytes: png },
                )
            } else {
                update(state, Message::EditorNewFromClipboard)
            }
        }
        Message::EditorNewFromClipboard => {
            state.active_menu = None;
            state.context_menu = None;
            let has_internal = state.snapshot.meta.has_pixel_clipboard;
            let last_synced = state.last_synced_clipboard_png.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        let os_png = read_clipboard_png_bytes();
                        match os_png {
                            Some(bytes) => {
                                if has_internal && last_synced.as_ref() == Some(&bytes) {
                                    Some(bytes)
                                } else {
                                    Some(bytes)
                                }
                            }
                            None => None,
                        }
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::EditorNewFromClipboardResolved,
            )
        }
        Message::EditorNewFromClipboardResolved(png_bytes) => {
            if png_bytes.is_none() && !state.snapshot.meta.has_pixel_clipboard {
                return show_toast(
                    state,
                    tr(state.snapshot.meta.language, "toast-clipboard-empty"),
                    ToastKind::Info,
                );
            }
            dispatch_to_core(
                state.core.clone(),
                Command::EditorNewFromClipboard { png_bytes },
            )
        }
        Message::NewDocDimAction(action) => {
            if let Some(InAppDialog::NewDocument { dims, error, .. }) = &mut state.dialog {
                dims.apply(action);
                *error = None;
            }
            Task::none()
        }
        Message::NewDocApplyPreset(w, h) => {
            if let Some(InAppDialog::NewDocument { dims, error, .. }) = &mut state.dialog {
                dims.sync_from_px(w, h);
                *error = None;
            }
            Task::none()
        }
        Message::NewDocSetBackground(bg) => {
            if let Some(InAppDialog::NewDocument {
                background,
                custom_rgb,
                ..
            }) = &mut state.dialog
            {
                *background = bg;
                if let NewDocBackground::Custom(rgb) = bg {
                    *custom_rgb = rgb;
                }
            }
            Task::none()
        }
        Message::OpenNewDocColorPicker => {
            if let Some(InAppDialog::NewDocument {
                dims, custom_rgb, ..
            }) = state.dialog.clone()
            {
                state.new_doc_draft = Some((dims, custom_rgb));
                state.reopen_new_doc_after_picker = true;
                state.dialog_manager.close(&DesktopDialogId::NewDocument);
                state.dialog = Some(InAppDialog::ColorPicker {
                    target: ColorPickerTarget::CanvasBackground,
                    r: custom_rgb[0],
                    g: custom_rgb[1],
                    b: custom_rgb[2],
                    hex: format_hex(custom_rgb),
                    transparent: false,
                    previous_viewport_bg: state.viewport_background,
                });
                open_dialog_window(state, DesktopDialogId::ColorPicker);
            }
            Task::none()
        }
        Message::ConfirmNewDocument => {
            if state.new_doc_create_from_clipboard {
                return update(state, Message::NewDocCreateFromClipboard);
            }
            let Some(InAppDialog::NewDocument {
                dims, background, ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let w = dims.width_px;
            let h = dims.height_px;
            if (1..=32_768).contains(&w) && (1..=32_768).contains(&h) {
                state.dialog = None;
                state.new_doc_draft = None;
                state.reopen_new_doc_after_picker = false;
                state.dialog_manager.close(&DesktopDialogId::NewDocument);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorNew {
                        width: w,
                        height: h,
                        background: background.to_rgba(),
                    },
                )
            } else {
                if let Some(InAppDialog::NewDocument { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                Task::none()
            }
        }
        Message::OpenCanvasSizeDialog => {
            state.active_menu = None;
            state.context_menu = None;
            state.canvas_size_draft = None;
            state.reopen_canvas_size_after_picker = false;
            if let Some(doc) = &state.snapshot.meta.editor {
                state.dialog = Some(InAppDialog::CanvasSize {
                    dims: DimensionInputState::from_dims(doc.width, doc.height),
                    anchor: photon_protocol::CanvasAnchor::Center,
                    fill: state.editor_ui.canvas_fill,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::CanvasSize);
            }
            Task::none()
        }
        Message::CanvasSizeDimAction(action) => {
            if let Some(InAppDialog::CanvasSize { dims, error, .. }) = &mut state.dialog {
                dims.apply(action);
                *error = None;
            }
            Task::none()
        }
        Message::CanvasSizeAnchorChanged(new_anchor) => {
            if let Some(InAppDialog::CanvasSize { anchor, .. }) = &mut state.dialog {
                *anchor = new_anchor;
            }
            Task::none()
        }
        Message::ConfirmCanvasSize => {
            let Some(InAppDialog::CanvasSize {
                dims, anchor, fill, ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let w = dims.width_px;
            let h = dims.height_px;
            if (1..=32_768).contains(&w) && (1..=32_768).contains(&h) {
                state.dialog = None;
                state.canvas_size_draft = None;
                state.reopen_canvas_size_after_picker = false;
                state.editor_ui.canvas_fill = fill;
                state.dialog_manager.close(&DesktopDialogId::CanvasSize);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorCanvasSize {
                        width: w,
                        height: h,
                        anchor,
                        fill,
                        fill_alpha: if fill.is_some() { 255 } else { 0 },
                    },
                )
            } else {
                if let Some(InAppDialog::CanvasSize { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                Task::none()
            }
        }
        Message::OpenResizeDialog => {
            state.active_menu = None;
            state.context_menu = None;
            if let Some(doc) = &state.snapshot.meta.editor {
                state.dialog = Some(InAppDialog::Resize {
                    dims: DimensionInputState::from_dims(doc.width, doc.height),
                    filter: ResizeFilter::Lanczos3,
                    mode: ResizeMode::Stretch,
                    fill: None,
                    error: None,
                });
                open_dialog_window(state, DesktopDialogId::Resize);
            }
            Task::none()
        }
        Message::ResizeDimAction(action) => {
            if let Some(InAppDialog::Resize { dims, error, .. }) = &mut state.dialog {
                dims.apply(action);
                *error = None;
            }
            Task::none()
        }
        Message::ResizeFilterChanged(f) => {
            if let Some(InAppDialog::Resize { filter, .. }) = &mut state.dialog {
                *filter = f;
            }
            Task::none()
        }
        Message::ResizeModeChanged(m) => {
            if let Some(InAppDialog::Resize { mode, .. }) = &mut state.dialog {
                *mode = m;
            }
            Task::none()
        }
        Message::ConfirmResize => {
            let Some(InAppDialog::Resize {
                dims,
                filter,
                mode,
                fill,
                ..
            }) = state.dialog.clone()
            else {
                return Task::none();
            };
            let w = dims.width_px;
            let h = dims.height_px;
            if (1..=32_768).contains(&w) && (1..=32_768).contains(&h) {
                state.dialog = None;
                state.dialog_manager.close(&DesktopDialogId::Resize);
                dispatch_to_core(
                    state.core.clone(),
                    Command::EditorResize {
                        width: w,
                        height: h,
                        filter,
                        mode,
                        fill,
                        fill_alpha: if fill.is_some() { 255 } else { 0 },
                    },
                )
            } else {
                if let Some(InAppDialog::Resize { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                Task::none()
            }
        }
        Message::ToggleCropMode => {
            state.active_menu = None;
            state.context_menu = None;
            if state.crop_uv.is_some() {
                state.crop_uv = None;
                if state.editor_ui.tool == EditorTool::Crop {
                    state.editor_ui.tool = EditorTool::Pointer;
                }
            } else if state.snapshot.meta.editor.is_some() {
                state.editor_ui.tool = EditorTool::Crop;
                state.crop_uv = Some([0.1, 0.1, 0.9, 0.9]);
            }
            sync_ui_tree_task(state)
        }
        Message::SetCropPreset(ratio) => {
            state.editor_ui.crop_aspect = ratio;
            if let Some(doc) = &state.snapshot.meta.editor {
                let iw = doc.width.max(1) as f32;
                let ih = doc.height.max(1) as f32;
                let uv = match ratio {
                    None => [0.1, 0.1, 0.9, 0.9],
                    Some(target_ratio) => {
                        let img_ratio = iw / ih;
                        if img_ratio > target_ratio {
                            let h = 0.8_f32;
                            let w = (h * target_ratio / img_ratio).min(0.95);
                            let dx = (1.0 - w) * 0.5;
                            [dx, 0.1, 1.0 - dx, 0.9]
                        } else {
                            let w = 0.8_f32;
                            let h = (w * img_ratio / target_ratio).min(0.95);
                            let dy = (1.0 - h) * 0.5;
                            [0.1, dy, 0.9, 1.0 - dy]
                        }
                    }
                };
                state.crop_uv = Some(uv);
            }
            Task::none()
        }
        Message::ApplyCrop => {
            let Some(uv) = state.crop_uv.take() else {
                return Task::none();
            };
            let Some(doc) = &state.snapshot.meta.editor else {
                return Task::none();
            };
            let (x, y, w, h) = crop_uv_to_signed(uv, doc.width, doc.height);
            if state.editor_ui.tool == EditorTool::Crop {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            let fill = state.editor_ui.crop_fill;
            dispatch_to_core(
                state.core.clone(),
                Command::EditorCrop {
                    x,
                    y,
                    width: w,
                    height: h,
                    fill,
                    fill_alpha: if fill.is_some() { 255 } else { 0 },
                },
            )
        }
        Message::CancelCrop => {
            state.crop_uv = None;
            if state.editor_ui.tool == EditorTool::Crop {
                state.editor_ui.tool = EditorTool::Pointer;
            }
            sync_ui_tree_task(state)
        }
        Message::DismissToast(id) => {
            if id.is_none() || state.toast.as_ref().is_some_and(|t| Some(t.id) == id) {
                state.toast = None;
            }
            Task::none()
        }
        Message::TogglePathEdit(editing) => {
            state.active_menu = None;
            state.path_editing = editing;
            if !editing && let Some(folder) = &state.snapshot.meta.current_folder {
                state.folder_input = folder.clone();
            }
            Task::none()
        }
        Message::FolderInputChanged(val) => {
            state.folder_input = val;
            Task::none()
        }
        Message::SubmitFolderInput => {
            state.path_editing = false;
            let path = state.folder_input.trim().to_owned();
            if path.is_empty() {
                Task::none()
            } else {
                dispatch_to_core(state.core.clone(), Command::OpenFolder { path })
            }
        }
        Message::BrowserFilterChanged(val) => {
            state.browser_filter = val;
            Task::none()
        }
        Message::ToggleBrowserFilterBar => {
            state.browser_filter_bar_open = !state.browser_filter_bar_open;
            if state.browser_filter_bar_open {
                Task::batch([
                    widget_operation::focus(browser_filter_input_id()),
                    sync_ui_tree_task(state),
                ])
            } else {
                sync_ui_tree_task(state)
            }
        }
        Message::ToggleBrowserFilterCaseSensitive => {
            state.browser_filter_case_sensitive = !state.browser_filter_case_sensitive;
            Task::none()
        }
        Message::ToggleBrowserFilterRegex => {
            state.browser_filter_regex = !state.browser_filter_regex;
            Task::none()
        }
        Message::ClearBrowserFilter => {
            if !state.browser_filter.is_empty() {
                state.browser_filter.clear();
                widget_operation::focus(browser_filter_input_id())
            } else {
                state.browser_filter_bar_open = false;
                sync_ui_tree_task(state)
            }
        }
        Message::SetBrowserViewMode(mode) => {
            state.active_menu = None;
            if state.snapshot.meta.layout.browser_view_mode == mode {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_view_mode = mode;
            })
        }
        Message::SetViewerViewMode(mode) => {
            // Close any open menus / context menus
            state.active_menu = None;
            state.context_menu = None;

            // If already in the requested mode, do nothing
            if state.viewer_mode == mode {
                return Task::none();
            }

            let prev_mode = state.viewer_mode;
            state.viewer_mode = mode;

            // Reset single-image view state whenever leaving/entering a mode
            state.zoom = 1.0;
            state.pan_x = 0.0;
            state.pan_y = 0.0;

            let mut tasks = vec![sync_ui_tree_task(state)];

            match mode {
                ViewerViewMode::Single => {
                    // Clear dual-spread pair
                    state.viewer_pair_frame = None;
                    // Reload current single image
                    if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                        tasks.push(load_viewer_frame_task(&state.core, path));
                    }
                }
                ViewerViewMode::Double => {
                    // Coming from Webtoon: clear webtoon state
                    if prev_mode == ViewerViewMode::Webtoon {
                        state.webtoon_visible_index = None;
                    }
                    // Load both spread slots
                    tasks.push(reload_dual_frames_task(state));
                }
                ViewerViewMode::Webtoon => {
                    // Clear dual-spread pair frame
                    state.viewer_pair_frame = None;
                    let center = state.snapshot.meta.viewer_index.unwrap_or(0);
                    state.webtoon_visible_index = Some(center);
                    tasks.push(schedule_webtoon_pages_near(state, center));
                    if let Some(y) = webtoon_offset_for_index(state.viewer_view_state(true), center)
                    {
                        tasks.push(widget_operation::scroll_to(
                            webtoon_scroll_id(),
                            scrollable::AbsoluteOffset {
                                x: None,
                                y: Some(y),
                            },
                        ));
                    }
                }
            }
            Task::batch(tasks)
        }
        Message::SetDefaultViewerViewMode(mode) => {
            state.viewer_mode = mode;
            if state.snapshot.meta.layout.viewer_view_mode == mode {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.viewer_view_mode = mode;
            })
        }
        Message::DualStepSpread(delta) => {
            if let Some(path) = step_dual_spread(state.viewer_view_state(true), delta) {
                dispatch_to_core(state.core.clone(), Command::ViewerOpen { path })
            } else {
                Task::none()
            }
        }
        Message::SetMangaRtl(rtl) => {
            state.viewer_manga_rtl = rtl;
            if state.viewer_mode == ViewerViewMode::Double {
                let (left_slot, right_slot) = dual_spread_slots(state.viewer_view_state(true));
                let swapped = match (
                    left_slot,
                    right_slot,
                    &state.viewer_frame,
                    &state.viewer_pair_frame,
                ) {
                    (Some((_, lp)), Some((_, rp)), Some(lf), Some(rf))
                        if lf.key == format!("viewer:{rp}") && rf.key == format!("viewer:{lp}") =>
                    {
                        let old_l = state.viewer_frame.take();
                        let old_r = state.viewer_pair_frame.take();
                        state.viewer_frame = old_r;
                        state.viewer_pair_frame = old_l;
                        true
                    }
                    _ => false,
                };
                if swapped {
                    Task::none()
                } else {
                    reload_dual_frames_task(state)
                }
            } else {
                Task::none()
            }
        }
        Message::SetMangaSeparateCover(sep) => {
            state.viewer_manga_cover_separate = sep;
            if state.viewer_mode == ViewerViewMode::Double {
                reload_dual_frames_task(state)
            } else {
                Task::none()
            }
        }
        Message::SetMangaPageGap(gap) => {
            state.viewer_manga_page_gap = gap.min(64);
            Task::none()
        }
        Message::SetWebtoonFitMode(fit) => {
            state.webtoon_zoom = 1.0;
            state.viewer_webtoon_fit_mode = fit;
            Task::none()
        }
        Message::SetSystemOpenWindowMode(mode) => commit_layout_with(state, |layout| {
            layout.system_open_window_mode = mode;
        }),
        Message::SetViewerFixedWidth(w) => commit_layout_with(state, |layout| {
            layout.viewer_fixed_width = w.clamp(480, 3840);
        }),
        Message::SetViewerFixedHeight(h) => commit_layout_with(state, |layout| {
            layout.viewer_fixed_height = h.clamp(360, 2160);
        }),
        Message::SetAppLaunchWindowMode(mode) => commit_layout_with(state, |layout| {
            layout.app_launch_window_mode = mode;
        }),
        Message::SetAntiAliasingMode(mode) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                layout.canvas_anti_aliasing = mode;
            })
        }
        Message::SetViewportDenoiseMode(mode) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                layout.viewport_denoise = mode;
            })
        }
        Message::WebtoonScrolled(vp) => {
            let y = vp.absolute_offset().y;
            let vh = vp.bounds().height;
            state.webtoon_scroll_y = y;
            if let Some(idx) = webtoon_index_at_scroll(state.viewer_view_state(true), y + vh * 0.45)
            {
                let changed = state.webtoon_visible_index != Some(idx);
                state.webtoon_visible_index = Some(idx);
                if changed {
                    return schedule_webtoon_pages_near(state, idx);
                }
            }
            Task::none()
        }
        Message::WebtoonPagesLoaded(items) => {
            for (path, w, h, orig_w, orig_h, rgba) in items {
                state.webtoon_loading.remove(&path);
                state
                    .image_sizes
                    .entry(path.clone())
                    .or_insert((orig_w, orig_h));
                if !state.webtoon_pages.contains_key(&path) {
                    state.webtoon_order.push_back(path.clone());
                }
                state
                    .webtoon_pages
                    .insert(path, iced_image::Handle::from_rgba(w, h, rgba));
                while state.webtoon_order.len() > 48 {
                    if let Some(old) = state.webtoon_order.pop_front() {
                        state.webtoon_pages.remove(&old);
                    }
                }
            }
            Task::none()
        }
        Message::Browser(msg) => update(state, map_browser_message(msg)),
        Message::Viewer(msg) => update(state, map_viewer_message(msg)),
        Message::Editor(msg) => update(state, map_editor_message(msg)),
        Message::BrowserEntryActivated {
            path,
            is_dir,
            is_image,
        } => {
            if state.shift_held {
                state.last_click = None;
                let anchor = state
                    .browser_selection_anchor
                    .as_deref()
                    .or_else(|| state.snapshot.meta.selection.first().map(String::as_str))
                    .unwrap_or(&path);
                let entries = &state.snapshot.entries;
                let a_idx = entries.iter().position(|e| e.path == anchor);
                let b_idx = entries.iter().position(|e| e.path == path);
                let paths = match (a_idx, b_idx) {
                    (Some(i), Some(j)) => {
                        let (lo, hi) = if i <= j { (i, j) } else { (j, i) };
                        entries[lo..=hi].iter().map(|e| e.path.clone()).collect()
                    }
                    _ => vec![path.clone()],
                };
                if state.browser_selection_anchor.is_none() {
                    state.browser_selection_anchor = Some(path);
                }
                return dispatch_to_core(state.core.clone(), Command::Select { paths });
            }
            if state.ctrl_held {
                state.last_click = None;
                let mut paths = state.snapshot.meta.selection.clone();
                if let Some(pos) = paths.iter().position(|p| p == &path) {
                    paths.remove(pos);
                } else {
                    paths.push(path.clone());
                }
                state.browser_selection_anchor = Some(path);
                return dispatch_to_core(state.core.clone(), Command::Select { paths });
            }
            let now = Instant::now();
            let double_click = state.last_click.as_ref().is_some_and(|(prev, ts)| {
                prev == &path && now.duration_since(*ts) <= Duration::from_millis(450)
            });
            if double_click {
                state.last_click = None;
                if is_dir {
                    return dispatch_to_core(state.core.clone(), Command::Navigate { path });
                }
                if is_image {
                    return open_or_focus_viewer_process(
                        vec![path],
                        state.snapshot.meta.viewer_single_instance,
                    );
                }
                if is_archive_path_str(&path) {
                    return dispatch_to_core(state.core.clone(), Command::OpenArchive { path });
                }
                xdg_open(Path::new(&path));
                return Task::none();
            }
            state.last_click = Some((path.clone(), now));
            state.browser_selection_anchor = Some(path.clone());
            dispatch_to_core(state.core.clone(), Command::Select { paths: vec![path] })
        }
        Message::OpenExternally(path) => {
            state.active_menu = None;
            state.context_menu = None;
            xdg_open(Path::new(&path));
            Task::none()
        }
        Message::OpenWith(path) => {
            state.active_menu = None;
            state.context_menu = None;
            Task::perform(
                photon_dialogs::open_with_dialog(PathBuf::from(path)),
                |()| Message::Noop,
            )
        }
        Message::ShowInBrowser(path) => {
            state.active_menu = None;
            state.context_menu = None;
            show_in_browser_or_forward(state, path)
        }
        Message::BrowserEditPath(path) => {
            state.active_menu = None;
            state.context_menu = None;
            open_in_editor_or_forward(state, path)
        }
        Message::ThumbnailsLoaded(attempted, items) => {
            for path in attempted {
                state.thumbnails_loading.remove(&path);
            }
            for (path, w, h, rgba) in items {
                state.thumbnails_loading.remove(&path);
                state
                    .thumbnails
                    .insert(path, iced_image::Handle::from_rgba(w, h, rgba));
            }
            // Top up free in-flight slots for the current view/visible range.
            if state.snapshot.meta.workspace == Workspace::Browser
                || state.snapshot.meta.workspace == Workspace::Viewer
                || state.snapshot.meta.viewer_archive.is_some()
            {
                schedule_missing_thumbnails(state)
            } else {
                Task::none()
            }
        }
        Message::BrowserVisibleRangeChanged { start, end } => {
            let next = (start, end);
            if state.browser_visible_range == Some(next) {
                return Task::none();
            }
            state.browser_visible_range = Some(next);
            schedule_missing_thumbnails(state)
        }
        Message::ViewerImageSizesProbed(items) => {
            let mut changed = false;
            for (path, dims) in items {
                if state.image_sizes.insert(path, dims) != Some(dims) {
                    changed = true;
                }
            }
            if changed && state.viewer_mode == ViewerViewMode::Double {
                return reload_dual_frames_task(state);
            }
            Task::none()
        }
        Message::ToggleFolderExpand(path) => {
            if state.expanded_folders.contains(&path) {
                state.expanded_folders.remove(&path);
                Task::none()
            } else {
                state.expanded_folders.insert(path.clone());
                load_folder_children_task(state.core.clone(), path)
            }
        }
        Message::FolderChildrenLoaded(path, children) => {
            state.folder_children.insert(path, children);
            Task::none()
        }
        Message::ZoomStep(factor) => {
            state.context_menu = None;
            if state.snapshot.meta.workspace == Workspace::Viewer
                && state.viewer_mode == ViewerViewMode::Webtoon
            {
                state.webtoon_zoom = (state.webtoon_zoom * factor).clamp(0.4, 3.0);
                return Task::none();
            }
            let (min_z, max_z) = active_workspace_zoom_bounds(state, None);
            state.zoom = (state.zoom * factor).clamp(min_z, max_z);
            sync_ui_tree_task(state)
        }
        Message::EditorZoomActual => {
            state.context_menu = None;
            if let Some(frame) = &state.editor_frame {
                let rect = resolved_editor_canvas_bounds(state);
                let (actual, min_z, max_z) =
                    actual_zoom_for_bounds(frame.width, frame.height, rect.width, rect.height);
                state.zoom = actual.clamp(min_z, max_z);
                state.pan_x = 0.0;
                state.pan_y = 0.0;
            }
            sync_ui_tree_task(state)
        }
        Message::CanvasPanTo { pan_x, pan_y } => {
            state.pan_x = pan_x;
            state.pan_y = pan_y;
            Task::none()
        }
        Message::Canvas(ev) => update_canvas::handle_editor_canvas_event(state, ev),
        Message::OpenManagePlugins => {
            state.active_menu = None;
            state.context_menu = None;
            if matches!(state.dialog, Some(InAppDialog::ManagePlugins { .. })) {
                state.dialog = None;
                state.dialog_manager.close(&DesktopDialogId::ManagePlugins);
            }
            state.settings_open = true;
            state.settings_tab = SettingsTab::Plugins;
            state.settings_confirm_factory_reset = false;
            if state.plugins_selected_id.is_none() {
                state.plugins_selected_id =
                    state.snapshot.meta.plugins.first().map(|p| p.id.clone());
            }
            open_dialog_window(state, DesktopDialogId::Settings);
            sync_ui_tree_task(state)
        }
        Message::ManagePluginsSearchChanged(val) => {
            if let Some(InAppDialog::ManagePlugins { search, .. }) = &mut state.dialog {
                *search = val;
            }
            Task::none()
        }
        Message::ManagePluginsSelect(id) => {
            if let Some(InAppDialog::ManagePlugins { selected_id, .. }) = &mut state.dialog {
                *selected_id = Some(id);
            }
            Task::none()
        }
        Message::PickPluginInstallFolder => {
            let title = tr(state.snapshot.meta.language, "action-install-plugin");
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, None).await },
                Message::PluginInstallFolderPicked,
            )
        }
        Message::PluginInstallFolderPicked(Some(path)) => dispatch_to_core(
            state.core.clone(),
            Command::PluginInstall {
                source_dir: path.to_string_lossy().into_owned(),
            },
        ),
        Message::PluginInstallFolderPicked(None) => Task::none(),
        Message::OpenPluginsFolder => {
            let dir = resolve_desktop_plugins_dir();
            let _ = std::fs::create_dir_all(&dir);
            open_folder_in_file_manager(&dir);
            Task::none()
        }
        Message::RunPlugin(id) => {
            state.active_menu = None;
            state.context_menu = None;
            let Some(plugin) = state
                .snapshot
                .meta
                .plugins
                .iter()
                .find(|p| p.id == id)
                .cloned()
            else {
                return Task::none();
            };
            if plugin.params.is_empty() {
                return dispatch_to_core(
                    state.core.clone(),
                    Command::PluginRun {
                        id,
                        params: BTreeMap::new(),
                        target: state.editor_ui.apply_target,
                    },
                );
            }
            let mut values = BTreeMap::new();
            for spec in &plugin.params {
                values.insert(spec.key.clone(), default_value_for_plugin_param(spec));
            }
            let show_target = state.snapshot.meta.workspace == Workspace::Editor
                && plugin
                    .capabilities
                    .contains(&PluginCapability::EditorFilter);
            state.dialog = Some(InAppDialog::PluginParams {
                plugin_id: plugin.id,
                plugin_name: plugin.name,
                specs: plugin.params,
                values,
                target: state.editor_ui.apply_target,
                show_target,
            });
            open_dialog_window(state, DesktopDialogId::PluginParams);
            sync_ui_tree_task(state)
        }
        Message::PluginParamChanged(key, val) => {
            if let Some(InAppDialog::PluginParams { values, .. }) = &mut state.dialog {
                values.insert(key, val);
            }
            Task::none()
        }
        Message::PluginParamTargetChanged(t) => {
            state.editor_ui.apply_target = t;
            if let Some(InAppDialog::PluginParams { target, .. }) = &mut state.dialog {
                *target = t;
            }
            Task::none()
        }
        Message::ConfirmPluginParams => {
            let Some(InAppDialog::PluginParams {
                plugin_id,
                values,
                target,
                ..
            }) = state.dialog.take()
            else {
                return Task::none();
            };
            state.dialog_manager.close(&DesktopDialogId::PluginParams);
            dispatch_to_core(
                state.core.clone(),
                Command::PluginRun {
                    id: plugin_id,
                    params: values,
                    target,
                },
            )
        }
        Message::BrowserCopyFiles(paths) => {
            state.active_menu = None;
            state.context_menu = None;
            let chosen = if paths.is_empty() {
                state.snapshot.meta.selection.clone()
            } else {
                paths
            };
            if chosen.is_empty() {
                return Task::none();
            }
            state.browser_file_clipboard = chosen.clone();
            state.last_synced_clipboard_png = None;
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-copied"),
                ToastKind::Success,
            );
            let uri_task = Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || write_file_uris_to_clipboard(&chosen))
                        .await
                        .unwrap_or(false)
                },
                |_| Message::Noop,
            );
            Task::batch([toast_task, uri_task])
        }
        Message::BrowserPasteFiles => {
            state.active_menu = None;
            state.context_menu = None;
            if state.snapshot.meta.current_folder.is_none() {
                return Task::none();
            }
            let in_app_files = state.browser_file_clipboard.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        resolve_browser_clipboard_paste(in_app_files)
                    })
                    .await
                    .ok()
                    .flatten()
                },
                Message::BrowserPasteResolved,
            )
        }
        Message::BrowserPasteResolved(Some(paste)) => {
            let Some(folder) = state.snapshot.meta.current_folder.clone() else {
                return Task::none();
            };
            match paste {
                BrowserClipboardPaste::Files(paths) => dispatch_to_core(
                    state.core.clone(),
                    Command::CopyTo {
                        paths,
                        destination: folder,
                    },
                ),
                BrowserClipboardPaste::ImagePng(png_bytes) => dispatch_to_core(
                    state.core.clone(),
                    Command::PasteImageBytes {
                        folder,
                        filename_hint: "pasted".to_owned(),
                        png_bytes,
                    },
                ),
            }
        }
        Message::BrowserPasteResolved(None) => show_toast(
            state,
            tr(state.snapshot.meta.language, "toast-clipboard-empty"),
            ToastKind::Info,
        ),
        Message::ToggleQuickAccessEditing => {
            state.quick_access_editing = !state.quick_access_editing;
            Task::none()
        }
        Message::BeginDragBrowserPanel(panel) => {
            state.browser_left_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverBrowserPanelDrop(panel) => {
            state.browser_left_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropBrowserPanel(target) => {
            if state.browser_left_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapBrowserLeftPanels => {
            if state.browser_left_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleBrowserPanelVisible(panel) => {
            state.active_menu = None;
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_open = !state.browser_quick_access_open;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_open = !state.browser_explorer_open;
                }
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleBrowserLeftPanelCollapse(panel) => {
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_collapsed = !state.browser_quick_access_collapsed;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_collapsed = !state.browser_explorer_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::CloseBrowserLeftPanel(panel) => {
            state.context_menu = None;
            match panel {
                BrowserLeftPanel::QuickAccess => state.browser_quick_access_open = false,
                BrowserLeftPanel::Explorer => state.browser_explorer_open = false,
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BeginDragBrowserRightPanel(panel) => {
            state.browser_right_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverBrowserRightPanelDrop(panel) => {
            state.browser_right_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropBrowserRightPanel(target) => {
            if state.browser_right_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapBrowserRightPanels => {
            if state.browser_right_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleBrowserRightPanelVisible(panel) => {
            state.active_menu = None;
            toggle_browser_right_panel(state, panel)
        }
        Message::ToggleBrowserRightPanelCollapse(panel) => {
            match panel {
                BrowserRightPanel::Preview => {
                    state.browser_preview_collapsed = !state.browser_preview_collapsed;
                }
                BrowserRightPanel::Info => {
                    state.browser_info_collapsed = !state.browser_info_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::CloseBrowserRightPanel(panel) => {
            match panel {
                BrowserRightPanel::Preview => state.browser_preview_open = false,
                BrowserRightPanel::Info => state.browser_info_open = false,
            }
            state.browser_inspector_open = state.browser_preview_open || state.browser_info_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleBrowserLeft(panel) => {
            state.active_menu = None;
            match panel {
                BrowserLeftPanel::QuickAccess => {
                    state.browser_quick_access_open = !state.browser_quick_access_open;
                }
                BrowserLeftPanel::Explorer => {
                    state.browser_explorer_open = !state.browser_explorer_open;
                }
            }
            let sidebar_open = state.browser_quick_access_open || state.browser_explorer_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("browser.sidebar".to_owned(), sidebar_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleBrowserRight(panel) => {
            state.active_menu = None;
            toggle_browser_right_panel(state, panel)
        }
        Message::PanelPopupToggleEditorTools => {
            state.active_menu = None;
            let next = !state.snapshot.meta.layout.panel("editor.tools");
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), next);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::PanelPopupToggleEditorPanel(panel) => {
            state.active_menu = None;
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_open = !state.editor_history_open;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_open = !state.editor_layers_open;
                }
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::BeginDragViewerPanel(panel) => {
            state.viewer_right_panels.begin_drag(panel);
            Task::none()
        }
        Message::HoverViewerPanelDrop(panel) => {
            state.viewer_right_panels.hover_drop(panel);
            Task::none()
        }
        Message::DropViewerPanel(target) => {
            if state.viewer_right_panels.commit_drop(Some(target)) {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::SwapViewerRightPanels => {
            if state.viewer_right_panels.swap_pair() {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ToggleViewerPanelCollapse(panel) => {
            state.context_menu = None;
            match panel {
                ViewerRightPanel::ViewAdjustment => {
                    state.viewer_adjustments_open = !state.viewer_adjustments_open;
                    state.viewer_color_popover_open = state.viewer_adjustments_open;
                }
                ViewerRightPanel::Info => {
                    state.viewer_info_open = !state.viewer_info_open;
                }
                ViewerRightPanel::Translate => {
                    state.viewer_translate.active = !state.viewer_translate.active;
                }
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseViewerPanel(panel) => {
            state.context_menu = None;
            match panel {
                ViewerRightPanel::ViewAdjustment => {
                    state.viewer_adjustments_open = false;
                    state.viewer_color_popover_open = false;
                }
                ViewerRightPanel::Info => {
                    state.viewer_info_open = false;
                }
                ViewerRightPanel::Translate => {
                    return update(
                        state,
                        Message::ViewerTranslate(ViewerMessage::CloseTranslateOverlay),
                    );
                }
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SelectEditorToolTab(tab) => {
            state.editor_tool_tab = tab;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorToolsVisible => {
            state.active_menu = None;
            let next = !state.snapshot.meta.layout.panel("editor.tools");
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), next);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseEditorToolsPanel => {
            state.context_menu = None;
            let layout_task = commit_layout_with(state, |layout| {
                layout.panels.insert("editor.tools".to_owned(), false);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::CloseEditorPanel(panel) => {
            state.context_menu = None;
            match panel {
                EditorRightPanel::ToolOptionsHistory => state.editor_history_open = false,
                EditorRightPanel::Layers => state.editor_layers_open = false,
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorPanelVisible(panel) => {
            state.active_menu = None;
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_open = !state.editor_history_open;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_open = !state.editor_layers_open;
                }
            }
            let inspector_open = state.editor_history_open || state.editor_layers_open;
            let layout_task = commit_layout_with(state, |layout| {
                layout
                    .panels
                    .insert("editor.inspector".to_owned(), inspector_open);
            });
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::ToggleEditorPanelCollapse(panel) => {
            match panel {
                EditorRightPanel::ToolOptionsHistory => {
                    state.editor_history_collapsed = !state.editor_history_collapsed;
                }
                EditorRightPanel::Layers => {
                    state.editor_layers_collapsed = !state.editor_layers_collapsed;
                }
            }
            sync_ui_tree_task(state)
        }
        Message::BeginDragEditorPanel(panel) => {
            state.editor_panels.begin_drag(panel);
            sync_editor_panel_compat(state);
            Task::none()
        }
        Message::HoverEditorPanelDrop(panel) => {
            state.editor_panels.hover_drop(panel);
            sync_editor_panel_compat(state);
            Task::none()
        }
        Message::DropEditorPanel(target) => {
            let changed = state.editor_panels.commit_drop(Some(target));
            sync_editor_panel_compat(state);
            if !changed {
                return Task::none();
            }
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SwapEditorPanelOrder => {
            if state.editor_panels.swap_pair() {
                sync_editor_panel_compat(state);
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::ViewerFrameLoaded(frame) => {
            if let Some(ref f) = frame
                && let Some(path) = f.key.strip_prefix("viewer:")
            {
                state
                    .image_sizes
                    .entry(path.to_owned())
                    .or_insert((f.width, f.height));
            }
            state.viewer_frame = frame;
            Task::none()
        }
        Message::ViewerPairFrameLoaded(frame) => {
            if let Some(ref f) = frame
                && let Some(path) = f.key.strip_prefix("viewer:")
            {
                state
                    .image_sizes
                    .entry(path.to_owned())
                    .or_insert((f.width, f.height));
            }
            state.viewer_pair_frame = frame;
            Task::none()
        }
        Message::EditorFrameLoaded(frame) => {
            state.editor_histogram = frame
                .as_ref()
                .map(|f| (f.revision, compute_luminance_histogram(f)));
            state.editor_frame = frame;
            Task::none()
        }
        Message::EditorOriginalFrameLoaded(frame) => {
            state.editor_original_frame = frame;
            Task::none()
        }
        Message::EditorSelectionMaskLoaded(mask) => {
            state.editor_selection_mask = mask;
            Task::none()
        }
        Message::MarchingAntsTick => {
            state.marching_ants_phase = (state.marching_ants_phase + 0.08) % 1.0;
            Task::none()
        }
        Message::EditorThumbnailsLoaded(_rev, thumbs) => {
            let mut layer_map = HashMap::with_capacity(thumbs.len());
            let mut mask_map = HashMap::new();
            for item in thumbs {
                layer_map.insert(
                    item.layer_id,
                    iced_image::Handle::from_rgba(item.width, item.height, item.layer_rgba),
                );
                if let Some(mask_rgba) = item.mask_rgba {
                    mask_map.insert(
                        item.layer_id,
                        iced_image::Handle::from_rgba(item.width, item.height, mask_rgba),
                    );
                }
            }
            state.editor_layer_thumbs = layer_map;
            state.editor_mask_thumbs = mask_map;
            Task::none()
        }
        Message::WindowCursorMoved(pt) => {
            state.last_cursor = Some(pt);
            if state.dialog_manager.is_dragging() {
                state.dialog_manager.update_gesture(pt, state.window_size);
            }
            if let Some(drag) = &mut state.webtoon_marquee_drag {
                let lx = (drag.start_local[0] + (pt.x - drag.start_cursor.x))
                    .clamp(0.0, drag.disp_w.max(1.0));
                let ly = (drag.start_local[1] + (pt.y - drag.start_cursor.y))
                    .clamp(0.0, drag.disp_h.max(1.0));
                drag.cur_local = [lx, ly];
                let x = drag.start_local[0].min(lx);
                let y = drag.start_local[1].min(ly);
                let w = (drag.start_local[0] - lx).abs();
                let h = (drag.start_local[1] - ly).abs();
                state.viewer_translate.webtoon_marquee = Some((drag.path.clone(), [x, y, w, h]));
            }
            match &mut state.translate_drag {
                TranslateDragTarget::None => {}
                TranslateDragTarget::Bubble {
                    page_path,
                    idx,
                    start_cursor,
                    dx_px,
                    dy_px,
                } => {
                    *dx_px = pt.x - start_cursor.x;
                    *dy_px = pt.y - start_cursor.y;
                    state.viewer_translate.active_drag_delta = Some((*idx, [*dx_px, *dy_px]));
                    state.viewer_translate.active_drag_page = page_path.clone();
                }
                TranslateDragTarget::TopBar {
                    start_cursor,
                    base_offset,
                } => {
                    state.viewer_translate.top_bar_offset = [
                        base_offset[0] + (pt.x - start_cursor.x),
                        base_offset[1] + (pt.y - start_cursor.y),
                    ];
                }
                TranslateDragTarget::ResultCard {
                    start_cursor,
                    base_offset,
                } => {
                    state.viewer_translate.result_card_offset = [
                        base_offset[0] + (pt.x - start_cursor.x),
                        base_offset[1] + (pt.y - start_cursor.y),
                    ];
                }
            }
            Task::none()
        }
        Message::WindowMouseUp => {
            if state.dialog_manager.is_dragging() {
                state.dialog_manager.end_gesture();
            }
            let drag = std::mem::take(&mut state.translate_drag);
            state.viewer_translate.active_drag_delta = None;
            state.viewer_translate.active_drag_page = None;
            if let TranslateDragTarget::Bubble {
                page_path,
                idx,
                dx_px,
                dy_px,
                ..
            } = drag
            {
                commit_translate_bubble_drag(state, page_path.as_deref(), idx, dx_px, dy_px);
            }
            if let Some(wb_drag) = state.webtoon_marquee_drag.take() {
                state.viewer_translate.webtoon_marquee = None;
                let rx = wb_drag.start_local[0].min(wb_drag.cur_local[0]);
                let ry = wb_drag.start_local[1].min(wb_drag.cur_local[1]);
                let rw = (wb_drag.start_local[0] - wb_drag.cur_local[0]).abs();
                let rh = (wb_drag.start_local[1] - wb_drag.cur_local[1]).abs();
                if rw >= 6.0 && rh >= 6.0 {
                    let (img_w, img_h) = state
                        .image_sizes
                        .get(&wb_drag.path)
                        .copied()
                        .unwrap_or((wb_drag.disp_w.round() as u32, wb_drag.disp_h.round() as u32));
                    let iw = img_w.max(1) as f32;
                    let ih = img_h.max(1) as f32;
                    let dw = wb_drag.disp_w.max(1.0);
                    let dh = wb_drag.disp_h.max(1.0);
                    // Account for `ContentFit::Contain` letterboxing inside (dw, dh)
                    let scale = (dw / iw).min(dh / ih);
                    let fit_w = (iw * scale).max(1.0);
                    let fit_h = (ih * scale).max(1.0);
                    let off_x = (dw - fit_w) * 0.5;
                    let off_y = (dh - fit_h) * 0.5;
                    let x0 = (rx - off_x).clamp(0.0, fit_w);
                    let y0 = (ry - off_y).clamp(0.0, fit_h);
                    let x1 = (rx + rw - off_x).clamp(0.0, fit_w);
                    let y1 = (ry + rh - off_y).clamp(0.0, fit_h);
                    if x1 - x0 >= 2.0 && y1 - y0 >= 2.0 {
                        let sx = iw / fit_w;
                        let sy = ih / fit_h;
                        let region = [
                            (x0 * sx).round().clamp(0.0, iw) as i32,
                            (y0 * sy).round().clamp(0.0, ih) as i32,
                            ((x1 - x0) * sx).round().max(8.0) as i32,
                            ((y1 - y0) * sy).round().max(8.0) as i32,
                        ];
                        return dispatch_viewer_translate_region(state, wb_drag.path, region);
                    }
                }
            }
            let mut order_changed = false;
            if state.browser_left_panels.is_dragging()
                && state.browser_left_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.browser_right_panels.is_dragging()
                && state.browser_right_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.viewer_right_panels.is_dragging()
                && state.viewer_right_panels.commit_drop(None)
            {
                order_changed = true;
            }
            if state.editor_panels.is_dragging() {
                if state.editor_panels.commit_drop(None) {
                    order_changed = true;
                }
                sync_editor_panel_compat(state);
            }
            if order_changed {
                let layout_task = commit_layout_with(state, |_| {});
                return Task::batch([layout_task, sync_ui_tree_task(state)]);
            }
            Task::none()
        }
        Message::WindowUnfocused => {
            state.viewer_translate.space_pan_held = false;
            state.translate_drag = TranslateDragTarget::None;
            state.viewer_translate.active_drag_delta = None;
            state.viewer_translate.active_drag_page = None;
            state.webtoon_marquee_drag = None;
            state.viewer_translate.webtoon_marquee = None;
            Task::none()
        }
        Message::DialogEvent(ev) => {
            state.dialog_manager.handle_event(ev, state.window_size);
            Task::none()
        }
        Message::OpenViewerContextMenu { path, pos } => {
            state.active_menu = None;
            let resolved_pos = pos
                .or_else(|| state.last_cursor.map(|p| [p.x, p.y]))
                .unwrap_or([
                    state.window_size.width * 0.5,
                    state.window_size.height * 0.5,
                ]);
            let resolved_path = path.or_else(|| {
                if state.viewer_mode == ViewerViewMode::Webtoon {
                    state
                        .webtoon_visible_index
                        .or(state.snapshot.meta.viewer_index)
                        .and_then(|i| state.snapshot.viewer_images.get(i).cloned())
                        .or_else(|| state.snapshot.meta.viewer_path.clone())
                } else {
                    state.snapshot.meta.viewer_path.clone()
                }
            });
            state.context_menu = Some(ActiveContextMenu::Viewer {
                x: resolved_pos[0],
                y: resolved_pos[1],
                target_path: resolved_path,
                zoom_submenu_open: false,
                plugins_submenu_open: false,
                active_plugin_submenu: None,
            });
            sync_ui_tree_task(state)
        }
        Message::OpenLayerContextMenu { layer_id } => {
            state.active_menu = None;
            if state.snapshot.meta.editor.is_some() {
                let p = state.last_cursor.unwrap_or(Point::new(
                    (state.window_size.width - 220.0).max(20.0),
                    220.0,
                ));
                state.context_menu = Some(ActiveContextMenu::EditorLayer {
                    x: p.x,
                    y: p.y,
                    layer_id,
                });
                return sync_ui_tree_task(state);
            }
            Task::none()
        }
        Message::CloseContextMenu => {
            if state.context_menu.take().is_some() {
                sync_ui_tree_task(state)
            } else {
                Task::none()
            }
        }
        Message::ToggleContextPluginsSubmenu => {
            match state.context_menu.as_mut() {
                Some(ActiveContextMenu::Viewer {
                    zoom_submenu_open,
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                }) => {
                    *plugins_submenu_open = !*plugins_submenu_open;
                    if *plugins_submenu_open {
                        *zoom_submenu_open = false;
                    } else {
                        *active_plugin_submenu = None;
                    }
                }
                Some(ActiveContextMenu::BrowserEntry {
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                }) => {
                    *plugins_submenu_open = !*plugins_submenu_open;
                    if !*plugins_submenu_open {
                        *active_plugin_submenu = None;
                    }
                }
                _ => {}
            }
            Task::none()
        }
        Message::OpenContextPluginsSubmenu => {
            match state.context_menu.as_mut() {
                Some(ActiveContextMenu::Viewer {
                    zoom_submenu_open,
                    plugins_submenu_open,
                    ..
                }) => {
                    *plugins_submenu_open = true;
                    *zoom_submenu_open = false;
                }
                Some(ActiveContextMenu::BrowserEntry {
                    plugins_submenu_open,
                    ..
                }) => {
                    *plugins_submenu_open = true;
                }
                _ => {}
            }
            Task::none()
        }
        Message::ToggleContextZoomSubmenu => {
            if let Some(ActiveContextMenu::Viewer {
                zoom_submenu_open,
                plugins_submenu_open,
                active_plugin_submenu,
                ..
            }) = state.context_menu.as_mut()
            {
                *zoom_submenu_open = !*zoom_submenu_open;
                if *zoom_submenu_open {
                    *plugins_submenu_open = false;
                    *active_plugin_submenu = None;
                }
            }
            Task::none()
        }
        Message::OpenContextZoomSubmenu => {
            if let Some(ActiveContextMenu::Viewer {
                zoom_submenu_open,
                plugins_submenu_open,
                active_plugin_submenu,
                ..
            }) = state.context_menu.as_mut()
            {
                *zoom_submenu_open = true;
                *plugins_submenu_open = false;
                *active_plugin_submenu = None;
            }
            Task::none()
        }
        Message::ToggleContextPluginGroup(plugin_id) => {
            match state.context_menu.as_mut() {
                Some(ActiveContextMenu::Viewer {
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                })
                | Some(ActiveContextMenu::BrowserEntry {
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                }) => {
                    *plugins_submenu_open = true;
                    if active_plugin_submenu.as_deref() == Some(plugin_id.as_str()) {
                        *active_plugin_submenu = None;
                    } else {
                        *active_plugin_submenu = Some(plugin_id);
                    }
                }
                _ => {}
            }
            Task::none()
        }
        Message::OpenContextPluginGroup(plugin_id) => {
            match state.context_menu.as_mut() {
                Some(ActiveContextMenu::Viewer {
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                })
                | Some(ActiveContextMenu::BrowserEntry {
                    plugins_submenu_open,
                    active_plugin_submenu,
                    ..
                }) => {
                    *plugins_submenu_open = true;
                    *active_plugin_submenu = Some(plugin_id);
                }
                _ => {}
            }
            Task::none()
        }
        Message::ContextMenuDispatch(cmd) => {
            state.context_menu = None;
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::CopyImageToClipboard(target_path) => {
            state.context_menu = None;
            state.browser_file_clipboard.clear();
            let frame_rgba =
                if state.snapshot.meta.workspace == Workspace::Editor && target_path.is_none() {
                    state
                        .editor_frame
                        .as_ref()
                        .map(|f| (f.width, f.height, f.rgba.clone()))
                } else if let Some(ref p) = target_path {
                    let viewer_key = format!("viewer:{p}");
                    state
                        .viewer_frame
                        .as_ref()
                        .filter(|f| f.key == viewer_key)
                        .map(|f| (f.width, f.height, f.rgba.clone()))
                } else {
                    None
                };
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        copy_image_png_to_clipboard(target_path.as_deref(), frame_rgba)
                    })
                    .await
                    .unwrap_or(false)
                },
                Message::CopyImageCompleted,
            )
        }
        Message::CopyImageCompleted(ok) => {
            let lang = state.snapshot.meta.language;
            if ok {
                show_toast(state, tr(lang, "toast-image-copied"), ToastKind::Success)
            } else {
                show_toast(
                    state,
                    tr(lang, "toast-clipboard-unavailable"),
                    ToastKind::Error,
                )
            }
        }
        Message::RevealInFolder(path) => {
            state.context_menu = None;
            reveal_path_in_folder(&path);
            Task::none()
        }
        Message::ViewerCanvas(pane_path, ev) => {
            update_canvas::handle_viewer_canvas_event(state, pane_path, ev)
        }
        Message::DockSplitterDrag(kind, val) => {
            state.dock_drag.set_drag(kind, val);
            Task::none()
        }
        Message::DockSplitterRelease(kind, val) => {
            if let Some(next_layout) =
                state
                    .dock_drag
                    .commit_release(&state.snapshot.meta.layout, kind, val)
            {
                return commit_layout_with(state, |layout| {
                    layout.editor_right_width = next_layout.editor_right_width;
                    layout.editor_right_split_pct = next_layout.editor_right_split_pct;
                });
            }
            Task::none()
        }
        Message::BrowserSplitterDrag(w) => {
            state.browser_left_width_drag = Some(w.clamp(160.0, 520.0));
            Task::none()
        }
        Message::BrowserSplitterRelease(w) => {
            state.browser_left_width_drag = None;
            let clamped = w.round().clamp(160.0, 520.0) as u32;
            if state.snapshot.meta.layout.left_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.left_width = clamped;
            })
        }
        Message::BrowserRightSplitterDrag(w) => {
            state.browser_right_width_drag = Some(w.clamp(200.0, 600.0));
            Task::none()
        }
        Message::BrowserRightSplitterRelease(w) => {
            state.browser_right_width_drag = None;
            let clamped = w.round().clamp(200.0, 600.0) as u32;
            if state.snapshot.meta.layout.right_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.right_width = clamped;
            })
        }
        Message::ViewerSplitterDrag(w) => {
            state.viewer_right_width_preview = Some(w.clamp(200.0, 600.0));
            Task::none()
        }
        Message::ViewerSplitterRelease(w) => {
            state.viewer_right_width_preview = None;
            let clamped = w.round().clamp(200.0, 600.0) as u32;
            if state.snapshot.meta.layout.right_width == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.right_width = clamped;
            })
        }
        Message::ViewerRightSplitDrag(pct) => {
            state.viewer_right_split_preview = Some(pct.clamp(20.0, 80.0));
            Task::none()
        }
        Message::ViewerRightSplitRelease(pct) => {
            state.viewer_right_split_preview = None;
            let clamped = pct.round().clamp(20.0, 80.0) as u16;
            if state.snapshot.meta.layout.viewer_right_split_pct == clamped {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.viewer_right_split_pct = clamped;
            })
        }
        Message::SetBrowserSortField(field) => {
            state.active_menu = None;
            commit_layout_with(state, |layout| {
                if layout.browser_sort_field == field {
                    layout.browser_sort_order = match layout.browser_sort_order {
                        SortOrder::Asc => SortOrder::Desc,
                        SortOrder::Desc => SortOrder::Asc,
                    };
                } else {
                    layout.browser_sort_field = field;
                }
            })
        }
        Message::SetBrowserSortOrder(order) => {
            state.active_menu = None;
            if state.snapshot.meta.layout.browser_sort_order == order {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_sort_order = order;
            })
        }
        Message::SetBrowserThumbSize(size) => {
            if state.snapshot.meta.layout.browser_thumb_size == size {
                return Task::none();
            }
            commit_layout_with(state, |layout| {
                layout.browser_thumb_size = size;
            })
        }
        Message::OpenBatchDialog(kind) => {
            state.active_menu = None;
            state.context_menu = None;
            let meta = &state.snapshot.meta;
            let sel_images: Vec<&str> = meta
                .selection
                .iter()
                .filter(|p| {
                    state
                        .snapshot
                        .entries
                        .iter()
                        .any(|e| &e.path == *p && e.is_image)
                })
                .map(String::as_str)
                .collect();
            let ref_dims = sel_images
                .first()
                .copied()
                .or_else(|| {
                    state
                        .snapshot
                        .entries
                        .iter()
                        .find(|e| e.is_image)
                        .map(|e| e.path.as_str())
                })
                .and_then(|p| photon_image::probe_dimensions(Path::new(p)));
            state.dialog = Some(InAppDialog::Batch(BatchDialogState::new(
                kind,
                !sel_images.is_empty(),
                meta.current_folder.as_deref(),
                ref_dims,
            )));
            open_dialog_window(state, batch_dialog_id(kind));
            Task::none()
        }
        Message::BatchDialog(msg) => {
            if matches!(msg, BatchDialogMsg::PickInputFolder) {
                let title = tr(state.snapshot.meta.language, "batch-source-custom");
                let start = match &state.dialog {
                    Some(InAppDialog::Batch(b)) if !b.input_folder.is_empty() => {
                        Some(PathBuf::from(&b.input_folder))
                    }
                    _ => state
                        .snapshot
                        .meta
                        .current_folder
                        .as_deref()
                        .map(PathBuf::from),
                };
                return Task::perform(
                    async move { photon_dialogs::pick_folder(&title, start).await },
                    Message::BatchInputFolderPicked,
                );
            }
            if matches!(msg, BatchDialogMsg::PickOutputFolder) {
                let title = tr(state.snapshot.meta.language, "batch-output-folder");
                let start = match &state.dialog {
                    Some(InAppDialog::Batch(b)) if !b.output_folder.is_empty() => {
                        Some(PathBuf::from(&b.output_folder))
                    }
                    _ => state
                        .snapshot
                        .meta
                        .current_folder
                        .as_deref()
                        .map(PathBuf::from),
                };
                return Task::perform(
                    async move { photon_dialogs::pick_folder(&title, start).await },
                    Message::BatchOutputFolderPicked,
                );
            }
            if matches!(msg, BatchDialogMsg::Confirm) {
                let (cmd_res, dialog_id) = match &mut state.dialog {
                    Some(InAppDialog::Batch(b)) => {
                        (b.build_command(&state.snapshot), batch_dialog_id(b.kind))
                    }
                    _ => return Task::none(),
                };
                match cmd_res {
                    Ok(cmd) => {
                        state.dialog = None;
                        state.dialog_manager.close(&dialog_id);
                        return dispatch_to_core(state.core.clone(), cmd);
                    }
                    Err(err_key) => {
                        if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                            b.error = Some(tr(state.snapshot.meta.language, &err_key));
                        }
                        return Task::none();
                    }
                }
            }
            if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                b.apply_msg(msg);
            }
            Task::none()
        }
        Message::BatchInputFolderPicked(Some(path)) => {
            if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                b.apply_msg(BatchDialogMsg::InputFolderPicked(Some(
                    path.to_string_lossy().into_owned(),
                )));
            }
            Task::none()
        }
        Message::BatchInputFolderPicked(None) => Task::none(),
        Message::BatchOutputFolderPicked(Some(path)) => {
            if let Some(InAppDialog::Batch(b)) = &mut state.dialog {
                b.apply_msg(BatchDialogMsg::OutputFolderPicked(Some(
                    path.to_string_lossy().into_owned(),
                )));
            }
            Task::none()
        }
        Message::BatchOutputFolderPicked(None) => Task::none(),
        Message::OpenBrowserContextMenu { path, x, y } => {
            state.active_menu = None;
            let pos = state.last_cursor.unwrap_or(Point::new(x, y));
            state.context_menu = Some(ActiveContextMenu::BrowserEntry {
                x: pos.x,
                y: pos.y,
                path,
                plugins_submenu_open: false,
                active_plugin_submenu: None,
            });
            sync_ui_tree_task(state)
        }
        Message::OpenQuickAccessContextMenu { path, x, y } => {
            state.active_menu = None;
            let pos = state.last_cursor.unwrap_or(Point::new(x, y));
            state.context_menu = Some(ActiveContextMenu::BrowserQuickAccess {
                x: pos.x,
                y: pos.y,
                path,
            });
            sync_ui_tree_task(state)
        }
        Message::BrowserCopyPath(path) => {
            state.context_menu = None;
            let toast_task = show_toast(
                state,
                tr(state.snapshot.meta.language, "toast-path-copied"),
                ToastKind::Success,
            );
            Task::batch([iced::clipboard::write(path), toast_task])
        }
        Message::BrowserStartRename(path) => {
            state.context_menu = None;
            let name = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_owned();
            state.dialog = Some(InAppDialog::RenameItem {
                path,
                name,
                error: None,
            });
            open_dialog_window(state, DesktopDialogId::RenameItem);
            Task::none()
        }
        Message::BrowserRenameChanged(val) => {
            if let Some(InAppDialog::RenameItem { name, error, .. }) = &mut state.dialog {
                *name = val;
                *error = None;
            }
            Task::none()
        }
        Message::BrowserConfirmRename => {
            let Some(InAppDialog::RenameItem { path, name, .. }) = state.dialog.clone() else {
                return Task::none();
            };
            let clean = name.trim().to_owned();
            if clean.is_empty() || clean.contains('/') || clean.contains('\\') {
                if let Some(InAppDialog::RenameItem { error, .. }) = &mut state.dialog {
                    *error = Some(tr(state.snapshot.meta.language, "dialog-error-title"));
                }
                return Task::none();
            }
            state.dialog = None;
            state.dialog_manager.close(&DesktopDialogId::RenameItem);
            dispatch_to_core(
                state.core.clone(),
                Command::RenamePath {
                    path,
                    new_name: clean,
                },
            )
        }
        Message::BrowserCopyTo(paths) => {
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "action-copy-to");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                move |dest| Message::BrowserCopyMoveDestPicked {
                    is_move: false,
                    paths,
                    dest,
                },
            )
        }
        Message::BrowserMoveTo(paths) => {
            state.context_menu = None;
            let title = tr(state.snapshot.meta.language, "action-move-to");
            let start = state
                .snapshot
                .meta
                .current_folder
                .as_deref()
                .map(PathBuf::from);
            Task::perform(
                async move { photon_dialogs::pick_folder(&title, start).await },
                move |dest| Message::BrowserCopyMoveDestPicked {
                    is_move: true,
                    paths,
                    dest,
                },
            )
        }
        Message::BrowserCopyMoveDestPicked {
            is_move,
            paths,
            dest: Some(dest),
        } => {
            let destination = dest.to_string_lossy().into_owned();
            let cmd = if is_move {
                Command::MoveTo { paths, destination }
            } else {
                Command::CopyTo { paths, destination }
            };
            dispatch_to_core(state.core.clone(), cmd)
        }
        Message::BrowserCopyMoveDestPicked { dest: None, .. } => Task::none(),
        Message::BrowserConfirmDeletePermanently(paths) => {
            state.context_menu = None;
            state.dialog = Some(InAppDialog::ConfirmDeletePermanently { paths });
            state
                .dialog_manager
                .bring_to_front(&DesktopDialogId::ConfirmDeletePermanently);
            Task::none()
        }
        Message::BrowserExecuteDeletePermanently(paths) => {
            state.dialog = None;
            state
                .dialog_manager
                .close(&DesktopDialogId::ConfirmDeletePermanently);
            dispatch_to_core(state.core.clone(), Command::DeletePermanently { paths })
        }
        Message::ToggleViewerColorPopover => {
            state.active_menu = None;
            state.context_menu = None;
            state.viewer_adjustments_open = !state.viewer_adjustments_open;
            state.viewer_color_popover_open = state.viewer_adjustments_open;
            let layout_task = commit_layout_with(state, |_| {});
            Task::batch([layout_task, sync_ui_tree_task(state)])
        }
        Message::SetViewerAdjExposure(v) => {
            state.viewer_adj.exposure = v;
            Task::none()
        }
        Message::SetViewerAdjGamma(v) => {
            state.viewer_adj.gamma = v.clamp(0.2, 3.0);
            Task::none()
        }
        Message::SetViewerAdjBrightness(v) => {
            state.viewer_adj.brightness = v;
            Task::none()
        }
        Message::SetViewerAdjContrast(v) => {
            state.viewer_adj.contrast = v;
            Task::none()
        }
        Message::SetViewerAdjSaturation(v) => {
            state.viewer_adj.saturation = v;
            Task::none()
        }
        Message::SetViewerAdjHighlights(v) => {
            state.viewer_adj.highlights = v;
            Task::none()
        }
        Message::SetViewerAdjShadows(v) => {
            state.viewer_adj.shadows = v;
            Task::none()
        }
        Message::ResetViewerAdj => {
            state.viewer_adj = ColorAdjustments::default();
            Task::none()
        }
        msg @ (Message::SetBrightness(..)
        | Message::SetContrast(..)
        | Message::SetSaturation(..)
        | Message::SetExposure(..)
        | Message::SetGamma(..)
        | Message::SetHighlights(..)
        | Message::SetShadows(..)
        | Message::ApplyColorAdjustments
        | Message::EditorToggleApplyToSelection(..)
        | Message::DenoiseDialogSelectionOnlyChanged(..)
        | Message::EditorResetPendingAdjustments
        | Message::EditorOpenAdjustPopup(..)
        | Message::EditorCloseAdjustPopup { .. }
        | Message::EditorCopyPixels { .. }
        | Message::EditorPastePixels { .. }
        | Message::EditorPasteResolved { .. }
        | Message::EditorSetLiquifyMode(..)
        | Message::EditorSetLiquifyRadius(..)
        | Message::EditorSetLiquifyStrength(..)
        | Message::EditorSelectTool(..)
        | Message::EditorSetSelectOp(..)
        | Message::EditorSetSelectFeather(..)
        | Message::EditorToggleSelectAntiAlias
        | Message::EditorSetWandTolerance(..)
        | Message::EditorToggleWandContiguous
        | Message::EditorToggleSelectSampleAll
        | Message::EditorSetQuickSelectRadius(..)
        | Message::EditorSetBrushSmoothingMode(..)
        | Message::EditorSetStabilizerDistance(..)
        | Message::EditorSetShapeFillMode(..)
        | Message::EditorSetShapeStrokeWidth(..)
        | Message::EditorSetFloodFillTolerance(..)
        | Message::EditorToggleFloodFillContiguous
        | Message::EditorSetGradientKind(..)
        | Message::EditorSetGradientColorMode(..)
        | Message::EditorCycleCompareMode
        | Message::EditorSetCompareMode(..)
        | Message::EditorSwapColors
        | Message::EditorResetColors
        | Message::OpenColorPicker(..)
        | Message::ColorPickerSetRgb(..)
        | Message::ColorPickerPickScreen
        | Message::ColorPickerScreenPicked(..)
        | Message::ColorPickerHexChanged(..)
        | Message::ColorPickerToggleTransparent(..)
        | Message::ResetViewportBackground
        | Message::ConfirmColorPicker
        | Message::EditorSetBrushRadius(..)
        | Message::EditorSetBrushOpacity(..)
        | Message::EditorSetBrushHardness(..)
        | Message::EditorSetBrushFlow(..)
        | Message::EditorSetBrushSpacing(..)
        | Message::EditorSetBrushTip(..)
        | Message::EditorSetApplyTarget(..)
        | Message::EditorSetHsvHue(..)
        | Message::EditorSetHsvSat(..)
        | Message::EditorSetHsvVal(..)
        | Message::EditorApplyHsv
        | Message::EditorSetBalanceZone(..)
        | Message::EditorSetBalanceAxis(..)
        | Message::EditorTogglePreserveLuminosity(..)
        | Message::EditorApplyColorBalance
        | Message::EditorSelectCurvesChannel(..)
        | Message::EditorCurvesChanged(..)
        | Message::EditorResetActiveCurvesChannel
        | Message::EditorApplyCurves
        | Message::EditorSetFloodFillOpacity(..)
        | Message::EditorToggleFloodFillSampleAll
        | Message::EditorSetGradientOpacity(..)
        | Message::EditorToggleGradientReverse
        | Message::EditorToggleGradientDither
        | Message::EditorSetGradientRepeat(..)
        | Message::EditorSetGradientBlendMode(..)
        | Message::EditorToggleGradientInteractive
        | Message::EditorApplyGradientPreset(..)
        | Message::EditorSelectGradientStop(..)
        | Message::EditorAddGradientStop(..)
        | Message::EditorRemoveSelectedGradientStop
        | Message::EditorSetGradientStopPos(..)
        | Message::EditorToggleGradientStopAlpha(..)
        | Message::EditorSetGradientStopFg(..)
        | Message::EditorApplyGradientDraft
        | Message::EditorCancelGradientDraft
        | Message::EditorSetHealingMode(..)
        | Message::EditorSetHealingRadius(..)
        | Message::EditorSetHealingHardness(..)
        | Message::EditorSetHealingStrength(..)
        | Message::EditorToggleHealingSampleAll
        | Message::EditorSetHealingSource(..)
        | Message::EditorSetDenoiseStrength(..)
        | Message::EditorSetDenoiseDetail(..)
        | Message::OpenDenoiseDialog
        | Message::DenoiseDialogStrengthChanged(..)
        | Message::DenoiseDialogDetailChanged(..)
        | Message::DenoiseDialogTargetChanged(..)
        | Message::ConfirmDenoiseDialog
        | Message::EditorSetTransformSpec(..)
        | Message::EditorTransformFramesLoaded(..)
        | Message::EditorTransformCommit
        | Message::EditorTransformCancel
        | Message::EditorCutPixels
        | Message::EditorPixelsCopiedSyncClipboard(..)
        | Message::RecordSyncedClipboardPng(..)
        | Message::EditorPasteCompleted(..)
        | Message::SmartPasteSetMode(..)
        | Message::SmartPasteSetWidth(..)
        | Message::SmartPasteSetHeight(..)
        | Message::SmartPasteSetOffsetX(..)
        | Message::SmartPasteSetOffsetY(..)
        | Message::SmartPasteApply
        | Message::SmartPasteDismiss
        | Message::EditorSetRotateAngle(..)
        | Message::EditorApplyRotateAngle
        | Message::OpenExportDialog
        | Message::PickExportDialog
        | Message::ExportPathPicked(..)
        | Message::ExportPathChanged(..)
        | Message::ExportQualityChanged(..)
        | Message::ExportMaxEdgeChanged(..)
        | Message::ExportMetadataToggled(..)
        | Message::ConfirmExport
        | Message::EditorStartLayerRename(..)
        | Message::EditorLayerRenameChanged(..)
        | Message::EditorCommitLayerRename
        | Message::EditorCancelLayerRename) => update_editor::handle_editor_message(state, msg),
        Message::ViewerAdjOpenInEditor => {
            state.context_menu = None;
            state.viewer_color_popover_open = false;
            if let Some(path) = state.snapshot.meta.viewer_path.clone() {
                if !is_standalone_viewer() {
                    state.transfer_adj_to_editor = Some(state.viewer_adj);
                }
                return open_in_editor_or_forward(state, path);
            }
            Task::none()
        }
    }
}

pub fn view(state: &PhotonGui, window_id: window::Id) -> Element<'_, Message> {
    photon_ui::set_active_typography(
        state.snapshot.meta.layout.font_scale(),
        iced_font_for(state.snapshot.meta.layout.display_font),
    );
    let role = state
        .windows
        .get(&window_id)
        .copied()
        .unwrap_or(WindowRole::Main);
    let lang = state.snapshot.meta.language;
    let ws = if role == WindowRole::AuxiliaryViewer {
        Workspace::Viewer
    } else {
        state.snapshot.meta.workspace
    };

    let top_bar = build_toolbar(state, role, ws);
    let body: Element<'_, Message> = match ws {
        Workspace::Browser => build_browser_view(state.browser_view_state()).map(Message::Browser),
        Workspace::Viewer => build_viewer_view(state.viewer_view_state(true)).map(Message::Viewer),
        Workspace::Editor => build_editor_view(state.editor_view_state()).map(Message::Editor),
    };

    let status_left = match ws {
        Workspace::Viewer => {
            viewer_status_summary(state.viewer_view_state(role == WindowRole::Main), lang)
        }
        Workspace::Editor => editor_status_summary(state, lang),
        Workspace::Browser => {
            let s = tr_ui_text(lang, &state.snapshot.meta.status);
            if s.is_empty() { tr(lang, "ready") } else { s }
        }
    };
    let backend = GPU_BACKEND_NAME.get().map(String::as_str).unwrap_or("wgpu");
    let right_meta = if ws == Workspace::Viewer {
        let counter_prefix =
            viewer_counter_summary(state.viewer_view_state(role == WindowRole::Main))
                .map(|c| format!("{c} · "))
                .unwrap_or_default();
        if state.snapshot.meta.safe_mode {
            format!("{counter_prefix}{}", tr(lang, "label-safe-mode"))
        } else {
            let mode_label = match state.viewer_mode {
                ViewerViewMode::Single => tr(lang, "action-viewer-single"),
                ViewerViewMode::Double => tr(lang, "action-viewer-double"),
                ViewerViewMode::Webtoon => tr(lang, "action-viewer-manhwa"),
            };
            if state.viewer_mode == ViewerViewMode::Webtoon {
                let fit_label = match state.viewer_webtoon_fit_mode {
                    WebtoonFitMode::Auto => tr(lang, "action-fit-auto"),
                    WebtoonFitMode::Width => tr(lang, "action-fit-width"),
                    WebtoonFitMode::Height => tr(lang, "action-fit-height"),
                    WebtoonFitMode::Original => tr(lang, "action-fit-original"),
                };
                format!(
                    "{counter_prefix}{:.0}% ({fit_label}) · {mode_label} · {backend}",
                    state.webtoon_zoom * 100.0
                )
            } else {
                format!(
                    "{counter_prefix}{:.0}% · {mode_label} · {backend}",
                    state.zoom * 100.0
                )
            }
        }
    } else if state.snapshot.meta.safe_mode {
        tr(lang, "label-safe-mode")
    } else if ws == Workspace::Editor {
        format!("{:.0}% · {backend}", state.zoom * 100.0)
    } else {
        backend.to_owned()
    };
    let status = status_bar(status_left, right_meta);
    let shell_col = column![top_bar, container(body).height(Fill).width(Fill), status];

    let shell = container(shell_col.height(Fill).width(Fill))
        .width(Fill)
        .height(Fill)
        .style(app_shell_style);

    let with_menu = match build_active_menu(state, role, ws) {
        Some((Some(left_offset), menu_el)) => {
            with_anchored_menu(shell, true, left_offset, 44.0, menu_el, Message::CloseMenu)
        }
        Some((None, menu_el)) => match state.active_menu {
            Some(ActiveMenu::WorkspaceSwitcher) => {
                with_top_right_menu_at(shell, true, 52.0, 52.0, menu_el, Message::CloseMenu)
            }
            Some(ActiveMenu::WorkspacePanels) => {
                with_top_right_menu_at(shell, true, 12.0, 52.0, menu_el, Message::CloseMenu)
            }
            Some(ActiveMenu::ReaderOptions) => with_top_right_menu_at(
                shell,
                true,
                viewer_reader_options_right_offset(state.viewer_view_state(true)),
                52.0,
                menu_el,
                Message::CloseMenu,
            ),
            Some(ActiveMenu::ViewerUtilities) => with_top_right_menu_at(
                shell,
                true,
                viewer_utilities_menu_right_offset(state.window_size.width),
                52.0,
                menu_el,
                Message::CloseMenu,
            ),
            Some(ActiveMenu::EditorAddLayer) => {
                let (est_h, add_layer_menu) =
                    build_editor_add_layer_menu(state.editor_view_state());
                let anchor = state.editor_add_layer_anchor.unwrap_or_else(|| {
                    Point::new(
                        (state.window_size.width - 240.0).max(16.0),
                        (state.window_size.height - est_h - 16.0).max(48.0),
                    )
                });
                let anchor_y = if anchor.y + est_h > state.window_size.height - 8.0 {
                    (anchor.y - est_h).max(8.0)
                } else {
                    anchor.y
                };
                with_context_menu(
                    shell,
                    true,
                    anchor.x,
                    anchor_y,
                    state.window_size,
                    224.0,
                    est_h,
                    add_layer_menu.map(Message::Editor),
                    Message::CloseMenu,
                )
            }
            Some(ActiveMenu::EditorToolFlyout(group)) => {
                let (_, flyout_el) = build_tool_group_flyout_menu(
                    &state.editor_ui,
                    state.snapshot.meta.language,
                    group,
                );
                let top_off = 104.0 + (group.row_index() as f32) * 36.0;
                with_anchored_menu(
                    shell,
                    true,
                    76.0,
                    top_off,
                    flyout_el.map(Message::Editor),
                    Message::CloseToolGroupFlyout,
                )
            }
            _ => with_top_right_menu(shell, true, menu_el, Message::CloseMenu),
        },
        None => iced::widget::stack![shell].width(Fill).height(Fill).into(),
    };

    let max_menu_h = (state.window_size.height - 16.0).max(160.0);
    fn clamp_menu_height<'a>(
        menu_el: Element<'a, Message>,
        est_h: f32,
        max_h: f32,
    ) -> (Element<'a, Message>, f32) {
        if est_h > max_h {
            (
                scrollable(menu_el).height(Length::Fixed(max_h)).into(),
                max_h,
            )
        } else {
            (menu_el, est_h)
        }
    }

    let with_ctx = match &state.context_menu {
        Some(ActiveContextMenu::BrowserEntry {
            x,
            y,
            path,
            plugins_submenu_open,
            active_plugin_submenu,
        }) => {
            let (raw_est_h, raw_menu, plugins_offset_y) =
                build_browser_context_menu_entries_with_offset(state.browser_view_state(), path);
            let (menu_el, est_h) =
                clamp_menu_height(raw_menu.map(Message::Browser), raw_est_h, max_menu_h);
            let mut submenus = Vec::new();
            if *plugins_submenu_open
                && let Some((sub_h, sub_el, group_offset_y)) =
                    build_browser_plugins_submenu_with_offset(
                        state.browser_view_state(),
                        path,
                        active_plugin_submenu.as_deref(),
                    )
            {
                let (sub_el, sub_h) =
                    clamp_menu_height(sub_el.map(Message::Browser), sub_h, max_menu_h);
                let mut sub2 = CascadingSubmenu::new(220.0, sub_h, sub_el);
                if let Some(off) = plugins_offset_y {
                    sub2 = sub2.parent_row_offset_y(off);
                }
                submenus.push(sub2);
                if let Some(plugin_id) = active_plugin_submenu.as_deref()
                    && let Some((grp_h, grp_el)) = build_browser_plugin_group_submenu(
                        state.browser_view_state(),
                        path,
                        plugin_id,
                    )
                {
                    let (grp_el, grp_h) =
                        clamp_menu_height(grp_el.map(Message::Browser), grp_h, max_menu_h);
                    let mut sub3 = CascadingSubmenu::new(220.0, grp_h, grp_el);
                    if let Some(off) = group_offset_y {
                        sub3 = sub3.parent_row_offset_y(off);
                    }
                    submenus.push(sub3);
                }
            }
            with_context_menu_and_cascading_submenus(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                225.0,
                est_h,
                menu_el,
                submenus,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::BrowserQuickAccess { x, y, path }) => {
            let (raw_est_h, raw_menu) =
                build_quick_access_context_menu(state.browser_view_state(), path);
            let (menu_el, est_h) =
                clamp_menu_height(raw_menu.map(Message::Browser), raw_est_h, max_menu_h);
            with_context_menu(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                220.0,
                est_h,
                menu_el,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::Viewer {
            x,
            y,
            target_path,
            zoom_submenu_open,
            plugins_submenu_open,
            active_plugin_submenu,
        }) => {
            let viewer_state = state.viewer_view_state(role == WindowRole::Main);
            let (raw_est_h, raw_menu, zoom_offset_y, plugins_offset_y) =
                build_viewer_context_menu_with_offset(viewer_state, target_path.as_deref());
            let (menu_el, est_h) =
                clamp_menu_height(raw_menu.map(Message::Viewer), raw_est_h, max_menu_h);
            let mut submenus = Vec::new();
            if *zoom_submenu_open {
                let (zoom_h, zoom_el) =
                    build_viewer_zoom_submenu(viewer_state, target_path.as_deref());
                let (zoom_el, zoom_h) =
                    clamp_menu_height(zoom_el.map(Message::Viewer), zoom_h, max_menu_h);
                let mut sub2 = CascadingSubmenu::new(210.0, zoom_h, zoom_el);
                if let Some(off) = zoom_offset_y {
                    sub2 = sub2.parent_row_offset_y(off);
                }
                submenus.push(sub2);
            } else if *plugins_submenu_open
                && let Some((sub_h, sub_el, group_offset_y)) =
                    build_viewer_plugins_submenu_with_offset(
                        viewer_state,
                        target_path.as_deref(),
                        active_plugin_submenu.as_deref(),
                    )
            {
                let (sub_el, sub_h) =
                    clamp_menu_height(sub_el.map(Message::Viewer), sub_h, max_menu_h);
                let mut sub2 = CascadingSubmenu::new(220.0, sub_h, sub_el);
                if let Some(off) = plugins_offset_y {
                    sub2 = sub2.parent_row_offset_y(off);
                }
                submenus.push(sub2);
                if let Some(plugin_id) = active_plugin_submenu.as_deref()
                    && let Some((grp_h, grp_el)) = build_viewer_plugin_group_submenu(
                        viewer_state,
                        target_path.as_deref(),
                        plugin_id,
                    )
                {
                    let (grp_el, grp_h) =
                        clamp_menu_height(grp_el.map(Message::Viewer), grp_h, max_menu_h);
                    let mut sub3 = CascadingSubmenu::new(220.0, grp_h, grp_el);
                    if let Some(off) = group_offset_y {
                        sub3 = sub3.parent_row_offset_y(off);
                    }
                    submenus.push(sub3);
                }
            }
            with_context_menu_and_cascading_submenus(
                with_menu,
                true,
                *x,
                *y,
                state.window_size,
                225.0,
                est_h,
                menu_el,
                submenus,
                Message::CloseContextMenu,
            )
        }
        Some(ActiveContextMenu::EditorCanvas { x, y }) => {
            if let Some((raw_est_h, raw_menu)) =
                build_editor_canvas_context_menu(state.editor_view_state())
            {
                let (menu_el, est_h) =
                    clamp_menu_height(raw_menu.map(Message::Editor), raw_est_h, max_menu_h);
                with_context_menu(
                    with_menu,
                    true,
                    *x,
                    *y,
                    state.window_size,
                    225.0,
                    est_h,
                    menu_el,
                    Message::CloseContextMenu,
                )
            } else {
                with_menu
            }
        }
        Some(ActiveContextMenu::EditorLayer { x, y, layer_id }) => {
            if let Some((raw_est_h, raw_menu)) =
                build_editor_layer_context_menu(state.editor_view_state(), *layer_id)
            {
                let (menu_el, est_h) =
                    clamp_menu_height(raw_menu.map(Message::Editor), raw_est_h, max_menu_h);
                with_context_menu(
                    with_menu,
                    true,
                    *x,
                    *y,
                    state.window_size,
                    220.0,
                    est_h,
                    menu_el,
                    Message::CloseContextMenu,
                )
            } else {
                with_menu
            }
        }
        Some(ActiveContextMenu::EditorTab { x, y, tab_id }) => {
            if let Some((raw_est_h, raw_menu)) =
                build_editor_tab_context_menu(state.editor_view_state(), *tab_id)
            {
                let (menu_el, est_h) =
                    clamp_menu_height(raw_menu.map(Message::Editor), raw_est_h, max_menu_h);
                with_context_menu(
                    with_menu,
                    true,
                    *x,
                    *y,
                    state.window_size,
                    210.0,
                    est_h,
                    menu_el,
                    Message::CloseContextMenu,
                )
            } else {
                with_menu
            }
        }
        None => iced::widget::stack![with_menu]
            .width(Fill)
            .height(Fill)
            .into(),
    };

    let toast_spec = state.toast.as_ref().map(|t| ToastSpec {
        message: t.message.clone(),
        kind: t.kind,
        on_dismiss: Message::DismissToast(Some(t.id)),
    });
    let with_toast = with_toast_banner(with_ctx, toast_spec);

    let theme = theme(state, window_id);
    let dialog_target = state
        .active_dialog_window
        .filter(|id| state.windows.contains_key(id))
        .or_else(|| {
            state
                .last_active_window
                .filter(|id| state.windows.contains_key(id))
        })
        .or(state.main_window)
        .or_else(|| state.windows.keys().next().copied());
    let show_dialogs_here =
        is_standalone_viewer() || state.windows.len() <= 1 || dialog_target == Some(window_id);

    let mut dialogs: Vec<HeaderedManagedDialogSpec<'_, DesktopDialogId, Message>> = Vec::new();
    if show_dialogs_here
        && ws == Workspace::Editor
        && let Some(active_tab) = state.editor_ui.adjust_popup_tab
    {
        dialogs.push(build_editor_adjustment_dialog(state, active_tab, &theme).into());
    }
    if state.settings_open && show_dialogs_here {
        let settings_header_strip = build_settings_header_strip(state);
        let settings_body = build_settings_body(state);
        let settings_footer = row![
            button::labeled(
                tr(lang, "action-reset-layout"),
                Some(Icon::Reset),
                ButtonVariant::Secondary,
                Some(Message::RequestResetLayout),
            ),
            iced::widget::space::horizontal(),
            button::labeled(
                tr(lang, "action-close"),
                None,
                ButtonVariant::Primary,
                Some(Message::CloseSettings),
            ),
        ]
        .align_y(Alignment::Center);

        dialogs.push(HeaderedManagedDialogSpec {
            spec: dialog_spec_to_managed(
                DesktopDialogId::Settings,
                DialogSpec {
                    open: true,
                    title: tr(lang, "settings-title"),
                    width: 660.0,
                    body: settings_body,
                    footer: settings_footer.into(),
                    on_close: Message::CloseSettings,
                    on_noop: Message::Noop,
                },
                default_window_state_for(DesktopDialogId::Settings),
                OverlayMode::default(),
                lang,
                &theme,
            ),
            header_strip: Some(settings_header_strip),
        });
    }

    if show_dialogs_here && let Some(active_dialog) = &state.dialog {
        dialogs.push(build_in_app_dialog(state, active_dialog, &theme).into());
    }

    managed_dialog_stack_with_headers(
        with_toast,
        &state.dialog_manager,
        state.window_size,
        dialogs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_start_policy_matrix() {
        // Browser/Editor opened with a file (e.g. `photon edit file`) maximized.
        assert_eq!(
            window_start_policy(
                false,
                true,
                AppLaunchWindowMode::Maximized,
                SystemOpenWindowMode::Remembered,
                false,
            ),
            (false, true)
        );
        // Remembered with no persisted maximized state stays windowed.
        assert_eq!(
            window_start_policy(
                false,
                true,
                AppLaunchWindowMode::Remembered,
                SystemOpenWindowMode::Remembered,
                false,
            ),
            (false, false)
        );
        // Standalone Viewer with FitImage uses neither fullscreen nor maximize.
        assert_eq!(
            window_start_policy(
                true,
                true,
                AppLaunchWindowMode::Maximized,
                SystemOpenWindowMode::FitImage,
                true,
            ),
            (false, false)
        );
        // Standalone Viewer with Maximized starts maximized.
        assert_eq!(
            window_start_policy(
                true,
                true,
                AppLaunchWindowMode::Remembered,
                SystemOpenWindowMode::Maximized,
                false,
            ),
            (false, true)
        );
        // Standalone Viewer with FixedSize starts windowed.
        assert_eq!(
            window_start_policy(
                true,
                true,
                AppLaunchWindowMode::Maximized,
                SystemOpenWindowMode::FixedSize,
                true,
            ),
            (false, false)
        );
        // Standalone Viewer with Remembered respects persisted viewer_window_maximized.
        assert_eq!(
            window_start_policy(
                true,
                true,
                AppLaunchWindowMode::Remembered,
                SystemOpenWindowMode::Remembered,
                true,
            ),
            (false, true)
        );
        // Standalone Viewer only goes fullscreen for Fullscreen + opened files.
        assert_eq!(
            window_start_policy(
                true,
                true,
                AppLaunchWindowMode::Maximized,
                SystemOpenWindowMode::Fullscreen,
                true,
            ),
            (true, false)
        );
    }

    #[test]
    fn free_pan_allows_overscroll_and_cursor_zoom_without_locking() {
        let (mut pan_x, mut pan_y) = (0.0_f32, 0.0_f32);
        let mut zoom = 1.0_f32;

        // Even at zoom = 1.0 (fit), panning freely moves off-center without snapping back to 0.
        apply_canvas_pan(&mut pan_x, &mut pan_y, 120.0, -80.0);
        assert_eq!(pan_x, 120.0);
        assert_eq!(pan_y, -80.0);

        // Large-image dynamic zoom range allows 1:1 and beyond without clamping to 16.0.
        let (actual, min_z, max_z) = actual_zoom_for_bounds(16000, 12000, 800.0, 600.0);
        assert!(actual >= 20.0);
        apply_canvas_zoom_at_cursor(
            &mut zoom,
            &mut pan_x,
            &mut pan_y,
            actual,
            Some([400.0, 300.0]),
            [800.0, 600.0],
            min_z,
            max_z,
        );
        assert!((zoom - actual).abs() < 1e-4);
    }

    #[test]
    fn selection_modify_kind_validates_ranges() {
        let lang = Language::English;
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("0.5", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("250.0", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("0.4", lang)
                .is_err()
        );
        assert!(
            SelectionModifyKind::Feather
                .parse_and_validate("250.1", lang)
                .is_err()
        );

        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("1", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("500", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("0", lang)
                .is_err()
        );
        assert!(
            SelectionModifyKind::Expand
                .parse_and_validate("501", lang)
                .is_err()
        );

        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("0", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("255", lang)
                .is_ok()
        );
        assert!(
            SelectionModifyKind::Grow
                .parse_and_validate("256", lang)
                .is_err()
        );
    }

    #[test]
    fn dynamic_large_image_actual_zoom_reaches_one_to_one_beyond_16x() {
        // 16000x12000 image in a 1000x800 viewport => fit_scale = 1000/16000 = 0.0625.
        // 1:1 pixel zoom = 16.0; for 32768x24576 in 800x600 => fit_scale = 800/32768 ≈ 0.0244,
        // so 1:1 pixel zoom ≈ 40.96 (well above the old hardcoded 16.0 ceiling).
        let (actual_32k, min_z, max_z) = actual_zoom_for_bounds(32768, 24576, 800.0, 600.0);
        assert_eq!(min_z, 0.02);
        assert!(
            actual_32k > 40.0,
            "expected 1:1 zoom > 40, got {actual_32k}"
        );
        assert!(max_z > actual_32k * 30.0);
        let fit_scale = (800.0_f32 / 32768.0).min(600.0_f32 / 24576.0);
        assert!((actual_32k * fit_scale - 1.0).abs() < 1e-4);
    }

    #[test]
    fn dialog_defaults_cover_curves_denoise_and_plugins() {
        let curves = default_window_state_for(DesktopDialogId::AdjustCurves);
        assert_eq!(curves.width, 500.0);
        assert_eq!(curves.height, 650.0);
        assert_eq!(curves.min_width, 420.0);
        assert_eq!(curves.min_height, 560.0);

        let plugins = default_window_state_for(DesktopDialogId::ManagePlugins);
        assert_eq!(plugins.width, 680.0);
        assert_eq!(plugins.height, 520.0);
        assert_eq!(plugins.min_width, 580.0);

        let denoise = default_window_state_for(DesktopDialogId::AdjustDenoise);
        assert_eq!(denoise.width, 440.0);
    }

    #[test]
    fn translate_lang_swap_maps_tesseract_and_iso_codes() {
        assert_eq!(tesseract_to_iso("eng"), "en");
        assert_eq!(tesseract_to_iso("jpn_vert"), "ja");
        assert_eq!(tesseract_to_iso("chi_sim"), "zh-CN");
        assert_eq!(tesseract_to_iso("eng+jpn"), "en");
        assert_eq!(iso_to_tesseract("vi"), "vie");
        assert_eq!(iso_to_tesseract("ja"), "jpn");
        assert_eq!(iso_to_tesseract("zh-CN"), "chi_sim");
        assert_eq!(iso_to_tesseract("en"), "eng");
    }

    #[test]
    fn combined_editor_curve_lut_composes_channel_and_master_curves() {
        let curves = photon_protocol::CurvesAdjustment {
            rgb: vec![[0.0, 1.0], [1.0, 0.0]],
            red: vec![[0.0, 0.0], [1.0, 1.0]],
            green: vec![[0.0, 0.0], [1.0, 1.0]],
            blue: vec![[0.0, 0.0], [1.0, 1.0]],
        };
        let lut = compute_editor_curve_lut(&curves);
        assert_eq!(lut[0], 255);
        assert_eq!(lut[1], 255);
        assert_eq!(lut[2], 255);
        assert_eq!(lut[3], 255);
        assert_eq!(lut[255 * 4], 0);
        assert_eq!(lut[255 * 4 + 3], 255);
    }

    #[test]
    fn canvas_size_dialog_has_sensible_defaults_and_id_mapping() {
        let state = default_window_state_for(DesktopDialogId::CanvasSize);
        assert_eq!(state.width, 460.0);
        assert_eq!(state.height, 390.0);
        let dlg = InAppDialog::CanvasSize {
            dims: DimensionInputState::from_dims(100, 100),
            anchor: photon_protocol::CanvasAnchor::Center,
            fill: None,
            error: None,
        };
        assert_eq!(dialog_id_for_in_app(&dlg), DesktopDialogId::CanvasSize);
    }

    #[test]
    fn new_document_dialog_defaults_and_background_presets() {
        let state = default_window_state_for(DesktopDialogId::NewDocument);
        assert_eq!(state.width, 480.0);
        assert_eq!(state.height, 460.0);
        let dlg = InAppDialog::NewDocument {
            dims: DimensionInputState::from_dims(1920, 1080),
            background: NewDocBackground::White,
            custom_rgb: [12, 34, 56],
            error: None,
        };
        assert_eq!(dialog_id_for_in_app(&dlg), DesktopDialogId::NewDocument);
        assert_eq!(NewDocBackground::Transparent.to_rgba(), [0, 0, 0, 0]);
        assert_eq!(NewDocBackground::White.to_rgba(), [255, 255, 255, 255]);
        assert_eq!(NewDocBackground::Black.to_rgba(), [0, 0, 0, 255]);
        assert_eq!(
            NewDocBackground::Custom([12, 34, 56]).to_rgba(),
            [12, 34, 56, 255]
        );
    }

    #[test]
    fn translate_drag_target_inverse_rotation_and_scale() {
        assert_eq!(TranslateDragTarget::default(), TranslateDragTarget::None);
        // Scale 2.0 (drawn 2x image): +20 screen px => +10 image px at 0 turns.
        assert_eq!(screen_delta_to_image_delta(20.0, -10.0, 2.0, 0), (10, -5));
        // 90 deg CW: screen +dx => -dy_img, screen +dy => +dx_img
        assert_eq!(screen_delta_to_image_delta(20.0, 10.0, 2.0, 1), (5, -10));
        // 180 deg: screen +dx => -dx_img, screen +dy => -dy_img
        assert_eq!(screen_delta_to_image_delta(20.0, 10.0, 2.0, 2), (-10, -5));
        // 270 deg CW: screen +dx => +dy_img, screen +dy => -dx_img
        assert_eq!(screen_delta_to_image_delta(20.0, 10.0, 2.0, 3), (-5, 10));
    }

    #[test]
    fn archive_thumbnail_window_indices_centers_and_orders_outward() {
        let win = archive_thumbnail_window_indices(120, 50, 2, 3);
        assert_eq!(win, vec![50, 51, 49, 52, 48, 53]);
        let start = archive_thumbnail_window_indices(120, 0, 10, 18);
        assert_eq!(start.first(), Some(&0));
        assert_eq!(start.len(), 19);
        assert!(start.iter().all(|&i| i < 19));
        assert!(archive_thumbnail_window_indices(0, 0, 10, 18).is_empty());
    }

    #[test]
    fn probe_window_is_bounded_and_clamped() {
        assert_eq!(probe_window(0, 0, 8, 8), 0..0);
        assert_eq!(probe_window(20, 10, 3, 3), 7..14);
        // Clamps at both edges instead of wrapping.
        assert_eq!(probe_window(20, 0, 8, 8), 0..9);
        assert_eq!(probe_window(20, 19, 8, 8), 11..20);
        // Center beyond the end is clamped to the last index.
        assert_eq!(probe_window(20, 999, 2, 2), 17..20);
        // Window never exceeds the folder, regardless of radius.
        assert_eq!(probe_window(5, 2, 100, 100), 0..5);
    }

    #[test]
    fn thumbnail_keep_set_caps_cache_but_preserves_loading() {
        let ordered: Vec<String> = ["a", "b", "c", "d", "e"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let loading: BTreeSet<String> = ["d".to_owned()].into_iter().collect();
        let keep = thumbnail_keep_set(&ordered, &loading, 2);
        // First two priorities fill the cap; in-flight `d` is never pruned.
        assert!(keep.contains("a"));
        assert!(keep.contains("b"));
        assert!(keep.contains("d"));
        assert!(!keep.contains("c"));
        assert!(!keep.contains("e"));

        // Without loading paths the cap is a strict prefix.
        let keep = thumbnail_keep_set(&ordered, &BTreeSet::new(), 3);
        assert_eq!(keep.len(), 3);
        assert!(keep.contains("c"));
        assert!(!keep.contains("d"));
    }

    #[test]
    fn resolve_double_marquee_region_splits_left_and_right_pages() {
        // Left (800x1000), Right (1200x1500), gap 10 -> target_h=1500, left_disp_w=1200, gap_doc=16.67, right_disp_w=1200
        let left = resolve_double_marquee_region(
            [150.0, 300.0, 300.0, 150.0],
            Some("/tmp/left.png"),
            (800, 1000),
            Some("/tmp/right.png"),
            Some((1200, 1500)),
            10.0,
            0,
        )
        .unwrap();
        assert_eq!(left.0, "/tmp/left.png");
        assert_eq!(left.1, [100, 200, 200, 100]);

        let right = resolve_double_marquee_region(
            [1317.0, 300.0, 200.0, 100.0],
            Some("/tmp/left.png"),
            (800, 1000),
            Some("/tmp/right.png"),
            Some((1200, 1500)),
            10.0,
            0,
        )
        .unwrap();
        assert_eq!(right.0, "/tmp/right.png");
        assert_eq!(right.1[1], 300);
        assert_eq!(right.1[2], 200);
    }

    #[test]
    fn window_app_ids_are_distinct_per_mode() {
        let browser = window_app_id_for(false, false);
        let editor = window_app_id_for(false, true);
        let viewer = window_app_id_for(true, false);
        assert_eq!(browser, "io.github.tienv2i.Photon");
        assert_eq!(editor, "io.github.tienv2i.Photon.Editor");
        assert_eq!(viewer, "io.github.tienv2i.Photon.Viewer");
        assert_ne!(browser, editor);
        assert_ne!(browser, viewer);
        assert_ne!(editor, viewer);
    }

    #[test]
    fn display_font_mapping_and_labels_cover_all_variants() {
        for font in DisplayFont::ALL {
            let _ = iced_font_for(font);
            let en = display_font_label(Language::English, font);
            let vi = display_font_label(Language::Vietnamese, font);
            assert!(!en.is_empty());
            assert!(!vi.is_empty());
        }
    }

    #[test]
    fn phase3_dialogs_and_ui_tree_expose_vector_menu_and_tools() {
        let te = default_window_state_for(DesktopDialogId::TransformEach);
        assert_eq!(te.width, 460.0);
        let vb = default_window_state_for(DesktopDialogId::VectorBlend);
        assert_eq!(vb.width, 400.0);
        let svg = default_window_state_for(DesktopDialogId::ExportVectorSvg);
        assert_eq!(svg.width, 460.0);

        assert_eq!(
            dialog_id_for_in_app(&InAppDialog::TransformEach {
                spec: TransformEachSpec::default(),
            }),
            DesktopDialogId::TransformEach
        );
        assert_eq!(
            dialog_id_for_in_app(&InAppDialog::VectorBlend {
                spec: VectorBlendSpec::default(),
            }),
            DesktopDialogId::VectorBlend
        );
        assert_eq!(
            dialog_id_for_in_app(&InAppDialog::ExportVectorSvg {
                path: "/tmp/out.svg".to_owned(),
                error: None,
            }),
            DesktopDialogId::ExportVectorSvg
        );

        let core = photon_core::Core::new(true, true);
        let mut snap = core.snapshot();
        snap.meta.workspace = Workspace::Editor;
        let tree = build_ui_tree(
            &snap,
            1,
            Size::new(1280.0, 800.0),
            1.0,
            1.0,
            0.0,
            0.0,
            "wgpu",
            &ViewerUiPresentation {
                view_mode: ViewerViewMode::Single,
                info_open: false,
                adjustments_open: false,
                filmstrip_open: false,
                color_popover_open: false,
                translate_open: false,
            },
            &EditorUiState::default(),
            None,
            false,
            "general",
        );
        for required_id in [
            "editor.tool.path_select",
            "editor.tool.text",
            "editor.menu.vector",
            "editor.menu.vector.new_layer",
            "editor.menu.vector.convert_to_path",
            "editor.menu.vector.join_path",
            "editor.menu.vector.split_path",
            "editor.menu.vector.close_open_path",
            "editor.menu.vector.reverse_path",
            "editor.menu.vector.simplify_path",
            "editor.menu.vector.smooth_path",
            "editor.menu.vector.transform_each",
            "editor.menu.vector.blend",
            "editor.menu.vector.rasterize",
            "editor.menu.vector.export_svg",
            "editor.layer.context_menu.rasterize",
        ] {
            assert!(
                tree.widgets.iter().any(|w| w.id == required_id),
                "missing semantic widget id: {required_id}"
            );
        }
    }
}
