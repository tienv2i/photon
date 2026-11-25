//! End-to-end automation, IPC, replay, and Core->i18n contract tests.

use std::fs;

use image::{Rgba, RgbaImage};
use photon_core::{Core, CoreRuntime, replay_journal_in_sandbox};
use photon_i18n::tr_ui_text;
use photon_ipc::{IpcRequest, IpcResponse, send_once, serve};
use photon_protocol::{
    ApplyTarget, ClientKind, Command, Envelope, Language, Query, UiText, Workspace,
};

#[tokio::test]
async fn ipc_dispatch_conflict_idempotency_and_sandbox_replay() {
    let base = std::env::temp_dir().join(format!("photon-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let photos = base.join("photos");
    fs::create_dir_all(&photos).unwrap();
    let img_path = photos.join("hero.png");
    RgbaImage::from_pixel(40, 20, Rgba([10, 120, 240, 255]))
        .save(&img_path)
        .unwrap();

    let sandbox_a = base.join("sandbox_a");
    let core = Core::for_sandbox(&sandbox_a);
    let socket_path = core.paths.socket_path.clone();
    let journal_path = core.paths.journal_file();

    let handle = CoreRuntime::spawn(core);
    let ipc_socket = socket_path.clone();
    let ipc_handle = handle.clone();
    let server = tokio::spawn(async move {
        let _ = serve(&ipc_socket, ipc_handle).await;
    });

    // Wait briefly for Unix socket bind
    for _ in 0..20 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // 1. Open folder via IPC
    let r1 = send_once(
        &socket_path,
        &IpcRequest::Dispatch {
            envelope: Envelope::new(
                "e2e-1",
                ClientKind::Cli,
                Command::OpenFolder {
                    path: photos.to_string_lossy().into_owned(),
                },
            ),
        },
    )
    .await
    .unwrap();
    let rev1 = match r1 {
        IpcResponse::Command { response } => {
            assert!(response.ok);
            response.revision
        }
        other => panic!("unexpected response: {other:?}"),
    };

    // 2. Stale revision must fail with revision_conflict
    let mut stale_env = Envelope::new(
        "e2e-stale",
        ClientKind::Automation,
        Command::SetWorkspace {
            workspace: Workspace::Viewer,
        },
    );
    stale_env.expected_revision = Some(rev1 - 1);
    let r_stale = send_once(
        &socket_path,
        &IpcRequest::Dispatch {
            envelope: stale_env,
        },
    )
    .await
    .unwrap();
    match r_stale {
        IpcResponse::Command { response } => {
            assert!(!response.ok);
            assert_eq!(
                response.error.as_ref().map(|e| e.code.as_str()),
                Some("revision_conflict")
            );
        }
        other => panic!("unexpected response: {other:?}"),
    }

    // 3. Idempotent edit via IPC
    let mut ed_env = Envelope::new(
        "e2e-edit",
        ClientKind::Mcp,
        Command::EditorOpen {
            path: img_path.to_string_lossy().into_owned(),
        },
    );
    ed_env.idempotency_key = Some("idem-open-hero".to_owned());
    let _ = send_once(
        &socket_path,
        &IpcRequest::Dispatch {
            envelope: ed_env.clone(),
        },
    )
    .await
    .unwrap();
    let r_dup = send_once(&socket_path, &IpcRequest::Dispatch { envelope: ed_env })
        .await
        .unwrap();
    assert!(matches!(r_dup, IpcResponse::Command { response } if response.ok));

    let _ = send_once(
        &socket_path,
        &IpcRequest::Dispatch {
            envelope: Envelope::new(
                "e2e-rot",
                ClientKind::Cli,
                Command::EditorRotate {
                    degrees: 90,
                    target: ApplyTarget::WholeImage,
                },
            ),
        },
    )
    .await
    .unwrap();

    // 4. Query state over IPC
    let state_resp = send_once(
        &socket_path,
        &IpcRequest::Query {
            query: Query::State,
        },
    )
    .await
    .unwrap();
    match state_resp {
        IpcResponse::Query { data } => {
            assert_eq!(data["workspace"], "editor");
            assert_eq!(data["editor"]["width"], 20);
            assert_eq!(data["editor"]["height"], 40);
        }
        other => panic!("unexpected state response: {other:?}"),
    }

    server.abort();

    // 5. Replay journal deterministically in a second fresh sandbox
    let sandbox_b = base.join("sandbox_b");
    let report = replay_journal_in_sandbox(&journal_path, &sandbox_b, false).unwrap();
    assert_eq!(report.divergences, 0);
    assert!(report.replayed >= 4);

    let _ = fs::remove_dir_all(&base);
}

#[test]
fn every_core_and_image_ui_text_code_translates_in_en_and_vi() {
    let codes = [
        ("ready", vec![]),
        ("refreshed", vec![]),
        ("folder_opened", vec![("path", "/tmp")]),
        ("folder_contents", vec![("items", "5"), ("images", "3")]),
        ("folder_error", vec![("error", "missing")]),
        ("opening_file", vec![("name", "a.png")]),
        (
            "viewer_opened",
            vec![("name", "a.png"), ("index", "1"), ("total", "3")],
        ),
        ("viewer_no_images", vec![]),
        (
            "editor_opened",
            vec![("name", "a.png"), ("width", "10"), ("height", "20")],
        ),
        ("editor_closed", vec![]),
        ("editor_saved", vec![("path", "/tmp/a.png")]),
        ("editor_undone", vec![]),
        ("editor_redone", vec![]),
        ("editor_reset", vec![]),
        ("editor_noop", vec![]),
        ("nothing_to_undo", vec![]),
        ("nothing_to_redo", vec![]),
        ("nothing_to_reset", vec![]),
        ("needs_a_document", vec![]),
        ("theme_set", vec![("theme", "dark")]),
        ("language_set", vec![("language", "vi")]),
        ("layout_saved", vec![]),
        ("settings_saved", vec![]),
        ("config_reset_done", vec![]),
        ("safe_mode_active", vec![]),
        ("screenshot_requested", vec![]),
        (
            "ui_action_requested",
            vec![("target", "nav.workspace.viewer")],
        ),
        ("created_folder", vec![("name", "sub")]),
        ("task_cancelled", vec![("taskId", "t1")]),
        ("op_resize", vec![("width", "100"), ("height", "100")]),
        ("op_crop", vec![("width", "50"), ("height", "50")]),
        ("op_rotate", vec![("degrees", "90")]),
        ("op_flip", vec![("axis", "horizontal")]),
        ("op_adjust_color", vec![]),
        ("op_canvas_size", vec![("width", "120"), ("height", "80")]),
        ("op_rotate_angle", vec![("degrees", "15.0")]),
        ("op_adjust_hsv", vec![]),
        ("op_color_balance", vec![]),
        ("op_adjust_curves", vec![]),
        ("op_brush", vec![]),
        ("op_eraser", vec![]),
        ("op_layer_add", vec![("name", "Layer 2")]),
        ("op_layer_duplicate", vec![]),
        ("op_layer_delete", vec![]),
        ("op_layer_reorder", vec![]),
        ("op_layer_props", vec![]),
        ("op_layer_merge_down", vec![]),
        ("op_layer_flatten", vec![]),
        ("op_canvas_bg", vec![]),
        ("editor_exported", vec![("path", "/tmp/out.jpg")]),
        ("layer_locked", vec![]),
        ("layer_hidden", vec![]),
        ("layer_not_found", vec![]),
        ("cannot_delete_last_layer", vec![]),
        ("cannot_merge_bottom_layer", vec![]),
        ("history_jumped", vec![("index", "2")]),
        ("op_liquify", vec![]),
        ("op_mask_add", vec![]),
        ("op_mask_delete", vec![]),
        ("op_mask_apply", vec![]),
        ("op_mask_invert", vec![]),
        ("trashed_items", vec![("count", "2")]),
        ("deleted_items", vec![("count", "2")]),
        ("copied_items", vec![("count", "2")]),
        ("moved_items", vec![("count", "2")]),
        ("renamed_item", vec![("name", "a.png")]),
        ("favorite_added", vec![("name", "Photos")]),
        ("favorite_removed", vec![("name", "Photos")]),
        ("batch_done", vec![("done", "3"), ("total", "3")]),
        ("batch_cancelled", vec![("done", "1"), ("total", "3")]),
        ("batch_already_running", vec![]),
        ("batch_no_images", vec![]),
        ("batch_too_many", vec![("max", "500")]),
        ("batch_duplicate_output", vec![("path", "/tmp/a.png")]),
        ("batch_output_is_source", vec![("path", "/tmp/a.png")]),
        ("batch_convert_overwrite", vec![("path", "/tmp/a.png")]),
        ("translate_done", vec![]),
        ("translate_ocr_unavailable", vec![]),
        ("ai_feature_disabled", vec![]),
    ];

    for (code, args) in codes {
        let ui = UiText::with(code, args);
        let en = tr_ui_text(Language::English, &ui);
        let vi = tr_ui_text(Language::Vietnamese, &ui);
        assert!(
            en != ui.english(),
            "en-US missing Fluent translation for UiText code `{code}` (got fallback `{en}`)"
        );
        assert!(
            vi != ui.english(),
            "vi-VN missing Fluent translation for UiText code `{code}` (got fallback `{vi}`)"
        );
    }
}
