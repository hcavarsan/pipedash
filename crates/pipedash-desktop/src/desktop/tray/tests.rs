use chrono::Utc;
use pipedash_core::domain::{
    notification_center::{
        CenterNotification,
        CenterTone,
    },
    PipelineMenuEntry,
    PipelineMenuSnapshot,
};
use pipedash_plugin_api::build_watch::BuildWatchRunStatus;

use super::*;

fn entry(id: &str, provider_id: i64, status: PipelineStatus) -> PipelineMenuEntry {
    PipelineMenuEntry {
        pipeline_id: id.to_string(),
        pipeline: Some(Pipeline {
            id: id.to_string(),
            provider_id,
            provider_type: "stub".to_string(),
            name: id.to_string(),
            status,
            last_run: None,
            last_updated: Utc::now(),
            repository: "acme/app".to_string(),
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: Default::default(),
            overdue_run: None,
            typical_duration_seconds: None,
            actions: Default::default(),
        }),
        provider_name: None,
    }
}

fn state(pins: Vec<PipelineMenuEntry>) -> Inner {
    Inner {
        generation: 1,
        vault_ready: true,
        snapshot: Some(PipelineMenuSnapshot {
            pipelines: pins,
            shortcuts: Vec::new(),
        }),
        ..Default::default()
    }
}

fn notice(key: &str, tone: CenterTone, read: bool) -> CenterNotification {
    CenterNotification {
        key: key.to_string(),
        at: 1,
        read,
        tone,
        level: None,
        title: key.to_string(),
        body: String::new(),
        provider_ids: Vec::new(),
        pipeline_id: None,
        link: None,
        run_number: None,
    }
}

fn watch(monitoring: BuildWatchMonitoring) -> BuildWatchSnapshot {
    BuildWatchSnapshot {
        session_id: "s".to_string(),
        pipeline_id: "api".to_string(),
        pipeline_name: "api".to_string(),
        provider_name: "CI".to_string(),
        provider_type: "stub".to_string(),
        run_number: 42,
        branch: None,
        status: BuildWatchRunStatus::Running,
        started_at: 1,
        concluded_at: None,
        updated_at: 1,
        monitoring,
        url: String::new(),
    }
}

fn link(status: RemoteStatus) -> Option<ClientLink> {
    Some(ClientLink {
        host: "ci.example.com".to_string(),
        status,
    })
}

fn prefs() -> NotificationPreferences {
    NotificationPreferences::default()
}

#[test]
fn status_follows_the_first_matching_row() {
    let prefs = prefs();
    let mut inner = state(vec![entry("api", 1, PipelineStatus::Success)]);
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Idle);

    inner
        .history
        .push(notice("done", CenterTone::Success, false));
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Attention);

    inner.watch = Some(watch(BuildWatchMonitoring::Watching));
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Running);

    inner
        .history
        .push(notice("broken", CenterTone::Danger, false));
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Failing);

    inner.history.mark_read(None);
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Running);

    inner.vault_ready = false;
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Offline);
}

#[test]
fn pinned_runs_and_failures_decide_the_status() {
    let prefs = prefs();
    for (status, expected) in [
        (PipelineStatus::Running, TrayStatus::Running),
        (PipelineStatus::Pending, TrayStatus::Running),
        (PipelineStatus::Failed, TrayStatus::Failing),
        (PipelineStatus::Degraded, TrayStatus::Failing),
        (PipelineStatus::Success, TrayStatus::Idle),
        (PipelineStatus::Cancelled, TrayStatus::Idle),
    ] {
        let inner = state(vec![entry("api", 1, status)]);
        assert_eq!(tray_status(&inner, &prefs), expected);
    }

    let inner = state(vec![
        entry("api", 1, PipelineStatus::Running),
        entry("web", 1, PipelineStatus::Failed),
    ]);
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Failing);
}

#[test]
fn muted_pipelines_and_providers_stay_calm_but_watched_ones_turn_red() {
    let inner = state(vec![entry("api", 7, PipelineStatus::Failed)]);

    let mut muted_pipeline = prefs();
    muted_pipeline.muted_pipelines = vec!["api".to_string()];
    assert_eq!(tray_status(&inner, &muted_pipeline), TrayStatus::Idle);

    let mut muted_provider = prefs();
    muted_provider.muted_providers = vec![7];
    assert_eq!(tray_status(&inner, &muted_provider), TrayStatus::Idle);

    let mut kind_off = prefs();
    kind_off.failed = false;
    assert_eq!(tray_status(&inner, &kind_off), TrayStatus::Idle);

    let mut watched = kind_off.clone();
    watched.watched_pipelines = vec!["api".to_string()];
    assert_eq!(tray_status(&inner, &watched), TrayStatus::Failing);

    let mut degraded_off = prefs();
    degraded_off.degraded = false;
    let degraded = state(vec![entry("api", 7, PipelineStatus::Degraded)]);
    assert_eq!(tray_status(&degraded, &degraded_off), TrayStatus::Idle);
}

#[test]
fn muted_failures_do_not_count_in_the_words() {
    let inner = state(vec![
        entry("api", 1, PipelineStatus::Failed),
        entry("web", 1, PipelineStatus::Failed),
        entry("db", 1, PipelineStatus::Failed),
    ]);
    let mut preferences = prefs();
    preferences.muted_pipelines = vec!["db".to_string()];

    assert_eq!(summary(&inner, &preferences), "2 failing");
}

#[test]
fn a_stopped_watch_and_read_notices_do_not_light_the_tray() {
    let prefs = prefs();
    let mut inner = state(Vec::new());
    inner.watch = Some(watch(BuildWatchMonitoring::Completed));
    inner.history.push(notice("old", CenterTone::Danger, true));

    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Idle);
    assert_eq!(inner.unread(), 0);
}

#[test]
fn client_mode_is_offline_until_the_connection_is_up() {
    let prefs = prefs();
    let mut inner = state(vec![entry("api", 1, PipelineStatus::Failed)]);
    inner.vault_ready = false;

    for status in [
        RemoteStatus::Connecting,
        RemoteStatus::Reconnecting,
        RemoteStatus::Removed,
    ] {
        inner.client = link(status);
        assert_eq!(tray_status(&inner, &prefs), TrayStatus::Offline);
    }

    inner.client = link(RemoteStatus::Connected);
    assert_eq!(tray_status(&inner, &prefs), TrayStatus::Failing);
}

#[test]
fn setup_and_locked_states_without_a_core_are_offline() {
    let prefs = prefs();
    assert_eq!(tray_status(&Inner::default(), &prefs), TrayStatus::Offline);
    assert_eq!(
        tray_status(
            &Inner {
                generation: 2,
                ..Default::default()
            },
            &prefs
        ),
        TrayStatus::Offline
    );
}

#[test]
fn words_name_the_host_failures_watch_and_unread_count() {
    let prefs = prefs();
    let mut inner = state(vec![
        entry("api", 1, PipelineStatus::Failed),
        entry("web", 1, PipelineStatus::Degraded),
    ]);
    inner.watch = Some(watch(BuildWatchMonitoring::Watching));
    assert_eq!(
        tooltip(&inner, &prefs),
        "Pipedash · 2 failing · watching api #42"
    );
    assert_eq!(title(&inner), None);

    inner.history.push(notice("a", CenterTone::Warning, false));
    inner.history.push(notice("b", CenterTone::Success, false));
    assert_eq!(
        summary(&inner, &prefs),
        "2 failing · watching api #42 · 2 unread"
    );
    assert_eq!(title(&inner), Some("2".to_string()));

    let quiet = state(Vec::new());
    assert_eq!(tooltip(&quiet, &prefs), "Pipedash");
    assert_eq!(summary(&quiet, &prefs), "All quiet");

    let mut offline = state(Vec::new());
    offline.vault_ready = false;
    offline.client = link(RemoteStatus::Reconnecting);
    assert_eq!(
        tooltip(&offline, &prefs),
        "Pipedash · Reconnecting to ci.example.com"
    );
    assert_eq!(offline_line(&offline), "Reconnecting to ci.example.com");
    offline.client = None;
    assert_eq!(tooltip(&offline, &prefs), "Pipedash");
    assert_eq!(offline_line(&offline), "Pipedash is locked");
}

fn base(size: u32) -> Vec<u8> {
    [10, 20, 30, 200].repeat((size * size) as usize)
}

fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * size + x) * 4) as usize;
    [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
}

#[test]
fn colour_dots_sit_bottom_right_and_leave_the_rest_alone() {
    let size = 32;
    for (status, rgb) in [
        (TrayStatus::Failing, [0xE5, 0x48, 0x4D]),
        (TrayStatus::Running, [0x3E, 0x63, 0xDD]),
        (TrayStatus::Attention, [0xF5, 0xA5, 0x24]),
    ] {
        let painted = paint(&base(size), size, size, status, false);

        assert_eq!(pixel(&painted, size, 29, 29), [rgb[0], rgb[1], rgb[2], 255]);
        assert_eq!(pixel(&painted, size, 2, 2), [10, 20, 30, 200]);
        assert_eq!(pixel(&painted, size, 29, 2), [10, 20, 30, 200]);
        assert_eq!(pixel(&painted, size, 2, 29), [10, 20, 30, 200]);
    }
}

#[test]
fn the_dot_is_about_thirty_percent_of_the_icon_with_a_clear_halo() {
    let size = 32;
    let painted = paint(&base(size), size, size, TrayStatus::Failing, false);
    let solid = (0..size)
        .flat_map(|y| (0..size).map(move |x| (x, y)))
        .filter(|(x, y)| pixel(&painted, size, *x, *y) == [0xE5, 0x48, 0x4D, 255])
        .count();
    let diameter = 0.3 * f64::from(size);
    let expected = std::f64::consts::PI * (diameter / 2.0).powi(2);

    assert!((solid as f64 - expected).abs() < expected * 0.25, "{solid}");
    assert_eq!(pixel(&painted, size, 21, 27)[3], 0);
}

#[test]
fn idle_keeps_the_base_icon_and_offline_fades_it() {
    let size = 8;
    let source = base(size);

    assert_eq!(paint(&source, size, size, TrayStatus::Idle, false), source);

    let faded = paint(&source, size, size, TrayStatus::Offline, false);
    assert_eq!(pixel(&faded, size, 7, 7), [10, 20, 30, 90]);
    assert_eq!(pixel(&faded, size, 0, 0), [10, 20, 30, 90]);
    let template = paint(&source, size, size, TrayStatus::Offline, true);
    assert_eq!(template, faded);
}

#[test]
fn template_icons_stay_monochrome_with_a_filled_dot_or_a_hollow_ring() {
    let size = 32;
    for status in [TrayStatus::Failing, TrayStatus::Attention] {
        let painted = paint(&base(size), size, size, status, true);
        assert_eq!(pixel(&painted, size, 29, 29), [0, 0, 0, 255]);
        assert_eq!(pixel(&painted, size, 2, 2), [10, 20, 30, 200]);
    }

    let ring = paint(&base(size), size, size, TrayStatus::Running, true);
    assert_eq!(pixel(&ring, size, 31, 27), [0, 0, 0, 255]);
    assert_eq!(pixel(&ring, size, 27, 27), [0, 0, 0, 0]);
}

#[test]
fn painting_survives_tiny_and_non_square_icons() {
    let tiny = paint(&[1, 2, 3, 255], 1, 1, TrayStatus::Failing, false);
    assert_eq!(tiny.len(), 4);

    let wide = [9, 9, 9, 255].repeat(16 * 8);
    let painted = paint(&wide, 16, 8, TrayStatus::Attention, false);
    assert_eq!(painted.len(), wide.len());
    assert_eq!(&painted[painted.len() - 4..], &[0xF5, 0xA5, 0x24, 255]);
}
