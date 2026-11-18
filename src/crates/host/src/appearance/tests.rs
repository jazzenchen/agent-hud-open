use super::defer_main;
use tauri::utils::config::WindowConfig;

#[test]
fn initial_settings_keeps_its_visibility_and_geometry_for_immediate_creation() {
    let mut configs = vec![WindowConfig {
        label: "main".into(),
        width: 960.0,
        height: 720.0,
        ..Default::default()
    }];
    let initial = defer_main(&mut configs).expect("the host must build Settings itself");
    assert!(initial.visible);
    assert_eq!((initial.width, initial.height), (960.0, 720.0));
    assert!(!configs[0].create);
}

#[test]
fn disabled_settings_and_other_configured_windows_are_left_alone() {
    let mut configs = vec![
        WindowConfig {
            label: "main".into(),
            create: false,
            ..Default::default()
        },
        WindowConfig {
            label: "product".into(),
            ..Default::default()
        },
    ];
    assert!(defer_main(&mut configs).is_none());
    assert!(configs[1].create);
}

#[test]
fn an_intentionally_hidden_initial_window_stays_hidden() {
    let mut configs = vec![WindowConfig {
        label: "main".into(),
        visible: false,
        ..Default::default()
    }];
    assert!(!defer_main(&mut configs).unwrap().visible);
}
