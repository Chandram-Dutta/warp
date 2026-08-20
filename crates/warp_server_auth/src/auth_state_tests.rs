use super::*;

#[test]
fn local_only_state_rejects_warp_credentials() {
    let state = AuthState::new_logged_out_for_test();

    state.set_credentials(Some(Credentials::ApiKey {
        key: "wk-test".to_owned(),
        owner_type: None,
    }));

    assert!(state.credentials().is_none());
    assert!(!state.is_logged_in());
}

#[test]
fn local_only_state_rejects_warp_user_identity() {
    let state = AuthState::new_logged_out_for_test();

    state.set_user(Some(User::test()));

    assert!(state.user_id().is_none());
}

#[test]
fn local_only_state_rejects_remote_server_auth_context() {
    let state = AuthState::new_logged_out_for_test();

    state.apply_remote_server_auth_context(
        "remote-token".to_owned(),
        "remote-user".to_owned(),
        "remote@example.com".to_owned(),
    );

    assert!(state.credentials().is_none());
    assert!(state.user_id().is_none());
}
