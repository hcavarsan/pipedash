use pipedash_core::infrastructure::config::ConfigLoader;

use crate::startup::config::{
    ensure_desktop_config,
    follow_app_container,
};

fn assert_generated_config_loads(with_legacy_db: bool) {
    let root = tempfile::tempdir().unwrap();
    let app_data_dir = if cfg!(windows) {
        root.path().join("com.henrique.pipedash")
    } else {
        root.path()
            .join(r"C:\Users\alice\AppData\Roaming\com.henrique.pipedash")
    };
    std::fs::create_dir_all(&app_data_dir).unwrap();
    if with_legacy_db {
        std::fs::write(app_data_dir.join("pipedash.db"), b"legacy").unwrap();
    }
    let config_path = app_data_dir.join("config.toml");

    assert_eq!(ensure_desktop_config(&app_data_dir, &config_path), Ok(true));

    let config = ConfigLoader::load(&config_path).expect("generated config.toml must load");
    assert_eq!(config.data_dir(), app_data_dir);
    assert!(config.storage.backend.is_sqlite());
    assert_eq!(config.general.default_refresh_interval, 300);
}

#[test]
fn fresh_config_loads_from_a_windows_style_data_dir() {
    assert_generated_config_loads(false);
}

#[test]
fn legacy_config_loads_from_a_windows_style_data_dir() {
    assert_generated_config_loads(true);
}

#[test]
fn the_desktop_config_templates_turn_status_pages_on() {
    let root = std::env::temp_dir().join(format!("pipedash-desktop-config-{}", std::process::id()));
    for legacy in [false, true] {
        let data_dir = root.join(if legacy { "legacy" } else { "fresh" });
        std::fs::create_dir_all(&data_dir).unwrap();
        if legacy {
            std::fs::write(data_dir.join("pipedash.db"), b"data").unwrap();
        }
        let config_path = data_dir.join("config.toml");

        assert!(ensure_desktop_config(&data_dir, &config_path).unwrap());

        let text = std::fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("status_pages_enabled = true"), "{text}");
        let config = ConfigLoader::load(&config_path).unwrap();
        assert!(config.general.status_pages_enabled);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_config_from_an_old_app_container_follows_the_current_one() {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("config.toml");
    std::fs::write(
            &config_path,
            "[general]\nmetrics_enabled = true\n\n[storage]\n\
             data_dir = \"/var/mobile/Containers/Data/Application/OLD-UUID/Library/Application Support/com.henrique.pipedash\"\n\
             backend = \"sqlite\"\n\n[providers.stub]\ntype = \"stub\"\ntoken = \"${STUB_TOKEN}\"\n",
        )
        .unwrap();

    assert_eq!(follow_app_container(&config_path), Ok(true));

    let text = std::fs::read_to_string(&config_path).unwrap();
    assert!(!text.contains("OLD-UUID"), "{text}");
    assert!(
        text.contains("${STUB_TOKEN}"),
        "env references must stay unexpanded: {text}"
    );
    let config = ConfigLoader::parse_raw(&text).unwrap();
    assert!(config.storage.data_dir.is_empty());
    assert!(config.storage.backend.is_sqlite());
    assert_eq!(follow_app_container(&config_path), Ok(false));
}
