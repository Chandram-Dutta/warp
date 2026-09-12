use typed_path::TypedPathBuf;
use warp_completer::meta::SpannedItem;
use warp_completer::parsers::ParsedToken;
use warpui::App;

use super::*;
use crate::terminal::model::session::command_executor::testing::TestCommandExecutor;
use crate::terminal::model::session::{Session, SessionInfo};

#[test]
fn finds_history_suggestions_in_current_directory_first() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::with_pwd_and_exit_code("cd Documents", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    let suggestions = find_potential_autosuggestions_from_history(
        history_entries.iter(),
        "cd D",
        Some("/Users/tadej"),
    )
    .into_iter()
    .map(|entry| entry.command)
    .collect_vec();

    assert_eq!(suggestions, ["cd Documents", "cd Dotfiles", "cd Downloads"]);
}

#[test]
fn finds_history_suggestions_without_a_working_directory() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("cd Dotfiles", "/Users/tadej", 0),
        HistoryEntry::command_only("cd Pictures"),
        HistoryEntry::with_pwd_and_exit_code("cd Downloads", "/Users/tadej/dev", 0),
    ];

    let suggestions =
        find_potential_autosuggestions_from_history(history_entries.iter(), "cd D", None)
            .into_iter()
            .map(|entry| entry.command)
            .collect_vec();

    assert_eq!(suggestions, ["cd Downloads", "cd Dotfiles"]);
}

#[test]
fn history_suggestions_include_failed_commands_and_commands_without_a_directory() {
    let history_entries = [
        HistoryEntry::with_pwd_and_exit_code("git diff", "/repo", 1),
        HistoryEntry::command_only("git status"),
    ];

    let suggestions =
        find_potential_autosuggestions_from_history(history_entries.iter(), "git", Some("/repo"))
            .into_iter()
            .map(|entry| entry.command)
            .collect_vec();

    assert_eq!(suggestions, ["git diff", "git status"]);
}

#[test]
fn history_suggestions_require_a_matching_prefix() {
    let history_entries = [HistoryEntry::command_only("git status")];

    let suggestions =
        find_potential_autosuggestions_from_history(history_entries.iter(), "cargo", None);

    assert!(suggestions.is_empty());
}

#[test]
fn feature_flag_argument_is_valid_without_whitespace_before_argument() {
    App::test((), |_| async move {
        let session = Session::new(
            SessionInfo::new_for_test(),
            Arc::new(TestCommandExecutor::default()),
        );
        let ctx = SessionContext::new_for_test(session, TypedPathBuf::from("/test/home/"));
        let full_command = "cargo run --features=with_local_server,fast_dev";
        let with_local_server = ParsedExpression::new(
            Expression::ValidatableArgument(vec![ArgType::Generator("feature_flags".into())]),
            ParsedToken::new("with_local_server".to_owned()),
        )
        .spanned((21, 38));
        let fast_dev = ParsedExpression::new(
            Expression::ValidatableArgument(vec![ArgType::Generator("feature_flags".into())]),
            ParsedToken::new("fast_dev".to_owned()),
        )
        .spanned((39, 47));

        assert!(is_arg_valid(full_command, &with_local_server, &ctx, None).await);
        assert!(is_arg_valid(full_command, &fast_dev, &ctx, None).await);
        assert!(is_command_valid(full_command, Some(&ctx), None).await);
    });
}
