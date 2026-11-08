use super::*;

static VAULT: Mutex<()> = Mutex::new(());

fn isolated() -> std::sync::MutexGuard<'static, ()> {
    let guard = VAULT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear().unwrap();
    guard
}

#[test]
fn a_shown_finish_uses_up_its_one_time_watch_and_the_app_resending_keeps_it_used() {
    let _vault = isolated();
    let request = pipedash_core::domain::OnceWatch {
        pipeline_id: "stub__1__api".into(),
        since: chrono::Utc::now(),
        fired_at: None,
    };
    let from_app = NotificationPreferences {
        watch_once: vec![request.clone()],
        ..NotificationPreferences::default()
    };
    write(PREFERENCES, &from_app).unwrap();
    let finish = RunFinish {
        at: request.since + chrono::Duration::minutes(1),
        previous: None,
    };

    fire_once(&[("stub__1__api".into(), finish)]).unwrap();
    let (resent, changed) = merge_preferences(&from_app).unwrap();

    assert!(!changed);
    assert_eq!(resent, preferences().unwrap());
    assert_eq!(
        resent.fired_watches(),
        [pipedash_core::domain::OnceWatch {
            fired_at: Some(finish.at),
            ..request.clone()
        }]
    );
    assert!(!resent.allows(
        pipedash_core::domain::AlertKind::Finished,
        &[1],
        Some("stub__1__api"),
        Some(&RunFinish {
            at: finish.at + chrono::Duration::minutes(30),
            previous: None,
        })
    ));
}

#[test]
fn saving_over_unreadable_preferences_replaces_them() {
    let _vault = isolated();
    vault::set(PREFERENCES, "not json").unwrap();
    let chosen = NotificationPreferences {
        outage: false,
        ..NotificationPreferences::default()
    };

    let (saved, changed) = merge_preferences(&chosen).unwrap();

    assert!(changed);
    assert_eq!(saved, chosen);
    assert_eq!(preferences().unwrap(), chosen);
}

#[test]
fn revocation_removes_authorized_credentials_and_observations() {
    let _vault = isolated();
    assert!(authorization().unwrap().is_none());
    let provider = ProviderConfig {
        id: Some(7),
        name: "CI".to_owned(),
        provider_type: "stub".to_owned(),
        token: "authorized-test-token".to_owned(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    save_authorization(&Authorization {
        providers: vec![provider],
        proxy_url: String::new(),
        status_pages_enabled: true,
    })
    .unwrap();
    save_observations(&NotificationState::default()).unwrap();
    assert_eq!(
        authorization().unwrap().unwrap().providers[0].token,
        "authorized-test-token"
    );
    clear().unwrap();
    assert!(authorization().unwrap().is_none());
    assert!(vault::get(OBSERVATIONS).unwrap().is_none());
}

fn host() -> AlertHost {
    AlertHost {
        server_url: "https://ci.example.com".to_owned(),
        token: "t".to_owned(),
        allow_private_http: false,
        device_id: "d1".to_owned(),
        host_name: "CI".to_owned(),
        standalone: true,
        notified_seq: 5,
        push: false,
    }
}

#[test]
fn the_alert_host_cursor_only_moves_forward_unless_reset() {
    let _vault = isolated();
    advance_alert_host(9).unwrap();
    assert!(alert_host().unwrap().is_none());

    save_alert_host(&host()).unwrap();
    advance_alert_host(8).unwrap();
    advance_alert_host(3).unwrap();
    assert_eq!(alert_host().unwrap().unwrap().notified_seq, 8);
    reset_alert_host_seq(2).unwrap();
    set_alert_host_push(&host(), true).unwrap();
    let saved = alert_host().unwrap().unwrap();
    assert_eq!((saved.notified_seq, saved.push), (2, true));

    let json: serde_json::Value =
        serde_json::from_str(&vault::get(ALERT_HOST).unwrap().unwrap()).unwrap();
    assert_eq!(json["server_url"], "https://ci.example.com");
    assert_eq!(json["allow_private_http"], false);

    clear().unwrap();
    assert!(alert_host().unwrap().is_none());
}

#[test]
fn updates_for_a_replaced_alert_host_are_discarded() {
    let _vault = isolated();
    let fetched = host();
    let replacement = AlertHost {
        device_id: "d2".to_owned(),
        server_url: "https://other.example.com".to_owned(),
        notified_seq: 40,
        push: true,
        ..host()
    };
    save_alert_host(&replacement).unwrap();

    assert!(!is_alert_host(&fetched).unwrap());
    assert!(!advance_alert_host_for(&fetched, 99).unwrap());
    assert!(!reset_alert_host_seq_for(&fetched, 1).unwrap());
    assert!(!set_alert_host_push(&fetched, false).unwrap());
    let saved = alert_host().unwrap().unwrap();
    assert_eq!((saved.notified_seq, saved.push), (40, true));

    assert!(is_alert_host(&replacement).unwrap());
    assert!(advance_alert_host_for(&replacement, 41).unwrap());
    assert_eq!(alert_host().unwrap().unwrap().notified_seq, 41);
}
