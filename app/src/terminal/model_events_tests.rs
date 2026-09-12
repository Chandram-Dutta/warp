use warpui::App;

use super::*;
use crate::terminal::model::session::{BootstrapSessionType, IsSSHWrapperSession};

#[test]
fn ssh_init_reaches_normal_bootstrap_without_remote_server_services() {
    App::test((), |mut app| async move {
        let (events_tx, events_rx) = async_channel::unbounded();
        let (init_tx, init_rx) = async_channel::unbounded();
        let sessions = app.update(|ctx| ctx.add_model(|_| Sessions::new_for_test()));
        let dispatcher = app.update(|ctx| {
            ctx.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx))
        });
        app.update(|ctx| {
            ctx.subscribe_to_model(&dispatcher, move |_, event, _| {
                if let ModelEvent::Handler(AnsiHandlerEvent::InitShell {
                    pending_session_info,
                }) = event
                {
                    init_tx.try_send(pending_session_info.clone()).unwrap();
                }
            });
        });

        let mut info = SessionInfo::new_for_test();
        info.session_id = SessionId::from(73);
        info.session_type = BootstrapSessionType::WarpifiedRemote;
        info.hostname = "ordinary-ssh-host".into();
        info.is_ssh_wrapper_session = IsSSHWrapperSession::Yes {
            socket_path: "/tmp/ordinary-ssh-control".into(),
            external_control_master: true,
        };
        events_tx
            .send(Event::Handler(HandlerEvent::InitShell {
                pending_session_info: Box::new(info),
            }))
            .await
            .unwrap();

        let forwarded = init_rx.recv().await.unwrap();
        assert_eq!(forwarded.session_id, SessionId::from(73));
        assert_eq!(forwarded.hostname, "ordinary-ssh-host");
        assert!(matches!(
            forwarded.is_ssh_wrapper_session,
            IsSSHWrapperSession::Yes {
                external_control_master: true,
                ..
            }
        ));
        assert!(sessions.read(&app, |sessions, _| {
            sessions.tracks_session(SessionId::from(73))
        }));
        assert!(!sessions.read(&app, |sessions, _| {
            sessions.tracks_session(SessionId::from(74))
        }));
    });
}
