use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::oneshot;

use super::{
    LogService,
    SearchSession,
    REVISION_GONE,
};
use crate::domain::{
    DomainError,
    DomainResult,
    LogLinesPage,
    LogLinesQuery,
    LogSearchQuery,
    LogSearchResult,
};
use crate::infrastructure::run_logs::{
    search_regex,
    DocError,
    DocKey,
    Document,
    ReadLease,
};

impl LogService {
    pub(super) async fn leased(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64,
    ) -> DomainResult<(DocKey, Arc<Document>, ReadLease)> {
        let key = self.doc_key(pipeline_id, run_number, node_id).await?;
        match self.store.lease(&key, revision).await? {
            Some((doc, lease)) => Ok((key, doc, lease)),
            None => Err(DomainError::NotFound(REVISION_GONE.to_string())),
        }
    }

    fn doc_failure(&self, key: &DocKey, revision: i64, error: DocError) -> DomainError {
        match error {
            DocError::Corrupt => {
                self.store.mark_corrupt(key, revision);
                DomainError::InternalError("Log cache is corrupted".to_string())
            }
            DocError::Io(e) => DomainError::InternalError(format!("Couldn't read the log: {e}")),
            DocError::Cancelled => DomainError::InternalError("Log read cancelled".to_string()),
        }
    }

    async fn sweep(&self, reading: &DocKey) {
        if let Err(e) = self.store.sweep_with(self.busy_runs(), Some(reading)).await {
            tracing::warn!(error = %e, "Failed to remove stale run logs");
        }
    }

    pub async fn read_lines(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, query: LogLinesQuery,
    ) -> DomainResult<LogLinesPage> {
        let (key, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, query.revision)
            .await?;
        let store = Arc::clone(&self.store);
        let page = tokio::task::spawn_blocking(move || {
            let page = doc.read_lines(
                store.block_cache(),
                query.start,
                query.count,
                query.max_line_units,
            );
            drop(lease);
            page
        })
        .await
        .map_err(|e| DomainError::InternalError(format!("Log reader stopped: {e}")))?;
        self.sweep(&key).await;

        page.map_err(|e| self.doc_failure(&key, query.revision, e))
    }

    pub async fn search(
        &self, pipeline_id: &str, run_number: i64, node_id: &str, revision: i64, session_id: &str,
        query: &LogSearchQuery,
    ) -> DomainResult<LogSearchResult> {
        let Some(regex) = search_regex(query)? else {
            return Ok(LogSearchResult::default());
        };

        let (key, doc, lease) = self
            .leased(pipeline_id, run_number, node_id, revision)
            .await?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = self.root.child_token();
        let session_key = (key.clone(), session_id.to_string());
        if let Some((_, previous)) = self
            .searches
            .insert(session_key.clone(), (id, cancel.clone()))
        {
            previous.cancel();
        }
        let _session = SearchSession {
            searches: &self.searches,
            key: session_key,
            id,
            cancel: cancel.clone(),
        };

        let permit = tokio::select! {
            permit = Arc::clone(&self.search_permits).acquire_owned() => permit,
            _ = cancel.cancelled() => return Err(search_cancelled()),
        }
        .map_err(|_| search_cancelled())?;

        let (tx, rx) = oneshot::channel();
        self.admit(|tracker| {
            tracker.spawn_blocking(move || {
                let result = doc.search(&regex, &cancel);
                drop(lease);
                drop(permit);
                let _ = tx.send(result);
            });
        })?;
        let result = rx.await.map_err(|_| search_cancelled())?;
        self.sweep(&key).await;

        result.map_err(|e| match e {
            DocError::Cancelled => search_cancelled(),
            other => self.doc_failure(&key, revision, other),
        })
    }
}

fn search_cancelled() -> DomainError {
    DomainError::InternalError("Search cancelled".to_string())
}
