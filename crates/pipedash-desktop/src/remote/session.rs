pub(super) mod connect;
pub(super) mod driver;
pub(super) mod protocol;

use std::collections::HashMap;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_tungstenite::tungstenite::Message;

use self::protocol::request_frame;
use super::model::RemoteError;

type RpcResult = Result<(u16, Vec<u8>), RemoteError>;

#[derive(Default)]
struct CursorQueue {
    pending: Option<i64>,
    epoch: u64,
    running: bool,
}

pub(super) struct Cursor {
    pub seen: i64,
    pub epoch: u64,
}

pub struct Session {
    host: String,
    outgoing: mpsc::Sender<Message>,
    pending: Mutex<HashMap<u64, oneshot::Sender<RpcResult>>>,
    next_id: AtomicU64,
    alerts_synced: AtomicBool,
    alerts_gate: tokio::sync::Mutex<()>,
    cursor: Mutex<CursorQueue>,
}

struct PendingRequest<'a> {
    session: &'a Session,
    id: u64,
}

impl Drop for PendingRequest<'_> {
    fn drop(&mut self) {
        self.session.pending().remove(&self.id);
    }
}

impl Session {
    pub(super) fn new(host: &str, outgoing: mpsc::Sender<Message>) -> Self {
        Self {
            host: host.to_string(),
            outgoing,
            pending: Mutex::default(),
            next_id: AtomicU64::new(1),
            alerts_synced: AtomicBool::new(false),
            alerts_gate: tokio::sync::Mutex::new(()),
            cursor: Mutex::default(),
        }
    }

    pub(super) fn alerts_synced(&self) -> bool {
        self.alerts_synced.load(Ordering::Acquire)
    }

    pub(super) fn mark_alerts_synced(&self) {
        self.alerts_synced.store(true, Ordering::Release);
    }

    pub(super) async fn alerts_gate(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.alerts_gate.lock().await
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<u64, oneshot::Sender<RpcResult>>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cursor(&self) -> MutexGuard<'_, CursorQueue> {
        self.cursor.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn queue_cursor(&self, seen: i64) -> bool {
        if !self.alerts_synced() {
            return false;
        }
        let mut queue = self.cursor();
        queue.pending = Some(queue.pending.map_or(seen, |pending| pending.max(seen)));

        !std::mem::replace(&mut queue.running, true)
    }

    pub(super) fn take_cursor(&self) -> Option<Cursor> {
        let mut queue = self.cursor();
        let Some(seen) = queue.pending.take() else {
            queue.running = false;
            return None;
        };

        Some(Cursor {
            seen,
            epoch: queue.epoch,
        })
    }

    pub(super) fn cursor_epoch(&self) -> u64 {
        self.cursor().epoch
    }

    pub(super) fn reset_cursor(&self) {
        let mut queue = self.cursor();
        queue.epoch += 1;
        queue.pending = None;
    }

    pub async fn request(
        &self, method: &str, path: &str, body: Option<&str>, timeout: Duration,
    ) -> RpcResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.pending().insert(id, reply);
        let _entry = PendingRequest { session: self, id };

        let outcome = tokio::time::timeout(timeout, async {
            self.outgoing
                .send(request_frame(id, method, path, body))
                .await
                .map_err(|_| RemoteError::Unreachable(self.host.clone()))?;
            answer
                .await
                .unwrap_or_else(|_| Err(RemoteError::Unreachable(self.host.clone())))
        })
        .await;

        outcome.unwrap_or(Err(RemoteError::Timeout))
    }

    pub(super) fn resolve(&self, id: u64, status: u16, body: Vec<u8>) {
        if let Some(reply) = self.pending().remove(&id) {
            let _ = reply.send(Ok((status, body)));
        }
    }

    fn fail_all(&self) {
        let pending = std::mem::take(&mut *self.pending());
        for reply in pending.into_values() {
            let _ = reply.send(Err(RemoteError::Unreachable(self.host.clone())));
        }
    }
}

#[cfg(test)]
mod tests;
