use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use fw_dialogs::{open_with_dialog, pick_folder};
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
use iced::widget::{Space, column, container, image as iced_image, mouse_area, row, rule, stack};
use iced::{
    Element, Event, Fill, Font, Point, Size, Subscription, Task, Theme as IcedTheme, window,
};
use photon_app_common::ipc_launch::spawn_viewer_window_child;
use photon_app_common::window_activation::request_platform_activation;
use photon_browser::{
    BrowserMenuKind, BrowserMessage, BrowserViewState,
    build_browser_context_menu_entries_with_offset, build_browser_edit_menu_entries,
    build_browser_help_menu_entries, build_browser_plugins_menu_entries, build_browser_sort_menu,
    build_browser_toolbar, build_browser_view, build_browser_window_menu_entries,
    build_quick_access_context_menu,
};
use photon_i18n::tr;
use photon_protocol::{
    BrowserLeftPanel, BrowserRightPanel, ClientKind, Command, DisplayFont, Event as CoreEvent,
    EventKind, PresentationState, Query, SemanticWidgetNode, Theme as PhotonTheme, UiStateSnapshot,
    UiTreeSnapshot, Workspace,
};

use crate::core::{ManagerCore, ManagerCoreCustom};

static CORE_HANDLE: OnceLock<CoreHandle<ManagerCore>> = OnceLock::new();
static BG_RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();
static GPU_BACKEND: OnceLock<String> = OnceLock::new();
static CLEANUP_SOCKET: OnceLock<PathBuf> = OnceLock::new();

pub fn init_globals(
    core: CoreHandle<ManagerCore>,
    bg_rt: tokio::runtime::Handle,
    gpu_backend: String,
    socket_path: PathBuf,
) {
    let _ = CORE_HANDLE.set(core);
    let _ = BG_RUNTIME.set(bg_rt);
    let _ = GPU_BACKEND.set(gpu_backend);
    let _ = CLEANUP_SOCKET.set(socket_path);
}

fn fast_exit() -> ! {
    if let Some(sock) = CLEANUP_SOCKET.get() {
        let _ = std::fs::remove_file(sock);
    }
    photon_storage::logging::log_shutdown();
    std::process::exit(0);
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
    Browser(BrowserMessage),
    IcedEvent(Event),
    StateUpdated(Arc<UiStateSnapshot>),
    CoreEventReceived(CoreEvent),
    ThumbDecoded {
        path: String,
        handle: Option<iced_image::Handle>,
    },
    FoldersListed {
        parent: String,
        folders: Vec<(String, String)>,
    },
    PickedFolder(Option<PathBuf>),
    WindowOpened(window::Id),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    WindowResized(window::Id, Size),
    CloseOverlays,
    Noop,
}

pub struct ManagerApp {
    core: CoreHandle<ManagerCore>,
    snapshot: Arc<UiStateSnapshot>,
    window_id: Option<window::Id>,
    window_size: Size,
    is_fullscreen: bool,
    thumbnails: HashMap<String, iced_image::Handle>,
    decoding_thumbs: HashSet<String>,
    expanded_folders: BTreeSet<String>,
    folder_children: HashMap<String, Vec<(String, String)>>,
    folder_input: String,
    path_editing: bool,
    browser_filter: String,
    browser_filter_bar_open: bool,
    browser_filter_case_sensitive: bool,
    browser_filter_regex: bool,
    browser_quick_access_open: bool,
    browser_quick_access_collapsed: bool,
    quick_access_editing: bool,
    browser_explorer_open: bool,
    browser_explorer_collapsed: bool,
    browser_preview_open: bool,
    browser_preview_collapsed: bool,
    browser_info_open: bool,
    browser_info_collapsed: bool,
    browser_left_panels: SidebarOrderState<BrowserLeftPanel>,
    browser_right_panels: SidebarOrderState<BrowserRightPanel>,
    browser_left_width_drag: Option<f32>,
    browser_right_width_drag: Option<f32>,
    active_menu: Option<BrowserMenuKind>,
    context_menu: Option<(String, bool, [f32; 2])>,
    pending_delete: Option<Vec<String>>,
    last_cursor: Option<Point>,
}

pub fn boot() -> (ManagerApp, Task<Message>) {
    let core = CORE_HANDLE
        .get()
        .expect("CORE_HANDLE not initialized")
        .clone();
    let snapshot = core.latest_snapshot();
    let (win_id, open_task) = window::open(window::Settings {
        size: Size::new(1280.0, 840.0),
        min_size: Some(Size::new(720.0, 480.0)),
        exit_on_close_request: false,
        ..Default::default()
    });

    let folder_input = snapshot.meta.current_folder.clone().unwrap_or_default();
    let mut app = ManagerApp {
        core,
        browser_left_panels: SidebarOrderState::new(
            snapshot.meta.layout.browser_left_panel_order.clone(),
            &[BrowserLeftPanel::QuickAccess, BrowserLeftPanel::Explorer],
        ),
        browser_right_panels: SidebarOrderState::new(
            snapshot.meta.layout.browser_right_panel_order.clone(),
            &[BrowserRightPanel::Preview, BrowserRightPanel::Info],
        ),
        snapshot,
        window_id: Some(win_id),
        window_size: Size::new(1280.0, 840.0),
        is_fullscreen: false,
        thumbnails: HashMap::new(),
        decoding_thumbs: HashSet::new(),
        expanded_folders: BTreeSet::new(),
        folder_children: HashMap::new(),
        folder_input,
        path_editing: false,
        browser_filter: String::new(),
        browser_filter_bar_open: false,
        browser_filter_case_sensitive: false,
        browser_filter_regex: false,
        browser_quick_access_open: true,
        browser_quick_access_collapsed: false,
        quick_access_editing: false,
        browser_explorer_open: true,
        browser_explorer_collapsed: false,
        browser_preview_open: true,
        browser_preview_collapsed: false,
        browser_info_open: true,
        browser_info_collapsed: false,
        browser_left_width_drag: None,
        browser_right_width_drag: None,
        active_menu: None,
        context_menu: None,
        pending_delete: None,
        last_cursor: None,
    };

    let thumb_task = app.schedule_visible_thumbnails(0, 60);
    let tree_task = app.report_ui_tree_task();
    let folder_task = app.refresh_root_folders();
    (
        app,
        Task::batch([
            open_task.map(Message::WindowOpened),
            thumb_task,
            tree_task,
            folder_task,
        ]),
    )
}

pub fn title(app: &ManagerApp, _id: window::Id) -> String {
    if let Some(folder) = &app.snapshot.meta.current_folder {
        format!("{folder} — Photon Manager")
    } else {
        "Photon Manager".to_owned()
    }
}

pub fn theme(app: &ManagerApp, _id: window::Id) -> IcedTheme {
    match app.snapshot.meta.theme {
        PhotonTheme::Dark => IcedTheme::Dark,
        PhotonTheme::Light => IcedTheme::Light,
    }
}

pub fn scale_factor(app: &ManagerApp, _id: window::Id) -> f32 {
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

pub fn subscription(_app: &ManagerApp) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(state_watch_stream),
        Subscription::run(event_broadcast_stream),
        iced::event::listen().map(Message::IcedEvent),
        window::close_events().map(Message::WindowClosed),
        window::close_requests().map(Message::WindowCloseRequested),
        window::resize_events().map(|(id, size)| Message::WindowResized(id, size)),
    ])
}

pub fn update(app: &mut ManagerApp, message: Message) -> Task<Message> {
    match message {
        Message::Noop => Task::none(),
        Message::WindowOpened(id) => {
            app.window_id = Some(id);
            app.report_ui_tree_task()
        }
        Message::WindowCloseRequested(_id) => fast_exit(),
        Message::WindowClosed(id) => {
            if app.window_id == Some(id) {
                fast_exit();
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
            let folder_changed = snapshot.meta.current_folder != app.snapshot.meta.current_folder;
            let focus_changed =
                snapshot.meta.focus_request_seq != app.snapshot.meta.focus_request_seq;
            app.snapshot = snapshot;
            if folder_changed && !app.path_editing {
                app.folder_input = app.snapshot.meta.current_folder.clone().unwrap_or_default();
            }
            let mut tasks = vec![
                app.schedule_visible_thumbnails(0, 60),
                app.report_ui_tree_task(),
            ];
            if folder_changed {
                tasks.push(app.refresh_root_folders());
            }
            if focus_changed && let Some(id) = app.window_id {
                request_platform_activation(std::process::id());
                tasks.push(window::gain_focus(id));
            }
            Task::batch(tasks)
        }
        Message::CoreEventReceived(ev) => {
            if let EventKind::UiActionRequested { target_id, .. } = ev.kind
                && target_id == "app.exit"
            {
                fast_exit();
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
        Message::FoldersListed { parent, folders } => {
            app.folder_children.insert(parent, folders);
            Task::none()
        }
        Message::PickedFolder(folder) => {
            if let Some(dir) = folder {
                app.dispatch(Command::OpenFolder {
                    path: dir.to_string_lossy().into_owned(),
                });
            }
            Task::none()
        }
        Message::IcedEvent(ev) => app.handle_iced_event(ev),
        Message::Browser(bm) => app.handle_browser_message(bm),
    }
}

pub fn view(app: &ManagerApp, _id: window::Id) -> Element<'_, Message> {
    let lang = app.snapshot.meta.language;
    let bstate = app.browser_view_state();

    let right_utilities: Element<'_, BrowserMessage> = button::icon_only(
        Icon::Fullscreen,
        ButtonVariant::Ghost,
        Some(BrowserMessage::ToggleFullscreen),
    )
    .into();

    let header: Element<'_, Message> = if app.is_fullscreen {
        Space::new().height(0).into()
    } else {
        column![
            build_browser_toolbar(bstate, right_utilities).map(Message::Browser),
            rule::horizontal(1).style(hairline_rule_style),
        ]
        .into()
    };

    let main_view = build_browser_view(bstate).map(Message::Browser);
    let count = app.snapshot.entries.len();
    let sel_count = app.snapshot.meta.selection.len();
    let status_text = if sel_count > 0 {
        format!("{count} items · {sel_count} selected")
    } else {
        format!("{count} items")
    };
    let status_bar: Element<'_, Message> = container(
        row![
            text(status_text).size(TEXT_SIZE_CAPTION),
            Space::new().width(Fill),
            text(app.snapshot.meta.status.english()).size(TEXT_SIZE_CAPTION),
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
        let menu_el: Option<Element<'_, BrowserMessage>> = match menu_kind {
            BrowserMenuKind::Sort => Some(build_browser_sort_menu(bstate)),
            BrowserMenuKind::Edit => Some(menu_panel_entries(
                build_browser_edit_menu_entries(bstate),
                220.0,
            )),
            BrowserMenuKind::Plugins => Some(menu_panel_entries(
                build_browser_plugins_menu_entries(bstate),
                220.0,
            )),
            BrowserMenuKind::Window => Some(menu_panel_entries(
                build_browser_window_menu_entries(bstate),
                220.0,
            )),
            BrowserMenuKind::Help => Some(menu_panel_entries(
                build_browser_help_menu_entries(lang),
                220.0,
            )),
            _ => None,
        };
        if let Some(el) = menu_el {
            let backdrop = mouse_area(
                container(column![Space::new().height(48), el.map(Message::Browser)])
                    .padding([0, 12])
                    .width(Fill)
                    .height(Fill),
            )
            .on_press(Message::CloseOverlays);
            layers.push(backdrop.into());
        }
    }

    if let Some((ref target_path, is_qa, pos)) = app.context_menu {
        let menu_el: Element<'_, Message> = if is_qa {
            let (_, el) = build_quick_access_context_menu(bstate, target_path);
            el.map(Message::Browser)
        } else {
            let (_, el, _) = build_browser_context_menu_entries_with_offset(bstate, target_path);
            el.map(Message::Browser)
        };
        let x = pos[0].clamp(8.0, (app.window_size.width - 230.0).max(8.0));
        let y = pos[1].clamp(8.0, (app.window_size.height - 320.0).max(8.0));
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
                            Some(Message::Browser(
                                BrowserMessage::BrowserConfirmDeletePermanently(Vec::new())
                            )),
                        ),
                        button::labeled(
                            tr(lang, "action-delete"),
                            Some(Icon::Trash),
                            ButtonVariant::Danger,
                            Some(Message::Browser(BrowserMessage::Dispatch(
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

impl ManagerApp {
    fn dispatch(&self, cmd: Command) {
        let _ = self
            .core
            .dispatch(Envelope::new("gui-manager", ClientKind::Gui, cmd));
    }

    fn browser_view_state(&self) -> BrowserViewState<'_> {
        BrowserViewState {
            snapshot: &self.snapshot,
            thumbnails: &self.thumbnails,
            expanded_folders: &self.expanded_folders,
            folder_children: &self.folder_children,
            folder_input: &self.folder_input,
            path_editing: self.path_editing,
            browser_filter: &self.browser_filter,
            browser_filter_bar_open: self.browser_filter_bar_open,
            browser_filter_case_sensitive: self.browser_filter_case_sensitive,
            browser_filter_regex: self.browser_filter_regex,
            browser_quick_access_open: self.browser_quick_access_open,
            browser_quick_access_collapsed: self.browser_quick_access_collapsed,
            quick_access_editing: self.quick_access_editing,
            browser_explorer_open: self.browser_explorer_open,
            browser_explorer_collapsed: self.browser_explorer_collapsed,
            browser_preview_open: self.browser_preview_open,
            browser_preview_collapsed: self.browser_preview_collapsed,
            browser_info_open: self.browser_info_open,
            browser_info_collapsed: self.browser_info_collapsed,
            browser_left_panels: &self.browser_left_panels,
            browser_right_panels: &self.browser_right_panels,
            browser_left_width_drag: self.browser_left_width_drag,
            browser_right_width_drag: self.browser_right_width_drag,
            is_fullscreen: self.is_fullscreen,
            last_cursor: self.last_cursor,
            window_size: self.window_size,
            active_menu: self.active_menu,
        }
    }

    fn schedule_visible_thumbnails(&mut self, start: usize, end: usize) -> Task<Message> {
        let mut tasks = Vec::new();
        let total = self.snapshot.entries.len();
        if total == 0 {
            return Task::none();
        }
        let s = start.min(total);
        let e = end.max(s + 24).min(total);
        for entry in &self.snapshot.entries[s..e] {
            if entry.is_dir {
                continue;
            }
            if !self.thumbnails.contains_key(&entry.path)
                && !self.decoding_thumbs.contains(&entry.path)
            {
                self.decoding_thumbs.insert(entry.path.clone());
                let path = entry.path.clone();
                tasks.push(Task::perform(
                    spawn_bg(move || {
                        let handle = photon_image::generate_thumbnail(Path::new(&path), 240)
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
        Task::batch(tasks)
    }

    fn refresh_root_folders(&self) -> Task<Message> {
        let Some(root) = self.snapshot.meta.current_folder.clone() else {
            return Task::none();
        };
        self.list_folders_task(root)
    }

    fn list_folders_task(&self, parent: String) -> Task<Message> {
        let core = self.core.clone();
        Task::perform(
            async move {
                let mut folders = Vec::new();
                if let Ok(Ok(val)) = core
                    .query(Query::ListFolders {
                        path: parent.clone(),
                    })
                    .await
                    && let Some(arr) = val.get("folders").and_then(|v| v.as_array())
                {
                    for item in arr {
                        if let (Some(name), Some(child_path)) = (
                            item.get("name").and_then(|v| v.as_str()),
                            item.get("path").and_then(|v| v.as_str()),
                        ) {
                            folders.push((name.to_owned(), child_path.to_owned()));
                        }
                    }
                }
                (parent, folders)
            },
            |(parent, folders)| Message::FoldersListed { parent, folders },
        )
    }

    fn handle_iced_event(&mut self, ev: Event) -> Task<Message> {
        match ev {
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                self.last_cursor = Some(position);
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
                                .map(PathBuf::from);
                            return Task::perform(
                                async move { pick_folder("Open Folder", start_dir).await },
                                Message::PickedFolder,
                            );
                        }
                        keyboard::Key::Character("f") => {
                            self.browser_filter_bar_open = !self.browser_filter_bar_open;
                        }
                        keyboard::Key::Character("w") | keyboard::Key::Character("q") => {
                            fast_exit();
                        }
                        _ => {}
                    }
                    return Task::none();
                }
                match key {
                    Key::Named(Named::Escape) => {
                        self.active_menu = None;
                        self.context_menu = None;
                        self.path_editing = false;
                    }
                    Key::Named(Named::F11) => return self.toggle_fullscreen(),
                    Key::Named(Named::F5) => self.dispatch(Command::Refresh),
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

    fn handle_browser_message(&mut self, msg: BrowserMessage) -> Task<Message> {
        match msg {
            BrowserMessage::Noop => {}
            BrowserMessage::Dispatch(cmd) | BrowserMessage::ContextMenuDispatch(cmd) => {
                self.active_menu = None;
                self.context_menu = None;
                self.pending_delete = None;
                self.dispatch(cmd);
            }
            BrowserMessage::ToggleMenu(kind) => {
                self.active_menu = if self.active_menu == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
            }
            BrowserMessage::ToggleFullscreen => return self.toggle_fullscreen(),
            BrowserMessage::PickFolderDialog => {
                let start_dir = self
                    .snapshot
                    .meta
                    .current_folder
                    .as_deref()
                    .map(PathBuf::from);
                return Task::perform(
                    async move { pick_folder("Open Folder", start_dir).await },
                    Message::PickedFolder,
                );
            }
            BrowserMessage::TogglePathEdit(editing) => {
                self.path_editing = editing;
                if editing {
                    self.folder_input = self
                        .snapshot
                        .meta
                        .current_folder
                        .clone()
                        .unwrap_or_default();
                }
            }
            BrowserMessage::FolderInputChanged(s) => self.folder_input = s,
            BrowserMessage::SubmitFolderInput => {
                self.path_editing = false;
                let path = self.folder_input.trim().to_owned();
                if !path.is_empty() {
                    self.dispatch(Command::OpenFolder { path });
                }
            }
            BrowserMessage::ToggleBrowserFilterBar => {
                self.browser_filter_bar_open = !self.browser_filter_bar_open;
            }
            BrowserMessage::BrowserFilterChanged(s) => self.browser_filter = s,
            BrowserMessage::ClearBrowserFilter => self.browser_filter.clear(),
            BrowserMessage::ToggleBrowserFilterCaseSensitive => {
                self.browser_filter_case_sensitive = !self.browser_filter_case_sensitive;
            }
            BrowserMessage::ToggleBrowserFilterRegex => {
                self.browser_filter_regex = !self.browser_filter_regex;
            }
            BrowserMessage::BrowserEntryActivated {
                path,
                is_dir,
                is_image,
            } => {
                if is_dir {
                    self.dispatch(Command::OpenFolder { path });
                } else if is_image {
                    spawn_viewer_window_child(&path, false, None);
                }
            }
            BrowserMessage::ToggleFolderExpand(path) => {
                if !self.expanded_folders.remove(&path) {
                    self.expanded_folders.insert(path.clone());
                    return self.list_folders_task(path);
                }
            }
            BrowserMessage::OpenBrowserContextMenu { path, x, y } => {
                self.context_menu = Some((path, false, [x, y]));
            }
            BrowserMessage::OpenQuickAccessContextMenu { path, x, y } => {
                self.context_menu = Some((path, true, [x, y]));
            }
            BrowserMessage::BrowserCopyPath(path) => {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(path);
                }
                self.context_menu = None;
            }
            BrowserMessage::BrowserConfirmDeletePermanently(paths) => {
                self.context_menu = None;
                self.pending_delete = if paths.is_empty() { None } else { Some(paths) };
            }
            BrowserMessage::OpenInViewerWindow(path) => {
                spawn_viewer_window_child(&path, false, None);
                self.context_menu = None;
            }
            BrowserMessage::EditPath(path) => {
                launch_sibling_bin("photon-editor", &[path]);
                self.context_menu = None;
            }
            BrowserMessage::OpenWith(path) => {
                self.context_menu = None;
                return Task::perform(open_with_dialog(PathBuf::from(path)), |_| Message::Noop);
            }
            BrowserMessage::VisibleRangeChanged { start, end } => {
                return self.schedule_visible_thumbnails(start, end);
            }
            BrowserMessage::ToggleBrowserLeftPanelCollapse(p) => match p {
                BrowserLeftPanel::QuickAccess => {
                    self.browser_quick_access_collapsed = !self.browser_quick_access_collapsed
                }
                BrowserLeftPanel::Explorer => {
                    self.browser_explorer_collapsed = !self.browser_explorer_collapsed
                }
            },
            BrowserMessage::CloseBrowserLeftPanel(p) => match p {
                BrowserLeftPanel::QuickAccess => self.browser_quick_access_open = false,
                BrowserLeftPanel::Explorer => self.browser_explorer_open = false,
            },
            BrowserMessage::ToggleBrowserRightPanelCollapse(p) => match p {
                BrowserRightPanel::Preview => {
                    self.browser_preview_collapsed = !self.browser_preview_collapsed
                }
                BrowserRightPanel::Info => {
                    self.browser_info_collapsed = !self.browser_info_collapsed
                }
            },
            BrowserMessage::CloseBrowserRightPanel(p) => match p {
                BrowserRightPanel::Preview => self.browser_preview_open = false,
                BrowserRightPanel::Info => self.browser_info_open = false,
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
            workspace: Workspace::Browser,
            presentation: PresentationState {
                window_count: 1,
                window_width: self.window_size.width,
                window_height: self.window_size.height,
                scale_factor: self.snapshot.meta.layout.ui_scale(),
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
                gpu_backend,
                gpu_adapter: String::new(),
            },
            widgets: vec![SemanticWidgetNode {
                id: "manager-root".to_owned(),
                role: "window".to_owned(),
                label: self
                    .snapshot
                    .meta
                    .current_folder
                    .clone()
                    .unwrap_or_default(),
                workspace: Workspace::Browser,
                enabled: true,
                visible: true,
                bounds: None,
                value: None,
            }],
        };
        let core = self.core.clone();
        Task::perform(
            async move {
                let _ = core.send_custom(ManagerCoreCustom::SetUiTree(tree)).await;
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
        .unwrap_or_else(|| PathBuf::from(name));
    let _ = std::process::Command::new(exe)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
