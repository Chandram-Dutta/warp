use warpui::App;

use super::*;

#[test]
fn remote_directory_never_uses_local_repository_detection() {
    App::test((), |mut app| async move {
        let result = app
            .update(|ctx| {
                detect_possible_git_repo(
                    RepoDetectionSessionType::Remote,
                    env!("CARGO_MANIFEST_DIR"),
                    RepoDetectionSource::TerminalNavigation,
                    ctx,
                )
            })
            .await;
        assert_eq!(result, None);
    });
}
