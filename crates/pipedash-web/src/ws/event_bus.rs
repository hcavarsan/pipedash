use async_trait::async_trait;
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use tokio::sync::broadcast;

pub struct WebSocketEventBus {
    tx: broadcast::Sender<CoreEvent>,
}

impl WebSocketEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn from_sender(tx: broadcast::Sender<CoreEvent>) -> Self {
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.tx.subscribe()
    }
}

impl Default for WebSocketEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for WebSocketEventBus {
    async fn emit(&self, event: CoreEvent) {
        let _ = self.tx.send(event);
    }

    async fn emit_to(&self, _target: &str, event: CoreEvent) {
        let _ = self.tx.send(event);
    }
}
