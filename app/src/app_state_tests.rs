use super::*;

#[test]
fn test_has_horizontal_split() {
    let single_leaf = PaneNodeSnapshot::Leaf(LeafSnapshot {
        is_focused: false,
        custom_vertical_tabs_title: None,
        contents: LeafContents::Settings(SettingsPaneSnapshot::Local {
            current_page: SettingsSection::Appearance,
            search_query: None,
        }),
    });
    assert!(!single_leaf.has_horizontal_split());

    let horizontal_split = PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Horizontal,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Settings(SettingsPaneSnapshot::Local {
                        current_page: SettingsSection::Appearance,
                        search_query: None,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Settings(SettingsPaneSnapshot::Local {
                        current_page: SettingsSection::Appearance,
                        search_query: None,
                    }),
                }),
            ),
        ],
    });
    assert!(horizontal_split.has_horizontal_split());
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_session_restore_allows_local_settings_but_rejects_agent_and_ide_panes() {
    let settings = LeafContents::Settings(SettingsPaneSnapshot::Local {
        current_page: SettingsSection::Appearance,
        search_query: None,
    });
    let ambient_agent = LeafContents::AmbientAgent(AmbientAgentPaneSnapshot {
        uuid: Vec::new(),
        task_id: None,
    });

    assert!(settings.is_available_in_product());
    assert!(!ambient_agent.is_available_in_product());
    assert!(!LeafContents::CustomRouterEditor.is_available_in_product());
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_layout_pruning_preserves_terminal_state_and_reassigns_focus() {
    let terminal_snapshot = TerminalPaneSnapshot {
        uuid: vec![1, 2, 3],
        cwd: Some("/tmp/project".to_string()),
        shell_launch_data: None,
        is_active: true,
        is_read_only: false,
        input_config: None,
        llm_model_override: None,
        active_profile_id: None,
        conversation_ids_to_restore: Vec::new(),
        active_conversation_id: None,
    };
    let layout = PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Horizontal,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: Some("Agent".to_string()),
                    contents: LeafContents::AmbientAgent(AmbientAgentPaneSnapshot {
                        uuid: vec![9],
                        task_id: None,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: Some("Shell".to_string()),
                    contents: LeafContents::Terminal(terminal_snapshot.clone()),
                }),
            ),
        ],
    });

    let pruned = layout
        .prune_unavailable()
        .expect("the terminal sibling should survive");
    let PaneNodeSnapshot::Leaf(leaf) = pruned else {
        panic!("a single surviving pane should collapse its parent branch");
    };
    assert!(leaf.is_focused);
    assert_eq!(leaf.custom_vertical_tabs_title.as_deref(), Some("Shell"));
    assert_eq!(leaf.contents, LeafContents::Terminal(terminal_snapshot));
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_layout_pruning_rejects_layouts_without_supported_panes() {
    let layout = PaneNodeSnapshot::Branch(BranchSnapshot {
        direction: SplitDirection::Vertical,
        children: vec![
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::CustomRouterEditor,
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::ExecutionProfileEditor,
                }),
            ),
        ],
    });

    assert_eq!(layout.prune_unavailable(), None);
}
