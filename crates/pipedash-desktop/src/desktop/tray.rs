use pipedash_core::domain::{
    AlertKind,
    NotificationPreferences,
    Pipeline,
    PipelineStatus,
};
use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchSnapshot,
};
use serde::Serialize;

use super::state::Inner;
use crate::remote::model::RemoteStatus;

const FAILING_RGB: [u8; 3] = [0xE5, 0x48, 0x4D];
const RUNNING_RGB: [u8; 3] = [0x3E, 0x63, 0xDD];
const ATTENTION_RGB: [u8; 3] = [0xF5, 0xA5, 0x24];
const TEMPLATE_RGB: [u8; 3] = [0, 0, 0];
const OFFLINE_ALPHA_PERCENT: u16 = 45;
const DOT_DIAMETER_RATIO: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayStatus {
    Offline,
    Failing,
    Running,
    Attention,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClientLink {
    pub host: String,
    pub status: RemoteStatus,
}

impl ClientLink {
    pub fn connected(&self) -> bool {
        self.status == RemoteStatus::Connected
    }

    fn line(&self) -> String {
        let host = clean(&self.host);
        match self.status {
            RemoteStatus::Connecting => format!("Connecting to {host}"),
            RemoteStatus::Connected => format!("Connected to {host}"),
            RemoteStatus::Reconnecting => format!("Reconnecting to {host}"),
            RemoteStatus::Removed => format!("Removed from {host}"),
        }
    }
}

pub(super) fn clean(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn pinned(inner: &Inner) -> impl Iterator<Item = (&str, &Pipeline)> {
    inner
        .snapshot
        .iter()
        .flat_map(|snapshot| &snapshot.pipelines)
        .filter_map(|entry| {
            entry
                .pipeline
                .as_ref()
                .map(|pipeline| (entry.pipeline_id.as_str(), pipeline))
        })
}

fn alerting(inner: &Inner, prefs: &NotificationPreferences) -> usize {
    pinned(inner)
        .filter(|(id, pipeline)| {
            let kind = match pipeline.status {
                PipelineStatus::Failed => AlertKind::Failed,
                PipelineStatus::Degraded => AlertKind::Degraded,
                _ => return false,
            };
            prefs.allows(kind, &[pipeline.provider_id], Some(id), None)
        })
        .count()
}

fn running(inner: &Inner) -> usize {
    pinned(inner)
        .filter(|(_, pipeline)| {
            matches!(
                pipeline.status,
                PipelineStatus::Running | PipelineStatus::Pending
            )
        })
        .count()
}

fn watching(inner: &Inner) -> Option<&BuildWatchSnapshot> {
    inner
        .watch
        .as_ref()
        .filter(|watch| watch.monitoring == BuildWatchMonitoring::Watching)
}

pub(super) fn tray_status(inner: &Inner, prefs: &NotificationPreferences) -> TrayStatus {
    if !inner.active() {
        TrayStatus::Offline
    } else if alerting(inner, prefs) > 0 || inner.unread_danger() {
        TrayStatus::Failing
    } else if watching(inner).is_some() || running(inner) > 0 {
        TrayStatus::Running
    } else if inner.unread() > 0 {
        TrayStatus::Attention
    } else {
        TrayStatus::Idle
    }
}

fn parts(inner: &Inner, prefs: &NotificationPreferences) -> Vec<String> {
    if !inner.active() {
        return inner.client.iter().map(ClientLink::line).collect();
    }
    let mut parts = Vec::new();
    let failing = alerting(inner, prefs);
    if failing > 0 {
        parts.push(format!("{failing} failing"));
    }
    let running = running(inner);
    if running > 0 {
        parts.push(format!("{running} running"));
    }
    if let Some(watch) = watching(inner) {
        parts.push(format!(
            "watching {} #{}",
            clean(&watch.pipeline_name),
            watch.run_number
        ));
    }
    let unread = inner.unread();
    if unread > 0 {
        parts.push(format!("{unread} unread"));
    }
    parts
}

pub(super) fn summary(inner: &Inner, prefs: &NotificationPreferences) -> String {
    let parts = parts(inner, prefs);
    if parts.is_empty() {
        "All quiet".to_string()
    } else {
        parts.join(" · ")
    }
}

pub(super) fn tooltip(inner: &Inner, prefs: &NotificationPreferences) -> String {
    let mut line = "Pipedash".to_string();
    for part in parts(inner, prefs) {
        line.push_str(" · ");
        line.push_str(&part);
    }
    line
}

pub(super) fn title(inner: &Inner) -> Option<String> {
    let unread = inner.unread();
    (unread > 0).then(|| unread.to_string())
}

pub(super) fn offline_line(inner: &Inner) -> String {
    match &inner.client {
        Some(link) => link.line(),
        None if inner.generation == 0 => "Open Pipedash to finish setup".to_string(),
        None => "Pipedash is locked".to_string(),
    }
}

pub(super) fn paint(
    base: &[u8], width: u32, height: u32, status: TrayStatus, template: bool,
) -> Vec<u8> {
    let mut rgba = base.to_vec();
    match status {
        TrayStatus::Idle => {}
        TrayStatus::Offline => {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel[3] = (u16::from(pixel[3]) * OFFLINE_ALPHA_PERCENT / 100) as u8;
            }
        }
        TrayStatus::Failing | TrayStatus::Running | TrayStatus::Attention => {
            dot(&mut rgba, width, height, status, template);
        }
    }
    rgba
}

fn dot(rgba: &mut [u8], width: u32, height: u32, status: TrayStatus, template: bool) {
    let size = width.min(height) as f32;
    let radius = (size * DOT_DIAMETER_RATIO / 2.0).max(1.0);
    let halo = (size / 16.0).max(1.0);
    let ring = (radius / 3.0).max(1.0);
    let hollow = template && status == TrayStatus::Running;
    let rgb = match (template, status) {
        (true, _) => TEMPLATE_RGB,
        (false, TrayStatus::Failing) => FAILING_RGB,
        (false, TrayStatus::Running) => RUNNING_RGB,
        (false, _) => ATTENTION_RGB,
    };
    let (center_x, center_y) = (width as f32 - radius, height as f32 - radius);
    for y in 0..height {
        for x in 0..width {
            let distance = (x as f32 + 0.5 - center_x).hypot(y as f32 + 0.5 - center_y);
            if distance > radius + halo {
                continue;
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            let Some(pixel) = rgba.get_mut(at..at + 4) else {
                continue;
            };
            let solid = distance <= radius && !(hollow && distance < radius - ring);
            if solid {
                pixel.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            } else {
                pixel.copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
}

#[cfg(test)]
mod tests;
