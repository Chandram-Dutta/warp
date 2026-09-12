use std::sync::Arc;

use chrono::{Local, TimeZone};
use warp_core::SessionId;
use warpui::{App, SingletonEntity};

use crate::input_suggestions::HistoryInputSuggestion;
use crate::suggestions::ignored_suggestions_model::{IgnoredSuggestionsModel, SuggestionType};
use crate::terminal::model::session::command_executor::NoOpCommandExecutor;
use crate::terminal::model::session::{Session, SessionInfo};
use crate::terminal::{History, HistoryEntry, LinkedWorkflowData};

fn command_entry(session_id: SessionId, command: &str, second: u32) -> HistoryEntry {
    HistoryEntry {
        session_id: Some(session_id),
        command: command.to_owned(),
        pwd: None,
        start_ts: Some(Local.with_ymd_and_hms(2024, 1, 1, 0, 0, second).unwrap()),
        completed_ts: None,
        exit_code: None,
        git_head: None,
        shell_host: None,
        workflow_id: None,
        workflow_command: None,
        is_for_restored_block: false,
        is_agent_executed: false,
    }
}

async fn add_command_history(app: &mut App, session_id: SessionId, entries: Vec<HistoryEntry>) {
    let mut session_info = SessionInfo::new_for_test();
    session_info.session_id = session_id;
    let session = Arc::new(Session::new(
        session_info,
        Arc::new(NoOpCommandExecutor::default()),
    ));
    let (initialized_tx, initialized_rx) = async_channel::bounded(1);
    let history = app.add_singleton_model(|_| History::default());
    app.update(|ctx| {
        ctx.subscribe_to_model(&history, move |_, event, _| match event {
            crate::terminal::HistoryEvent::Initialized(id) if *id == session_id => {
                let _ = initialized_tx.try_send(());
            }
            crate::terminal::HistoryEvent::Initialized(_) => {}
        });
        history.update(ctx, |history, ctx| {
            history.init_session_with(session, async { Vec::new() }, ctx);
        });
    });
    initialized_rx.recv().await.unwrap();
    history.update(app, |history, _| {
        history.append_commands(session_id, entries)
    });
}

#[test]
fn command_history_keeps_latest_duplicate_and_excludes_empty_commands() {
    App::test((), |mut app| async move {
        let session = SessionId::from(1);
        add_command_history(
            &mut app,
            session,
            vec![
                command_entry(session, " same ", 1),
                command_entry(session, "older command", 2),
                command_entry(session, "same", 3),
                command_entry(session, "   ", 4),
            ],
        )
        .await;
        app.read(|ctx| {
            let history = History::as_ref(ctx).up_arrow_suggestions(Some(session), ctx);
            assert_eq!(
                history
                    .iter()
                    .map(HistoryInputSuggestion::normalized_text)
                    .collect::<Vec<_>>(),
                vec!["older command", "same"]
            );
        });
    });
}

#[test]
fn command_history_preserves_workflow_and_historical_agent_commands() {
    App::test((), |mut app| async move {
        let session = SessionId::from(1);
        let mut command = command_entry(session, "deploy", 1);
        command.workflow_command = Some("deploy {{environment}}".to_owned());
        command.is_agent_executed = true;
        add_command_history(&mut app, session, vec![command]).await;
        app.read(|ctx| {
            let history = History::as_ref(ctx).up_arrow_suggestions(Some(session), ctx);
            assert_eq!(history.len(), 1);
            let entry = history[0].entry;
            assert_eq!(entry.command, "deploy");
            assert_eq!(
                entry.linked_workflow_data(),
                Some(LinkedWorkflowData::Command(
                    "deploy {{environment}}".to_owned()
                ))
            );
        });
    });
}

#[test]
fn command_history_excludes_ignored_commands() {
    App::test((), |mut app| async move {
        let session = SessionId::from(1);
        add_command_history(
            &mut app,
            session,
            vec![
                command_entry(session, "keep command", 1),
                command_entry(session, "ignore command", 2),
            ],
        )
        .await;
        app.add_singleton_model(|_| {
            IgnoredSuggestionsModel::new(vec![(
                "ignore command".to_owned(),
                SuggestionType::ShellCommand,
            )])
        });
        app.read(|ctx| {
            let history = History::as_ref(ctx).up_arrow_suggestions(Some(session), ctx);
            assert_eq!(
                history
                    .iter()
                    .map(HistoryInputSuggestion::normalized_text)
                    .collect::<Vec<_>>(),
                vec!["keep command"]
            );
            assert!(
                History::as_ref(ctx)
                    .up_arrow_suggestions(None, ctx)
                    .is_empty()
            );
        });
    });
}
