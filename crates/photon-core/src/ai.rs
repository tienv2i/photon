//! Async AI plugin host, model download manager, Quick Actions, OCR/Translate,
//! Caption sidecars, and Editor AI tools (`remove_background`, `dewatermark`,
//! `ai_inpaint`, `refine_alpha`) integrated with Photon's `CoreRuntime`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use image::RgbaImage;
use photon_image::LayerOperation;
use photon_plugins::{
    AiWorkerEvent, LoadedPlugin, PluginRegistry, REMBG_ACTION, commit_browser_image_output,
    delete_model_file, download_model, resolve_models_dir_for, resolve_plugins_dir,
    run_jsonl_inference_streaming,
};
use photon_protocol::{
    AiDevice, AiMaskStroke, AiTaskState, ApplyTarget, PluginModelSpec, ProtocolError,
    TaskRunStatus, TaskSnapshot, TranslateProvider, TranslationBlock, UiText,
};
use serde_json::{Value, json};

use crate::core::{Core, commit_active_layer_pixels, map_image_err, no_doc_err};
use crate::translate;

/// Background AI task plan handed off from `Core::dispatch` to `CoreRuntime`.
#[derive(Debug, Clone)]
pub enum PendingAiJob {
    DownloadModel {
        task_id: String,
        spec: PluginModelSpec,
        models_dir: PathBuf,
        cancel: Arc<AtomicBool>,
    },
    PluginInference {
        task_id: String,
        plugin: LoadedPlugin,
        action: String,
        model_id: String,
        model_path: PathBuf,
        device: AiDevice,
        models_dir: PathBuf,
        input_path: PathBuf,
        output_path: PathBuf,
        params: Value,
        cancel: Arc<AtomicBool>,
    },
    BuiltinTranslate {
        task_id: String,
        image_path: PathBuf,
        region: Option<[i32; 4]>,
        source_lang: String,
        target_lang: String,
        settings: photon_protocol::TranslateSettings,
    },
}

impl PendingAiJob {
    pub fn task_id(&self) -> &str {
        match self {
            Self::DownloadModel { task_id, .. }
            | Self::PluginInference { task_id, .. }
            | Self::BuiltinTranslate { task_id, .. } => task_id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::DownloadModel { .. } => "download",
            Self::BuiltinTranslate { .. } => "translate",
            Self::PluginInference { action, .. } => {
                if action.starts_with("viewer.translate")
                    || action.starts_with("viewer.ocr")
                    || action.starts_with("viewer.ai_ocr")
                {
                    "translate"
                } else if action.contains("caption") {
                    "caption"
                } else {
                    "inference"
                }
            }
        }
    }

    /// Execute the pending AI job synchronously on a worker thread, forwarding
    /// progress and completion `AiWorkerEvent`s to `on_event`.
    pub fn run(self, mut on_event: impl FnMut(AiWorkerEvent)) {
        match self {
            Self::DownloadModel {
                spec,
                models_dir,
                cancel,
                ..
            } => {
                download_model(&spec, &models_dir, cancel, on_event);
            }
            Self::PluginInference {
                plugin,
                action,
                model_id,
                model_path,
                device,
                models_dir,
                input_path,
                output_path,
                params,
                cancel,
                ..
            } => {
                run_jsonl_inference_streaming(
                    &plugin,
                    &action,
                    &model_id,
                    &model_path,
                    device,
                    &models_dir,
                    &input_path,
                    &output_path,
                    params,
                    cancel,
                    on_event,
                );
            }
            Self::BuiltinTranslate {
                image_path,
                region,
                source_lang,
                target_lang,
                settings,
                ..
            } => {
                on_event(AiWorkerEvent::Progress {
                    percent: 25,
                    stage: "Running OCR…".to_owned(),
                });
                match translate::run_ocr_and_translate(
                    &image_path,
                    region,
                    &source_lang,
                    &target_lang,
                    &settings,
                ) {
                    Ok(res) => on_event(AiWorkerEvent::InferenceDone {
                        model_id: String::new(),
                        output_path: PathBuf::new(),
                        device_used: "cpu".to_owned(),
                        clipboard_text: (!res.original_text.trim().is_empty())
                            .then(|| res.original_text.clone()),
                        translation: Some(res),
                        caption_text: None,
                        saved_path: None,
                    }),
                    Err(err) => on_event(AiWorkerEvent::Failed {
                        message: err.message,
                    }),
                }
            }
        }
    }
}

/// Load `<image_stem>.txt` sidecar caption if present beside `image_path`.
pub fn load_caption_for_image(image_path: &Path) -> Option<String> {
    let txt_path = image_path.with_extension("txt");
    let content = fs::read_to_string(txt_path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

impl Core {
    pub fn effective_models_dir(&self) -> PathBuf {
        resolve_models_dir_for(None, Some(&self.paths.models_dir()))
    }

    pub fn take_pending_ai(&mut self) -> Option<PendingAiJob> {
        self.pending_ai.take()
    }

    /// Execute any queued AI job inline on the current thread (used by unit/headless tests).
    pub fn run_pending_ai_blocking(&mut self) -> bool {
        let Some(job) = self.take_pending_ai() else {
            return false;
        };
        let task_id = job.task_id().to_owned();
        let mut final_ev: Option<AiWorkerEvent> = None;
        job.run(|ev| match ev {
            AiWorkerEvent::Progress { percent, stage } => {
                self.apply_ai_progress(&task_id, percent, stage);
            }
            other => {
                final_ev = Some(other);
            }
        });
        if let Some(ev) = final_ev {
            self.apply_ai_finished(&task_id, ev);
        }
        true
    }

    pub fn apply_ai_progress(
        &mut self,
        task_id: &str,
        percent: u8,
        stage: String,
    ) -> Option<TaskSnapshot> {
        if let Some(task) = self.ai_task.as_mut() {
            task.percent = percent.min(99);
            task.stage = stage.clone();
        }
        if let Some(snap) = self.tasks.iter_mut().find(|t| t.task_id == task_id) {
            snap.percent = percent.min(99);
            snap.stage = stage;
            return Some(snap.clone());
        }
        None
    }

    pub fn apply_ai_finished(&mut self, task_id: &str, event: AiWorkerEvent) -> (bool, String) {
        let mut ok = true;
        let finished_msg: String;

        match event {
            AiWorkerEvent::Progress { percent, stage } => {
                self.apply_ai_progress(task_id, percent, stage);
                return (true, String::new());
            }
            AiWorkerEvent::DownloadDone { model_id, .. } => {
                let _ = self.refresh_plugins_registry();
                if let Some(task) = self.ai_task.as_mut() {
                    task.running = false;
                    task.percent = 100;
                    task.stage = format!("Downloaded {model_id}");
                    task.error = None;
                }
                finished_msg = format!("Downloaded {model_id}");
                self.status = UiText::new("ready");
            }
            AiWorkerEvent::InferenceDone {
                model_id,
                output_path,
                device_used,
                clipboard_text,
                translation,
                caption_text,
                saved_path,
            } => {
                if let Some(tr) = translation {
                    self.ai_quick_action_target = None;
                    self.latest_translation = Some(tr.clone());
                    if let Some(task) = self.ai_task.as_mut() {
                        task.running = false;
                        task.percent = 100;
                        task.stage = "Translated".to_owned();
                        task.error = None;
                        task.clipboard_text = clipboard_text;
                        task.translation = Some(tr);
                    }
                    finished_msg = "Translation complete".to_owned();
                    self.status = UiText::new("translate_done");
                } else if let Some(text) = clipboard_text {
                    self.ai_quick_action_target = None;
                    if let Some(cap) = &caption_text {
                        self.editor_caption = Some(cap.clone());
                    }
                    let chars = text.chars().count();
                    let stage = format!("Copied {chars} characters");
                    if let Some(task) = self.ai_task.as_mut() {
                        task.running = false;
                        task.percent = 100;
                        task.stage = stage.clone();
                        task.error = None;
                        task.clipboard_text = Some(text);
                        task.caption_text = caption_text;
                        task.saved_path = saved_path;
                    }
                    finished_msg = stage;
                    self.status = UiText::with(
                        "plugin_executed",
                        [("name", self.ai_plugin_label.as_deref().unwrap_or("Plugin"))],
                    );
                } else if let Some(cap) = caption_text {
                    self.ai_quick_action_target = None;
                    self.editor_caption = Some(cap.clone());
                    if let Some(task) = self.ai_task.as_mut() {
                        task.running = false;
                        task.percent = 100;
                        task.stage = "Caption generated".to_owned();
                        task.error = None;
                        task.caption_text = Some(cap);
                        task.saved_path = saved_path;
                    }
                    finished_msg = "Caption generated".to_owned();
                    self.status = UiText::with(
                        "plugin_executed",
                        [("name", self.ai_plugin_label.as_deref().unwrap_or("Caption"))],
                    );
                } else if let Some((plugin_id, action, src_path)) =
                    self.ai_quick_action_target.take()
                {
                    match commit_browser_image_output(&plugin_id, &action, &src_path, &output_path)
                    {
                        Ok(dest) => {
                            let out_name = dest
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("image")
                                .to_owned();
                            let stage = format!("Saved {out_name} ({device_used})");
                            if let Some(task) = self.ai_task.as_mut() {
                                task.running = false;
                                task.percent = 100;
                                task.stage = stage.clone();
                                task.error = None;
                                task.saved_path = Some(dest.to_string_lossy().into_owned());
                            }
                            self.refresh_after_file_op();
                            finished_msg = stage;
                            self.status = UiText::with(
                                "plugin_executed",
                                [(
                                    "name",
                                    self.ai_plugin_label.as_deref().unwrap_or(&plugin_id),
                                )],
                            );
                        }
                        Err(err) => {
                            ok = false;
                            let msg = err.to_string();
                            if let Some(task) = self.ai_task.as_mut() {
                                task.running = false;
                                task.error = Some(msg.clone());
                            }
                            finished_msg = msg.clone();
                            self.status = UiText::with(
                                "plugin_failed",
                                [
                                    (
                                        "name",
                                        self.ai_plugin_label.as_deref().unwrap_or(&plugin_id),
                                    ),
                                    ("error", msg.as_str()),
                                ],
                            );
                        }
                    }
                } else {
                    match image::open(&output_path).map(|img| img.into_rgba8()) {
                        Ok(rgba) => {
                            let label = self
                                .ai_plugin_label
                                .clone()
                                .unwrap_or_else(|| format!("AI ({model_id})"));
                            let apply_target = self.ai_apply_target;
                            match self.commit_ai_editor_pixels(rgba, &label, apply_target) {
                                Ok(()) => {
                                    let stage = format!("Done ({device_used})");
                                    if let Some(task) = self.ai_task.as_mut() {
                                        task.running = false;
                                        task.percent = 100;
                                        task.stage = stage.clone();
                                        task.error = None;
                                    }
                                    finished_msg = stage;
                                    self.status =
                                        UiText::with("plugin_executed", [("name", label)]);
                                }
                                Err(err) => {
                                    ok = false;
                                    if let Some(task) = self.ai_task.as_mut() {
                                        task.running = false;
                                        task.error = Some(err.message.clone());
                                    }
                                    finished_msg = err.message.clone();
                                    self.status = UiText::with(
                                        "plugin_failed",
                                        [("name", label.as_str()), ("error", err.message.as_str())],
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            ok = false;
                            let msg = format!("cannot decode plugin output PNG: {err}");
                            if let Some(task) = self.ai_task.as_mut() {
                                task.running = false;
                                task.error = Some(msg.clone());
                            }
                            finished_msg = msg.clone();
                            self.status = UiText::with(
                                "plugin_failed",
                                [
                                    ("name", self.ai_plugin_label.as_deref().unwrap_or("AI")),
                                    ("error", msg.as_str()),
                                ],
                            );
                        }
                    }
                }
            }
            AiWorkerEvent::Failed { message } => {
                ok = false;
                if let Some(task) = self.ai_task.as_mut() {
                    task.running = false;
                    task.error = Some(message.clone());
                }
                finished_msg = message.clone();
                self.status = UiText::with(
                    "plugin_failed",
                    [
                        ("name", self.ai_plugin_label.as_deref().unwrap_or("AI")),
                        ("error", message.as_str()),
                    ],
                );
            }
            AiWorkerEvent::Cancelled => {
                ok = false;
                if let Some(task) = self.ai_task.as_mut() {
                    task.running = false;
                    task.stage = "Cancelled".to_owned();
                }
                finished_msg = "Cancelled".to_owned();
                self.status = UiText::with("task_cancelled", [("taskId", task_id)]);
            }
        }

        self.cleanup_ai_job_state(task_id, ok, &finished_msg);
        self.revision += 1;
        (ok, finished_msg)
    }

    fn cleanup_ai_job_state(&mut self, task_id: &str, ok: bool, finished_msg: &str) {
        self.ai_cancel = None;
        self.ai_quick_action_target = None;
        self.ai_apply_target = ApplyTarget::WholeImage;
        self.ai_plugin_label = None;
        self.ai_active_task_id = None;
        if let Some(p) = self.ai_temp_input.take() {
            let _ = fs::remove_file(p);
        }
        if let Some(p) = self.ai_temp_output.take() {
            let _ = fs::remove_file(p);
        }
        if let Some(task) = self.ai_task.as_mut() {
            task.running = false;
        }
        if let Some(snap) = self.tasks.iter_mut().find(|t| t.task_id == task_id) {
            snap.status = if finished_msg == "Cancelled" {
                TaskRunStatus::Cancelled
            } else if ok {
                TaskRunStatus::Completed
            } else {
                TaskRunStatus::Failed
            };
            if ok {
                snap.percent = 100;
            }
            snap.stage = finished_msg.to_owned();
        }
    }

    fn commit_ai_editor_pixels(
        &mut self,
        rgba: RgbaImage,
        layer_label: &str,
        target: ApplyTarget,
    ) -> Result<(), ProtocolError> {
        let doc = self.editor.as_mut().ok_or_else(no_doc_err)?;
        let (cw, ch) = doc.dimensions();
        let (offset_x, offset_y, single_full) = {
            let stack = doc.stack();
            let active = stack.active_layer();
            let single_full = stack.layers.len() == 1
                && stack.background.is_none()
                && active.offset_x == 0
                && active.offset_y == 0
                && active.dimensions() == (cw, ch);
            if target == ApplyTarget::ActiveLayer {
                (active.offset_x, active.offset_y, true)
            } else {
                (0, 0, single_full)
            }
        };
        let replace_active =
            target != ApplyTarget::NewLayer && (target == ApplyTarget::ActiveLayer || single_full);
        if replace_active {
            commit_active_layer_pixels(doc, rgba)?;
        } else {
            doc.apply_layer_op(LayerOperation::AddImage {
                name: layer_label.to_owned(),
                offset_x,
                offset_y,
                pixels: Arc::new(rgba),
            })
            .map_err(map_image_err)?;
        }
        Ok(())
    }

    pub(crate) fn refresh_plugins_registry(&mut self) -> Result<usize, ProtocolError> {
        let plugins_dir = resolve_plugins_dir(self.paths.plugins_dir());
        if self.plugin_registry.plugins_dir() != plugins_dir {
            self.plugin_registry =
                PluginRegistry::discover(&plugins_dir).map_err(crate::core::map_plugin_err)?;
        } else {
            self.plugin_registry
                .refresh()
                .map_err(crate::core::map_plugin_err)?;
        }
        Ok(self.plugin_registry.plugins().count())
    }

    fn ensure_ai_idle(&self) -> Result<(), ProtocolError> {
        if self.pending_ai.is_some() || self.ai_active_task_id.is_some() {
            return Err(ProtocolError::new("busy", "an AI task is already running"));
        }
        Ok(())
    }

    fn next_ai_task_id(&mut self) -> String {
        let seq = self.next_job_seq;
        self.next_job_seq += 1;
        format!("ai-{seq}")
    }

    pub(crate) fn ai_set_settings(
        &mut self,
        device: AiDevice,
        auto_unload_secs: u32,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.config.ai.device = device;
        self.config.ai.auto_unload_secs = auto_unload_secs;
        self.config.ai = self.config.ai.clone().sanitized();
        self.persist_config();
        Ok((Some(UiText::new("settings_saved")), None))
    }

    pub(crate) fn ai_download_model(
        &mut self,
        model_id: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.ensure_ai_idle()?;
        let _ = self.refresh_plugins_registry();
        let Some(spec) = self
            .plugin_registry
            .catalog_models()
            .into_iter()
            .find(|m| m.id == model_id)
        else {
            return Err(ProtocolError::new(
                "not_found",
                format!("unknown AI model: {model_id}"),
            ));
        };

        let task_id = self.next_ai_task_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let stage = format!("Downloading {}…", spec.name);
        self.ai_cancel = Some(Arc::clone(&cancel));
        self.ai_active_task_id = Some(task_id.clone());
        self.ai_plugin_label = Some(spec.name.clone());
        self.ai_task = Some(AiTaskState {
            kind: "download".to_owned(),
            target: spec.id.clone(),
            running: true,
            percent: 0,
            stage: stage.clone(),
            ..Default::default()
        });
        self.tasks.push(TaskSnapshot {
            task_id: task_id.clone(),
            kind: "download".to_owned(),
            status: TaskRunStatus::Running,
            percent: 0,
            stage,
        });
        let models_dir = self.paths.models_dir();
        self.pending_ai = Some(PendingAiJob::DownloadModel {
            task_id: task_id.clone(),
            spec,
            models_dir,
            cancel,
        });
        Ok((None, Some(json!({ "jobId": task_id }))))
    }

    pub(crate) fn ai_delete_model(
        &mut self,
        model_id: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let _ = self.refresh_plugins_registry();
        let Some(spec) = self
            .plugin_registry
            .catalog_models()
            .into_iter()
            .find(|m| m.id == model_id)
        else {
            return Err(ProtocolError::new(
                "not_found",
                format!("unknown AI model: {model_id}"),
            ));
        };
        delete_model_file(&spec, &self.paths.models_dir()).map_err(crate::core::map_plugin_err)?;
        self.config.ai.custom_model_paths.remove(model_id);
        self.persist_config();
        Ok((Some(UiText::new("settings_saved")), None))
    }

    pub(crate) fn ai_set_custom_model(
        &mut self,
        model_id: &str,
        path: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.config.ai.custom_model_paths.remove(model_id);
        } else {
            let candidate = PathBuf::from(trimmed);
            if !candidate.is_file() {
                return Err(ProtocolError::new(
                    "not_found",
                    format!("model file not found: {}", candidate.display()),
                ));
            }
            self.config
                .ai
                .custom_model_paths
                .insert(model_id.to_owned(), trimmed.to_owned());
        }
        self.persist_config();
        Ok((Some(UiText::new("settings_saved")), None))
    }

    pub(crate) fn ai_cancel(&mut self) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        if let Some(cancel) = &self.ai_cancel {
            cancel.store(true, Ordering::SeqCst);
            let tid = self
                .ai_active_task_id
                .clone()
                .unwrap_or_else(|| "ai".to_owned());
            Ok((
                Some(UiText::with("task_cancelled", [("taskId", tid)])),
                None,
            ))
        } else {
            Err(ProtocolError::new(
                "not_running",
                "no AI task is currently running",
            ))
        }
    }

    fn editor_ai_input(
        &self,
        target: ApplyTarget,
    ) -> Result<(Arc<RgbaImage>, i32, i32), ProtocolError> {
        let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
        if target == ApplyTarget::ActiveLayer {
            let active = doc.stack().active_layer();
            if active.locked {
                return Err(ProtocolError::line(
                    "layer_locked",
                    UiText::new("layer_locked"),
                ));
            }
            if !active.visible {
                return Err(ProtocolError::line(
                    "layer_hidden",
                    UiText::new("layer_hidden"),
                ));
            }
            Ok((Arc::clone(&active.pixels), active.offset_x, active.offset_y))
        } else {
            Ok((Arc::clone(&doc.current), 0, 0))
        }
    }

    fn find_enabled_plugin_for_action(&self, action: &str) -> Result<LoadedPlugin, ProtocolError> {
        self.plugin_registry
            .find_by_action(action, &self.config.disabled_plugins)
            .cloned()
            .ok_or_else(|| {
                ProtocolError::with_line(
                    "plugin_not_found",
                    format!("no enabled plugin provides action '{action}'"),
                    UiText::with("plugin_not_found", [("id", action)]),
                )
            })
    }

    fn queue_editor_image_plugin(
        &mut self,
        action: &str,
        stage_label: &str,
        model_id: &str,
        model_path: PathBuf,
        params: Value,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.ensure_ai_idle()?;
        let (pixels, _, _) = self.editor_ai_input(target)?;
        let _ = self.refresh_plugins_registry();
        let plugin = self.find_enabled_plugin_for_action(action)?;

        let ipc_dir = self.paths.ai_ipc_dir();
        fs::create_dir_all(&ipc_dir)
            .map_err(|e| ProtocolError::new("io_error", format!("cannot create ai-ipc: {e}")))?;

        self.ai_generation += 1;
        let seq_num = self.ai_generation;
        let tmp_in = ipc_dir.join(format!("ai-in-{seq_num}.png"));
        let tmp_out = ipc_dir.join(format!("ai-out-{seq_num}.png"));
        let _ = fs::remove_file(&tmp_out);
        pixels.save(&tmp_in).map_err(|e| {
            ProtocolError::new("io_error", format!("cannot write plugin input PNG: {e}"))
        })?;

        let task_id = self.next_ai_task_id();
        let cancel = Arc::new(AtomicBool::new(false));
        self.ai_cancel = Some(Arc::clone(&cancel));
        self.ai_active_task_id = Some(task_id.clone());
        self.ai_temp_input = Some(tmp_in.clone());
        self.ai_temp_output = Some(tmp_out.clone());
        self.ai_quick_action_target = None;
        self.ai_apply_target = target;
        self.ai_plugin_label = Some(plugin.manifest.name.clone());
        self.ai_task = Some(AiTaskState {
            kind: "inference".to_owned(),
            target: action.to_owned(),
            running: true,
            percent: 5,
            stage: stage_label.to_owned(),
            ..Default::default()
        });
        self.tasks.push(TaskSnapshot {
            task_id: task_id.clone(),
            kind: "inference".to_owned(),
            status: TaskRunStatus::Running,
            percent: 5,
            stage: stage_label.to_owned(),
        });

        let models_dir = self.effective_models_dir();
        self.pending_ai = Some(PendingAiJob::PluginInference {
            task_id: task_id.clone(),
            plugin,
            action: action.to_owned(),
            model_id: model_id.to_owned(),
            model_path,
            device: self.config.ai.device,
            models_dir,
            input_path: tmp_in,
            output_path: tmp_out,
            params,
            cancel,
        });
        Ok((None, Some(json!({ "jobId": task_id }))))
    }

    pub(crate) fn editor_ai_remove_background(
        &mut self,
        model_id: &str,
        feather: u8,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let _ = self.refresh_plugins_registry();
        let models_dir = self.effective_models_dir();
        let statuses = self
            .plugin_registry
            .model_statuses(&models_dir, &self.config.ai);
        let Some(model_status) = statuses.iter().find(|m| m.id == model_id) else {
            return Err(ProtocolError::new(
                "not_found",
                format!("unknown AI model: {model_id}"),
            ));
        };
        let Some(model_path_str) = &model_status.effective_path else {
            return Err(ProtocolError::new(
                "model_missing",
                format!("AI model '{model_id}' is not downloaded yet"),
            ));
        };
        self.queue_editor_image_plugin(
            REMBG_ACTION,
            "Starting Remove Background…",
            model_id,
            PathBuf::from(model_path_str),
            json!({ "feather": feather.min(20) }),
            target,
        )
    }

    pub(crate) fn editor_dewatermark_gemini(
        &mut self,
        size: &str,
        logo_value: f32,
        force: bool,
        offset_x: i32,
        offset_y: i32,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.queue_editor_image_plugin(
            "editor.dewatermark_gemini",
            "Removing Gemini logo…",
            "",
            PathBuf::new(),
            json!({
                "size": size,
                "logoValue": logo_value,
                "force": force,
                "offsetX": offset_x,
                "offsetY": offset_y,
            }),
            target,
        )
    }

    pub(crate) fn editor_inpaint_region(
        &mut self,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        feather: u32,
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let (_, ox, oy) = self.editor_ai_input(target)?;
        self.queue_editor_image_plugin(
            "editor.inpaint_region",
            "Removing watermark region…",
            "",
            PathBuf::new(),
            json!({
                "x": x - ox,
                "y": y - oy,
                "width": width,
                "height": height,
                "feather": feather,
            }),
            target,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn editor_ai_inpaint(
        &mut self,
        detector_model: &str,
        inpainter_model: &str,
        confidence: f32,
        mask_padding: u32,
        region_x: i32,
        region_y: i32,
        region_width: i32,
        region_height: i32,
        brush_strokes: &[AiMaskStroke],
        target: ApplyTarget,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let (_, ox, oy) = self.editor_ai_input(target)?;
        let local_strokes: Vec<AiMaskStroke> = if ox == 0 && oy == 0 {
            brush_strokes.to_vec()
        } else {
            brush_strokes
                .iter()
                .map(|s| AiMaskStroke {
                    radius: s.radius,
                    points: s
                        .points
                        .iter()
                        .map(|[px, py]| [px - ox as f32, py - oy as f32])
                        .collect(),
                })
                .collect()
        };
        let models_dir = self.effective_models_dir().display().to_string();
        self.queue_editor_image_plugin(
            "editor.ai_inpaint",
            "Detecting & removing watermark (AI)…",
            "",
            PathBuf::new(),
            json!({
                "modelsDir": models_dir,
                "detectorModel": detector_model,
                "inpainterModel": inpainter_model,
                "confidence": confidence,
                "maskPadding": mask_padding,
                "regionX": region_x - ox,
                "regionY": region_y - oy,
                "regionWidth": region_width,
                "regionHeight": region_height,
                "brushStrokes": local_strokes,
            }),
            target,
        )
    }

    pub(crate) fn editor_refine_alpha(
        &mut self,
        erase_strokes: &[AiMaskStroke],
        restore_strokes: &[AiMaskStroke],
        softness: f32,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        if erase_strokes.is_empty() && restore_strokes.is_empty() {
            return Ok((None, None));
        }
        let (working, original, ox, oy) = {
            let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
            let active = doc.stack().active_layer();
            if active.locked {
                return Err(ProtocolError::line(
                    "layer_locked",
                    UiText::new("layer_locked"),
                ));
            }
            if !active.visible {
                return Err(ProtocolError::line(
                    "layer_hidden",
                    UiText::new("layer_hidden"),
                ));
            }
            (
                Arc::clone(&active.pixels),
                doc.original_pixels(),
                active.offset_x as f32,
                active.offset_y as f32,
            )
        };
        let (w, h) = working.dimensions();
        let softness = softness.clamp(0.0, 1.0);
        let mut out = (*working).clone();

        let stamp_strokes = |strokes: &[AiMaskStroke]| -> Vec<f32> {
            let mut map = vec![0.0f32; (w as usize) * (h as usize)];
            for stroke in strokes {
                let r = stroke.radius.clamp(2.0, 240.0);
                let inner = r * (1.0 - softness);
                let mut stamp_disk = |cx: f32, cy: f32| {
                    let x0 = ((cx - r).floor() as i32).clamp(0, w as i32 - 1);
                    let y0 = ((cy - r).floor() as i32).clamp(0, h as i32 - 1);
                    let x1 = ((cx + r).ceil() as i32).clamp(0, w as i32 - 1);
                    let y1 = ((cy + r).ceil() as i32).clamp(0, h as i32 - 1);
                    for y in y0..=y1 {
                        let dy = y as f32 - cy;
                        let row = (y as usize) * (w as usize);
                        for x in x0..=x1 {
                            let dx = x as f32 - cx;
                            let d = dx.hypot(dy);
                            if d > r {
                                continue;
                            }
                            let weight = if d <= inner || r <= inner + 1e-4 {
                                1.0
                            } else {
                                let t = ((r - d) / (r - inner)).clamp(0.0, 1.0);
                                t * t * (3.0 - 2.0 * t)
                            };
                            let idx = row + x as usize;
                            if weight > map[idx] {
                                map[idx] = weight;
                            }
                        }
                    }
                };
                if stroke.points.len() == 1 {
                    stamp_disk(stroke.points[0][0] - ox, stroke.points[0][1] - oy);
                } else {
                    for win in stroke.points.windows(2) {
                        let (x0, y0) = (win[0][0] - ox, win[0][1] - oy);
                        let (x1, y1) = (win[1][0] - ox, win[1][1] - oy);
                        let dist = (x1 - x0).hypot(y1 - y0).max(1.0);
                        let step = (r * 0.30).max(1.5);
                        let steps = (dist / step).ceil() as usize;
                        for i in 0..=steps {
                            let t = i as f32 / steps.max(1) as f32;
                            stamp_disk(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
                        }
                    }
                }
            }
            map
        };

        if !restore_strokes.is_empty() {
            let restore_map = stamp_strokes(restore_strokes);
            let (ow, oh) = original.dimensions();
            for y in 0..h {
                let row = (y as usize) * (w as usize);
                let sy = if oh == h {
                    y
                } else {
                    ((y as u64 * oh as u64) / h.max(1) as u64).min(oh.saturating_sub(1) as u64)
                        as u32
                };
                for x in 0..w {
                    let alpha_w = restore_map[row + x as usize];
                    if alpha_w <= 0.001 {
                        continue;
                    }
                    let sx = if ow == w {
                        x
                    } else {
                        ((x as u64 * ow as u64) / w.max(1) as u64).min(ow.saturating_sub(1) as u64)
                            as u32
                    };
                    let cur = out.get_pixel(x, y);
                    let src = original.get_pixel(sx, sy);
                    let inv = 1.0 - alpha_w;
                    out.put_pixel(
                        x,
                        y,
                        image::Rgba([
                            (cur[0] as f32 * inv + src[0] as f32 * alpha_w).round() as u8,
                            (cur[1] as f32 * inv + src[1] as f32 * alpha_w).round() as u8,
                            (cur[2] as f32 * inv + src[2] as f32 * alpha_w).round() as u8,
                            (cur[3] as f32 * inv + src[3] as f32 * alpha_w).round() as u8,
                        ]),
                    );
                }
            }
        }

        if !erase_strokes.is_empty() {
            let erase_map = stamp_strokes(erase_strokes);
            for y in 0..h {
                let row = (y as usize) * (w as usize);
                for x in 0..w {
                    let ew = erase_map[row + x as usize];
                    if ew <= 0.001 {
                        continue;
                    }
                    let cur = out.get_pixel(x, y);
                    let new_a = ((cur[3] as f32) * (1.0 - ew)).round().clamp(0.0, 255.0) as u8;
                    out.put_pixel(x, y, image::Rgba([cur[0], cur[1], cur[2], new_a]));
                }
            }
        }

        self.commit_ai_editor_pixels(out, "Refine Alpha Mask", ApplyTarget::ActiveLayer)?;
        Ok((
            Some(UiText::with("op_plugin", [("name", "Refine Alpha")])),
            None,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn editor_generate_caption(
        &mut self,
        model_id: &str,
        general_threshold: f32,
        character_threshold: f32,
        replace_underscores: bool,
        prefix: &str,
        suffix: &str,
        exclude_tags: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.ensure_ai_idle()?;
        let src_path = self
            .editor
            .as_ref()
            .ok_or_else(no_doc_err)?
            .source_path
            .clone();
        let _ = self.refresh_plugins_registry();
        let plugin = self.find_enabled_plugin_for_action("editor.caption")?;

        let models_dir = self.effective_models_dir();
        let model_path = self
            .plugin_registry
            .model_statuses(&models_dir, &self.config.ai)
            .into_iter()
            .find(|m| m.id == model_id)
            .and_then(|m| m.effective_path)
            .map(PathBuf::from)
            .unwrap_or_default();

        let task_id = self.next_ai_task_id();
        let cancel = Arc::new(AtomicBool::new(false));
        self.ai_cancel = Some(Arc::clone(&cancel));
        self.ai_active_task_id = Some(task_id.clone());
        self.ai_temp_input = None;
        self.ai_temp_output = None;
        self.ai_quick_action_target = None;
        self.ai_plugin_label = Some(plugin.manifest.name.clone());
        self.ai_task = Some(AiTaskState {
            kind: "caption".to_owned(),
            target: model_id.to_owned(),
            running: true,
            percent: 10,
            stage: "Generating caption…".to_owned(),
            ..Default::default()
        });
        self.tasks.push(TaskSnapshot {
            task_id: task_id.clone(),
            kind: "caption".to_owned(),
            status: TaskRunStatus::Running,
            percent: 10,
            stage: "Generating caption…".to_owned(),
        });

        let params = json!({
            "modelId": model_id,
            "modelsDir": models_dir.display().to_string(),
            "generalThreshold": general_threshold,
            "characterThreshold": character_threshold,
            "replaceUnderscores": replace_underscores,
            "prefix": prefix,
            "suffix": suffix,
            "excludeTags": exclude_tags,
        });
        self.pending_ai = Some(PendingAiJob::PluginInference {
            task_id: task_id.clone(),
            plugin,
            action: "editor.caption".to_owned(),
            model_id: model_id.to_owned(),
            model_path,
            device: self.config.ai.device,
            models_dir,
            input_path: src_path,
            output_path: PathBuf::new(),
            params,
            cancel,
        });
        Ok((None, Some(json!({ "jobId": task_id }))))
    }

    pub(crate) fn editor_save_caption(
        &mut self,
        text: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let doc = self.editor.as_ref().ok_or_else(no_doc_err)?;
        let txt_path = doc.source_path.with_extension("txt");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if txt_path.exists() {
                fs::remove_file(&txt_path)
                    .map_err(|e| ProtocolError::new("io_error", format!("cannot remove: {e}")))?;
            }
            self.editor_caption = None;
            return Ok((Some(UiText::new("ready")), None));
        }

        let tmp = txt_path.with_extension("txt.tmp");
        fs::write(&tmp, trimmed.as_bytes())
            .map_err(|e| ProtocolError::new("io_error", format!("cannot write: {e}")))?;
        if let Err(err) = fs::rename(&tmp, &txt_path) {
            let _ = fs::remove_file(&tmp);
            return Err(ProtocolError::new(
                "io_error",
                format!("cannot save: {err}"),
            ));
        }
        self.editor_caption = Some(trimmed.to_owned());
        Ok((
            Some(UiText::new("settings_saved")),
            Some(json!({ "path": txt_path })),
        ))
    }

    pub(crate) fn quick_action_run(
        &mut self,
        plugin_id: &str,
        action_id: &str,
        paths: Vec<String>,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.ensure_ai_idle()?;
        let Some(first_path) = paths.into_iter().find(|p| !p.trim().is_empty()) else {
            return Err(ProtocolError::new(
                "invalid_input",
                "no image path provided for quick action",
            ));
        };
        let src_path = PathBuf::from(&first_path);
        if !src_path.is_file() {
            return Err(ProtocolError::new(
                "not_found",
                format!("image not found: {}", src_path.display()),
            ));
        }

        let _ = self.refresh_plugins_registry();
        let Some(plugin) = self.plugin_registry.get(plugin_id).cloned() else {
            return Err(ProtocolError::with_line(
                "plugin_not_found",
                format!("plugin '{plugin_id}' not found"),
                UiText::with("plugin_not_found", [("id", plugin_id)]),
            ));
        };
        if self
            .config
            .disabled_plugins
            .iter()
            .any(|id| id == plugin_id)
        {
            return Err(ProtocolError::with_line(
                "plugin_disabled",
                format!("plugin '{plugin_id}' is disabled"),
                UiText::with("plugin_disabled", [("name", plugin.manifest.name.clone())]),
            ));
        }
        let Some(qa) = plugin
            .quick_actions()
            .iter()
            .find(|q| q.id == action_id)
            .cloned()
        else {
            return Err(ProtocolError::new(
                "not_found",
                format!("quick action '{action_id}' not found on plugin '{plugin_id}'"),
            ));
        };

        let models_dir = self.effective_models_dir();
        let model_id = qa
            .params
            .get("modelId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let model_path = if model_id.is_empty() {
            PathBuf::new()
        } else {
            let statuses = self
                .plugin_registry
                .model_statuses(&models_dir, &self.config.ai);
            let Some(ms) = statuses.iter().find(|m| m.id == model_id) else {
                return Err(ProtocolError::new(
                    "not_found",
                    format!("unknown AI model: {model_id}"),
                ));
            };
            let Some(eff) = &ms.effective_path else {
                return Err(ProtocolError::new(
                    "model_missing",
                    format!("AI model '{model_id}' is not downloaded yet"),
                ));
            };
            PathBuf::from(eff)
        };

        let ipc_dir = self.paths.ai_ipc_dir();
        fs::create_dir_all(&ipc_dir)
            .map_err(|e| ProtocolError::new("io_error", format!("cannot create ai-ipc: {e}")))?;

        self.ai_generation += 1;
        let seq_num = self.ai_generation;
        let ext = src_path
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| !e.is_empty())
            .unwrap_or("png");
        let tmp_out = if qa.action.starts_with("utils.") {
            ipc_dir.join(format!("qa-out-{seq_num}.{ext}"))
        } else {
            ipc_dir.join(format!("qa-out-{seq_num}.png"))
        };
        let _ = fs::remove_file(&tmp_out);

        let task_id = self.next_ai_task_id();
        let cancel = Arc::new(AtomicBool::new(false));
        self.ai_cancel = Some(Arc::clone(&cancel));
        self.ai_active_task_id = Some(task_id.clone());
        self.ai_temp_input = None;
        self.ai_temp_output = Some(tmp_out.clone());
        self.ai_quick_action_target = Some((
            plugin.manifest.id.clone(),
            qa.action.clone(),
            src_path.clone(),
        ));
        self.ai_plugin_label = Some(qa.label.clone());
        self.ai_task = Some(AiTaskState {
            kind: "inference".to_owned(),
            target: qa.label.clone(),
            running: true,
            percent: 5,
            stage: format!("{}…", qa.label),
            ..Default::default()
        });
        self.tasks.push(TaskSnapshot {
            task_id: task_id.clone(),
            kind: "inference".to_owned(),
            status: TaskRunStatus::Running,
            percent: 5,
            stage: format!("{}…", qa.label),
        });

        let mut params = qa.params.clone();
        if let Some(obj) = params.as_object_mut() {
            obj.insert(
                "modelsDir".to_owned(),
                json!(models_dir.display().to_string()),
            );
        }

        self.pending_ai = Some(PendingAiJob::PluginInference {
            task_id: task_id.clone(),
            plugin,
            action: qa.action,
            model_id,
            model_path,
            device: self.config.ai.device,
            models_dir,
            input_path: src_path,
            output_path: tmp_out,
            params,
            cancel,
        });
        Ok((None, Some(json!({ "jobId": task_id }))))
    }

    pub(crate) fn viewer_translate(
        &mut self,
        path: &str,
        source_lang: &str,
        target_lang: &str,
        region: Option<[i32; 4]>,
        ocr_engine: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.ensure_ai_idle()?;
        let src_path = PathBuf::from(path);
        if !src_path.is_file() {
            return Err(ProtocolError::new(
                "not_found",
                format!("image not found: {}", src_path.display()),
            ));
        }

        let _ = self.refresh_plugins_registry();
        let ocr_only = target_lang.trim().is_empty();
        let use_ai_ocr = ocr_engine == "manga_ocr" || ocr_engine == "rapidocr";
        let wanted_action = if use_ai_ocr {
            "viewer.ai_ocr"
        } else if ocr_only {
            "viewer.ocr_copy"
        } else {
            "viewer.translate"
        };

        let task_id = self.next_ai_task_id();
        let stage = if use_ai_ocr {
            if ocr_engine == "manga_ocr" {
                "Running Manga-OCR…".to_owned()
            } else {
                "Running RapidOCR…".to_owned()
            }
        } else if ocr_only {
            "Scanning text (OCR)…".to_owned()
        } else {
            "Translating…".to_owned()
        };

        let models_dir = self.effective_models_dir();
        let plugin_opt = self
            .plugin_registry
            .find_by_action(wanted_action, &self.config.disabled_plugins)
            .cloned();

        // Use built-in Tesseract + LibreTranslate/GoogleWeb runner when LibreTranslate is
        // selected or when the Tesseract/Translate plugin is not installed yet.
        let use_builtin = !use_ai_ocr
            && (self.config.translate.provider == TranslateProvider::LibreTranslate
                || plugin_opt.is_none());

        if use_builtin {
            let cancel = Arc::new(AtomicBool::new(false));
            self.ai_cancel = Some(Arc::clone(&cancel));
            self.ai_active_task_id = Some(task_id.clone());
            self.ai_plugin_label = Some("Translate".to_owned());
            self.ai_task = Some(AiTaskState {
                kind: "translate".to_owned(),
                target: path.to_owned(),
                running: true,
                percent: 10,
                stage: stage.clone(),
                ..Default::default()
            });
            self.tasks.push(TaskSnapshot {
                task_id: task_id.clone(),
                kind: "translate".to_owned(),
                status: TaskRunStatus::Running,
                percent: 10,
                stage,
            });
            self.pending_ai = Some(PendingAiJob::BuiltinTranslate {
                task_id: task_id.clone(),
                image_path: src_path,
                region,
                source_lang: source_lang.to_owned(),
                target_lang: target_lang.to_owned(),
                settings: self.config.translate.clone(),
            });
            return Ok((None, Some(json!({ "jobId": task_id }))));
        }

        let plugin = plugin_opt.ok_or_else(|| {
            ProtocolError::with_line(
                "plugin_not_found",
                format!("no enabled plugin provides action '{wanted_action}'"),
                UiText::with("plugin_not_found", [("id", wanted_action)]),
            )
        })?;

        let mut params = if use_ai_ocr {
            json!({
                "engine": ocr_engine,
                "sourceLang": source_lang,
                "targetLang": target_lang,
                "modelsDir": models_dir.display().to_string(),
            })
        } else if ocr_only {
            json!({
                "lang": source_lang,
                "modelsDir": models_dir.display().to_string(),
                "returnTranslation": true,
            })
        } else {
            json!({
                "sourceLang": source_lang,
                "targetLang": target_lang,
                "modelsDir": models_dir.display().to_string(),
            })
        };
        if let Some([x, y, w, h]) = region {
            params["region"] = json!({ "x": x, "y": y, "width": w, "height": h });
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.ai_cancel = Some(Arc::clone(&cancel));
        self.ai_active_task_id = Some(task_id.clone());
        self.ai_temp_input = None;
        self.ai_temp_output = None;
        self.ai_quick_action_target = None;
        self.ai_plugin_label = Some(plugin.manifest.name.clone());
        self.ai_task = Some(AiTaskState {
            kind: "translate".to_owned(),
            target: path.to_owned(),
            running: true,
            percent: 10,
            stage: stage.clone(),
            ..Default::default()
        });
        self.tasks.push(TaskSnapshot {
            task_id: task_id.clone(),
            kind: "translate".to_owned(),
            status: TaskRunStatus::Running,
            percent: 10,
            stage,
        });
        self.pending_ai = Some(PendingAiJob::PluginInference {
            task_id: task_id.clone(),
            plugin,
            action: wanted_action.to_owned(),
            model_id: String::new(),
            model_path: PathBuf::new(),
            device: self.config.ai.device,
            models_dir,
            input_path: src_path,
            output_path: PathBuf::new(),
            params,
            cancel,
        });
        Ok((None, Some(json!({ "jobId": task_id }))))
    }

    pub(crate) fn viewer_save_translation(
        &mut self,
        path: &str,
        source_lang: &str,
        target_lang: &str,
        blocks: Vec<TranslationBlock>,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let img_path = Path::new(path);
        let saved = translate::save_translation_blocks(img_path, source_lang, target_lang, blocks)
            .map_err(|e| ProtocolError::new("io_error", e))?;
        let active_viewer = self
            .viewer_index
            .and_then(|idx| self.viewer_images.get(idx))
            .map(String::as_str);
        if active_viewer == Some(path) {
            self.viewer_saved_translation = saved;
        }
        Ok((Some(UiText::new("translate_done")), None))
    }

    pub(crate) fn viewer_delete_translation(
        &mut self,
        path: &str,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        let deleted = translate::delete_sidecar(Path::new(path))
            .map_err(|e| ProtocolError::new("io_error", e))?;
        let active_viewer = self
            .viewer_index
            .and_then(|idx| self.viewer_images.get(idx))
            .map(String::as_str);
        if active_viewer == Some(path) {
            self.viewer_saved_translation = None;
        }
        Ok((
            Some(UiText::new(if deleted { "ready" } else { "ready" })),
            None,
        ))
    }

    pub(crate) fn set_translate_settings(
        &mut self,
        settings: photon_protocol::TranslateSettings,
    ) -> Result<(Option<UiText>, Option<Value>), ProtocolError> {
        self.config.translate = settings;
        self.persist_config();
        Ok((Some(UiText::new("settings_saved")), None))
    }
}
