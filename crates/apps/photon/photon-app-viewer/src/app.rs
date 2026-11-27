use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use fw_panels::SidebarOrderState;
use fw_protocol::Envelope;
use fw_runtime::CoreHandle;
use fw_ui::button::{self, ButtonVariant};
use fw_ui::icons::Icon;
use fw_ui::menu::menu_panel_entries;
use fw_ui::theme::{
    TEXT_SIZE_CAPTION, TEXT_SIZE_CONTROL, hairline_rule_style, surface_panel_style,
};
use fw_ui::typography::text;
use iced::keyboard::{self, Key, key::Named};
use iced::widget::{
    Space, column, container, image as iced_image, mouse_area, row, rule, scrollable, stack,
};
use iced::{
    Alignment, Element, Event, Fill, Font, Point, Size, Subscription, Task, Theme as IcedTheme,
    window,
};
use photon_app_common::image_dialogs::open_images;
use photon_app_common::ipc_launch::spawn_viewer_window_child;
use photon_app_common::window_activation::request_platform_activation;
use photon_i18n::tr;
use photon_protocol::{
    CanvasAntiAliasingMode, ClientKind, ColorAdjustments, Command, DisplayFont, Event as CoreEvent,
    PresentationState, SemanticWidgetNode, Theme as PhotonTheme, UiStateSnapshot, UiTreeSnapshot,
    ViewerRightPanel, ViewerViewMode, ViewportDenoiseMode, WebtoonFitMode, Workspace,
};

use crate::core::{ViewerCore, ViewerCoreCustom};
use photon_viewer::{
    ViewerMessage, ViewerTranslateState, ViewerViewState, build_viewer_context_menu_with_offset,
    build_viewer_quality_menu_entries, build_viewer_toolbar, build_viewer_view,
    filmstrip_scroll_id, step_dual_spread, viewer_counter_summary,
    viewer_reader_options_right_offset, viewer_status_summary, webtoon_index_at_scroll,
    webtoon_offset_for_index, webtoon_scroll_id,
};
use photon_viewer_canvas::{ViewerCanvasEvent, ViewerImageFrame};

static CORE_HANDLE: OnceLock<CoreHandle<ViewerCore>> = OnceLock::new();
static BG_RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();
static GPU_BACKEND: OnceLock<String> = OnceLock::new();

pub fn init_globals(
    core: CoreHandle<ViewerCore>,
    bg_rt: tokio::runtime::Handle,
    gpu_backend: String,
) {
    let _ = CORE_HANDLE.set(core);
    let _ = BG_RUNTIME.set(bg_rt);
    let _ = GPU_BACKEND.set(gpu_backend);
}

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
        DisplayFont::BundledSans => Font::with_name("Inter"),
        DisplayFont::Inter => Font::with_name("Inter"),
        DisplayFont::Cantarell => Font::with_name("Cantarell"),
        DisplayFont::NotoSans => Font::with_name("Noto Sans"),
        DisplayFont::DejaVuSans => Font::with_name("DejaVu Sans"),
        DisplayFont::Serif => Font::with_name("serif"),
        DisplayFont::Monospace => Font::MONOSPACE,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Viewer(ViewerMessage),
    IcedEvent(Event),
    StateUpdated(Arc<UiStateSnapshot>),
    CoreEventReceived(CoreEvent),
    FrameDecoded {
        path: String,
        frame: Result<ViewerImageFrame, String>,
    },
    ThumbDecoded {
        path: String,
        handle: Option<iced_image::Handle>,
    },
    WebtoonPageDecoded {
        path: String,
        handle: Option<iced_image::Handle>,
    },
    PickedImages(Vec<std::path::PathBuf>),
    WindowOpened(window::Id),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    WindowResized(window::Id, Size),
    CloseOverlays,
    Noop,
}

pub struct ViewerApp {
    core: CoreHandle<ViewerCore>,
    snapshot: Arc<UiStateSnapshot>,
    window_id: Option<window::Id>,
    window_size: Size,
    is_fullscreen: bool,
    viewer_mode: ViewerViewMode,
    viewer_frame: Option<ViewerImageFrame>,
    viewer_pair_frame: Option<ViewerImageFrame>,
    frame_cache: HashMap<String, ViewerImageFrame>,
    decoding_frames: HashSet<String>,
    image_sizes: HashMap<String, (u32, u32)>,
    thumbnails: HashMap<String, iced_image::Handle>,
    decoding_thumbs: HashSet<String>,
    webtoon_pages: HashMap<String, iced_image::Handle>,
    decoding_webtoon: HashSet<String>,
    webtoon_visible_index: Option<usize>,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    webtoon_zoom: f32,
    viewer_quarter_turns: u8,
    viewer_adj: ColorAdjustments,
    viewer_manga_rtl: bool,
    viewer_manga_cover_separate: bool,
    viewer_manga_page_gap: u16,
    viewer_webtoon_fit_mode: WebtoonFitMode,
    viewer_adjustments_open: bool,
    viewer_info_open: bool,
    viewer_filmstrip_open: bool,
    viewer_right_panels: SidebarOrderState<ViewerRightPanel>,
    viewer_right_width_preview: Option<f32>,
    viewer_right_split_preview: Option<f32>,
    reader_options_open: bool,
    utilities_menu_open: bool,
    context_menu: Option<(Option<String>, [f32; 2])>,
    pending_delete: Option<Vec<String>>,
    aa_mode: CanvasAntiAliasingMode,
    denoise_mode: ViewportDenoiseMode,
    viewport_background: Option<[u8; 3]>,
    translate: ViewerTranslateState,
    cursor_pos: Option<Point>,
}

pub fn boot() -> (ViewerApp, Task<Message>) {
    let core = CORE_HANDLE
        .get()
        .expect("CORE_HANDLE not initialized")
        .clone();
    let snapshot = core.latest_snapshot();
    let (win_id, open_task) = window::open(window::Settings {
        size: Size::new(1240.0, 820.0),
        min_size: Some(Size::new(640.0, 420.0)),
        exit_on_close_request: false,
        ..Default::default()
    });

    let mut app = ViewerApp {
        core,
        viewer_mode: ViewerViewMode::Single,
        viewer_right_panels: SidebarOrderState::new(
            snapshot.meta.layout.viewer_right_panel_order.clone(),
            &[ViewerRightPanel::ViewAdjustment, ViewerRightPanel::Info],
        ),
        viewer_adjustments_open: false,
        viewer_info_open: false,
        viewer_filmstrip_open: false,
        aa_mode: snapshot.meta.layout.canvas_anti_aliasing,
        denoise_mode: snapshot.meta.layout.viewport_denoise,
        viewport_background: None,
        snapshot,
        window_id: Some(win_id),
        window_size: Size::new(1240.0, 820.0),
        is_fullscreen: false,
        viewer_frame: None,
        viewer_pair_frame: None,
        frame_cache: HashMap::new(),
        decoding_frames: HashSet::new(),
        image_sizes: HashMap::new(),
        thumbnails: HashMap::new(),
        decoding_thumbs: HashSet::new(),
        webtoon_pages: HashMap::new(),
        decoding_webtoon: HashSet::new(),
        webtoon_visible_index: None,
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        webtoon_zoom: 1.0,
        viewer_quarter_turns: 0,
        viewer_adj: ColorAdjustments::default(),
        viewer_manga_rtl: false,
        viewer_manga_cover_separate: false,
        viewer_manga_page_gap: 16,
        viewer_webtoon_fit_mode: WebtoonFitMode::default(),
        viewer_right_width_preview: None,
        viewer_right_split_preview: None,
        reader_options_open: false,
        utilities_menu_open: false,
        context_menu: None,
        pending_delete: None,
        translate: ViewerTranslateState::default(),
        cursor_pos: None,
    };

    let load_task = app.sync_viewer_media();
    let tree_task = app.report_ui_tree_task();
    (
        app,
        Task::batch([open_task.map(Message::WindowOpened), load_task, tree_task]),
    )
}

pub fn title(app: &ViewerApp, _id: window::Id) -> String {
    if let Some(path) = &app.snapshot.meta.viewer_path {
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path.as_str());
        format!("{name} — Photon Viewer")
    } else {
        "Photon Viewer".to_owned()
    }
}

pub fn theme(app: &ViewerApp, _id: window::Id) -> IcedTheme {
    match app.snapshot.meta.theme {
        PhotonTheme::Dark => IcedTheme::Dark,
        PhotonTheme::Light => IcedTheme::Light,
    }
}

pub fn scale_factor(app: &ViewerApp, _id: window::Id) -> f32 {
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

pub fn subscription(_app: &ViewerApp) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(state_watch_stream),
        Subscription::run(event_broadcast_stream),
        iced::event::listen().map(Message::IcedEvent),
        window::close_events().map(Message::WindowClosed),
        window::close_requests().map(Message::WindowCloseRequested),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ])
}

pub fn update(app: &mut ViewerApp, message: Message) -> Task<Message> {
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
            app.reader_options_open = false;
            app.utilities_menu_open = false;
            app.context_menu = None;
            Task::none()
        }
        Message::StateUpdated(snapshot) => {
            let prev_path = app.snapshot.meta.viewer_path.clone();
            let focus_changed =
                snapshot.meta.focus_request_seq != app.snapshot.meta.focus_request_seq;
            app.snapshot = snapshot;
            app.aa_mode = app.snapshot.meta.layout.canvas_anti_aliasing;
            app.denoise_mode = app.snapshot.meta.layout.viewport_denoise;
            if prev_path != app.snapshot.meta.viewer_path {
                app.zoom = 1.0;
                app.pan_x = 0.0;
                app.pan_y = 0.0;
                app.viewer_quarter_turns = 0;
            }
            let mut tasks = vec![app.sync_viewer_media(), app.report_ui_tree_task()];
            if focus_changed && let Some(id) = app.window_id {
                request_platform_activation(std::process::id());
                tasks.push(window::gain_focus(id));
            }
            Task::batch(tasks)
        }
        Message::CoreEventReceived(ev) => {
            if let photon_protocol::EventKind::UiActionRequested { target_id, .. } = ev.kind
                && target_id == "app.exit"
            {
                return iced::exit();
            }
            Task::none()
        }
        Message::FrameDecoded { path, frame } => {
            app.decoding_frames.remove(&path);
            if let Ok(frame) = frame {
                app.image_sizes
                    .insert(path.clone(), (frame.width, frame.height));
                if app.frame_cache.len() >= 12 {
                    app.frame_cache.clear();
                }
                app.frame_cache.insert(path.clone(), frame.clone());
                if app.snapshot.meta.viewer_path.as_deref() == Some(path.as_str()) {
                    app.viewer_frame = Some(frame);
                } else {
                    let pair_target = app.current_pair_path();
                    if pair_target.as_deref() == Some(path.as_str()) {
                        app.viewer_pair_frame = Some(frame);
                    }
                }
                return app.report_ui_tree_task();
            }
            Task::none()
        }
        Message::ThumbDecoded { path, handle } => {
            app.decoding_thumbs.remove(&path);
            if let Some(h) = handle {
                app.thumbnails.insert(path, h);
            }
            Task::none()
        }
        Message::WebtoonPageDecoded { path, handle } => {
            app.decoding_webtoon.remove(&path);
            if let Some(h) = handle {
                app.webtoon_pages.insert(path, h);
            }
            Task::none()
        }
        Message::PickedImages(paths) => {
            if !paths.is_empty() {
                let strs = paths
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                app.dispatch(Command::OpenFiles { paths: strs });
            }
            Task::none()
        }
        Message::IcedEvent(ev) => app.handle_iced_event(ev),
        Message::Viewer(vm) => app.handle_viewer_message(vm),
    }
}

pub fn view(app: &ViewerApp, _id: window::Id) -> Element<'_, Message> {
    let lang = app.snapshot.meta.language;
    let vstate = app.viewer_view_state();

    let right_utilities: Element<'_, ViewerMessage> = button::icon_only(
        Icon::Sliders,
        ButtonVariant::Ghost,
        Some(ViewerMessage::ToggleReaderOptions),
    )
    .into();

    let header: Element<'_, Message> = if app.is_fullscreen {
        Space::new().height(0).into()
    } else {
        column![
            build_viewer_toolbar(vstate, right_utilities).map(Message::Viewer),
            rule::horizontal(1).style(hairline_rule_style),
        ]
        .into()
    };

    let main_view = build_viewer_view(vstate).map(Message::Viewer);

    let status_bar: Element<'_, Message> = if app.is_fullscreen {
        Space::new().height(0).into()
    } else {
        let left_text = viewer_status_summary(vstate, lang);
        let right_text = viewer_counter_summary(vstate).unwrap_or_default();
        let zoom_pct = format!("{}%", (app.zoom * 100.0).round() as i32);
        container(
            row![
                text(left_text).size(TEXT_SIZE_CAPTION),
                Space::new().width(Fill),
                text(zoom_pct).size(TEXT_SIZE_CAPTION),
                Space::new().width(16),
                text(right_text).size(TEXT_SIZE_CAPTION),
            ]
            .align_y(Alignment::Center)
            .padding([4, 12]),
        )
        .width(Fill)
        .style(surface_panel_style)
        .into()
    };

    let base: Element<'_, Message> = column![header, main_view, status_bar]
        .width(Fill)
        .height(Fill)
        .into();

    let mut layers: Vec<Element<'_, Message>> = vec![base];

    if app.reader_options_open {
        let entries = build_viewer_quality_menu_entries(vstate);
        let right_off = viewer_reader_options_right_offset(vstate);
        let panel = menu_panel_entries(entries, 220.0).map(Message::Viewer);
        let backdrop = mouse_area(
            container(
                row![
                    Space::new().width(Fill),
                    column![Space::new().height(48), panel],
                    Space::new().width(right_off.max(8.0)),
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

    if let Some((ref target_path, pos)) = app.context_menu {
        let (est_h, menu_el, _sub_w, _sub_h) =
            build_viewer_context_menu_with_offset(vstate, target_path.as_deref());
        let menu_el = menu_el.map(Message::Viewer);
        let x = pos[0].clamp(8.0, (app.window_size.width - 230.0).max(8.0));
        let y = pos[1].clamp(8.0, (app.window_size.height - est_h - 12.0).max(8.0));
        let backdrop = mouse_area(
            container(
                column![
                    Space::new().height(y),
                    row![Space::new().width(x), menu_el].width(Fill),
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

    if let Some(paths) = &app.pending_delete {
        let msg = format!("Delete {} file(s) permanently?", paths.len());
        let dialog = container(
            container(
                column![
                    text(msg).size(TEXT_SIZE_CONTROL),
                    row![
                        button::labeled(
                            tr(lang, "action-cancel"),
                            None,
                            ButtonVariant::Secondary,
                            Some(Message::Viewer(ViewerMessage::ConfirmDeletePermanently(
                                Vec::new()
                            ))),
                        ),
                        button::labeled(
                            tr(lang, "action-delete"),
                            Some(Icon::Trash),
                            ButtonVariant::Danger,
                            Some(Message::Viewer(ViewerMessage::Dispatch(
                                Command::DeletePermanently {
                                    paths: paths.clone(),
                                }
                            ))),
                        ),
                    ]
                    .spacing(8),
                ]
                .spacing(12)
                .padding(16),
            )
            .style(surface_panel_style),
        )
        .center_x(Fill)
        .center_y(Fill)
        .width(Fill)
        .height(Fill);
        layers.push(dialog.into());
    }

    stack(layers).width(Fill).height(Fill).into()
}

impl ViewerApp {
    fn dispatch(&self, cmd: Command) {
        let _ = self
            .core
            .dispatch(Envelope::new("gui-viewer", ClientKind::Gui, cmd));
    }

    fn viewer_view_state(&self) -> ViewerViewState<'_> {
        ViewerViewState {
            snapshot: &self.snapshot,
            viewer_mode: self.viewer_mode,
            viewer_frame: self.viewer_frame.as_ref(),
            viewer_pair_frame: self.viewer_pair_frame.as_ref(),
            image_sizes: &self.image_sizes,
            thumbnails: &self.thumbnails,
            viewer_adj: self.viewer_adj,
            viewer_quarter_turns: self.viewer_quarter_turns,
            zoom: self.zoom,
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            webtoon_zoom: self.webtoon_zoom,
            webtoon_visible_index: self.webtoon_visible_index,
            webtoon_pages: &self.webtoon_pages,
            viewer_manga_rtl: self.viewer_manga_rtl,
            viewer_manga_cover_separate: self.viewer_manga_cover_separate,
            viewer_manga_page_gap: self.viewer_manga_page_gap,
            viewer_webtoon_fit_mode: self.viewer_webtoon_fit_mode,
            viewer_adjustments_open: self.viewer_adjustments_open,
            viewer_info_open: self.viewer_info_open,
            viewer_filmstrip_open: self.viewer_filmstrip_open,
            viewer_right_panels: &self.viewer_right_panels,
            viewer_right_width_preview: self.viewer_right_width_preview,
            viewer_right_split_preview: self.viewer_right_split_preview,
            is_fullscreen: self.is_fullscreen,
            window_size: self.window_size,
            is_main_window: false,
            reader_options_open: self.reader_options_open,
            workspace_switcher_open: false,
            utilities_menu_open: self.utilities_menu_open,
            aa_mode: self.aa_mode,
            denoise_mode: self.denoise_mode,
            viewport_background: self.viewport_background,
            translate: &self.translate,
        }
    }

    fn current_pair_path(&self) -> Option<String> {
        if self.viewer_mode != ViewerViewMode::Double {
            return None;
        }
        let vstate = self.viewer_view_state();
        let (left_slot, right_slot) = photon_viewer::dual_spread_slots(vstate);
        let cur = self.snapshot.meta.viewer_path.as_deref();
        match (left_slot.map(|(_, p)| p), right_slot.map(|(_, p)| p)) {
            (Some(l), Some(r)) if Some(l) == cur => Some(r.to_owned()),
            (Some(l), Some(r)) if Some(r) == cur => Some(l.to_owned()),
            _ => None,
        }
    }

    fn sync_viewer_media(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        if let Some(cur_path) = self.snapshot.meta.viewer_path.clone() {
            if let Some(cached) = self.frame_cache.get(&cur_path).cloned() {
                self.viewer_frame = Some(cached);
            } else {
                tasks.push(self.schedule_decode_frame(cur_path));
            }
        } else {
            self.viewer_frame = None;
        }

        if let Some(pair_path) = self.current_pair_path() {
            if let Some(cached) = self.frame_cache.get(&pair_path).cloned() {
                self.viewer_pair_frame = Some(cached);
            } else {
                tasks.push(self.schedule_decode_frame(pair_path));
            }
        } else {
            self.viewer_pair_frame = None;
        }

        // Pre-decode adjacent pages for instant Next/Prev navigation.
        if let Some(idx) = self.snapshot.meta.viewer_index {
            for adj in [idx.checked_sub(1), idx.checked_add(1)]
                .into_iter()
                .flatten()
            {
                if let Some(p) = self.snapshot.viewer_images.get(adj).cloned()
                    && !self.frame_cache.contains_key(&p)
                {
                    tasks.push(self.schedule_decode_frame(p));
                }
            }
        }

        if self.viewer_filmstrip_open {
            for p in self.snapshot.viewer_images.iter().take(48) {
                if !self.thumbnails.contains_key(p) && !self.decoding_thumbs.contains(p) {
                    self.decoding_thumbs.insert(p.clone());
                    let path = p.clone();
                    tasks.push(Task::perform(
                        spawn_bg(move || {
                            let handle = photon_image::generate_thumbnail(Path::new(&path), 160)
                                .ok()
                                .map(|img| {
                                    let (w, h) = img.dimensions();
                                    iced_image::Handle::from_rgba(w, h, img.into_raw())
                                });
                            (path, handle)
                        }),
                        |(path, handle)| Message::ThumbDecoded { path, handle },
                    ));
                }
            }
        }

        if self.viewer_mode == ViewerViewMode::Webtoon {
            for p in self.snapshot.viewer_images.iter().take(24) {
                if !self.webtoon_pages.contains_key(p) && !self.decoding_webtoon.contains(p) {
                    self.decoding_webtoon.insert(p.clone());
                    let path = p.clone();
                    tasks.push(Task::perform(
                        spawn_bg(move || {
                            let handle = decode_rgba(&path)
                                .ok()
                                .map(|(w, h, rgba)| iced_image::Handle::from_rgba(w, h, rgba));
                            (path, handle)
                        }),
                        |(path, handle)| Message::WebtoonPageDecoded { path, handle },
                    ));
                }
            }
        }

        Task::batch(tasks)
    }

    fn schedule_decode_frame(&mut self, path: String) -> Task<Message> {
        if !self.decoding_frames.insert(path.clone()) {
            return Task::none();
        }
        Task::perform(
            spawn_bg(move || {
                let frame = decode_rgba(&path).map(|(w, h, rgba)| ViewerImageFrame {
                    key: path.clone(),
                    revision: 1,
                    width: w,
                    height: h,
                    rgba: Arc::new(rgba),
                });
                (path, frame)
            }),
            |(path, frame)| Message::FrameDecoded { path, frame },
        )
    }

    fn handle_iced_event(&mut self, ev: Event) -> Task<Message> {
        match ev {
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                self.cursor_pos = Some(position);
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character("o") => {
                            let start_dir = self
                                .snapshot
                                .meta
                                .current_folder
                                .as_deref()
                                .map(std::path::PathBuf::from);
                            return Task::perform(
                                async move { open_images("Open Images", start_dir).await },
                                Message::PickedImages,
                            );
                        }
                        keyboard::Key::Character("w") | keyboard::Key::Character("q") => {
                            return iced::exit();
                        }
                        keyboard::Key::Character("0") => {
                            self.zoom = 1.0;
                            self.pan_x = 0.0;
                            self.pan_y = 0.0;
                        }
                        _ => {}
                    }
                    return Task::none();
                }
                match key {
                    Key::Named(Named::Escape) => {
                        if self.reader_options_open || self.context_menu.is_some() {
                            self.reader_options_open = false;
                            self.context_menu = None;
                        } else if self.is_fullscreen {
                            return self.toggle_fullscreen();
                        } else {
                            return iced::exit();
                        }
                    }
                    Key::Named(Named::F11) => return self.toggle_fullscreen(),
                    Key::Named(Named::ArrowRight | Named::PageDown | Named::Space) => {
                        if self.viewer_mode == ViewerViewMode::Double {
                            let step = if self.viewer_manga_rtl { -1 } else { 1 };
                            if let Some(target) = step_dual_spread(self.viewer_view_state(), step) {
                                self.dispatch(Command::ViewerOpen { path: target });
                            }
                        } else {
                            self.dispatch(Command::ViewerNext);
                        }
                    }
                    Key::Named(Named::ArrowLeft | Named::PageUp) => {
                        if self.viewer_mode == ViewerViewMode::Double {
                            let step = if self.viewer_manga_rtl { 1 } else { -1 };
                            if let Some(target) = step_dual_spread(self.viewer_view_state(), step) {
                                self.dispatch(Command::ViewerOpen { path: target });
                            }
                        } else {
                            self.dispatch(Command::ViewerPrevious);
                        }
                    }
                    Key::Named(Named::Home) => {
                        if let Some(first) = self.snapshot.viewer_images.first().cloned() {
                            self.dispatch(Command::ViewerOpen { path: first });
                        }
                    }
                    Key::Named(Named::End) => {
                        if let Some(last) = self.snapshot.viewer_images.last().cloned() {
                            self.dispatch(Command::ViewerOpen { path: last });
                        }
                    }
                    Key::Character(ref c) => match c.as_str() {
                        "f" | "F" => return self.toggle_fullscreen(),
                        "r" | "R" => {
                            self.viewer_quarter_turns = (self.viewer_quarter_turns + 1) % 4
                        }
                        "t" | "T" => {
                            self.viewer_filmstrip_open = !self.viewer_filmstrip_open;
                            return self.sync_viewer_media();
                        }
                        "i" | "I" => self.viewer_info_open = !self.viewer_info_open,
                        "+" | "=" => self.zoom = (self.zoom * 1.2).min(16.0),
                        "-" => self.zoom = (self.zoom / 1.2).max(0.05),
                        "0" => {
                            self.zoom = 1.0;
                            self.pan_x = 0.0;
                            self.pan_y = 0.0;
                        }
                        "q" | "Q" => {
                            return iced::exit();
                        }
                        _ => {}
                    },
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

    fn handle_viewer_message(&mut self, msg: ViewerMessage) -> Task<Message> {
        match msg {
            ViewerMessage::Noop => {}
            ViewerMessage::Dispatch(cmd) | ViewerMessage::ContextMenuDispatch(cmd) => {
                self.context_menu = None;
                self.pending_delete = None;
                self.dispatch(cmd);
            }
            ViewerMessage::PickImagesDialog => {
                let start_dir = self
                    .snapshot
                    .meta
                    .current_folder
                    .as_deref()
                    .map(std::path::PathBuf::from);
                return Task::perform(
                    async move { open_images("Open Images", start_dir).await },
                    Message::PickedImages,
                );
            }
            ViewerMessage::DualStepSpread(delta) => {
                if let Some(target) = step_dual_spread(self.viewer_view_state(), delta) {
                    self.dispatch(Command::ViewerOpen { path: target });
                }
            }
            ViewerMessage::ZoomStep(factor) => {
                self.zoom = (self.zoom * factor).clamp(0.05, 16.0);
            }
            ViewerMessage::ViewerZoomActual => {
                if let Some(frame) = &self.viewer_frame {
                    let vp_w = self.window_size.width.max(1.0);
                    let vp_h = self.window_size.height.max(1.0);
                    let fit =
                        (vp_w / frame.width.max(1) as f32).min(vp_h / frame.height.max(1) as f32);
                    if fit > 0.001 {
                        self.zoom = (1.0 / fit).clamp(0.05, 16.0);
                    }
                    self.pan_x = 0.0;
                    self.pan_y = 0.0;
                }
            }
            ViewerMessage::ViewerRotatePreview(delta) => {
                let next = (i16::from(self.viewer_quarter_turns) + i16::from(delta)).rem_euclid(4);
                self.viewer_quarter_turns = next as u8;
            }
            ViewerMessage::ToggleFullscreen => return self.toggle_fullscreen(),
            ViewerMessage::SetViewerViewMode(mode) => {
                self.viewer_mode = mode;
                if mode == ViewerViewMode::Webtoon
                    && let Some(idx) = self.snapshot.meta.viewer_index
                    && let Some(off) = webtoon_offset_for_index(self.viewer_view_state(), idx)
                {
                    return iced::widget::operation::scroll_to(
                        webtoon_scroll_id(),
                        scrollable::AbsoluteOffset { x: 0.0, y: off },
                    );
                }
                return self.sync_viewer_media();
            }
            ViewerMessage::SetAntiAliasingMode(mode) => {
                self.aa_mode = mode;
                self.reader_options_open = false;
            }
            ViewerMessage::SetViewportDenoiseMode(mode) => {
                self.denoise_mode = mode;
                self.reader_options_open = false;
            }
            ViewerMessage::ToggleReaderOptions => {
                self.reader_options_open = !self.reader_options_open;
            }
            ViewerMessage::ToggleFilmstrip | ViewerMessage::CloseFilmstrip => {
                self.viewer_filmstrip_open = !self.viewer_filmstrip_open;
                return self.sync_viewer_media();
            }
            ViewerMessage::FilmstripWheelScroll(delta) => {
                let dx = match delta {
                    iced::mouse::ScrollDelta::Lines { y, .. } => -y * 60.0,
                    iced::mouse::ScrollDelta::Pixels { y, .. } => -y,
                };
                return iced::widget::operation::scroll_by(
                    filmstrip_scroll_id(),
                    scrollable::AbsoluteOffset { x: dx, y: 0.0 },
                );
            }
            ViewerMessage::ToggleViewerInspector => {
                self.viewer_info_open = !self.viewer_info_open;
            }
            ViewerMessage::ToggleViewerColorPopover => {
                self.viewer_adjustments_open = !self.viewer_adjustments_open;
            }
            ViewerMessage::CloseViewerPanel(panel) => match panel {
                ViewerRightPanel::Info => self.viewer_info_open = false,
                ViewerRightPanel::ViewAdjustment => self.viewer_adjustments_open = false,
                ViewerRightPanel::Translate => self.translate.active = false,
            },
            ViewerMessage::SetViewerAdjExposure(v) => self.viewer_adj.exposure = v,
            ViewerMessage::SetViewerAdjGamma(v) => self.viewer_adj.gamma = v,
            ViewerMessage::SetViewerAdjBrightness(v) => self.viewer_adj.brightness = v,
            ViewerMessage::SetViewerAdjContrast(v) => self.viewer_adj.contrast = v,
            ViewerMessage::SetViewerAdjSaturation(v) => self.viewer_adj.saturation = v,
            ViewerMessage::SetViewerAdjHighlights(v) => self.viewer_adj.highlights = v,
            ViewerMessage::SetViewerAdjShadows(v) => self.viewer_adj.shadows = v,
            ViewerMessage::ResetViewerAdj => self.viewer_adj = ColorAdjustments::default(),
            ViewerMessage::OpenViewerContextMenu { path, pos } => {
                let p = pos.unwrap_or_else(|| {
                    self.cursor_pos
                        .map(|c| [c.x, c.y])
                        .unwrap_or([self.window_size.width * 0.5, self.window_size.height * 0.5])
                });
                self.context_menu = Some((path, p));
            }
            ViewerMessage::ConfirmDeletePermanently(paths) => {
                self.context_menu = None;
                self.pending_delete = if paths.is_empty() { None } else { Some(paths) };
            }
            ViewerMessage::CopyViewerPath => {
                if let Some(p) = &self.snapshot.meta.viewer_path
                    && let Ok(mut cb) = arboard::Clipboard::new()
                {
                    let _ = cb.set_text(p.clone());
                }
                self.context_menu = None;
            }
            ViewerMessage::CopyImageToClipboard(target) => {
                let path = target.or_else(|| self.snapshot.meta.viewer_path.clone());
                if let Some(p) = path
                    && let Ok((w, h, rgba)) = decode_rgba(&p)
                    && let Ok(mut cb) = arboard::Clipboard::new()
                {
                    let _ = cb.set_image(arboard::ImageData {
                        width: w as usize,
                        height: h as usize,
                        bytes: std::borrow::Cow::Owned(rgba),
                    });
                }
                self.context_menu = None;
            }
            ViewerMessage::EditPath(path) => {
                launch_sibling_bin("photon-editor", &[path]);
                self.context_menu = None;
            }
            ViewerMessage::ViewerAdjOpenInEditor => {
                if let Some(p) = &self.snapshot.meta.viewer_path {
                    launch_sibling_bin("photon-editor", &[p.clone()]);
                }
            }
            ViewerMessage::OpenBrowserWindow => {
                launch_sibling_bin("photon-manager", &[]);
            }
            ViewerMessage::ShowInBrowser(path) => {
                let folder = Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(path);
                launch_sibling_bin("photon-manager", &[folder]);
                self.context_menu = None;
            }
            ViewerMessage::OpenInNewViewerWindow(path) => {
                spawn_viewer_window_child(&path, false, None);
                self.context_menu = None;
            }
            ViewerMessage::WebtoonScrolled(vp) => {
                let center_y = vp.absolute_offset().y + self.window_size.height * 0.5;
                self.webtoon_visible_index =
                    webtoon_index_at_scroll(self.viewer_view_state(), center_y);
            }
            ViewerMessage::Canvas(ev) | ViewerMessage::ViewerCanvas(_, ev) => match ev {
                ViewerCanvasEvent::ZoomBy {
                    factor,
                    cursor,
                    bounds,
                } => {
                    let old_zoom = self.zoom.max(0.02);
                    let new_zoom = (old_zoom * factor).clamp(0.05, 16.0);
                    let ratio = new_zoom / old_zoom;
                    if let Some([cx, cy]) = cursor {
                        let dx = cx - bounds[0] * 0.5 - self.pan_x;
                        let dy = cy - bounds[1] * 0.5 - self.pan_y;
                        self.pan_x += dx * (1.0 - ratio);
                        self.pan_y += dy * (1.0 - ratio);
                    }
                    self.zoom = new_zoom;
                }
                ViewerCanvasEvent::PanBy { dx, dy } => {
                    self.pan_x += dx;
                    self.pan_y += dy;
                }
                ViewerCanvasEvent::ResetFit => {
                    self.zoom = 1.0;
                    self.pan_x = 0.0;
                    self.pan_y = 0.0;
                }
                ViewerCanvasEvent::ContextMenuRequested { pos, .. } => {
                    self.context_menu = Some((self.snapshot.meta.viewer_path.clone(), pos));
                }
                ViewerCanvasEvent::SelectMarqueeCommit { .. } => {}
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
            workspace: Workspace::Viewer,
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
                id: "viewer-root".to_owned(),
                role: "window".to_owned(),
                label: self.snapshot.meta.viewer_path.clone().unwrap_or_default(),
                workspace: Workspace::Viewer,
                enabled: true,
                visible: true,
                bounds: None,
                value: Some(format!("{:.2}", self.zoom)),
            }],
        };
        let core = self.core.clone();
        Task::perform(
            async move {
                let _ = core.send_custom(ViewerCoreCustom::SetUiTree(tree)).await;
            },
            |_| Message::Noop,
        )
    }
}

fn launch_sibling_bin(name: &str, args: &[String]) {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from(name));
    let _ = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn decode_rgba(path: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let img = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Ok((w, h, rgba.into_raw()))
}
