mod app;
mod core;

use std::time::Duration;

use clap::Parser;
use core::EditorCore;
use fw_protocol::Envelope;
use fw_runtime::{AppCore, CoreRuntime};
use fw_shell::{LaunchLock, acquire_launch_lock};
use photon_app_common::{
    AppRole, Cli, forward_to_owner, resolve_initial_command_for_role, try_forward_to_running,
};
use photon_ipc::{bind_socket, serve, serve_bound};
use photon_protocol::{ClientKind, Command, Language, Workspace};
use photon_storage::AppPaths;
use photon_storage::logging::{initialize, log_shutdown, log_startup};

const LAUNCH_LOCK_WAIT: Duration = Duration::from_millis(1500);

#[cfg(unix)]
type PreboundListener = tokio::net::UnixListener;
#[cfg(not(unix))]
type PreboundListener = ();

fn main() {
    let cli = Cli::parse();
    let identity = photon_storage::current_identity();
    let paths = AppPaths::for_identity(&identity);
    let _ = initialize(&paths, true);

    let mut initial_cmd = resolve_initial_command_for_role(&cli, AppRole::Editor);
    let extra_editor_paths: Vec<String> = if cli.sub.is_none() && cli.paths.len() > 1 {
        cli.paths[1..]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    } else {
        Vec::new()
    };

    let socket_path = paths.editor_socket_path();
    if !cli.new_window {
        let forward_cmd = initial_cmd.clone().unwrap_or(Command::FocusWindow);
        if try_forward_to_running(&socket_path, forward_cmd) {
            for extra in &extra_editor_paths {
                let _ = try_forward_to_running(
                    &socket_path,
                    Command::EditorOpen {
                        path: extra.clone(),
                    },
                );
            }
            if initial_cmd.is_some() {
                let _ = try_forward_to_running(&socket_path, Command::FocusWindow);
            }
            return;
        }
    }

    if initial_cmd.is_none() {
        initial_cmd = Some(Command::SetWorkspace {
            workspace: Workspace::Editor,
        });
    }

    log_startup(&identity, &paths, false);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut _launch_lock: Option<LaunchLock> = None;
    let mut prebound_listener: Option<PreboundListener> = None;
    if !cli.new_window {
        let lock = match acquire_launch_lock(&socket_path, LAUNCH_LOCK_WAIT) {
            Ok(lock) => lock,
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                if forward_to_owner(&socket_path, &initial_cmd) {
                    return;
                }
                eprintln!(
                    "photon-editor: another instance is starting for {} but did not answer; exiting",
                    socket_path.display()
                );
                std::process::exit(1);
            }
            Err(err) => {
                eprintln!(
                    "photon-editor: cannot acquire launch lock for {}: {err}",
                    socket_path.display()
                );
                std::process::exit(1);
            }
        };
        _launch_lock = Some(lock);

        #[cfg(unix)]
        {
            match rt.block_on(bind_socket(&socket_path)) {
                Ok(listener) => prebound_listener = Some(listener),
                Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                    if forward_to_owner(&socket_path, &initial_cmd) {
                        return;
                    }
                    eprintln!(
                        "photon-editor: {} is owned by another instance that did not answer; exiting",
                        socket_path.display()
                    );
                    std::process::exit(1);
                }
                Err(err) => {
                    eprintln!(
                        "photon-editor: failed to bind {}: {err}",
                        socket_path.display()
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    let mut core = EditorCore::new(cli.safe_mode);
    if let Some(lang_str) = cli.lang.as_deref() {
        let lang = match lang_str.to_ascii_lowercase().as_str() {
            "vi" => Some(Language::Vietnamese),
            "en" => Some(Language::English),
            "system" => Some(Language::System),
            _ => None,
        };
        if let Some(language) = lang {
            let _ = core.dispatch(Envelope::new(
                "boot-lang",
                ClientKind::Cli,
                Command::SetLanguage { language },
            ));
        }
    }
    if let Some(cmd) = initial_cmd {
        let _ = core.dispatch(Envelope::new("boot-open", ClientKind::Cli, cmd));
    }
    for (idx, extra) in extra_editor_paths.iter().enumerate() {
        let _ = core.dispatch(Envelope::new(
            format!("boot-open-extra-{idx}"),
            ClientKind::Cli,
            Command::EditorOpen {
                path: extra.clone(),
            },
        ));
    }

    let hw_accel = core.config.hardware_acceleration;
    let initial_display_font = core.config.layout.display_font;
    let initial_font_scale = core.config.layout.font_scale();
    let use_software_renderer = cli.safe_mode || !hw_accel;

    let (handle, gpu_backend) = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let ipc_socket = socket_path.clone();
        rt.spawn(async move {
            #[cfg(unix)]
            let result = match prebound_listener {
                Some(listener) => serve_bound(listener, ipc_handle).await,
                None => serve(&ipc_socket, ipc_handle).await,
            };
            #[cfg(not(unix))]
            let result = {
                let _ = prebound_listener;
                serve(&ipc_socket, ipc_handle).await
            };
            if let Err(err) = result {
                tracing::error!(target: "photon::ipc", %err, "editor IPC server stopped");
            }
        });
        let gpu_backend = if use_software_renderer
            || std::env::var("ICED_BACKEND").ok().as_deref() == Some("tiny-skia")
        {
            "tiny-skia"
        } else {
            "wgpu"
        };
        (handle, gpu_backend.to_owned())
    };

    let bg_rt = rt.handle().clone();
    let _rt_keeper = std::thread::spawn(move || {
        rt.block_on(std::future::pending::<()>());
    });

    if use_software_renderer {
        unsafe { std::env::set_var("ICED_BACKEND", "tiny-skia") };
    } else {
        if std::env::var_os("ICED_BACKEND").is_none() {
            unsafe { std::env::set_var("ICED_BACKEND", "wgpu") };
        }
        #[cfg(target_os = "linux")]
        if std::env::var_os("WGPU_BACKEND").is_none() {
            unsafe { std::env::set_var("WGPU_BACKEND", "vulkan") };
        }
    }

    app::init_globals(handle, bg_rt, gpu_backend);

    if let Err(err) = iced::daemon(app::boot, app::update, app::view)
        .settings(iced::Settings {
            default_font: app::iced_font_for(initial_display_font),
            default_text_size: iced::Pixels(13.0 * initial_font_scale),
            ..Default::default()
        })
        .title(app::title)
        .theme(app::theme)
        .scale_factor(app::scale_factor)
        .subscription(app::subscription)
        .run()
    {
        tracing::error!(target: "photon::editor", %err, "iced runtime exited with error");
    }

    log_shutdown();
}
