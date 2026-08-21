use super::*;

#[test]
fn test_has_horizontal_split() {
    let single_leaf = PaneNodeSnapshot::Leaf(LeafSnapshot {
        is_focused: false,
        custom_vertical_tabs_title: None,
        contents: LeafContents::Code(CodePaneSnapShot::Local {
            tabs: vec![CodePaneTabSnapshot {
                path: Some(PathBuf::new()),
            }],
            active_tab_index: 0,
            source: None,
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
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![CodePaneTabSnapshot {
                            path: Some(PathBuf::new()),
                        }],
                        active_tab_index: 0,
                        source: None,
                    }),
                }),
            ),
            (
                PaneFlex(1.),
                PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: false,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![CodePaneTabSnapshot {
                            path: Some(PathBuf::new()),
                        }],
                        active_tab_index: 0,
                        source: None,
                    }),
                }),
            ),
        ],
    });
    assert!(horizontal_split.has_horizontal_split());
}

#[test]
fn test_code_pane_snapshot_single_tab() {
    let snapshot = CodePaneSnapShot::Local {
        tabs: vec![CodePaneTabSnapshot {
            path: Some(PathBuf::from("/tmp/test.rs")),
        }],
        active_tab_index: 0,
        source: Some(CodeSource::FileTree {
            location: crate::code::buffer_location::LocalOrRemotePath::Local(PathBuf::from(
                "/tmp/test.rs",
            )),
        }),
    };
    let CodePaneSnapShot::Local {
        tabs,
        active_tab_index,
        source,
    } = &snapshot;
    assert_eq!(tabs.len(), 1);
    assert_eq!(*active_tab_index, 0);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/test.rs")));
    assert!(matches!(source, Some(CodeSource::FileTree { .. })));
}

#[test]
fn test_code_pane_snapshot_with_multiple_tabs() {
    let snapshot = CodePaneSnapShot::Local {
        tabs: vec![
            CodePaneTabSnapshot {
                path: Some(PathBuf::from("/tmp/main.rs")),
            },
            CodePaneTabSnapshot {
                path: Some(PathBuf::from("/tmp/lib.rs")),
            },
            CodePaneTabSnapshot { path: None },
        ],
        active_tab_index: 1,
        source: Some(CodeSource::Link {
            path: PathBuf::from("/tmp/main.rs"),
            range_start: None,
            range_end: None,
        }),
    };
    let CodePaneSnapShot::Local {
        tabs,
        active_tab_index,
        source,
    } = &snapshot;
    assert_eq!(tabs.len(), 3);
    assert_eq!(*active_tab_index, 1);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/main.rs")));
    assert_eq!(tabs[1].path, Some(PathBuf::from("/tmp/lib.rs")));
    assert_eq!(tabs[2].path, None);
    assert!(matches!(source, Some(CodeSource::Link { .. })));
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
    assert!(!LeafContents::GetStarted.is_available_in_product());
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
                    contents: LeafContents::GetStarted,
                }),
            ),
        ],
    });

    assert_eq!(layout.prune_unavailable(), None);
}
