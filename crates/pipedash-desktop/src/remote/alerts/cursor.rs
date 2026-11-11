use std::path::Path;

use crate::local_notifications::settings::advance_alert_host;
use crate::remote::connection::RemoteConnection;
use crate::remote::session::Session;

pub(super) async fn drain(remote: &RemoteConnection, dir: &Path, session: &Session) {
    while let Some(cursor) = session.take_cursor() {
        let _op = remote.op.lock().await;
        if session.cursor_epoch() != cursor.epoch || remote.alerts_position(session).is_none() {
            continue;
        }
        remote.move_alerts_seen(dir, cursor.seen, false).await;
        advance_alert_host(cursor.seen).await;
    }
}
