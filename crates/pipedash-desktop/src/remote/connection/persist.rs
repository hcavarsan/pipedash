use std::path::Path;

use super::RemoteConnection;
use crate::remote::model::RemoteError;
use crate::remote::store::{
    self,
    SavedConnection,
    SavedMode,
};

impl RemoteConnection {
    pub(in crate::remote) async fn update_saved(
        &self, dir: &Path, change: impl FnOnce(&mut SavedConnection) -> Result<bool, RemoteError>,
    ) -> Result<(), RemoteError> {
        let (generation, mut saved) = {
            let inner = self.lock();
            (inner.generation, inner.saved.clone())
        };
        if !change(&mut saved)? {
            return Ok(());
        }

        let dir = dir.to_path_buf();
        let saved = store::run_blocking(move || store::write(&dir, &saved).map(|()| saved)).await?;
        let mut inner = self.lock();
        if inner.generation != generation {
            return Err(RemoteError::Storage);
        }
        inner.saved = saved;

        Ok(())
    }

    pub(in crate::remote) async fn move_alerts_seen(
        &self, dir: &Path, seen: i64, allow_lower: bool,
    ) {
        let moved = self
            .update_saved(dir, |saved| {
                let current = saved.alerts_seen;
                if !matches!(saved.mode, SavedMode::Client { .. })
                    || current == Some(seen)
                    || (!allow_lower && current.is_some_and(|current| current > seen))
                {
                    return Ok(false);
                }
                saved.alerts_seen = Some(seen);
                Ok(true)
            })
            .await;
        if let Err(error) = moved {
            tracing::debug!(%error, "Couldn't save the alert position");
        }
    }

    pub(crate) fn preferences_pending(&self) -> bool {
        self.lock().saved.preferences_pending
    }

    pub(crate) async fn set_preferences_pending(&self, dir: &Path, pending: bool) {
        let _op = self.op.lock().await;
        let saved = self
            .update_saved(dir, |saved| {
                if !matches!(saved.mode, SavedMode::Client { .. })
                    || saved.preferences_pending == pending
                {
                    return Ok(false);
                }
                saved.preferences_pending = pending;
                Ok(true)
            })
            .await;
        if let Err(error) = saved {
            tracing::debug!(%error, "Couldn't save the preference sync state");
        }
    }
}
