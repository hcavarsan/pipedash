use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::mpsc;

use super::{
    failure_message,
    AttemptGuard,
    Finished,
    IngestWork,
    Stop,
    INGEST_QUEUE,
    PROGRESS_TICK,
    PROVIDER_MISBEHAVED,
};
use crate::domain::{
    LogAvailability,
    LogReadMode,
    LogSink,
};
use crate::event::CoreEvent;
use crate::infrastructure::run_logs::{
    Base,
    BlockLimits,
    Ingester,
    PublishOutcome,
};

pub(super) async fn ingest(guard: AttemptGuard, work: IngestWork) {
    let service = Arc::clone(&work.service);
    let generation = work.lease.generation;
    let epoch = work.epoch;
    let partial = work.node.log == Some(LogAvailability::Partial);
    let read = work.run();
    tokio::pin!(read);
    let finished = tokio::select! {
        finished = &mut read => finished,
        _ = service.interest_expired(&work.key), if partial => {
            work.cancel.cancel();
            read.await
        }
    };
    service
        .conclude(guard, finished, generation, epoch, partial)
        .await;
}

impl IngestWork {
    async fn run(&self) -> Finished {
        let service = &self.service;
        let provider_id = self.key.provider_id;
        let interrupted = || service.interrupted(provider_id, self.epoch, &self.cancel);

        let _permit = tokio::select! {
            permit = Arc::clone(&service.ingest_permits).acquire_owned() => permit,
            _ = self.cancel.cancelled() => {
                return Finished::Stopped(interrupted().unwrap_or(Stop::Silent));
            }
        };
        if let Some(stop) = interrupted() {
            return Finished::Stopped(stop);
        }

        if let Err(e) = service.ensure_active(provider_id) {
            return Finished::Failed(failure_message(&e));
        }

        let base = match service.store.base(&self.key).await {
            Ok(base) => base,
            Err(e) => return Finished::Failed(e.to_string()),
        };
        let tmp = match service.store.tmp_dir().await {
            Ok(tmp) => tmp,
            Err(e) => return Finished::Failed(e.to_string()),
        };
        let finished = self.transfer(&tmp, base).await;
        if !matches!(finished, Finished::Published { .. }) {
            service.store.discard(tmp).await;
        }

        finished
    }

    async fn report_progress(&self, reported: &mut u64) {
        let total = self.received.load(Ordering::Relaxed);
        if total == *reported {
            return;
        }
        *reported = total;
        self.service
            .event_bus
            .emit(CoreEvent::RunLogProgress {
                pipeline_id: self.key.pipeline_id.clone(),
                run_number: self.key.run_number,
                node_id: self.key.node_id.clone(),
                received_bytes: total,
            })
            .await;
    }

    async fn transfer(&self, tmp: &Path, base: Option<Base>) -> Finished {
        let service = &self.service;
        let key = &self.key;
        let provider_id = key.provider_id;
        let interrupted = || service.interrupted(provider_id, self.epoch, &self.cancel);
        let expected = base.as_ref().map(|base| base.revision);
        let from = base
            .as_ref()
            .and_then(|base| base.cursor(&self.steps))
            .cloned();

        let (tx, mut rx) = mpsc::channel::<Bytes>(INGEST_QUEUE);
        let mut sink = LogSink::new(tx);
        let ingester = Ingester {
            key: key.clone(),
            revision: self.revision,
            complete: self.node.log == Some(LogAvailability::Ready),
            dir: tmp.to_path_buf(),
            dialect: self.lease.provider.log_dialect(),
            steps: self.steps.clone(),
            received: Arc::clone(&self.received),
            limits: BlockLimits::default(),
            base,
            restart: sink.restart_signal(),
        };
        let writer = tokio::task::spawn_blocking(move || ingester.run(&mut rx));

        let read_result = {
            let read = async {
                let result = self
                    .lease
                    .provider
                    .read_log(&self.run, &key.node_id, from.as_ref(), &mut sink)
                    .await;
                drop(sink);
                result
            };
            tokio::pin!(read);

            let mut ticker = tokio::time::interval(PROGRESS_TICK);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut reported = 0;
            loop {
                tokio::select! {
                    result = &mut read => break Some(result),
                    _ = self.cancel.cancelled() => break None,
                    _ = ticker.tick() => {
                        self.report_progress(&mut reported).await;
                        if service.store.epoch(provider_id) != self.epoch {
                            self.cancel.cancel();
                        }
                    }
                }
            }
        };

        let written = writer.await;
        if let Some(stop) = interrupted() {
            return Finished::Stopped(stop);
        }
        let ingested = match written {
            Ok(Ok(ingested)) => ingested,
            Ok(Err(e)) => return Finished::Failed(format!("Couldn't save the log: {e}")),
            Err(e) => return Finished::Failed(format!("Couldn't save the log: {e}")),
        };
        let read = match read_result {
            Some(Ok(read)) => Some(read),
            Some(Err(e)) if !ingested.truncated() => {
                service.note_limit(&self.lease, &e).await;
                return Finished::Failed(failure_message(&e));
            }
            _ => None,
        };
        if let Some(read) = &read {
            if ingested.resumed() != (read.mode == LogReadMode::Append) {
                return Finished::Failed(PROVIDER_MISBEHAVED.to_string());
            }
            if ingested.unchanged(read) {
                return Finished::Unchanged {
                    resumable: read.cursor.is_some(),
                };
            }
        }

        let cursor = read.and_then(|read| read.cursor);
        let outcome = match tokio::task::spawn_blocking(move || ingested.finish(cursor)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => return Finished::Failed(format!("Couldn't save the log: {e}")),
            Err(e) => return Finished::Failed(format!("Couldn't save the log: {e}")),
        };

        let busy = service.busy_runs();
        let committed = service
            .provider_service
            .commit(&self.lease, || {
                service
                    .store
                    .publish(key, tmp, self.revision, self.epoch, expected, busy)
            })
            .await;
        match committed {
            Some(Ok(PublishOutcome::Published)) => Finished::Published {
                revision: self.revision,
                resumable: outcome.resumable,
            },
            Some(Ok(PublishOutcome::StaleEpoch)) => {
                Finished::Stopped(interrupted().unwrap_or(Stop::ProviderChanged))
            }
            Some(Ok(PublishOutcome::StaleBase)) => Finished::Unchanged { resumable: false },
            Some(Err(e)) => Finished::Failed(e.to_string()),
            None => Finished::Stopped(Stop::ProviderChanged),
        }
    }
}
