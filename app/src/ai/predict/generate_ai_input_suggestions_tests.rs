use super::*;
use crate::persistence::model::Command;
use crate::terminal::model::terminal_model::BlockIndex;
use crate::terminal::model::test_utils::create_test_block_with_grids;
use crate::test_util::mock_blockgrid;

#[cfg(feature = "local_only")]
#[test]
fn local_command_context_does_not_collect_terminal_output() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_block("git status", "terminal-output-must-not-be-collected");

    let messages = get_local_command_context_messages(Arc::new(FairMutex::new(model)), 5);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].input, "git status");
    assert!(messages[0].output.is_empty());
    assert!(messages[0].context.git_branch.is_none());
    assert!(
        !serde_json::to_string(&messages)
            .unwrap()
            .contains("terminal-output-must-not-be-collected")
    );
}

#[cfg(feature = "local_only")]
#[test]
fn local_context_excludes_output_agent_and_codebase_fields() {
    use crate::ai_assistant::execution_context::{WarpAiExecutionContext, WarpAiOsContext};

    let context = NextCommandContext {
        history_contexts: Vec::new(),
        ai_execution_context: WarpAiExecutionContext {
            os: WarpAiOsContext {
                category: Some("os-context-must-not-be-sent".to_owned()),
                distribution: None,
            },
            shell_name: "zsh".to_owned(),
            shell_version: Some("5.9".to_owned()),
        },
        context_messages: vec![ContextMessageInput {
            input: "git status".to_owned(),
            output: "terminal-output-must-not-be-sent".to_owned(),
            context: CommandContext {
                pwd: Some("/tmp/project".to_owned()),
                git_branch: Some("codebase-context-must-not-be-sent".to_owned()),
                exit_code: 0,
            },
        }],
    };

    let serialized =
        serde_json::to_string(&create_local_next_command_context(&context, Some("git "))).unwrap();
    assert!(serialized.contains("git status"));
    assert!(serialized.contains("/tmp/project"));
    assert!(!serialized.contains("terminal-output-must-not-be-sent"));
    assert!(!serialized.contains("codebase-context-must-not-be-sent"));
    assert!(!serialized.contains("os-context-must-not-be-sent"));
}

#[test]
fn test_merge_history_contexts_to_string_basic() {
    let mut all_commands = vec![];
    for i in 0..15 {
        all_commands.push(Command {
            id: i,
            command: format!("command {i}"),
            pwd: Some("/home/user".to_string()),
            exit_code: Some(0),
            git_branch: Some("main".to_string()),
            start_ts: None,
            ..Default::default()
        });
    }

    // Create some overlapping history contexts
    let mut all_history_contexts = vec![HistoryContext {
        previous_commands: all_commands[0..3].to_vec(),
        next_command: all_commands[3].clone(),
    }];
    all_history_contexts.push(HistoryContext {
        previous_commands: all_commands[1..4].to_vec(),
        next_command: all_commands[4].clone(),
    });
    all_history_contexts.push(HistoryContext {
        previous_commands: all_commands[4..6].to_vec(),
        next_command: all_commands[6].clone(),
    });

    // Gap here (...)

    // These should be merged together
    all_history_contexts.push(HistoryContext {
        previous_commands: all_commands[7..8].to_vec(),
        next_command: all_commands[8].clone(),
    });
    all_history_contexts.push(HistoryContext {
        previous_commands: all_commands[8..9].to_vec(),
        next_command: all_commands[9].clone(),
    });

    // Gap here (...)

    all_history_contexts.push(HistoryContext {
        previous_commands: all_commands[12..14].to_vec(),
        next_command: all_commands[14].clone(),
    });

    let result = merge_history_contexts_to_string(all_history_contexts);

    assert_eq!(
        result,
        r#"{"command":"command 0","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 1","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 2","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 3","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 4","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 5","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 6","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
...
{"command":"command 7","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 8","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 9","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
...
{"command":"command 12","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 13","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}
{"command":"command 14","pwd":"/home/user","exit_code":0,"git_branch":"main","start_ts":null}"#
    );
}

#[test]
fn test_get_block_content_summary_extreme_narrow_vs_wide() {
    // Create blocks with many lines to see the difference in truncation
    let mut massive_input = String::new();
    for i in 0..1000 {
        massive_input.push_str(&format!("Input line {i} with some content\r\n"));
    }
    let prompt_grid = mock_blockgrid(&massive_input);
    let rprompt_grid = mock_blockgrid("");

    let mut massive_output = String::new();
    for i in 0..1000 {
        massive_output.push_str(&format!("Output line {i} with some content\r\n"));
    }
    let output_grid = mock_blockgrid(&massive_output);

    // Note terminal model isn't being used here (i.e. we're not creating a test model
    // and using the # of columns from it), instead we simulate this test.
    let block = create_test_block_with_grids(
        BlockIndex::zero(),
        prompt_grid,
        rprompt_grid,
        output_grid,
        false, // honor_ps1
    );

    // Test with extremely narrow terminal
    let (input_narrow, output_narrow) = block.get_block_content_summary(
        20, // Very narrow terminal
        75, // Top lines
        25, // Bottom lines
    );

    // Test with wide terminal
    let (input_wide, output_wide) = block.get_block_content_summary(
        200, // Wide terminal
        75,  // Top lines
        25,  // Bottom lines
    );

    // Narrow terminal - top should have 562 lines and bottom should have 188 lines.

    assert!(input_narrow.contains("Input line 561"));
    assert!(!input_narrow.contains("Input line 562"));
    assert!(!input_narrow.contains("Input line 812"));
    assert!(input_narrow.contains("Input line 813"));

    assert!(output_narrow.contains("Output line 561"));
    assert!(!output_narrow.contains("Output line 562"));
    assert!(!output_narrow.contains("Output line 812"));
    assert!(output_narrow.contains("Output line 813"));

    // Wide terminal - top should have 75 lines and bottom should have 25 lines.
    assert!(input_wide.contains("Input line 74"));
    assert!(!input_wide.contains("Input line 75"));
    assert!(!input_wide.contains("Input line 974"));
    assert!(input_wide.contains("Input line 975"));

    assert!(output_wide.contains("Output line 74"));
    assert!(!output_wide.contains("Output line 75"));
    assert!(!output_wide.contains("Output line 974"));
    assert!(output_wide.contains("Output line 975"));
}
