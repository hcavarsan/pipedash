use std::io;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::Arc;

use bytes::Bytes;
use futures::Stream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    LogExport,
    LogService,
    EXPORT_INTERRUPTED,
    EXPORT_QUEUE,
    REVISION_GONE,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::run_logs::{
    Document,
    ReadLease,
};

impl LogService {
    pub async fn prepare_export(
        self: &Arc<Self>, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64,
    ) -> DomainResult<LogExport> {
        let provider_id = self.provider_of(pipeline_id).await?;
        let epoch = self.store.epoch(provider_id);
        let (_, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, revision)
            .await?;

        Ok(LogExport {
            doc,
            lease,
            provider_id,
            epoch,
            service: Arc::clone(self),
        })
    }
}

impl LogExport {
    pub fn into_stream(
        self,
    ) -> DomainResult<impl Stream<Item = io::Result<Bytes>> + Send + 'static> {
        let Self {
            doc,
            lease,
            provider_id,
            epoch,
            service,
        } = self;
        if service.store.epoch(provider_id) != epoch {
            return Err(DomainError::NotFound(REVISION_GONE.to_string()));
        }

        let (tx, rx) = mpsc::channel::<io::Result<Bytes>>(EXPORT_QUEUE);
        let finished = Arc::new(AtomicBool::new(false));
        let root = service.root.clone();
        let done = Arc::clone(&finished);
        service.admit(|tracker| {
            tracker.spawn(export(doc, lease, tx, root, done));
        })?;

        Ok(futures::stream::unfold(Some(rx), move |rx| {
            let finished = Arc::clone(&finished);
            async move {
                let mut rx = rx?;
                match rx.recv().await {
                    Some(item) => Some((item, Some(rx))),
                    None if finished.load(Ordering::SeqCst) => None,
                    None => Some((Err(io::Error::other(EXPORT_INTERRUPTED)), None)),
                }
            }
        }))
    }
}

async fn export(
    doc: Arc<Document>, lease: ReadLease, tx: mpsc::Sender<io::Result<Bytes>>,
    root: CancellationToken, finished: Arc<AtomicBool>,
) {
    let _lease = lease;
    for index in 0..doc.block_count() {
        if root.is_cancelled() {
            return;
        }
        let block = Arc::clone(&doc);
        let item = match tokio::task::spawn_blocking(move || block.text_block(index)).await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(e)) => Err(io::Error::other(e.to_string())),
            Err(e) => Err(io::Error::other(e)),
        };
        let failed = item.is_err();
        tokio::select! {
            sent = tx.send(item) => {
                if sent.is_err() {
                    return;
                }
            }
            _ = root.cancelled() => return,
        }
        if failed {
            finished.store(true, Ordering::SeqCst);
            return;
        }
    }
    finished.store(true, Ordering::SeqCst);
}
