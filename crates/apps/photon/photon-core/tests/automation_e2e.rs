//! End-to-end automation, IPC, replay, and Core->i18n contract tests.

use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use image::{Rgba, RgbaImage};
use photon_core::{Core, CoreRuntime, replay_journal_in_sandbox};
use photon_i18n::tr_ui_text;
use photon_ipc::{IpcRequest, IpcResponse, send_once, serve};
use photon_protocol::{
    AppLaunchWindowMode, ApplyTarget, CanvasAntiAliasingMode, ClientKind, Command, DrawShapeKind,
    Envelope, GradientColorMode, GradientKind, HealingMode, Language, LayerTransformSpec,
    LayoutState, PluginParamValue, Query, SelectionBooleanOp, SelectionShape, ShapeFillMode,
    UiText, ViewportDenoiseMode, Workspace,
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
        ("toast_copied", vec![]),
        ("toast_cut", vec![]),
        ("toast_pasted", vec![]),
        ("toast_clipboard_empty", vec![]),
        ("plugins_refreshed", vec![("count", "2")]),
        ("plugin_enabled", vec![("name", "Invert")]),
        ("plugin_disabled", vec![("name", "Invert")]),
        ("plugin_installed", vec![("name", "Invert")]),
        ("plugin_uninstalled", vec![("name", "Invert")]),
        ("plugin_executed", vec![("name", "Invert")]),
        ("plugin_failed", vec![("name", "Invert"), ("error", "boom")]),
        ("plugin_not_found", vec![("id", "com.example.missing")]),
        ("plugin_invalid_manifest", vec![("error", "bad json")]),
        ("op_flood_fill", vec![]),
        ("op_gradient", vec![]),
        ("op_draw_shape", vec![]),
        ("op_healing", vec![]),
        ("op_denoise", vec![]),
        ("op_selection_clear", vec![]),
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

fn encode_png_bytes(img: &RgbaImage) -> Vec<u8> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    out
}

#[test]
fn fill_gradient_shape_healing_denoise_and_transform_e2e() {
    let dir = std::env::temp_dir().join(format!("photon-tools-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let img_path = dir.join("canvas.png");
    RgbaImage::from_pixel(24, 24, Rgba([20, 40, 60, 255]))
        .save(&img_path)
        .unwrap();

    let mut core = Core::for_sandbox(&dir.join("sandbox"));
    assert!(
        core.dispatch(Envelope::new(
            "t1",
            ClientKind::Gui,
            Command::EditorOpen {
                path: img_path.to_string_lossy().into_owned(),
            },
        ))
        .ok
    );

    // 1. FloodFill with explicit color & opacity
    assert!(
        core.dispatch(Envelope::new(
            "t2",
            ClientKind::Gui,
            Command::EditorFloodFill {
                x: 5,
                y: 5,
                tolerance: 5,
                contiguous: true,
                color: Some([200, 100, 50]),
                opacity: Some(1.0),
                sample_all_layers: false,
            },
        ))
        .ok
    );
    assert_eq!(
        core.editor_document().unwrap().current.get_pixel(5, 5).0,
        [200, 100, 50, 255]
    );

    // 2. FloodFill sample_all_layers across layers
    assert!(
        core.dispatch(Envelope::new(
            "t2b",
            ClientKind::Gui,
            Command::EditorLayerAdd {
                name: "Overlay".into(),
            },
        ))
        .ok
    );
    assert!(
        core.dispatch(Envelope::new(
            "t2c",
            ClientKind::Gui,
            Command::EditorFloodFill {
                x: 6,
                y: 6,
                tolerance: 5,
                contiguous: true,
                color: Some([30, 210, 90]),
                opacity: Some(1.0),
                sample_all_layers: true,
            },
        ))
        .ok
    );
    let active_px = core
        .editor_document()
        .unwrap()
        .stack()
        .active_layer()
        .pixels
        .get_pixel(6, 6)
        .0;
    assert_eq!(active_px, [30, 210, 90, 255]);

    // Undo back to base layer
    assert!(
        core.dispatch(Envelope::new("t2u", ClientKind::Gui, Command::EditorUndo))
            .ok
    );

    // 3. DrawGradient with explicit fg/bg colors, reverse, dither
    assert!(
        core.dispatch(Envelope::new(
            "t3",
            ClientKind::Gui,
            Command::EditorDrawGradient {
                start: [0.0, 0.0],
                end: [23.0, 0.0],
                kind: GradientKind::Linear,
                color_mode: GradientColorMode::FgToBg,
                fg_color: Some([255, 0, 0]),
                bg_color: Some([0, 0, 255]),
                stops: Vec::new(),
                opacity: Some(1.0),
                repeat: photon_protocol::GradientRepeat::None,
                reverse: true,
                dither: true,
                blend_mode: photon_protocol::BlendMode::Normal,
            },
        ))
        .ok
    );
    // Reversed: left edge near bg_color (blue), right edge near fg_color (red)
    let left_px = core.editor_document().unwrap().current.get_pixel(0, 0).0;
    let right_px = core.editor_document().unwrap().current.get_pixel(23, 0).0;
    assert!(left_px[2] > left_px[0]);
    assert!(right_px[0] > right_px[2]);

    // 4. DrawShape with explicit color, radius, opacity
    assert!(
        core.dispatch(Envelope::new(
            "t4",
            ClientKind::Gui,
            Command::EditorDrawShape {
                kind: DrawShapeKind::Rect,
                points: vec![[4.0, 4.0], [18.0, 18.0]],
                nodes: vec![],
                fill_mode: ShapeFillMode::Fill,
                closed: true,
                color: Some([10, 220, 40]),
                radius: Some(2.0),
                opacity: Some(1.0),
            },
        ))
        .ok
    );
    assert_eq!(
        core.editor_document().unwrap().current.get_pixel(10, 10).0,
        [10, 220, 40, 255]
    );

    // 5. Healing stroke + Denoise + TransformLayer + undo/redo
    assert!(
        core.dispatch(Envelope::new(
            "t5",
            ClientKind::Gui,
            Command::EditorHealingStroke {
                points: vec![[10.0, 10.0], [12.0, 12.0]],
                mode: HealingMode::Spot,
                radius: 3.0,
                hardness: 0.8,
                strength: 1.0,
                source: None,
            },
        ))
        .ok
    );

    assert!(
        core.dispatch(Envelope::new(
            "t6",
            ClientKind::Gui,
            Command::EditorDenoise {
                strength: 0.6,
                detail_preserve: 0.5,
                target: ApplyTarget::ActiveLayer,
                selection_only: true,
            },
        ))
        .ok
    );

    let active_id = core.meta().editor.as_ref().unwrap().active_layer_id;
    let before_transform = (*core.editor_document().unwrap().current).clone();
    assert!(
        core.dispatch(Envelope::new(
            "t7",
            ClientKind::Gui,
            Command::EditorTransformLayer {
                layer_id: Some(active_id),
                transform: LayerTransformSpec {
                    translate_x: 4.0,
                    translate_y: 2.0,
                    rotation_deg: 15.0,
                    ..Default::default()
                },
            },
        ))
        .ok
    );
    assert_ne!(*core.editor_document().unwrap().current, before_transform);

    assert!(
        core.dispatch(Envelope::new("t8", ClientKind::Gui, Command::EditorUndo))
            .ok
    );
    assert_eq!(*core.editor_document().unwrap().current, before_transform);
    assert!(
        core.dispatch(Envelope::new("t9", ClientKind::Gui, Command::EditorRedo))
            .ok
    );
    assert_ne!(*core.editor_document().unwrap().current, before_transform);

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cut_copy_copy_merged_paste_and_transform_frames_e2e() {
    let dir = std::env::temp_dir().join(format!("photon-clip-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let img_path = dir.join("base.png");
    RgbaImage::from_pixel(20, 20, Rgba([200, 20, 20, 255]))
        .save(&img_path)
        .unwrap();

    let core = Core::for_sandbox(&dir.join("sandbox"));
    let handle = CoreRuntime::spawn(core);

    assert!(
        handle
            .dispatch(Envelope::new(
                "c1",
                ClientKind::Gui,
                Command::EditorOpen {
                    path: img_path.to_string_lossy().into_owned(),
                },
            ))
            .await
            .unwrap()
            .ok
    );
    assert!(!handle.latest_snapshot().meta.has_pixel_clipboard);

    // Add a second layer with a blue square at 0..20
    assert!(
        handle
            .dispatch(Envelope::new(
                "c2",
                ClientKind::Gui,
                Command::EditorLayerAdd { name: "Top".into() },
            ))
            .await
            .unwrap()
            .ok
    );
    assert!(
        handle
            .dispatch(Envelope::new(
                "c3",
                ClientKind::Gui,
                Command::EditorDrawShape {
                    kind: DrawShapeKind::Rect,
                    points: vec![[4.0, 4.0], [12.0, 12.0]],
                    nodes: vec![],
                    fill_mode: ShapeFillMode::Fill,
                    closed: true,
                    color: Some([20, 40, 220]),
                    radius: Some(1.0),
                    opacity: Some(1.0),
                },
            ))
            .await
            .unwrap()
            .ok
    );

    // Check CoreHandle::read_editor_transform_frames returns base excluding active & active layer
    let frames = handle.read_editor_transform_frames().await.unwrap();
    assert_eq!(frames.canvas_width, 20);
    assert_eq!(frames.canvas_height, 20);
    assert_eq!(frames.layer_width, 20);
    assert_eq!(frames.layer_height, 20);
    // Base excluding top layer is pure red at (8, 8)
    let idx = (8 * 20 + 8) * 4;
    assert_eq!(
        &frames.base_rgba_excluding_active[idx..idx + 4],
        &[200, 20, 20, 255]
    );
    // Active layer is blue at (8, 8)
    assert_eq!(&frames.active_layer_rgba[idx..idx + 4], &[20, 40, 220, 255]);

    // Select 2x2..6x6 (overlaps both red base outside [4..12] and blue top inside [4..12])
    assert!(
        handle
            .dispatch(Envelope::new(
                "c4",
                ClientKind::Gui,
                Command::EditorSelectShape {
                    shape: SelectionShape::Rect {
                        x: 2.0,
                        y: 2.0,
                        width: 6.0,
                        height: 6.0,
                        feather: 0.0,
                        anti_alias: false,
                    },
                    op: SelectionBooleanOp::Replace,
                },
            ))
            .await
            .unwrap()
            .ok
    );

    // Copy merged -> Query::EditorClipboardPng returns valid PNG with merged pixels
    assert!(
        handle
            .dispatch(Envelope::new(
                "c5",
                ClientKind::Gui,
                Command::EditorCopyPixels { merged: true },
            ))
            .await
            .unwrap()
            .ok
    );
    assert!(handle.latest_snapshot().meta.has_pixel_clipboard);
    let clip_q = handle
        .query(Query::EditorClipboardPng)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(clip_q["hasClipboard"], true);
    assert_eq!(clip_q["width"], 6);
    assert_eq!(clip_q["height"], 6);
    assert_eq!(clip_q["offsetX"], 2);
    assert_eq!(clip_q["offsetY"], 2);
    let png_vec: Vec<u8> = clip_q["pngBytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    let decoded = image::load_from_memory(&png_vec).unwrap().to_rgba8();
    // (0,0) in copied 6x6 corresponds to canvas (2,2) -> red from base layer
    assert_eq!(decoded.get_pixel(0, 0).0, [200, 20, 20, 255]);
    // (4,4) in copied 6x6 corresponds to canvas (6,6) -> blue from top layer
    assert_eq!(decoded.get_pixel(4, 4).0, [20, 40, 220, 255]);

    // Cut active layer selection -> clears selected pixels on active layer and supports undo/redo
    let layers_before_cut = handle
        .latest_snapshot()
        .meta
        .editor
        .as_ref()
        .unwrap()
        .layers
        .len();
    assert!(
        handle
            .dispatch(Envelope::new(
                "c6",
                ClientKind::Gui,
                Command::EditorCutPixels,
            ))
            .await
            .unwrap()
            .ok
    );
    // Paste internal clipboard -> adds a 3rd layer
    assert!(
        handle
            .dispatch(Envelope::new(
                "c7",
                ClientKind::Gui,
                Command::EditorPastePixels {
                    png_bytes: None,
                    into_selection: false,
                },
            ))
            .await
            .unwrap()
            .ok
    );
    assert_eq!(
        handle
            .latest_snapshot()
            .meta
            .editor
            .as_ref()
            .unwrap()
            .layers
            .len(),
        layers_before_cut + 1
    );

    // Undo paste & undo cut
    assert!(
        handle
            .dispatch(Envelope::new("c8", ClientKind::Gui, Command::EditorUndo))
            .await
            .unwrap()
            .ok
    );
    assert_eq!(
        handle
            .latest_snapshot()
            .meta
            .editor
            .as_ref()
            .unwrap()
            .layers
            .len(),
        layers_before_cut
    );
    assert!(
        handle
            .dispatch(Envelope::new("c9", ClientKind::Gui, Command::EditorUndo))
            .await
            .unwrap()
            .ok
    );
    // Redo cut + redo paste
    assert!(
        handle
            .dispatch(Envelope::new("c10", ClientKind::Gui, Command::EditorRedo))
            .await
            .unwrap()
            .ok
    );
    assert!(
        handle
            .dispatch(Envelope::new("c11", ClientKind::Gui, Command::EditorRedo))
            .await
            .unwrap()
            .ok
    );
    assert_eq!(
        handle
            .latest_snapshot()
            .meta
            .editor
            .as_ref()
            .unwrap()
            .layers
            .len(),
        layers_before_cut + 1
    );

    // Paste external PNG bytes into_selection
    let ext_img = RgbaImage::from_pixel(10, 10, Rgba([0, 255, 0, 255]));
    let ext_png = encode_png_bytes(&ext_img);
    assert!(
        handle
            .dispatch(Envelope::new(
                "c12",
                ClientKind::Gui,
                Command::EditorPastePixels {
                    png_bytes: Some(ext_png),
                    into_selection: true,
                },
            ))
            .await
            .unwrap()
            .ok
    );
    assert_eq!(
        handle
            .latest_snapshot()
            .meta
            .editor
            .as_ref()
            .unwrap()
            .layers
            .len(),
        layers_before_cut + 2
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn paste_image_bytes_and_copy_to_same_folder_auto_rename_e2e() {
    let dir = std::env::temp_dir().join(format!("photon-paste-bytes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let photos = dir.join("photos");
    fs::create_dir_all(&photos).unwrap();

    let mut core = Core::for_sandbox(&dir.join("sandbox"));
    assert!(
        core.dispatch(Envelope::new(
            "p0",
            ClientKind::Gui,
            Command::OpenFolder {
                path: photos.to_string_lossy().into_owned(),
            },
        ))
        .ok
    );

    let png = encode_png_bytes(&RgbaImage::from_pixel(8, 8, Rgba([12, 34, 56, 255])));
    let r1 = core.dispatch(Envelope::new(
        "p1",
        ClientKind::Gui,
        Command::PasteImageBytes {
            folder: photos.to_string_lossy().into_owned(),
            filename_hint: "clip.png".into(),
            png_bytes: png.clone(),
        },
    ));
    assert!(r1.ok);
    assert!(photos.join("clip.png").is_file());
    assert_eq!(core.snapshot().entries.len(), 1);

    // Second paste with same hint auto-renames without overwriting
    let r2 = core.dispatch(Envelope::new(
        "p2",
        ClientKind::Gui,
        Command::PasteImageBytes {
            folder: photos.to_string_lossy().into_owned(),
            filename_hint: "clip.png".into(),
            png_bytes: png,
        },
    ));
    assert!(r2.ok);
    assert!(photos.join("clip (copy).png").is_file());
    assert_eq!(core.snapshot().entries.len(), 2);

    // Same-folder CopyTo also auto-renames cleanly
    let r3 = core.dispatch(Envelope::new(
        "p3",
        ClientKind::Gui,
        Command::CopyTo {
            paths: vec![photos.join("clip.png").to_string_lossy().into_owned()],
            destination: photos.to_string_lossy().into_owned(),
        },
    ));
    assert!(r3.ok);
    assert!(photos.join("clip (copy 2).png").is_file());
    assert_eq!(core.snapshot().entries.len(), 3);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn plugins_headless_lifecycle_run_undo_redo_and_replay_e2e() {
    let dir = std::env::temp_dir().join(format!("photon-plugin-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let photos = dir.join("photos");
    let pkg_src = dir.join("mock_pkg");
    fs::create_dir_all(&photos).unwrap();
    fs::create_dir_all(&pkg_src).unwrap();

    let img_path = photos.join("pic.png");
    RgbaImage::from_pixel(6, 6, Rgba([10, 20, 30, 255]))
        .save(&img_path)
        .unwrap();

    // Write a mock Python plugin supporting both editor_filter & browser_action
    let manifest = r#"{
        "id": "com.test.mock",
        "name": "Mock Plugin",
        "version": "1.0.0",
        "entry": "plugin.py",
        "capabilities": ["editor_filter", "browser_action"],
        "params": [
            {"key": "mode", "label": "Mode", "kind": "select", "default": "replace", "options": ["replace", "new_layer"]}
        ],
        "permissions": ["read_image"]
    }"#;
    let script = r#"#!/usr/bin/env python3
import json, os, sys

req = json.loads(sys.stdin.readline())
cap = req["capability"]
if cap == "editor_filter":
    import shutil
    shutil.copyfile(req["input_path"], req["output_path"])
    mode = req["params"].get("mode", "replace")
    resp = {
        "ok": True,
        "mode": "new_layer" if mode == "new_layer" else "replace_layer",
        "layer_name": "Mock Layer",
        "message": "filtered"
    }
    print(json.dumps(resp), flush=True)
elif cap == "browser_action":
    folder = req["current_folder"]
    with open(os.path.join(folder, "from_plugin.txt"), "w") as f:
        f.write("ok")
    print(json.dumps({"ok": True, "mode": "browser_refresh", "message": "created"}), flush=True)
else:
    print(json.dumps({"ok": False, "error": "unsupported"}), flush=True)
"#;
    fs::write(pkg_src.join("plugin.json"), manifest).unwrap();
    let py_path = pkg_src.join("plugin.py");
    fs::write(&py_path, script).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&py_path, fs::Permissions::from_mode(0o755)).unwrap();

    let sandbox = dir.join("sandbox");
    let mut core = Core::for_sandbox(&sandbox);
    let journal_path = core.paths.journal_file();

    // Install & refresh
    let inst = core.dispatch(Envelope::new(
        "pl1",
        ClientKind::Cli,
        Command::PluginInstall {
            source_dir: pkg_src.to_string_lossy().into_owned(),
        },
    ));
    assert!(inst.ok, "install failed: {:?}", inst.error);
    assert_eq!(core.meta().plugins.len(), 1);
    assert!(core.meta().plugins[0].enabled);

    // Disable -> config.disabled_plugins persists & PluginRun rejects
    assert!(
        core.dispatch(Envelope::new(
            "pl2",
            ClientKind::Cli,
            Command::PluginSetEnabled {
                id: "com.test.mock".into(),
                enabled: false,
            },
        ))
        .ok
    );
    assert!(!core.meta().plugins[0].enabled);
    let disabled_run = core.dispatch(Envelope::new(
        "pl3",
        ClientKind::Cli,
        Command::PluginRun {
            id: "com.test.mock".into(),
            params: BTreeMap::new(),
            target: ApplyTarget::WholeImage,
        },
    ));
    assert!(!disabled_run.ok);
    assert_eq!(disabled_run.error.unwrap().code, "plugin_disabled");

    // Re-enable
    assert!(
        core.dispatch(Envelope::new(
            "pl4",
            ClientKind::Cli,
            Command::PluginSetEnabled {
                id: "com.test.mock".into(),
                enabled: true,
            },
        ))
        .ok
    );

    // Browser action plugin creates a file and refreshes listing
    assert!(
        core.dispatch(Envelope::new(
            "pl5",
            ClientKind::Cli,
            Command::OpenFolder {
                path: photos.to_string_lossy().into_owned(),
            },
        ))
        .ok
    );
    let b_run = core.dispatch(Envelope::new(
        "pl6",
        ClientKind::Cli,
        Command::PluginRun {
            id: "com.test.mock".into(),
            params: BTreeMap::new(),
            target: ApplyTarget::WholeImage,
        },
    ));
    assert!(b_run.ok, "browser plugin failed: {:?}", b_run.error);
    assert!(photos.join("from_plugin.txt").is_file());
    assert!(
        core.snapshot()
            .entries
            .iter()
            .any(|e| e.name == "from_plugin.txt")
    );

    // Editor plugin (new_layer & replace_layer) + undo/redo
    assert!(
        core.dispatch(Envelope::new(
            "pl7",
            ClientKind::Cli,
            Command::EditorOpen {
                path: img_path.to_string_lossy().into_owned(),
            },
        ))
        .ok
    );
    let mut new_layer_params = BTreeMap::new();
    new_layer_params.insert(
        "mode".to_owned(),
        PluginParamValue::Select("new_layer".to_owned()),
    );
    let e_new = core.dispatch(Envelope::new(
        "pl8",
        ClientKind::Cli,
        Command::PluginRun {
            id: "com.test.mock".into(),
            params: new_layer_params,
            target: ApplyTarget::WholeImage,
        },
    ));
    assert!(
        e_new.ok,
        "editor plugin new_layer failed: {:?}",
        e_new.error
    );
    assert_eq!(core.meta().editor.as_ref().unwrap().layers.len(), 2);

    assert!(
        core.dispatch(Envelope::new("pl9", ClientKind::Cli, Command::EditorUndo))
            .ok
    );
    assert_eq!(core.meta().editor.as_ref().unwrap().layers.len(), 1);
    assert!(
        core.dispatch(Envelope::new("pl10", ClientKind::Cli, Command::EditorRedo))
            .ok
    );
    assert_eq!(core.meta().editor.as_ref().unwrap().layers.len(), 2);

    // Replace layer mode
    let e_rep = core.dispatch(Envelope::new(
        "pl11",
        ClientKind::Cli,
        Command::PluginRun {
            id: "com.test.mock".into(),
            params: BTreeMap::new(),
            target: ApplyTarget::ActiveLayer,
        },
    ));
    assert!(e_rep.ok);
    assert_eq!(core.meta().editor.as_ref().unwrap().layers.len(), 2);

    // Refresh & Uninstall
    assert!(
        core.dispatch(Envelope::new(
            "pl12",
            ClientKind::Cli,
            Command::PluginRefresh,
        ))
        .ok
    );
    assert!(
        core.dispatch(Envelope::new(
            "pl13",
            ClientKind::Cli,
            Command::PluginUninstall {
                id: "com.test.mock".into(),
            },
        ))
        .ok
    );
    assert!(core.meta().plugins.is_empty());

    // Deterministic replay in a fresh sandbox
    let sandbox_b = dir.join("sandbox_b");
    let report = replay_journal_in_sandbox(&journal_path, &sandbox_b, false).unwrap();
    assert_eq!(report.divergences, 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn layout_state_new_fields_deserialize_with_defaults() {
    // Legacy layout JSON without new fields deserializes with defaults
    let legacy = r#"{"windowWidth":1280,"windowHeight":800}"#;
    let parsed_legacy: LayoutState = serde_json::from_str(legacy).unwrap();
    assert_eq!(
        parsed_legacy.app_launch_window_mode,
        AppLaunchWindowMode::Maximized
    );
    assert!(parsed_legacy.window_maximized);
    assert_eq!(
        parsed_legacy.canvas_anti_aliasing,
        CanvasAntiAliasingMode::Smooth
    );
    assert_eq!(parsed_legacy.viewport_denoise, ViewportDenoiseMode::Off);

    // Explicit new fields round-trip cleanly
    let custom = r#"{
        "windowWidth": 1440,
        "windowHeight": 900,
        "appLaunchWindowMode": "maximized",
        "windowMaximized": false,
        "canvasAntiAliasing": "high_quality",
        "viewportDenoise": "medium"
    }"#;
    let parsed: LayoutState = serde_json::from_str(custom).unwrap();
    assert_eq!(
        parsed.app_launch_window_mode,
        AppLaunchWindowMode::Maximized
    );
    assert!(!parsed.window_maximized);
    assert_eq!(
        parsed.canvas_anti_aliasing,
        CanvasAntiAliasingMode::HighQuality
    );
    assert_eq!(parsed.viewport_denoise, ViewportDenoiseMode::Medium);
}

#[tokio::test]
async fn async_ai_runtime_streams_progress_and_commits_quick_action() {
    use photon_core::CoreRuntime;
    use photon_protocol::EventKind;

    let dir = std::env::temp_dir().join(format!("photon-async-ai-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let photos = dir.join("photos");
    let sandbox = dir.join("sandbox");
    let plugins_dir = sandbox.join("data/plugins");
    fs::create_dir_all(&photos).unwrap();
    fs::create_dir_all(plugins_dir.join("photon.async.qa/bin")).unwrap();

    let img_path = photos.join("sample.png");
    RgbaImage::from_pixel(8, 8, Rgba([50, 100, 150, 255]))
        .save(&img_path)
        .unwrap();

    let worker = r#"#!/usr/bin/env python3
import json, shutil, sys
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("op") == "shutdown":
        break
    if msg.get("op") == "run":
        print(json.dumps({"id": msg["id"], "event": "progress", "percent": 55, "stage": "Working"}), flush=True)
        shutil.copyfile(msg["input"], msg["output"])
        print(json.dumps({"id": msg["id"], "event": "done", "meta": {"device": "cpu"}}), flush=True)
"#;
    let py = plugins_dir.join("photon.async.qa/bin/worker.py");
    fs::write(&py, worker).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&py, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        plugins_dir.join("photon.async.qa/manifest.json"),
        r#"{
            "apiVersion": 1,
            "id": "photon.async.qa",
            "name": "Async QA",
            "version": "1.0.0",
            "entry": { "command": "bin/worker.py", "args": [], "protocol": "jsonl-stdio" },
            "capabilities": [{ "action": "editor.mock", "workspace": "editor", "label": "Mock", "output": "rgba_image" }],
            "quickActions": [{ "id": "qa-run", "label": "Mock QA", "action": "editor.mock", "workspaces": ["browser"], "params": {} }]
        }"#,
    )
    .unwrap();

    let core = Core::for_sandbox(&sandbox);
    let handle = CoreRuntime::spawn(core);
    let mut events = handle.subscribe_events();

    let resp = handle
        .dispatch(Envelope::new(
            "async-1",
            ClientKind::Gui,
            Command::QuickActionRun {
                plugin_id: "photon.async.qa".into(),
                action_id: "qa-run".into(),
                paths: vec![img_path.to_string_lossy().into_owned()],
            },
        ))
        .await
        .unwrap();
    assert!(resp.ok, "dispatch failed: {:?}", resp.error);

    let mut saw_started = false;
    let mut saw_progress = false;
    let mut saw_finished = false;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline && !saw_finished {
        let ev = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
            .await
            .expect("timed out waiting for AI task event")
            .unwrap();
        match ev.kind {
            EventKind::TaskStarted { .. } => saw_started = true,
            EventKind::TaskProgress { percent: 55, .. } => saw_progress = true,
            EventKind::TaskFinished { ok: true, .. } => saw_finished = true,
            _ => {}
        }
    }

    assert!(saw_started);
    assert!(saw_progress);
    assert!(saw_finished);
    assert!(photos.join("sample.png~").is_file());
    let snap = handle.latest_snapshot();
    assert!(
        snap.meta
            .ai_task
            .as_ref()
            .is_some_and(|t| !t.running && t.percent == 100)
    );

    let _ = fs::remove_dir_all(&dir);
}
