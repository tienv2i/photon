//! Domain-agnostic Tokio actor runtime hosting an `AppCore` state machine
//! with `watch` state snapshots and `broadcast` event bus.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use fw_protocol::{CommandResponse, Envelope, ProtocolCommand, ProtocolError};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

/// Contract implemented by each application's authoritative state machine.
pub trait AppCore: Send + Sized + 'static {
    type State: Clone + Send + Sync + 'static;
    type Command: ProtocolCommand + Send + Sync + 'static;
    type Query: Send + 'static;
    type Event: Clone + Send + Sync + 'static;
    type Custom: Send + 'static;

    fn snapshot(&self) -> Self::State;
    fn revision(&self) -> u64;
    fn dispatch(&mut self, envelope: Envelope<Self::Command>) -> CommandResponse;
    fn query(&self, query: Self::Query) -> Result<serde_json::Value, ProtocolError>;

    /// Called after every `dispatch` so the app core can publish updated snapshots,
    /// emit domain events, or spawn background jobs using `ctx`.
    fn on_dispatch_done(
        &mut self,
        _envelope: &Envelope<Self::Command>,
        _response: &CommandResponse,
        _ctx: &RuntimeContext<Self>,
    ) {
    }

    /// Handle application-specific side-channel messages (e.g. background job progress
    /// or high-frequency data reads).
    fn handle_custom(&mut self, _msg: Self::Custom, _ctx: &RuntimeContext<Self>) {}

    /// Called once when the actor loop exits cleanly after all handles drop.
    fn on_shutdown(&mut self) {}
}

enum GatewayMsg<A: AppCore> {
    Dispatch {
        envelope: Envelope<A::Command>,
        reply: oneshot::Sender<CommandResponse>,
    },
    Query {
        query: A::Query,
        reply: oneshot::Sender<Result<serde_json::Value, ProtocolError>>,
    },
    EmitEvent {
        event: A::Event,
    },
    Custom {
        msg: A::Custom,
    },
}

/// Sender for application-specific side-channel messages back into the actor loop.
pub struct CustomSender<M> {
    tx: mpsc::Sender<M>,
}

impl<M> Clone for CustomSender<M> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl<M: Send + 'static> CustomSender<M> {
    pub async fn send(&self, msg: M) -> Result<(), String> {
        self.tx
            .send(msg)
            .await
            .map_err(|_| "core runtime stopped".to_owned())
    }

    pub fn blocking_send(&self, msg: M) -> Result<(), String> {
        self.tx
            .blocking_send(msg)
            .map_err(|_| "core runtime stopped".to_owned())
    }
}

/// Execution context passed to `AppCore` hooks inside the actor task.
pub struct RuntimeContext<A: AppCore> {
    state_tx: watch::Sender<Arc<A::State>>,
    event_tx: broadcast::Sender<A::Event>,
    seq: Arc<AtomicU64>,
    custom_tx: mpsc::Sender<A::Custom>,
}

impl<A: AppCore> RuntimeContext<A> {
    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn emit_event(&self, event: A::Event) {
        let _ = self.event_tx.send(event);
    }

    pub fn publish_snapshot(&self, state: A::State) {
        let _ = self.state_tx.send(Arc::new(state));
    }

    pub fn custom_sender(&self) -> CustomSender<A::Custom> {
        CustomSender {
            tx: self.custom_tx.clone(),
        }
    }
}

/// Cloneable async handle used by GUI, Local IPC server, CLI, and MCP adapters.
pub struct CoreHandle<A: AppCore> {
    tx: mpsc::Sender<GatewayMsg<A>>,
    state_rx: watch::Receiver<Arc<A::State>>,
    event_tx: broadcast::Sender<A::Event>,
}

impl<A: AppCore> Clone for CoreHandle<A> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            state_rx: self.state_rx.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}

impl<A: AppCore> CoreHandle<A> {
    pub async fn dispatch(
        &self,
        envelope: Envelope<A::Command>,
    ) -> Result<CommandResponse, String> {
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
        query: A::Query,
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

    pub async fn send_custom(&self, msg: A::Custom) -> Result<(), String> {
        self.tx
            .send(GatewayMsg::Custom { msg })
            .await
            .map_err(|_| "core runtime stopped".to_owned())
    }

    pub async fn emit_event(&self, event: A::Event) {
        let _ = self.tx.send(GatewayMsg::EmitEvent { event }).await;
    }

    pub fn latest_snapshot(&self) -> Arc<A::State> {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe_state(&self) -> watch::Receiver<Arc<A::State>> {
        self.state_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<A::Event> {
        self.event_tx.subscribe()
    }
}

pub struct CoreRuntime;

impl CoreRuntime {
    pub fn spawn<A: AppCore>(mut core: A) -> CoreHandle<A> {
        let initial = Arc::new(core.snapshot());
        let (tx, mut rx) = mpsc::channel::<GatewayMsg<A>>(128);
        let (state_tx, state_rx) = watch::channel(initial);
        let (event_tx, _) = broadcast::channel::<A::Event>(256);
        let seq = Arc::new(AtomicU64::new(1));

        let (custom_tx, mut custom_rx) = mpsc::channel::<A::Custom>(128);
        let ctx = RuntimeContext {
            state_tx,
            event_tx: event_tx.clone(),
            seq,
            custom_tx,
        };

        let handle = CoreHandle {
            tx,
            state_rx,
            event_tx,
        };

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    maybe_msg = rx.recv() => {
                        let Some(msg) = maybe_msg else {
                            break;
                        };
                        match msg {
                            GatewayMsg::Dispatch { envelope, reply } => {
                                let resp = core.dispatch(envelope.clone());
                                core.on_dispatch_done(&envelope, &resp, &ctx);
                                let _ = reply.send(resp);
                            }
                            GatewayMsg::Query { query, reply } => {
                                let _ = reply.send(core.query(query));
                            }
                            GatewayMsg::EmitEvent { event } => {
                                ctx.emit_event(event);
                            }
                            GatewayMsg::Custom { msg } => {
                                core.handle_custom(msg, &ctx);
                            }
                        }
                    }
                    maybe_custom = custom_rx.recv() => {
                        let Some(msg) = maybe_custom else {
                            continue;
                        };
                        core.handle_custom(msg, &ctx);
                    }
                }
            }
            core.on_shutdown();
        });

        handle
    }
}
