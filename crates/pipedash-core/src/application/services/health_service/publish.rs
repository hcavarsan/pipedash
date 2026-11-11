use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::time::Instant;

use super::derive::{
    derive,
    same_health,
};
use super::{
    Actor,
    HealthSnapshot,
};
use crate::application::fetch_scheduler::{
    ForcedClaim,
    Generation,
};

impl Actor {
    pub(super) async fn refresh(&mut self, now: Instant) {
        let wall = Utc::now();
        self.detect_gaps(now);

        let mut states = HashMap::with_capacity(self.providers.len());
        for (id, entry) in &mut self.providers {
            states.insert(*id, derive(entry, &mut self.feeds, self.enabled, now, wall));
        }
        let snapshot = Arc::new(HealthSnapshot { providers: states });
        self.snapshot.send_replace(Arc::clone(&snapshot));

        let mut changed: Vec<(i64, Generation)> = snapshot
            .providers
            .iter()
            .filter(|(id, state)| {
                self.published
                    .get(*id)
                    .is_none_or(|published| !same_health(published, state))
            })
            .map(|(id, state)| (*id, state.generation))
            .collect();
        changed.sort_unstable();
        for (id, generation) in changed {
            self.provider_service.publish_summary(id, generation).await;
        }
        self.published = snapshot.providers.clone();

        self.flush().await;
    }

    async fn flush(&mut self) {
        for alert in std::mem::take(&mut self.outbox) {
            let mut current = Vec::with_capacity(alert.provider_ids.len());
            for id in alert.provider_ids.iter().copied() {
                let known = self.providers.get(&id).map(|entry| entry.generation);
                if known.is_some() && self.provider_service.generation_of(id).await == known {
                    current.push(id);
                }
            }
            if current.is_empty() {
                continue;
            }
            self.event_bus.emit(alert.into_event(current)).await;
        }

        for (id, generation) in std::mem::take(&mut self.recoveries) {
            if let ForcedClaim::Claimed(claim) =
                self.provider_service
                    .scheduler()
                    .claim_forced(id, generation, Instant::now())
            {
                self.provider_service.dispatcher().fetch_now(vec![claim]);
            }
        }
    }
}
