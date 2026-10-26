use std::collections::HashMap;
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use axum::http::StatusCode;
use pipedash_core::application::LogExport;
use tokio::time::Instant;

use crate::error::{
    ApiError,
    AppError,
};

const MAX_TICKETS: usize = 32;
const TICKET_TTL: Duration = Duration::from_secs(60);

type Entry = (Instant, String, LogExport);

#[derive(Default)]
pub struct DownloadTickets {
    map: Mutex<HashMap<String, Entry>>,
}

impl DownloadTickets {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn issue(
        self: &Arc<Self>, file_name: String, export: LogExport,
    ) -> Result<String, AppError> {
        let ticket = format!("{:032x}", rand::random::<u128>());
        {
            let mut map = self.lock();
            if map.len() >= MAX_TICKETS {
                return Err(AppError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    ApiError::new(
                        "TOO_MANY_DOWNLOADS",
                        "Too many downloads are pending. Try again in a minute.",
                    ),
                ));
            }
            map.insert(ticket.clone(), (Instant::now(), file_name, export));
        }

        let tickets = Arc::clone(self);
        let expiring = ticket.clone();
        tokio::spawn(async move {
            tokio::time::sleep(TICKET_TTL).await;
            let _expired = tickets.lock().remove(&expiring);
        });
        Ok(ticket)
    }

    pub fn take(&self, ticket: &str) -> Option<(String, LogExport)> {
        let (issued, file_name, export) = self.lock().remove(ticket)?;
        (issued.elapsed() < TICKET_TTL).then_some((file_name, export))
    }

    pub fn clear(&self) {
        let _released = std::mem::take(&mut *self.lock());
    }
}
