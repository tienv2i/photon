//! Tokio actor runtime hosting `Core` with watch state snapshots & broadcast event bus.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use photon_image::{EditorDocument, LayerThumbnail, render_layer_thumbnails};
use photon_protocol::{
    Command, CommandResponse, Envelope, Event, EventKind, ProtocolError, Query, UiStateSnapshot,
    UiText, UiTreeSnapshot,
};
use photon_storage::journal::now_ms;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::Core;
use crate::batch::BatchEvent;

pub type PixelFrameData = (u64, u32, u32, Vec<u8>);

enum GatewayMsg {
    Dispatch {
        envelope: Envelope,
        reply: oneshot::Sender<CommandResponse>,
    },
    Query {
        query: Query,
        reply: oneshot::Sender<Result<serde_json::Value, ProtocolError>>,
    },
    UpdateUiTree {
        tree: UiTreeSnapshot,
    },
    ReadEditorPixels {
        reply: oneshot::Sender<Option<PixelFrameData>>,
    },
    ReadEditorOriginalPixels {
        reply: oneshot::Sender<Option<PixelFrameData>>,
    },
    ReadEditorThumbnails {
        reply: oneshot::Sender<(u64, Vec<LayerThumbnail>)>,
    },
    EmitEvent {
        kind: EventKind,
    },
    BatchProgress {
        task_id: String,
        done: usize,
        failed: usize,
        current: String,
    },
    BatchFinished {
        task_id: String,
        done: usize,
        failed: usize,
        cancelled: bool,
        message: UiText,
    },
}

/// Cloneable async handle used by GUI (`iced`), Local IPC server, CLI, and MCP.
#[derive(Clone)]
pub struct CoreHandle {
    tx: mpsc::Sender<GatewayMsg>,
    state_rx: watch::Receiver<Arc<UiStateSnapshot>>,
    event_tx: broadcast::Sender<Event>,
}

impl CoreHandle {
    pub async fn dispatch(&self, envelope: Envelope) -> Result<CommandResponse, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(GatewayMsg::Dispatch {
                envelope,
                reply: reply_tx,
            })
            .await
            .map_err(|_| "core runtime stopped".to_owned())?;
        reply_rx
            .await
            .map_err(|_| "core runtime dropped reply".to_owned())
    }

    pub async fn query(
        &self,
        query: Query,
    ) -> Result<Result<serde_json::Value, ProtocolError>, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(GatewayMsg::Query {
                query,
                reply: reply_tx,
            })
            .await
            .map_err(|_| "core runtime stopped".to_owned())?;
        reply_rx
            .await
            .map_err(|_| "core runtime dropped reply".to_owned())
    }

    pub async fn update_ui_tree(&self, tree: UiTreeSnapshot) {
        let _ = self.tx.send(GatewayMsg::UpdateUiTree { tree }).await;
    }

    pub async fn read_editor_pixels(&self) -> Option<PixelFrameData> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(GatewayMsg::ReadEditorPixels { reply: reply_tx })
            .await
            .ok()?;
        reply_rx.await.ok().flatten()
    }

    pub async fn read_editor_original_pixels(&self) -> Option<PixelFrameData> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(GatewayMsg::ReadEditorOriginalPixels { reply: reply_tx })
            .await
            .ok()?;
        reply_rx.await.ok().flatten()
    }

    pub async fn read_editor_thumbnails(&self) -> Option<(u64, Vec<LayerThumbnail>)> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(GatewayMsg::ReadEditorThumbnails { reply: reply_tx })
            .await
            .ok()?;
        reply_rx.await.ok()
    }

    pub async fn emit_event(&self, kind: EventKind) {
        let _ = self.tx.send(GatewayMsg::EmitEvent { kind }).await;
    }

    pub fn latest_snapshot(&self) -> Arc<UiStateSnapshot> {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe_state(&self) -> watch::Receiver<Arc<UiStateSnapshot>> {
        self.state_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<Event> {
        self.event_tx.subscribe()
    }
}

pub struct CoreRuntime;

impl CoreRuntime {
    pub fn spawn(mut core: Core) -> CoreHandle {
        let initial = Arc::new(core.snapshot());
        let (tx, mut rx) = mpsc::channel::<GatewayMsg>(128);
        let (state_tx, state_rx) = watch::channel(initial);
        let (event_tx, _) = broadcast::channel::<Event>(256);
        let seq = Arc::new(AtomicU64::new(1));

        let handle = CoreHandle {
            tx: tx.clone(),
            state_rx,
            event_tx: event_tx.clone(),
        };

        tokio::spawn(async move {
            let worker_tx = tx;
            while let Some(msg) = rx.recv().await {
                match msg {
                    GatewayMsg::Dispatch { envelope, reply } => {
                        let cmd_name = envelope.command.name().to_owned();
                        let req_id = envelope.id.clone();
                        let cmd = envelope.command.clone();
                        let resp = core.dispatch(envelope);
                        let rev = core.revision();
                        if resp.ok {
                            let snap = Arc::new(core.snapshot());
                            let listing_epoch = snap.meta.listing_epoch;
                            let ws = snap.meta.workspace;
                            let _ = state_tx.send(snap);
                            let _ = event_tx.send(Event {
                                seq_id: seq.fetch_add(1, Ordering::Relaxed),
                                timestamp_ms: now_ms(),
                                revision: rev,
                                kind: EventKind::StateChanged {
                                    workspace: ws,
                                    listing_epoch,
                                },
                            });
                            if let Command::DebugScreenshot { output_path } = &cmd {
                                let resolved = resp
                                    .data
                                    .as_ref()
                                    .and_then(|d| d.get("outputPath"))
                                    .and_then(|v| v.as_str())
                                    .map(str::to_owned)
                                    .or_else(|| output_path.clone());
                                let _ = event_tx.send(Event {
                                    seq_id: seq.fetch_add(1, Ordering::Relaxed),
                                    timestamp_ms: now_ms(),
                                    revision: rev,
                                    kind: EventKind::ScreenshotRequested {
                                        request_id: req_id.clone(),
                                        output_path: resolved,
                                    },
                                });
                            } else if let Command::DebugUiAction { target_id, action } = &cmd {
                                let _ = event_tx.send(Event {
                                    seq_id: seq.fetch_add(1, Ordering::Relaxed),
                                    timestamp_ms: now_ms(),
                                    revision: rev,
                                    kind: EventKind::UiActionRequested {
                                        request_id: req_id.clone(),
                                        target_id: target_id.clone(),
                                        action: action.clone(),
                                    },
                                });
                            }

                            if let Some((plan, cancel)) = core.take_pending_batch() {
                                let job_id = plan.job_id.clone();
                                let _ = event_tx.send(Event {
                                    seq_id: seq.fetch_add(1, Ordering::Relaxed),
                                    timestamp_ms: now_ms(),
                                    revision: rev,
                                    kind: EventKind::TaskStarted {
                                        task_id: job_id.clone(),
                                        kind: "batch".to_owned(),
                                    },
                                });
                                let tx_bg = worker_tx.clone();
                                tokio::task::spawn_blocking(move || {
                                    let tid = job_id;
                                    plan.run(cancel, |ev| match ev {
                                        BatchEvent::Progress {
                                            done,
                                            failed,
                                            current,
                                        } => {
                                            let _ =
                                                tx_bg.blocking_send(GatewayMsg::BatchProgress {
                                                    task_id: tid.clone(),
                                                    done,
                                                    failed,
                                                    current,
                                                });
                                        }
                                        BatchEvent::Finished {
                                            done,
                                            failed,
                                            cancelled,
                                            message,
                                        } => {
                                            let _ =
                                                tx_bg.blocking_send(GatewayMsg::BatchFinished {
                                                    task_id: tid.clone(),
                                                    done,
                                                    failed,
                                                    cancelled,
                                                    message,
                                                });
                                        }
                                    });
                                });
                            }
                        }
                        let _ = event_tx.send(Event {
                            seq_id: seq.fetch_add(1, Ordering::Relaxed),
                            timestamp_ms: now_ms(),
                            revision: rev,
                            kind: EventKind::CommandExecuted {
                                request_id: req_id,
                                command: cmd_name,
                                ok: resp.ok,
                            },
                        });
                        let _ = reply.send(resp);
                    }
                    GatewayMsg::Query { query, reply } => {
                        let _ = reply.send(core.query(&query));
                    }
                    GatewayMsg::UpdateUiTree { tree } => {
                        core.set_ui_tree(tree);
                    }
                    GatewayMsg::ReadEditorPixels { reply } => {
                        let pixels = core.editor_document().map(|doc: &EditorDocument| {
                            let (w, h) = doc.dimensions();
                            (doc.pixel_revision, w, h, doc.current.as_raw().clone())
                        });
                        let _ = reply.send(pixels);
                    }
                    GatewayMsg::ReadEditorOriginalPixels { reply } => {
                        let pixels = core.editor_document().map(|doc: &EditorDocument| {
                            let orig = doc.original_pixels();
                            let (w, h) = orig.dimensions();
                            (doc.original_revision, w, h, orig.as_raw().clone())
                        });
                        let _ = reply.send(pixels);
                    }
                    GatewayMsg::ReadEditorThumbnails { reply } => {
                        let thumbs = core
                            .editor_document()
                            .map(|doc: &EditorDocument| {
                                (doc.pixel_revision, render_layer_thumbnails(doc.stack(), 48))
                            })
                            .unwrap_or_default();
                        let _ = reply.send(thumbs);
                    }
                    GatewayMsg::EmitEvent { kind } => {
                        let _ = event_tx.send(Event {
                            seq_id: seq.fetch_add(1, Ordering::Relaxed),
                            timestamp_ms: now_ms(),
                            revision: core.revision(),
                            kind,
                        });
                    }
                    GatewayMsg::BatchProgress {
                        task_id,
                        done,
                        failed,
                        current,
                    } => {
                        if let Some(task) = core.apply_batch_progress(done, failed, current) {
                            let snap = Arc::new(core.snapshot());
                            let _ = state_tx.send(snap);
                            let _ = event_tx.send(Event {
                                seq_id: seq.fetch_add(1, Ordering::Relaxed),
                                timestamp_ms: now_ms(),
                                revision: core.revision(),
                                kind: EventKind::TaskProgress {
                                    task_id,
                                    percent: task.percent,
                                    stage: task.stage,
                                },
                            });
                        }
                    }
                    GatewayMsg::BatchFinished {
                        task_id,
                        done,
                        failed,
                        cancelled,
                        message,
                    } => {
                        let task = core.apply_batch_finished(done, failed, cancelled, message);
                        let snap = Arc::new(core.snapshot());
                        let listing_epoch = snap.meta.listing_epoch;
                        let ws = snap.meta.workspace;
                        let rev = core.revision();
                        let _ = state_tx.send(snap);
                        let _ = event_tx.send(Event {
                            seq_id: seq.fetch_add(1, Ordering::Relaxed),
                            timestamp_ms: now_ms(),
                            revision: rev,
                            kind: EventKind::TaskFinished {
                                task_id,
                                ok: !cancelled && failed == 0,
                                message: task.map(|t| t.stage).unwrap_or_default(),
                            },
                        });
                        let _ = event_tx.send(Event {
                            seq_id: seq.fetch_add(1, Ordering::Relaxed),
                            timestamp_ms: now_ms(),
                            revision: rev,
                            kind: EventKind::StateChanged {
                                workspace: ws,
                                listing_epoch,
                            },
                        });
                    }
                }
            }
        });

        handle
    }
}
