use super::*;

#[path = "git_status_parser_tests.rs"]
mod parser_tests;

#[test]
fn renamed_path_does_not_consume_following_untracked_entry() {
    let status = "# branch.head main\0\
        2 R. N... 100644 100644 100644 abc def R100 new name.txt\0old name.txt\0\
        ? next\tfile.txt\0";
    assert_eq!(
        parse_git_status(status).unwrap(),
        vec![
            (
                "new name.txt".into(),
                GitFileStatus::Renamed {
                    old_path: "old name.txt".into()
                }
            ),
            ("next\tfile.txt".into(), GitFileStatus::Untracked),
        ]
    );
}

#[tokio::test]
async fn working_tree_counts_include_untracked_text_but_not_binary_lines() {
    let directory = tempfile::tempdir().unwrap();
    let repo = directory.path();
    run_git_command(repo, &["init"]).await.unwrap();
    std::fs::write(repo.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();
    run_git_command(repo, &["add", "tracked.txt"])
        .await
        .unwrap();
    run_git_command(
        repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "initial",
        ],
    )
    .await
    .unwrap();
    std::fs::write(repo.join("tracked.txt"), "one\nreplacement\nextra\nthree\n").unwrap();
    std::fs::write(repo.join("untracked.txt"), "new\nlines\n").unwrap();
    std::fs::write(repo.join("binary.bin"), b"\0\0\0\n\n").unwrap();

    let metadata = diff_metadata_against_head(repo).await.unwrap();

    assert_eq!(metadata.aggregate_stats.files_changed, 3);
    assert_eq!(metadata.aggregate_stats.total_additions, 4);
    assert_eq!(metadata.aggregate_stats.total_deletions, 1);
    let binary = metadata
        .files
        .iter()
        .find(|file| file.path == "binary.bin")
        .unwrap();
    assert_eq!((binary.additions, binary.deletions), (0, 0));
}
