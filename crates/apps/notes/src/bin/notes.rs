use std::sync::{Arc, OnceLock};

use clap::Parser;
use fw_ipc::serve;
use fw_protocol::{ClientKind, Envelope};
use fw_runtime::{CoreHandle, CoreRuntime};
use fw_ui::button::{self, ButtonVariant};
use fw_ui::icons::Icon;
use fw_ui::theme::{TEXT_SIZE_CAPTION, hairline_rule_style, surface_panel_style};
use fw_ui::typography::text;
use iced::widget::{Space, column, container, row, rule, scrollable, text_input};
use iced::{Element, Fill, Size, Subscription, Task, Theme, window};
use notes_app::{NoteCommand, NotesCore, NotesState, notes_paths};

#[derive(Parser)]
#[command(name = "notes", version, about = "Minimal Notes app on fw-*")]
struct Cli {
    #[arg(long)]
    headless: bool,
}

static CORE_HANDLE: OnceLock<CoreHandle<NotesCore>> = OnceLock::new();

fn main() {
    let cli = Cli::parse();
    let paths = notes_paths();
    let _ = paths.ensure_runtime_dirs();
    let core = NotesCore::new(Some(paths.data_dir.join("notes.json")));

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let handle = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let socket_path = paths.socket_path.clone();
        rt.spawn(async move {
            if let Err(err) = serve(&socket_path, ipc_handle).await {
                tracing::error!(target: "notes::ipc", %err, "IPC server stopped");
            }
        });
        handle
    };

    if cli.headless {
        rt.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
        return;
    }

    let _rt_keeper = std::thread::spawn(move || {
        rt.block_on(std::future::pending::<()>());
    });

    #[cfg(target_os = "linux")]
    if std::env::var_os("WGPU_BACKEND").is_none() {
        unsafe { std::env::set_var("WGPU_BACKEND", "vulkan") };
    }

    let _ = CORE_HANDLE.set(handle);

    let _ = iced::daemon(boot, update, view)
        .title(title)
        .theme(theme)
        .subscription(subscription)
        .run();
}

#[derive(Debug, Clone)]
enum Message {
    StateUpdated(Arc<NotesState>),
    Dispatch(NoteCommand),
    TitleChanged(String),
    BodyChanged(String),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    Noop,
}

struct NotesGui {
    core: CoreHandle<NotesCore>,
    state: Arc<NotesState>,
    window_id: Option<window::Id>,
    title_buf: String,
    body_buf: String,
}

fn boot() -> (NotesGui, Task<Message>) {
    let core = CORE_HANDLE.get().expect("CORE_HANDLE").clone();
    let state = core.latest_snapshot();
    let (title_buf, body_buf) = state
        .selected_id
        .and_then(|id| state.notes.iter().find(|n| n.id == id))
        .map(|n| (n.title.clone(), n.body.clone()))
        .unwrap_or_default();
    let (win_id, open_task) = window::open(window::Settings {
        size: Size::new(920.0, 620.0),
        exit_on_close_request: false,
        ..Default::default()
    });
    (
        NotesGui {
            core,
            state,
            window_id: Some(win_id),
            title_buf,
            body_buf,
        },
        open_task.map(|_| Message::Noop),
    )
}

fn title(app: &NotesGui, _id: window::Id) -> String {
    format!("Notes (rev {})", app.state.revision)
}

fn theme(_app: &NotesGui, _id: window::Id) -> Theme {
    Theme::Dark
}

fn state_stream() -> impl iced::futures::Stream<Item = Message> {
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

fn subscription(_app: &NotesGui) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(state_stream),
        window::close_requests().map(Message::WindowCloseRequested),
        window::close_events().map(Message::WindowClosed),
    ])
}

fn update(app: &mut NotesGui, msg: Message) -> Task<Message> {
    match msg {
        Message::Noop => Task::none(),
        Message::WindowCloseRequested(id) => window::close(id),
        Message::WindowClosed(id) => {
            if app.window_id == Some(id) {
                app.window_id = None;
                return iced::exit();
            }
            Task::none()
        }
        Message::StateUpdated(snap) => {
            if let Some(note) = snap
                .selected_id
                .and_then(|id| snap.notes.iter().find(|n| n.id == id))
            {
                app.title_buf = note.title.clone();
                app.body_buf = note.body.clone();
            }
            app.state = snap;
            Task::none()
        }
        Message::Dispatch(cmd) => {
            let _ = app
                .core
                .dispatch(Envelope::new("notes-gui", ClientKind::Gui, cmd));
            Task::none()
        }
        Message::TitleChanged(s) => {
            app.title_buf = s;
            Task::none()
        }
        Message::BodyChanged(s) => {
            app.body_buf = s;
            Task::none()
        }
    }
}

fn view(app: &NotesGui, _id: window::Id) -> Element<'_, Message> {
    let mut list = column![].spacing(4).padding(8);
    for note in &app.state.notes {
        let active = app.state.selected_id == Some(note.id);
        let variant = if active {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Ghost
        };
        list = list.push(button::labeled(
            note.title.clone(),
            None,
            variant,
            Some(Message::Dispatch(NoteCommand::SelectNote { id: note.id })),
        ));
    }

    let sidebar = container(column![
        container(
            row![
                text("Notes").size(14.0),
                Space::new().width(Fill),
                button::icon_only(
                    Icon::Plus,
                    ButtonVariant::Secondary,
                    Some(Message::Dispatch(NoteCommand::CreateNote {
                        title: format!("Note {}", app.state.notes.len() + 1),
                        body: String::new(),
                    })),
                ),
            ]
            .padding(8),
        )
        .width(Fill),
        rule::horizontal(1).style(hairline_rule_style),
        scrollable(list).height(Fill),
    ])
    .width(240)
    .height(Fill)
    .style(surface_panel_style);

    let save_btn = app.state.selected_id.map(|id| {
        button::labeled(
            "Save",
            Some(Icon::Check),
            ButtonVariant::Primary,
            Some(Message::Dispatch(NoteCommand::UpdateNote {
                id,
                title: app.title_buf.clone(),
                body: app.body_buf.clone(),
            })),
        )
    });

    let del_btn = app.state.selected_id.map(|id| {
        button::icon_only(
            Icon::Trash,
            ButtonVariant::Danger,
            Some(Message::Dispatch(NoteCommand::DeleteNote { id })),
        )
    });

    let mut editor_top =
        row![text_input("Title...", &app.title_buf).on_input(Message::TitleChanged)].spacing(8);
    if let Some(b) = save_btn {
        editor_top = editor_top.push(b);
    }
    if let Some(b) = del_btn {
        editor_top = editor_top.push(b);
    }

    let editor = container(
        column![
            editor_top,
            text_input("Body...", &app.body_buf)
                .on_input(Message::BodyChanged)
                .width(Fill),
            Space::new().height(Fill),
            text(format!(
                "rev {} · {} notes",
                app.state.revision,
                app.state.notes.len()
            ))
            .size(TEXT_SIZE_CAPTION),
        ]
        .spacing(12)
        .padding(16),
    )
    .width(Fill)
    .height(Fill);

    row![
        sidebar,
        rule::vertical(1).style(hairline_rule_style),
        editor
    ]
    .width(Fill)
    .height(Fill)
    .into()
}
