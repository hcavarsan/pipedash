use std::sync::{
    Mutex,
    PoisonError,
};

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(crate) fn start_once<T>(
    cancel: &CancellationToken, input: &Mutex<Option<T>>, task: &Mutex<Option<JoinHandle<()>>>,
    spawn: impl FnOnce(T) -> JoinHandle<()>,
) {
    if cancel.is_cancelled() {
        return;
    }
    let Some(input) = input.lock().unwrap_or_else(PoisonError::into_inner).take() else {
        return;
    };
    *task.lock().unwrap_or_else(PoisonError::into_inner) = Some(spawn(input));
}

pub(crate) async fn stop_task(
    cancel: &CancellationToken, task: &Mutex<Option<JoinHandle<()>>>, name: &str,
) {
    cancel.cancel();
    let task = task.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(task) = task {
        if let Err(e) = task.await {
            tracing::error!(error = %e, "{name} task failed");
        }
    }
}
