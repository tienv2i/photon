mod app;
mod core;

use std::time::Duration;

use clap::Parser;
use core::ViewerCore;
use fw_protocol::Envelope;
use fw_runtime::{AppCore, CoreRuntime};
use photon_app_common::{
    AppRole, Cli, canonicalize_path_str, forward_to_owner, main_instance_viewing_path,
    resolve_initial_command_for_role, spawn_viewer_window_child, try_forward_to_running,
};
use photon_ipc::{
    ViewerInstanceInfo, acquire_viewer_path_lock, find_any_live_viewer, find_live_viewer_for_path,
    register_viewer_instance, serve, viewer_socket_path,
};
use photon_protocol::{ClientKind, Command, Language};
use photon_storage::logging::{initialize, log_shutdown, log_startup};
use photon_storage::{AppPaths, Config};

fn main() {
    let cli = Cli::parse();
    let identity = photon_storage::current_identity();
    let paths = AppPaths::for_identity(&identity);
    let _ = initialize(&paths, true);

    let mut initial_cmd = resolve_initial_command_for_role(&cli, AppRole::Viewer);
    let raw_files: Option<Vec<String>> = match &initial_cmd {
        Some(Command::OpenFiles { paths }) => Some(paths.clone()),
        Some(Command::OpenArchive { path }) => Some(vec![path.clone()]),
        _ => None,
    };
    if let Some(raw_files) = raw_files
        && !cli.viewer_window
        && !cli.new_window
    {
        let viewer_single = !cli.safe_mode
            && Config::load_or_create(&paths.config_dir.join("config.json"))
                .0
                .viewer_single_instance;
        if viewer_single && let Some(live) = find_any_live_viewer(&paths) {
            let _ =
                try_forward_to_running(&live.socket_path, Command::OpenFiles { paths: raw_files });
            let _ = try_forward_to_running(&live.socket_path, Command::FocusWindow);
            return;
        }
        let mut to_open = Vec::new();
        for raw in raw_files {
            let canon = canonicalize_path_str(&raw);
            let _lock = acquire_viewer_path_lock(&paths, &canon, Duration::from_secs(2));
            if let Some(matched) = find_live_viewer_for_path(&paths, &canon) {
                let _ = try_forward_to_running(&matched.info.socket_path, Command::FocusWindow);
            } else if main_instance_viewing_path(&paths.socket_path, &canon) {
                let _ = try_forward_to_running(&paths.socket_path, Command::FocusWindow);
            } else {
                to_open.push(raw);
            }
        }
        if to_open.is_empty() {
            return;
        }
        if to_open.len() > 1 {
            for extra in &to_open[1..] {
                spawn_viewer_window_child(extra, cli.safe_mode, cli.lang.as_deref());
            }
        }
        if matches!(initial_cmd, Some(Command::OpenFiles { .. }))
            && let Some(first) = to_open.into_iter().next()
        {
            initial_cmd = Some(Command::OpenFiles { paths: vec![first] });
        }
    } else if cli.new_window && initial_cmd.is_none() {
        // Keep fresh window
    } else if let Some(live) = find_any_live_viewer(&paths)
        && !cli.new_window
        && !cli.viewer_window
        && forward_to_owner(&live.socket_path, &initial_cmd)
    {
        return;
    }

    let pid = std::process::id();
    let socket_path = viewer_socket_path(&paths, pid);
    log_startup(&identity, &paths, false);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut core = ViewerCore::new(cli.safe_mode);
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

    let initial_viewer_path = core.snapshot().meta.viewer_path.clone();
    let initial_viewer_canon = initial_viewer_path.as_deref().map(canonicalize_path_str);
    let hw_accel = core.config.hardware_acceleration;
    let initial_display_font = core.config.layout.display_font;
    let initial_font_scale = core.config.layout.font_scale();
    let use_software_renderer = cli.safe_mode || !hw_accel;

    let (handle, gpu_backend, _viewer_reg_guard) = {
        let _guard = rt.enter();
        let handle = CoreRuntime::spawn(core);
        let ipc_handle = handle.clone();
        let ipc_socket = socket_path.clone();
        rt.spawn(async move {
            if let Err(err) = serve(&ipc_socket, ipc_handle).await {
                tracing::error!(target: "photon::ipc", %err, "viewer IPC server stopped");
            }
        });
        let active_canonical_path = initial_viewer_canon.unwrap_or_default();
        let open_canonical_paths = if active_canonical_path.as_os_str().is_empty() {
            Vec::new()
        } else {
            vec![active_canonical_path.clone()]
        };
        let info = ViewerInstanceInfo {
            pid,
            socket_path: socket_path.clone(),
            active_canonical_path,
            open_canonical_paths,
        };
        let viewer_guard = match register_viewer_instance(&paths, &info) {
            Ok(guard) => Some(guard),
            Err(err) => {
                tracing::warn!(target: "photon::ipc", %err, "failed to register viewer instance");
                None
            }
        };
        let gpu_backend = if use_software_renderer
            || std::env::var("ICED_BACKEND").ok().as_deref() == Some("tiny-skia")
        {
            "tiny-skia"
        } else {
            "wgpu"
        };
        (handle, gpu_backend.to_owned(), viewer_guard)
    };

    let bg_rt = rt.handle().clone();
    let (predecode_tx, mut predecode_rx) = tokio::sync::watch::channel::<
        Option<(
            String,
            Result<photon_viewer_canvas::ViewerImageFrame, String>,
        )>,
    >(None);
    if let Some(path) = initial_viewer_path {
        bg_rt.spawn_blocking(move || {
            let frame = app::decode_rgba(&path).map(|(w, h, rgba)| {
                photon_viewer_canvas::ViewerImageFrame {
                    key: path.clone(),
                    revision: 1,
                    width: w,
                    height: h,
                    rgba: std::sync::Arc::new(rgba),
                }
            });
            let _ = predecode_tx.send(Some((path, frame)));
        });
        rt.block_on(async {
            let _ = tokio::time::timeout(Duration::from_millis(150), predecode_rx.changed()).await;
        });
    }
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

    let reg_path = paths
        .runtime_dir
        .join("viewers")
        .join(format!("v-{pid}.json"));
    app::init_globals(
        handle,
        bg_rt,
        gpu_backend,
        socket_path,
        reg_path,
        predecode_rx,
    );

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
        tracing::error!(target: "photon::viewer", %err, "iced runtime exited with error");
    }

    log_shutdown();
}
