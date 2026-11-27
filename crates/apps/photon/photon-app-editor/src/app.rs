use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use fw_panels::SidebarOrderState;
use fw_protocol::Envelope;
use fw_runtime::CoreHandle;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::icons::Icon;
use fw_ui::menu::menu_panel_entries;
use fw_ui::theme::{TEXT_SIZE_CAPTION, hairline_rule_style, surface_panel_style};
use fw_ui::typography::text;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Space, column, container, image as iced_image, mouse_area, row, rule, stack};
use iced::{Element, Event, Fill, Font, Size, Subscription, Task, Theme as IcedTheme, window};
use photon_app_common::image_dialogs::{open_images, save_image_as};
use photon_app_common::window_activation::request_platform_activation;
use photon_canvas::{CanvasEvent, CpuImageFrame, CpuMaskFrame, TransformPreview};
use photon_editor::panel_splitter::DockDragState;
use photon_editor::{
    EditorMenuKind, EditorMessage, EditorUiState, EditorViewState, build_editor_add_layer_menu,
    build_editor_canvas_context_menu, build_editor_plugins_menu_entries, build_editor_toolbar,
    build_editor_vector_menu_entries, build_editor_view,
};
use photon_protocol::{
    CanvasAntiAliasingMode, ClientKind, ColorAdjustments, Command, DisplayFont, EditorRightPanel,
    EditorToolTab, Event as CoreEvent, EventKind, PresentationState, SemanticWidgetNode,
    Theme as PhotonTheme, UiStateSnapshot, UiTreeSnapshot, ViewportDenoiseMode, Workspace,
};

use crate::core::{EditorCore, EditorCoreCustom};

static CORE_HANDLE: OnceLock<CoreHandle<EditorCore>> = OnceLock::new();
static BG_RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();
static GPU_BACKEND: OnceLock<String> = OnceLock::new();

pub fn init_globals(
    core: CoreHandle<EditorCore>,
    bg_rt: tokio::runtime::Handle,
    gpu_backend: String,
) {
    let _ = CORE_HANDLE.set(core);
    let _ = BG_RUNTIME.set(bg_rt);
    let _ = GPU_BACKEND.set(gpu_backend);
}

#[allow(dead_code)]
fn spawn_bg<F, T>(f: F) -> impl Future<Output = T> + Send + 'static
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = BG_RUNTIME
        .get()
        .expect("BG_RUNTIME not initialized")
        .clone();
    async move { handle.spawn_blocking(f).await.expect("bg task panicked") }
}

pub fn iced_font_for(font: DisplayFont) -> Font {
    match font {
        DisplayFont::System => Font::DEFAULT,
        DisplayFont::BundledSans | DisplayFont::Inter => Font::with_name("Inter"),
        DisplayFont::Cantarell => Font::with_name("Cantarell"),
        DisplayFont::NotoSans => Font::with_name("Noto Sans"),
        DisplayFont::DejaVuSans => Font::with_name("DejaVu Sans"),
        DisplayFont::Serif => Font::with_name("serif"),
        DisplayFont::Monospace => Font::MONOSPACE,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(EditorMessage),
    IcedEvent(Event),
    StateUpdated(Arc<UiStateSnapshot>),
    CoreEventReceived(CoreEvent),
    EditorFrameLoaded(Option<CpuImageFrame>),
    PickedImages(Vec<PathBuf>),
    PickedSavePath(Option<PathBuf>),
    WindowOpened(window::Id),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    WindowResized(window::Id, Size),
    CloseOverlays,
    Noop,
}

pub struct EditorApp {
    core: CoreHandle<EditorCore>,
    snapshot: Arc<UiStateSnapshot>,
    window_id: Option<window::Id>,
    window_size: Size,
    is_fullscreen: bool,
    editor_frame: Option<CpuImageFrame>,
    editor_original_frame: Option<CpuImageFrame>,
    selection_mask: Option<CpuMaskFrame>,
    marching_ants_phase: f32,
    editor_layer_thumbs: HashMap<u64, iced_image::Handle>,
    editor_mask_thumbs: HashMap<u64, iced_image::Handle>,
    editor_ui: EditorUiState,
    pending_adj: ColorAdjustments,
    curve_lut: Option<[u8; 1024]>,
    aa_mode: CanvasAntiAliasingMode,
    denoise_mode: ViewportDenoiseMode,
    transform_preview: Option<TransformPreview>,
    crop_uv: Option<[f32; 4]>,
    viewport_background: Option<[u8; 3]>,
    editor_tool_tab: EditorToolTab,
    editor_history_open: bool,
    editor_layers_open: bool,
    editor_history_collapsed: bool,
    editor_layers_collapsed: bool,
    editor_panels: SidebarOrderState<EditorRightPanel>,
    dock_drag: DockDragState,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    active_menu: Option<EditorMenuKind>,
    context_menu: Option<[f32; 2]>,
    last_revision: u64,
}

pub fn boot() -> (EditorApp, Task<Message>) {
    let core = CORE_HANDLE
        .get()
        .expect("CORE_HANDLE not initialized")
        .clone();
    let snapshot = core.latest_snapshot();
    let (win_id, open_task) = window::open(window::Settings {
        size: Size::new(1360.0, 880.0),
        min_size: Some(Size::new(800.0, 540.0)),
        exit_on_close_request: false,
        ..Default::default()
    });

    let mut app = EditorApp {
        core,
        aa_mode: snapshot.meta.layout.canvas_anti_aliasing,
        denoise_mode: snapshot.meta.layout.viewport_denoise,
        editor_tool_tab: snapshot.meta.layout.editor_tool_tab,
        editor_panels: SidebarOrderState::new(
            snapshot.meta.layout.editor_right_panel_order.clone(),
            &[
                EditorRightPanel::ToolOptionsHistory,
                EditorRightPanel::Layers,
            ],
        ),
        snapshot,
        window_id: Some(win_id),
        window_size: Size::new(1360.0, 880.0),
        is_fullscreen: false,
        editor_frame: None,
        editor_original_frame: None,
        selection_mask: None,
        marching_ants_phase: 0.0,
        editor_layer_thumbs: HashMap::new(),
        editor_mask_thumbs: HashMap::new(),
        editor_ui: EditorUiState::default(),
        pending_adj: ColorAdjustments::default(),
        curve_lut: None,
        transform_preview: None,
        crop_uv: None,
        viewport_background: None,
        editor_history_open: true,
        editor_layers_open: true,
        editor_history_collapsed: false,
        editor_layers_collapsed: false,
        dock_drag: DockDragState::default(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        active_menu: None,
        context_menu: None,
        last_revision: 0,
    };

    let frame_task = app.load_editor_frame_task();
    let tree_task = app.report_ui_tree_task();
    (
        app,
        Task::batch([open_task.map(Message::WindowOpened), frame_task, tree_task]),
    )
}

pub fn title(app: &EditorApp, _id: window::Id) -> String {
    if let Some(ed) = &app.snapshot.meta.editor {
        let dirty = if ed.modified { " *" } else { "" };
        let name = if ed.name.is_empty() {
            "Untitled"
        } else {
            &ed.name
        };
        format!("{name}{dirty} — Photon Editor")
    } else {
        "Photon Editor".to_owned()
    }
}

pub fn theme(app: &EditorApp, _id: window::Id) -> IcedTheme {
    match app.snapshot.meta.theme {
        PhotonTheme::Dark => IcedTheme::Dark,
        PhotonTheme::Light => IcedTheme::Light,
    }
}

pub fn scale_factor(app: &EditorApp, _id: window::Id) -> f32 {
    app.snapshot.meta.layout.ui_scale().clamp(0.75, 2.5)
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
            if output.send(Message::StateUpdated(snap)).await.is_err() {
                break;
            }
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
        while let Ok(ev) = rx.recv().await {
            if output.send(Message::CoreEventReceived(ev)).await.is_err() {
                break;
            }
        }
    })
}

pub fn subscription(_app: &EditorApp) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(state_watch_stream),
        Subscription::run(event_broadcast_stream),
        iced::event::listen().map(Message::IcedEvent),
        window::close_events().map(Message::WindowClosed),
        window::close_requests().map(Message::WindowCloseRequested),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ])
}

pub fn update(app: &mut EditorApp, message: Message) -> Task<Message> {
    match message {
        Message::Noop => Task::none(),
        Message::WindowOpened(id) => {
            app.window_id = Some(id);
            app.report_ui_tree_task()
        }
        Message::WindowCloseRequested(_id) => iced::exit(),
        Message::WindowClosed(id) => {
            if app.window_id == Some(id) {
                app.window_id = None;
                return iced::exit();
            }
            Task::none()
        }
        Message::WindowResized(id, size) => {
            if app.window_id == Some(id) {
                app.window_size = size;
                return app.report_ui_tree_task();
            }
            Task::none()
        }
        Message::CloseOverlays => {
            app.active_menu = None;
            app.context_menu = None;
            Task::none()
        }
        Message::StateUpdated(snapshot) => {
            let focus_changed =
                snapshot.meta.focus_request_seq != app.snapshot.meta.focus_request_seq;
            app.snapshot = snapshot;
            app.aa_mode = app.snapshot.meta.layout.canvas_anti_aliasing;
            app.denoise_mode = app.snapshot.meta.layout.viewport_denoise;
            let mut tasks = vec![app.load_editor_frame_task(), app.report_ui_tree_task()];
            if focus_changed && let Some(id) = app.window_id {
                request_platform_activation(std::process::id());
                tasks.push(window::gain_focus(id));
            }
            Task::batch(tasks)
        }
        Message::EditorFrameLoaded(frame) => {
            app.editor_frame = frame;
            Task::none()
        }
        Message::CoreEventReceived(ev) => {
            if let EventKind::UiActionRequested { target_id, .. } = ev.kind
                && target_id == "app.exit"
            {
                return iced::exit();
            }
            Task::none()
        }
        Message::PickedImages(paths) => {
            for p in paths {
                app.dispatch(Command::EditorOpen {
                    path: p.to_string_lossy().into_owned(),
                });
            }
            Task::none()
        }
        Message::PickedSavePath(path) => {
            if let Some(p) = path {
                app.dispatch(Command::EditorSaveAs {
                    path: p.to_string_lossy().into_owned(),
                });
            }
            Task::none()
        }
        Message::IcedEvent(ev) => app.handle_iced_event(ev),
        Message::Editor(em) => app.handle_editor_message(em),
    }
}

pub fn view(app: &EditorApp, _id: window::Id) -> Element<'_, Message> {
    let estate = app.editor_view_state();

    let right_utilities: Element<'_, EditorMessage> = button::icon_only(
        Icon::Fullscreen,
        ButtonVariant::Ghost,
        Some(EditorMessage::ToggleWorkspacePanels),
    )
    .into();

    let header: Element<'_, Message> = if app.is_fullscreen {
        Space::new().height(0).into()
    } else {
        column![
            build_editor_toolbar(estate, right_utilities).map(Message::Editor),
            rule::horizontal(1).style(hairline_rule_style),
        ]
        .into()
    };

    let main_view = build_editor_view(estate).map(Message::Editor);
    let doc_info = app
        .snapshot
        .meta
        .editor
        .as_ref()
        .map(|ed| format!("{}×{} px · {} layers", ed.width, ed.height, ed.layers.len()))
        .unwrap_or_else(|| "No document".to_owned());
    let zoom_pct = format!("{}%", (app.zoom * 100.0).round() as i32);
    let status_bar: Element<'_, Message> = container(
        row![
            text(doc_info).size(TEXT_SIZE_CAPTION),
            Space::new().width(Fill),
            text(zoom_pct).size(TEXT_SIZE_CAPTION),
        ]
        .padding([4, 12]),
    )
    .width(Fill)
    .style(surface_panel_style)
    .into();

    let base: Element<'_, Message> = column![header, main_view, status_bar]
        .width(Fill)
        .height(Fill)
        .into();

    let mut layers: Vec<Element<'_, Message>> = vec![base];

    if let Some(menu_kind) = app.active_menu {
        let menu_el: Option<Element<'_, EditorMessage>> = match menu_kind {
            EditorMenuKind::Plugins => Some(menu_panel_entries(
                build_editor_plugins_menu_entries(estate),
                220.0,
            )),
            EditorMenuKind::AddLayer => Some(build_editor_add_layer_menu(estate).1),
            EditorMenuKind::Vector => Some(menu_panel_entries(
                build_editor_vector_menu_entries(estate),
                220.0,
            )),
            _ => None,
        };
        if let Some(el) = menu_el {
            let backdrop = mouse_area(
                container(column![Space::new().height(48), el.map(Message::Editor)])
                    .padding([0, 12])
                    .width(Fill)
                    .height(Fill),
            )
            .on_press(Message::CloseOverlays);
            layers.push(backdrop.into());
        }
    }

    if let Some(pos) = app.context_menu
        && let Some((_, menu_el)) = build_editor_canvas_context_menu(estate)
    {
        let x = pos[0].clamp(8.0, (app.window_size.width - 230.0).max(8.0));
        let y = pos[1].clamp(8.0, (app.window_size.height - 320.0).max(8.0));
        let backdrop = mouse_area(
            container(
                column![
                    Space::new().height(y),
                    row![Space::new().width(x), menu_el.map(Message::Editor)].width(Fill),
                ]
                .width(Fill)
                .height(Fill),
            )
            .width(Fill)
            .height(Fill),
        )
        .on_press(Message::CloseOverlays);
        layers.push(backdrop.into());
    }

    stack(layers).width(Fill).height(Fill).into()
}

impl EditorApp {
    fn dispatch(&self, cmd: Command) {
        let _ = self
            .core
            .dispatch(Envelope::new("gui-editor", ClientKind::Gui, cmd));
    }

    fn load_editor_frame_task(&mut self) -> Task<Message> {
        let Some(ed) = &self.snapshot.meta.editor else {
            self.editor_frame = None;
            return Task::none();
        };
        self.last_revision = self.last_revision.wrapping_add(1);
        let rev = self.last_revision;
        let key = if ed.path.is_empty() {
            "editor-doc".to_owned()
        } else {
            ed.path.clone()
        };
        let core = self.core.clone();
        Task::perform(
            async move {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if core
                    .send_custom(EditorCoreCustom::ReadPixels(tx))
                    .await
                    .is_ok()
                    && let Ok(Some((pixel_rev, w, h, rgba))) = rx.await
                {
                    return Some(CpuImageFrame {
                        key,
                        revision: pixel_rev.max(rev),
                        width: w.max(1),
                        height: h.max(1),
                        rgba: Arc::new(rgba),
                        dirty_rect: None,
                    });
                }
                None
            },
            Message::EditorFrameLoaded,
        )
    }

    fn editor_view_state(&self) -> EditorViewState<'_> {
        EditorViewState {
            snapshot: &self.snapshot,
            editor_frame: self.editor_frame.as_ref(),
            editor_original_frame: self.editor_original_frame.as_ref(),
            selection_mask: self.selection_mask.as_ref(),
            marching_ants_phase: self.marching_ants_phase,
            editor_layer_thumbs: &self.editor_layer_thumbs,
            editor_mask_thumbs: &self.editor_mask_thumbs,
            editor_ui: &self.editor_ui,
            pending_adj: &self.pending_adj,
            curve_lut: self.curve_lut,
            aa_mode: self.aa_mode,
            denoise_mode: self.denoise_mode,
            transform_preview: self.transform_preview.as_ref(),
            crop_uv: self.crop_uv,
            viewport_background: self.viewport_background,
            editor_tool_tab: self.editor_tool_tab,
            editor_history_open: self.editor_history_open,
            editor_layers_open: self.editor_layers_open,
            editor_history_collapsed: self.editor_history_collapsed,
            editor_layers_collapsed: self.editor_layers_collapsed,
            editor_panels: &self.editor_panels,
            dock_drag: &self.dock_drag,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            window_size: self.window_size,
            active_menu: self.active_menu,
        }
    }

    fn handle_iced_event(&mut self, ev: Event) -> Task<Message> {
        match ev {
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character("o") => {
                            return Task::perform(
                                async { open_images("Open Image in Editor", None).await },
                                Message::PickedImages,
                            );
                        }
                        keyboard::Key::Character("s") => {
                            return Task::perform(
                                async { save_image_as("Export / Save Image", None, None).await },
                                Message::PickedSavePath,
                            );
                        }
                        keyboard::Key::Character("z") => {
                            if modifiers.shift() {
                                self.dispatch(Command::EditorRedo);
                            } else {
                                self.dispatch(Command::EditorUndo);
                            }
                        }
                        keyboard::Key::Character("w") | keyboard::Key::Character("q") => {
                            return iced::exit();
                        }
                        _ => {}
                    }
                    return Task::none();
                }
                match key {
                    Key::Named(Named::Escape) => {
                        self.active_menu = None;
                        self.context_menu = None;
                    }
                    Key::Named(Named::F11) => return self.toggle_fullscreen(),
                    _ => {}
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn toggle_fullscreen(&mut self) -> Task<Message> {
        self.is_fullscreen = !self.is_fullscreen;
        if let Some(id) = self.window_id {
            let mode = if self.is_fullscreen {
                window::Mode::Fullscreen
            } else {
                window::Mode::Windowed
            };
            return window::set_mode(id, mode);
        }
        Task::none()
    }

    fn handle_editor_message(&mut self, msg: EditorMessage) -> Task<Message> {
        match msg {
            EditorMessage::Noop => {}
            EditorMessage::Dispatch(cmd) => {
                self.active_menu = None;
                self.context_menu = None;
                self.dispatch(cmd);
            }
            EditorMessage::ToggleMenu(kind) => {
                self.active_menu = if self.active_menu == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
            }
            EditorMessage::HoverMenu(kind) => {
                if self.active_menu.is_some() {
                    self.active_menu = Some(kind);
                }
            }
            EditorMessage::ToggleWorkspacePanels => {
                self.editor_history_open = !self.editor_history_open;
                self.editor_layers_open = !self.editor_layers_open;
            }
            EditorMessage::OpenExportDialog => {
                return Task::perform(
                    async { save_image_as("Export / Save Image", None, None).await },
                    Message::PickedSavePath,
                );
            }
            EditorMessage::EditorSelectTool(tool) => self.editor_ui.tool = tool,
            EditorMessage::Canvas(ev) => match ev {
                CanvasEvent::ZoomBy {
                    factor,
                    cursor,
                    bounds,
                } => {
                    let old_zoom = self.zoom.max(0.02);
                    let new_zoom = (old_zoom * factor).clamp(0.05, 32.0);
                    let ratio = new_zoom / old_zoom;
                    if let Some([cx, cy]) = cursor {
                        let dx = cx - bounds[0] * 0.5 - self.pan_x;
                        let dy = cy - bounds[1] * 0.5 - self.pan_y;
                        self.pan_x += dx * (1.0 - ratio);
                        self.pan_y += dy * (1.0 - ratio);
                    }
                    self.zoom = new_zoom;
                }
                CanvasEvent::PanBy { dx, dy } => {
                    self.pan_x += dx;
                    self.pan_y += dy;
                }
                CanvasEvent::ResetFit => {
                    self.zoom = 1.0;
                    self.pan_x = 0.0;
                    self.pan_y = 0.0;
                }
                CanvasEvent::ContextMenuRequested { pos, .. } => {
                    self.context_menu = Some(pos);
                }
                _ => {}
            },
            _ => {}
        }
        Task::none()
    }

    fn report_ui_tree_task(&self) -> Task<Message> {
        if self.window_id.is_none() {
            return Task::none();
        }
        let gpu_backend = GPU_BACKEND
            .get()
            .cloned()
            .unwrap_or_else(|| "wgpu".to_owned());
        let tree = UiTreeSnapshot {
            revision: self.snapshot.meta.revision,
            workspace: Workspace::Editor,
            presentation: PresentationState {
                window_count: 1,
                window_width: self.window_size.width,
                window_height: self.window_size.height,
                scale_factor: self.snapshot.meta.layout.ui_scale(),
                zoom: self.zoom,
                pan_x: self.pan_x,
                pan_y: self.pan_y,
                gpu_backend,
                gpu_adapter: String::new(),
            },
            widgets: vec![SemanticWidgetNode {
                id: "editor-root".to_owned(),
                role: "window".to_owned(),
                label: "Photon Editor".to_owned(),
                workspace: Workspace::Editor,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{:.2}", self.zoom)),
            }],
        };
        let core = self.core.clone();
        Task::perform(
            async move {
                let _ = core.send_custom(EditorCoreCustom::SetUiTree(tree)).await;
            },
            |_| Message::Noop,
        )
    }
}
