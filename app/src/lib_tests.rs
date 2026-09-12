use super::*;

#[test]
#[cfg(not(feature = "local_only"))]
fn app_api_key_requires_validation() {
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: Some("app-api-key".to_owned()),
    };

    assert!(matches!(
        app.auth_initialization(),
        AuthInitialization::PendingApiKey(api_key) if api_key == "app-api-key"
    ));
}

#[test]
#[cfg(not(feature = "local_only"))]
fn startup_without_api_key_loads_persisted_auth() {
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: None,
    };

    assert!(matches!(
        app.auth_initialization(),
        AuthInitialization::Persisted
    ));
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_startup_ignores_warp_credentials() {
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: Some("app-api-key".to_owned()),
    };

    assert!(matches!(
        app.auth_initialization(),
        AuthInitialization::LoggedOut
    ));
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_startup_skips_profiling() {
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: None,
    };

    assert!(!app.needs_profiling());
}
