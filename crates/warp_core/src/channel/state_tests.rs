use super::derive_http_origin_from_ws_url;

#[test]
fn wss_becomes_https_and_strips_path() {
    let got = derive_http_origin_from_ws_url("wss://rtc.app.warp.dev/graphql/v2");
    assert_eq!(got.as_deref(), Some("https://rtc.app.warp.dev"));
}

#[test]
fn ws_becomes_http_and_preserves_port() {
    let got = derive_http_origin_from_ws_url("ws://localhost:8080/graphql/v2");
    assert_eq!(got.as_deref(), Some("http://localhost:8080"));
}

#[test]
fn unparseable_input_returns_none() {
    assert!(derive_http_origin_from_ws_url("not a url").is_none());
    assert!(derive_http_origin_from_ws_url("https://app.warp.dev").is_none());
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_profile_cannot_be_redirected_to_warp() {
    use super::{ChannelState, WarpServerConfig};

    assert!(!ChannelState::allows_warp_network());
    assert_eq!(
        WarpServerConfig::local_only().server_root_url,
        ChannelState::server_root_url()
    );

    ChannelState::override_server_root_url("https://app.warp.dev").unwrap();
    ChannelState::override_ws_server_url("wss://rtc.app.warp.dev/graphql/v2").unwrap();
    ChannelState::override_session_sharing_server_url("wss://sessions.app.warp.dev").unwrap();

    assert_eq!(
        ChannelState::server_root_url(),
        "warp-network-disabled://local-only"
    );
    assert_eq!(
        ChannelState::ws_server_url(),
        "warp-network-disabled://local-only"
    );
    assert!(ChannelState::session_sharing_server_url().is_none());
    assert!(ChannelState::iap_config().is_none());
    assert!(!ChannelState::is_telemetry_available());
    assert!(!ChannelState::is_crash_reporting_available());
}
