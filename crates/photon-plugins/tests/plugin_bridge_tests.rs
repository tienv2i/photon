use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use image::{GrayImage, Luma, Rgba, RgbaImage};
use photon_plugins::{
    MAX_PLUGIN_IMAGE_DIMENSION, PLUGINS_DIR_ENV, PluginError, PluginOutputMode, PluginRegistry,
    install_from_dir, load_plugin_from_dir, resolve_plugins_dir, resolve_safe_entry,
    run_browser_plugin, run_editor_plugin, run_editor_plugin_with_timeout, uninstall,
    validate_manifest, validate_plugin_id,
};
use photon_protocol::{PluginCapability, PluginManifest, PluginParamValue};

static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

struct TestTempDir {
    path: PathBuf,
}

impl TestTempDir {
    fn new(prefix: &str) -> Self {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "photon-plugins-test-{prefix}-{}-{seq}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn write_plugin(
    dir: &Path,
    id: &str,
    entry: &str,
    capabilities: &[&str],
    script_body: &str,
    extra_manifest_fields: &str,
) {
    fs::create_dir_all(dir).expect("create plugin dir");
    let caps_json: Vec<String> = capabilities.iter().map(|c| format!("\"{c}\"")).collect();
    let comma_extra = if extra_manifest_fields.is_empty() {
        String::new()
    } else {
        format!(",{extra_manifest_fields}")
    };
    let manifest = format!(
        r#"{{
            "id": "{id}",
            "name": "Test Plugin {id}",
            "version": "1.0.0",
            "entry": "{entry}",
            "capabilities": [{}]{comma_extra}
        }}"#,
        caps_json.join(",")
    );
    fs::write(dir.join("plugin.json"), manifest).expect("write plugin.json");
    if !entry.is_empty() && !entry.contains("..") && !entry.starts_with('/') {
        let entry_path = dir.join(entry);
        if let Some(parent) = entry_path.parent() {
            fs::create_dir_all(parent).expect("create entry parent");
        }
        fs::write(&entry_path, script_body).expect("write script");
    }
}

#[test]
fn validate_plugin_id_accepts_and_rejects() {
    for valid in ["com.example.demo", "my-plugin_1", "a", "Plugin123"] {
        assert!(validate_plugin_id(valid).is_ok(), "expected valid: {valid}");
    }
    for invalid in [
        "",
        ".",
        "..",
        "../evil",
        "com..example",
        ".hidden",
        "trailing.",
        "a/b",
        "a\\b",
        "bad id",
        "bad:id",
    ] {
        assert!(
            matches!(
                validate_plugin_id(invalid),
                Err(PluginError::InvalidPluginId { .. })
            ),
            "expected invalid id: {invalid}"
        );
    }
}

#[test]
fn path_traversal_and_symlink_escape_rejected() {
    let tmp = TestTempDir::new("traversal");
    let plugin_dir = tmp.path().join("my-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    for bad_entry in [
        "",
        "   ",
        ".",
        "./",
        "../outside.py",
        "sub/../../outside.py",
        "..\\outside.py",
        "/bin/sh",
        "\\Windows\\System32\\cmd.exe",
        "bad\0entry.py",
    ] {
        let err = resolve_safe_entry(&plugin_dir, bad_entry).unwrap_err();
        assert!(
            matches!(err, PluginError::PathTraversal { .. }),
            "expected PathTraversal for {bad_entry:?}, got {err:?}"
        );
    }

    let manifest = PluginManifest {
        id: "com.example.bad".to_owned(),
        name: "Bad".to_owned(),
        version: "1.0.0".to_owned(),
        description: String::new(),
        author: String::new(),
        entry: "../escape.sh".to_owned(),
        args: Vec::new(),
        capabilities: vec![PluginCapability::EditorFilter],
        params: Vec::new(),
        permissions: Vec::new(),
        timeout_ms: None,
    };
    assert!(matches!(
        validate_manifest(&manifest, &plugin_dir),
        Err(PluginError::PathTraversal { .. })
    ));

    #[cfg(unix)]
    {
        let outside = tmp.path().join("outside.sh");
        fs::write(&outside, "#!/bin/sh\necho '{}'").unwrap();
        let link = plugin_dir.join("escape.sh");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let err = resolve_safe_entry(&plugin_dir, "escape.sh").unwrap_err();
        assert!(matches!(err, PluginError::PathTraversal { .. }));
    }
}

#[test]
fn registry_discovers_plugins_skips_invalid_and_applies_disabled_list() {
    let tmp = TestTempDir::new("discover");
    let root = tmp.path().join("plugins");

    // Missing root returns empty cleanly.
    let empty_reg = PluginRegistry::discover(&root).unwrap();
    assert_eq!(empty_reg.plugins().count(), 0);

    // Valid plugin A & B.
    write_plugin(
        &root.join("com.example.alpha"),
        "com.example.alpha",
        "run.py",
        &["editor_filter"],
        "print('{}')\n",
        "",
    );
    write_plugin(
        &root.join("com.example.beta"),
        "com.example.beta",
        "sub/action.sh",
        &["browser_action"],
        "echo '{}'\n",
        "",
    );
    // Invalid plugin (path traversal) should be rejected and recorded in errors.
    write_plugin(
        &root.join("evil"),
        "com.example.evil",
        "../outside.py",
        &["editor_filter"],
        "",
        "",
    );
    // Unrelated folder without plugin.json should be ignored silently.
    fs::create_dir_all(root.join("cache")).unwrap();

    let reg = PluginRegistry::discover(&root).unwrap();
    assert_eq!(reg.plugins().count(), 2);
    assert!(reg.get("com.example.alpha").is_some());
    assert!(reg.get("com.example.beta").is_some());
    assert!(reg.get("com.example.evil").is_none());
    assert_eq!(reg.errors().len(), 1);

    let infos = reg.plugin_infos(&["com.example.beta".to_owned()]);
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].id, "com.example.alpha");
    assert!(infos[0].enabled);
    assert_eq!(infos[1].id, "com.example.beta");
    assert!(!infos[1].enabled);
}

#[test]
fn resolve_plugins_dir_honors_env_override() {
    let default_dir = PathBuf::from("/tmp/default-photon-plugins");
    let custom_dir = PathBuf::from("/tmp/custom-photon-plugins-override");

    // SAFETY: single-threaded check of env override within this test.
    unsafe {
        std::env::set_var(PLUGINS_DIR_ENV, &custom_dir);
    }
    let resolved = resolve_plugins_dir(&default_dir);
    unsafe {
        std::env::remove_var(PLUGINS_DIR_ENV);
    }
    assert_eq!(resolved, custom_dir);
    assert_eq!(resolve_plugins_dir(&default_dir), default_dir);
}

#[test]
fn install_and_uninstall_roundtrip_and_rejects_unsafe_packages() {
    let tmp = TestTempDir::new("install");
    let src = tmp.path().join("src-plugin");
    let root = tmp.path().join("installed-plugins");

    write_plugin(
        &src,
        "com.example.pkg",
        "bin/filter.sh",
        &["editor_filter"],
        "#!/bin/sh\necho '{}'\n",
        "",
    );
    fs::write(src.join("README.txt"), "hello").unwrap();

    let installed = install_from_dir(&src, &root).unwrap();
    assert_eq!(installed.manifest.id, "com.example.pkg");
    assert!(root.join("com.example.pkg/plugin.json").is_file());
    assert!(root.join("com.example.pkg/bin/filter.sh").is_file());
    assert!(root.join("com.example.pkg/README.txt").is_file());

    let reg = PluginRegistry::discover(&root).unwrap();
    assert_eq!(reg.plugins().count(), 1);

    // Reject uninstall with traversal id.
    assert!(matches!(
        uninstall("../com.example.pkg", &root),
        Err(PluginError::InvalidPluginId { .. })
    ));

    uninstall("com.example.pkg", &root).unwrap();
    assert!(!root.join("com.example.pkg").exists());
    assert!(matches!(
        uninstall("com.example.pkg", &root),
        Err(PluginError::NotFound { .. })
    ));

    // Reject installing a package containing a symlink.
    #[cfg(unix)]
    {
        let symlink_src = tmp.path().join("symlink-pkg");
        write_plugin(
            &symlink_src,
            "com.example.sym",
            "run.sh",
            &["editor_filter"],
            "echo '{}'\n",
            "",
        );
        std::os::unix::fs::symlink("/etc/hosts", symlink_src.join("leak.txt")).unwrap();
        assert!(matches!(
            install_from_dir(&symlink_src, &root),
            Err(PluginError::PathTraversal { .. })
        ));
    }
}

#[test]
fn editor_plugin_ndjson_roundtrip_modifies_png_and_cleans_temp_dir() {
    let tmp = TestTempDir::new("editor-run");
    let plugin_dir = tmp.path().join("com.example.invert");

    // Pure-stdlib Python script that reads NDJSON on stdin, asserts fields,
    // writes a valid modified 2x2 RGBA PNG to output_path, and returns new_layer JSON.
    let py_script = r#"
import json, struct, sys, zlib

line = sys.stdin.readline()
req = json.loads(line)
assert req["protocol"] == 1
assert req["action"] == "run"
assert req["capability"] == "editor_filter"
assert req["width"] == 2
assert req["height"] == 2
assert req["selection_mask_path"] is not None
assert req["params"]["gain"] == 2.5
assert req["params"]["invert"] is True

def png_chunk(chunk_type, data):
    return struct.pack("!I", len(data)) + chunk_type + data + struct.pack("!I", zlib.crc32(chunk_type + data) & 0xffffffff)

# Build a 2x2 RGBA PNG filled with green (10, 200, 30, 255).
raw_rows = b"".join([b"\x00" + bytes([10, 200, 30, 255]) * 2 for _ in range(2)])
ihdr = struct.pack("!IIBBBBB", 2, 2, 8, 6, 0, 0, 0)
png = (
    b"\x89PNG\r\n\x1a\n"
    + png_chunk(b"IHDR", ihdr)
    + png_chunk(b"IDAT", zlib.compress(raw_rows))
    + png_chunk(b"IEND", b"")
)
with open(req["output_path"], "wb") as f:
    f.write(png)

# Record the temp dir path so the Rust test can assert Drop cleaned it up.
with open("last_temp_input.txt", "w") as f:
    f.write(req["input_path"])

print(json.dumps({
    "ok": True,
    "mode": "new_layer",
    "layer_name": "Inverted Output",
    "message": "Done"
}), flush=True)
"#;

    write_plugin(
        &plugin_dir,
        "com.example.invert",
        "filter.py",
        &["editor_filter"],
        py_script,
        r#""params":[{"key":"invert","label":"Invert","kind":"bool","default":true}]"#,
    );

    let loaded = load_plugin_from_dir(&plugin_dir).unwrap();
    let input = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
    let mask = GrayImage::from_pixel(2, 2, Luma([255]));
    let mut params = BTreeMap::new();
    params.insert("gain".to_owned(), PluginParamValue::Float(2.5));

    let res = run_editor_plugin(&loaded, &input, Some(&mask), &params).unwrap();
    assert_eq!(res.mode, PluginOutputMode::NewLayer);
    assert_eq!(res.layer_name.as_deref(), Some("Inverted Output"));
    assert_eq!(res.message.as_deref(), Some("Done"));
    assert_eq!(res.image.dimensions(), (2, 2));
    assert_eq!(*res.image.get_pixel(0, 0), Rgba([10, 200, 30, 255]));

    // Verify scoped temp directory was cleaned up by TempDirGuard Drop.
    let recorded_input = fs::read_to_string(plugin_dir.join("last_temp_input.txt")).unwrap();
    let temp_dir_used = Path::new(&recorded_input).parent().unwrap();
    assert!(!temp_dir_used.exists(), "temp dir should be cleaned up");
}

#[test]
fn browser_plugin_executes_executable_sh_and_sends_selection() {
    let tmp = TestTempDir::new("browser-run");
    let plugin_dir = tmp.path().join("com.example.browser");

    let sh_script = r#"#!/bin/sh
read -r req
case "$req" in
  *'"capability":"browser_action"'*'"/photos/a.png"'*'"current_folder":"/photos"'*)
    printf '{"ok":true,"mode":"browser_refresh","message":"Tagged 1 item"}\n'
    ;;
  *)
    printf '{"ok":false,"error":"unexpected payload"}\n'
    ;;
esac
"#;

    write_plugin(
        &plugin_dir,
        "com.example.browser",
        "action.sh",
        &["browser_action"],
        sh_script,
        "",
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = plugin_dir.join("action.sh");
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
    }

    let loaded = load_plugin_from_dir(&plugin_dir).unwrap();
    let selected = vec!["/photos/a.png".to_owned()];
    let res = run_browser_plugin(&loaded, &selected, Some("/photos"), &BTreeMap::new()).unwrap();
    assert_eq!(res.mode, PluginOutputMode::BrowserRefresh);
    assert_eq!(res.message.as_deref(), Some("Tagged 1 item"));

    // Wrong capability check: running a browser plugin as editor filter must fail.
    let input = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
    let err = run_editor_plugin(&loaded, &input, None, &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, PluginError::UnsupportedCapability { .. }));
}

#[test]
fn plugin_timeout_kills_process_and_plugin_error_is_surfaced() {
    let tmp = TestTempDir::new("timeout-and-err");

    // 1. Timeout test using a sleeping shell script.
    let slow_dir = tmp.path().join("com.example.slow");
    write_plugin(
        &slow_dir,
        "com.example.slow",
        "slow.sh",
        &["browser_action"],
        "sleep 5\n",
        r#""timeoutMs": 120"#,
    );
    let slow = load_plugin_from_dir(&slow_dir).unwrap();
    let start = Instant::now();
    let err = run_browser_plugin(&slow, &[], None, &BTreeMap::new()).unwrap_err();
    assert!(
        matches!(err, PluginError::Timeout { .. }),
        "expected Timeout, got {err:?}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "timeout took too long: {:?}",
        start.elapsed()
    );

    // 2. Explicit error response test.
    let err_dir = tmp.path().join("com.example.fail");
    write_plugin(
        &err_dir,
        "com.example.fail",
        "fail.sh",
        &["browser_action"],
        "read -r _\nprintf '{\"ok\":false,\"error\":\"bad input\"}\\n'\n",
        "",
    );
    let failing = load_plugin_from_dir(&err_dir).unwrap();
    let err = run_browser_plugin(&failing, &[], None, &BTreeMap::new()).unwrap_err();
    match err {
        PluginError::PluginReportedError { message, .. } => assert_eq!(message, "bad input"),
        other => panic!("expected PluginReportedError, got {other:?}"),
    }
}

#[test]
fn editor_plugin_rejects_oversized_output_png_dimensions() {
    let tmp = TestTempDir::new("oversized");
    let plugin_dir = tmp.path().join("com.example.huge");

    // Writes PNG signature + IHDR claiming 40000x40000 + minimal IDAT so header probing succeeds without OOM.
    let py_script = r#"
import json, struct, sys, zlib

req = json.loads(sys.stdin.readline())
ihdr = struct.pack("!IIBBBBB", 40000, 40000, 8, 6, 0, 0, 0)
def chunk(t, d):
    return struct.pack("!I", len(d)) + t + d + struct.pack("!I", zlib.crc32(t + d) & 0xffffffff)
with open(req["output_path"], "wb") as f:
    f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(b"\x00")) + chunk(b"IEND", b""))
print(json.dumps({"ok": True, "mode": "replace_layer"}), flush=True)
"#;

    write_plugin(
        &plugin_dir,
        "com.example.huge",
        "huge.py",
        &["editor_filter"],
        py_script,
        "",
    );
    let loaded = load_plugin_from_dir(&plugin_dir).unwrap();
    let input = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
    let err = run_editor_plugin_with_timeout(
        &loaded,
        &input,
        None,
        &BTreeMap::new(),
        Some(Duration::from_secs(5)),
    )
    .unwrap_err();

    match err {
        PluginError::InvalidOutputDimensions { width, height, max } => {
            assert_eq!(width, 40_000);
            assert_eq!(height, 40_000);
            assert_eq!(max, MAX_PLUGIN_IMAGE_DIMENSION);
        }
        other => panic!("expected InvalidOutputDimensions, got {other:?}"),
    }
}

#[test]
fn jsonl_manifest_discovery_traversal_rejection_and_install() {
    let tmp = TestTempDir::new("jsonl-manifest");
    let data_dir = tmp.path().join("data");
    let root = data_dir.join("plugins");
    let pkg_dir = tmp.path().join("pkg");
    fs::create_dir_all(pkg_dir.join("bin")).unwrap();

    fs::write(pkg_dir.join("bin/worker.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::write(
        pkg_dir.join("manifest.json"),
        r#"{
            "apiVersion": 1,
            "id": "photon.demo.onnx",
            "name": "Demo ONNX",
            "version": "1.2.0",
            "entry": {
                "command": "bin/worker.sh",
                "args": ["--device", "${device}"],
                "protocol": "jsonl-stdio"
            },
            "permissions": { "network": false, "gpu": true },
            "models": [
                { "id": "fast-m", "name": "Fast", "tier": "fast", "file": "fast.onnx" },
                { "id": "hq-m", "name": "HQ", "tier": "high", "file": "hq.onnx" }
            ],
            "capabilities": [
                {
                    "action": "editor.demo",
                    "workspace": "editor",
                    "label": "Demo Filter",
                    "output": "rgba_image",
                    "params": [
                        { "key": "feather", "type": "int", "min": 0, "max": 20, "default": 4 }
                    ]
                }
            ],
            "quickActions": [
                {
                    "id": "demo-qa",
                    "label": "Demo Quick",
                    "action": "editor.demo",
                    "workspaces": ["browser", "viewer"],
                    "params": { "modelId": "hq-m", "feather": 2 }
                }
            ]
        }"#,
    )
    .unwrap();

    let installed = install_from_dir(&pkg_dir, &root).unwrap();
    assert!(installed.is_jsonl_stdio());
    assert_eq!(installed.manifest.id, "photon.demo.onnx");
    assert!(
        installed
            .manifest
            .capabilities
            .contains(&PluginCapability::EditorFilter)
    );
    assert!(
        installed
            .manifest
            .capabilities
            .contains(&PluginCapability::BrowserAction)
    );
    assert_eq!(installed.manifest.permissions, vec!["gpu"]);

    let reg = PluginRegistry::discover(&root).unwrap();
    assert!(reg.get("photon.demo.onnx").is_some());

    // Reject path traversal in manifest.json entry.command.
    let bad_dir = root.join("bad-jsonl");
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(
        bad_dir.join("manifest.json"),
        r#"{
            "apiVersion": 1,
            "id": "photon.bad.jsonl",
            "name": "Bad",
            "version": "1.0.0",
            "entry": { "command": "../escape.sh", "protocol": "jsonl-stdio" },
            "capabilities": []
        }"#,
    )
    .unwrap();
    let reg2 = PluginRegistry::discover(&root).unwrap();
    assert!(reg2.get("photon.bad.jsonl").is_none());
    assert_eq!(reg2.errors().len(), 1);
}

#[test]
fn jsonl_stdio_multiline_progress_done_and_error_execution() {
    let tmp = TestTempDir::new("jsonl-run");
    let data_dir = tmp.path().join("data");
    let plugins_dir = data_dir.join("plugins");
    let models_dir = data_dir.join("models");
    let plugin_dir = plugins_dir.join("photon.mock.jsonl");
    fs::create_dir_all(plugin_dir.join("bin")).unwrap();
    fs::create_dir_all(&models_dir).unwrap();
    fs::write(models_dir.join("demo.onnx"), b"weights").unwrap();

    let py_worker = r#"
import json, os, shutil, sys

saw_shutdown = False
req = None
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("op") == "shutdown":
        saw_shutdown = True
        break
    if msg.get("op") == "run":
        req = msg
        assert req["action"] == "editor.mock"
        assert req["device"] == "cpu"
        assert "--device" in sys.argv and "cpu" in sys.argv
        assert req["modelId"] == "demo-m"
        assert req["modelPath"].endswith("models/demo.onnx")
        assert req["params"]["modelsDir"].endswith("models")
        if os.name == "posix":
            assert "bin" in os.environ.get("LD_LIBRARY_PATH", "")
        if req["params"].get("forceFail"):
            print(json.dumps({"id": req["id"], "event": "progress", "percent": 15, "stage": "Failing"}), flush=True)
            print(json.dumps({"id": req["id"], "event": "error", "code": "boom", "message": "synthetic jsonl error"}), flush=True)
            sys.exit(0)
        print(json.dumps({"id": req["id"], "event": "progress", "percent": 50, "stage": "Halfway"}), flush=True)
        shutil.copyfile(req["input"], req["output"])
        print(json.dumps({
            "id": req["id"],
            "event": "done",
            "meta": {"captionText": "1girl, solo"}
        }), flush=True)

assert saw_shutdown
"#;
    fs::write(plugin_dir.join("bin/worker.py"), py_worker).unwrap();
    fs::write(
        plugin_dir.join("manifest.json"),
        r#"{
            "apiVersion": 1,
            "id": "photon.mock.jsonl",
            "name": "Mock JSONL",
            "version": "1.0.0",
            "entry": {
                "command": "bin/worker.py",
                "args": ["--device", "${device}"],
                "protocol": "jsonl-stdio"
            },
            "models": [
                { "id": "demo-m", "name": "Demo", "tier": "fast", "file": "demo.onnx" }
            ],
            "capabilities": [
                {
                    "action": "editor.mock",
                    "workspace": "editor",
                    "label": "Mock",
                    "output": "rgba_image"
                }
            ],
            "quickActions": [
                {
                    "id": "mock-qa",
                    "label": "Mock QA",
                    "action": "editor.mock",
                    "workspaces": ["browser"],
                    "params": {}
                }
            ]
        }"#,
    )
    .unwrap();

    let loaded = load_plugin_from_dir(&plugin_dir).unwrap();
    let input = RgbaImage::from_pixel(3, 2, Rgba([42, 128, 200, 255]));

    // 1. Editor execution with progress + done + captionText metadata.
    let res = run_editor_plugin(&loaded, &input, None, &BTreeMap::new()).unwrap();
    assert_eq!(res.image, input);
    assert_eq!(res.message.as_deref(), Some("1girl, solo"));

    // 2. Browser execution creates numbered backup `photo.png~` safely.
    let sample_file = tmp.path().join("photo.png");
    input.save(&sample_file).unwrap();
    let b_res = run_browser_plugin(
        &loaded,
        &[sample_file.display().to_string()],
        Some(tmp.path().to_str().unwrap()),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(b_res.mode, PluginOutputMode::BrowserRefresh);
    assert!(tmp.path().join("photo.png~").is_file());

    // 3. Error event surfaced as PluginReportedError.
    let mut err_params = BTreeMap::new();
    err_params.insert("forceFail".to_owned(), PluginParamValue::Bool(true));
    let err = run_editor_plugin(&loaded, &input, None, &err_params).unwrap_err();
    match err {
        PluginError::PluginReportedError { message, .. } => {
            assert_eq!(message, "synthetic jsonl error");
        }
        other => panic!("expected PluginReportedError, got {other:?}"),
    }
}

#[test]
fn registry_exports_viewer_actions_quick_actions_and_model_statuses() {
    use photon_plugins::{AiWorkerEvent, run_jsonl_inference_streaming};
    use photon_protocol::{AiDevice, AiSettings};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let tmp = TestTempDir::new("models-and-streaming");
    let data_dir = tmp.path().join("data");
    let plugins_dir = data_dir.join("plugins");
    let models_dir = data_dir.join("models");
    let plugin_dir = plugins_dir.join("photon.ocr.mock");
    fs::create_dir_all(plugin_dir.join("bin")).unwrap();
    fs::create_dir_all(&models_dir).unwrap();

    let worker = r#"
import json, sys, time
for line in sys.stdin:
    msg = json.loads(line)
    if msg.get("op") == "shutdown":
        break
    if msg.get("op") == "run":
        if msg["params"].get("sleep"):
            time.sleep(5)
        print(json.dumps({"id": msg["id"], "event": "progress", "percent": 42, "stage": "OCR"}), flush=True)
        print(json.dumps({
            "id": msg["id"],
            "event": "done",
            "meta": {
                "device": "cpu",
                "clipboardText": "hello world",
                "translation": {
                    "path": msg["input"],
                    "sourceLang": "eng",
                    "targetLang": "vi",
                    "originalText": "hello",
                    "translatedText": "xin chao",
                    "blocks": [{"x": 1, "y": 2, "w": 30, "h": 12, "original": "hello", "translated": "xin chao"}]
                }
            }
        }), flush=True)
"#;
    fs::write(plugin_dir.join("bin/worker.py"), worker).unwrap();
    fs::write(
        plugin_dir.join("manifest.json"),
        r#"{
            "apiVersion": 1,
            "id": "photon.ocr.mock",
            "name": "Mock Viewer OCR",
            "version": "1.0.0",
            "entry": { "command": "bin/worker.py", "args": [], "protocol": "jsonl-stdio" },
            "models": [
                {
                    "id": "mock-ocr-pack",
                    "name": "Mock OCR Pack",
                    "tier": "ocr",
                    "file": "mock.traineddata",
                    "sizeBytes": 128,
                    "sha256": "",
                    "url": "https://example.invalid/mock.traineddata"
                }
            ],
            "capabilities": [
                { "action": "viewer.translate", "workspace": "viewer", "label": "Translate", "output": "translation" }
            ],
            "quickActions": [
                { "id": "mock-qa", "label": "Copy OCR", "action": "viewer.translate", "workspaces": ["viewer"], "params": {} }
            ]
        }"#,
    )
    .unwrap();

    let reg = PluginRegistry::discover(&plugins_dir).unwrap();
    let infos = reg.plugin_infos(&[]);
    assert_eq!(infos.len(), 1);
    assert!(infos[0].available);
    assert_eq!(infos[0].viewer_actions, vec!["viewer.translate"]);
    assert_eq!(infos[0].quick_actions.len(), 1);
    assert_eq!(infos[0].models.len(), 1);

    let qa = reg.quick_actions(&[]);
    assert_eq!(qa.len(), 1);
    assert_eq!(qa[0].plugin_id, "photon.ocr.mock");
    assert!(reg.find_by_action("viewer.translate", &[]).is_some());
    assert!(
        reg.find_by_action("viewer.translate", &["photon.ocr.mock".to_owned()])
            .is_none()
    );

    let custom_file = tmp.path().join("custom.onnx");
    fs::write(&custom_file, b"onnx").unwrap();
    let mut settings = AiSettings::default();
    settings
        .custom_model_paths
        .insert("rmbg-1.4".to_owned(), custom_file.display().to_string());
    let statuses = reg.model_statuses(&models_dir, &settings);
    let rmbg = statuses.iter().find(|m| m.id == "rmbg-1.4").unwrap();
    assert!(rmbg.installed);
    assert_eq!(
        rmbg.effective_path.as_deref(),
        Some(custom_file.to_str().unwrap())
    );
    let ocr_pack = statuses.iter().find(|m| m.id == "mock-ocr-pack").unwrap();
    assert!(!ocr_pack.installed);

    let loaded = reg.get("photon.ocr.mock").unwrap();
    let img = tmp.path().join("in.png");
    fs::write(&img, b"fake").unwrap();
    let mut events = Vec::new();
    run_jsonl_inference_streaming(
        loaded,
        "viewer.translate",
        "",
        Path::new(""),
        AiDevice::Cpu,
        &models_dir,
        &img,
        Path::new(""),
        serde_json::json!({}),
        Arc::new(AtomicBool::new(false)),
        |ev| events.push(ev),
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AiWorkerEvent::Progress { percent: 42, stage } if stage == "OCR"))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        AiWorkerEvent::InferenceDone {
            clipboard_text: Some(txt),
            translation: Some(tr),
            ..
        } if txt == "hello world" && tr.translated_text == "xin chao"
    )));

    // Cancellation interrupts sleeping worker.
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_bg = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        cancel_bg.store(true, Ordering::SeqCst);
    });
    let mut cancel_events = Vec::new();
    run_jsonl_inference_streaming(
        loaded,
        "viewer.translate",
        "",
        Path::new(""),
        AiDevice::Cpu,
        &models_dir,
        &img,
        Path::new(""),
        serde_json::json!({ "sleep": true }),
        cancel,
        |ev| cancel_events.push(ev),
    );
    assert!(
        cancel_events
            .iter()
            .any(|e| matches!(e, AiWorkerEvent::Cancelled))
    );
}
