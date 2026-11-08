use pipedash_core::event::CoreEvent;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinSet;

use crate::state::AppState;
use crate::ws::closed;

const MAX_JOBS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Worker {
    FailureNotifier,
    AlertPush,
}

fn start_job(jobs: &mut JoinSet<()>, state: &AppState, worker: Worker, event: CoreEvent) {
    let state = state.clone();
    match (worker, event) {
        (Worker::FailureNotifier, CoreEvent::PipelinesFailed { pipelines }) => {
            jobs.spawn(async move {
                let Ok(service) = state.push_service().await else {
                    return;
                };
                if let Err(error) = service.notify_failures(&pipelines).await {
                    tracing::warn!(%error, "Couldn't send the failure pushes");
                }
            });
        }
        (Worker::AlertPush, CoreEvent::AlertRecorded { id, seq }) => {
            jobs.spawn(async move {
                let Ok(service) = state.push_service().await else {
                    return;
                };
                if let Err(error) = service.push_recorded_alert(&id, seq).await {
                    tracing::warn!(%error, "Couldn't push a recorded alert");
                }
            });
        }
        _ => {}
    }
}

fn spawn_worker(state: AppState, worker: Worker) {
    let mut events = state.ws_event_bus.subscribe();
    let mut closing = state.closing.subscribe();

    tokio::spawn(async move {
        let mut jobs = JoinSet::new();
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(_) if jobs.len() >= MAX_JOBS => {
                        tracing::warn!("Dropping a push event, too many jobs in flight");
                    }
                    Ok(event) => start_job(&mut jobs, &state, worker, event),
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "The push worker missed events");
                    }
                    Err(RecvError::Closed) => break,
                },
                Some(_) = jobs.join_next(), if !jobs.is_empty() => {}
                () = closed(&mut closing) => break,
            }
        }
        jobs.shutdown().await;
    });
}

pub fn spawn_failure_notifier(state: AppState) {
    spawn_worker(state, Worker::FailureNotifier);
}

pub fn spawn_alert_push(state: AppState) {
    spawn_worker(state, Worker::AlertPush);
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod gone_tests;
