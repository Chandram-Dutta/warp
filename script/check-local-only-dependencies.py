#!/usr/bin/env python3

import pathlib
import subprocess
import tomllib


REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
EXCLUDED_PACKAGES = {
    "aws-config",
    "aws-credential-types",
    "aws-sdk-sts",
    "warp_multi_agent_client",
}
PERSISTENCE_RUNTIME_PACKAGES = {"warp_multi_agent_api"}


def dependency_packages(features: str) -> set[str]:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            "warp",
            "--no-default-features",
            "--features",
            features,
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return {line.split(maxsplit=1)[0] for line in result.stdout.splitlines() if line}


def package_dependencies(package: str) -> set[str]:
    result = subprocess.run(
        [
            "cargo",
            "tree",
            "-p",
            package,
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return {line.split(maxsplit=1)[0] for line in result.stdout.splitlines() if line}


def main() -> None:
    manifest = tomllib.loads((REPO_ROOT / "app" / "Cargo.toml").read_text())
    app_root = (REPO_ROOT / "app" / "src" / "lib.rs").read_text()
    core_cli_agent = (
        REPO_ROOT / "crates" / "warp_core" / "src" / "cli_agent_protocol.rs"
    ).read_text()
    app_cli_agent = (REPO_ROOT / "app" / "src" / "terminal" / "cli_agent.rs").read_text()
    local_only_workflow = (
        REPO_ROOT / ".github" / "workflows" / "local_only.yml"
    ).read_text()
    macos_bundle = (REPO_ROOT / "script" / "macos" / "bundle").read_text()
    bundled_resources = (REPO_ROOT / "script" / "prepare_bundled_resources").read_text()
    core_input = (REPO_ROOT / "crates" / "warp_core" / "src" / "input.rs").read_text()
    editor = (REPO_ROOT / "crates" / "editor" / "src" / "editor.rs").read_text()
    session_config = (REPO_ROOT / "app" / "src" / "tab_configs" / "session_config.rs").read_text()
    cli_agent_sessions = (
        REPO_ROOT / "app" / "src" / "terminal" / "cli_agent_sessions" / "mod.rs"
    ).read_text()
    terminal_input = (REPO_ROOT / "app" / "src" / "terminal" / "input.rs").read_text()
    terminal_block_filter = (
        REPO_ROOT / "app" / "src" / "terminal" / "block_filter.rs"
    ).read_text()
    persistence_root = (REPO_ROOT / "app" / "src" / "persistence" / "mod.rs").read_text()
    sqlite = (REPO_ROOT / "app" / "src" / "persistence" / "sqlite.rs").read_text()
    restored_conversations = (
        REPO_ROOT / "app" / "src" / "ai" / "restored_conversations.rs"
    ).read_text()
    agent_mode = set(manifest["features"]["agent_mode"])
    agent_runtime = set(manifest["features"]["warp_agent_runtime"])
    local_only = set(manifest["features"]["local_only"])

    expected_feature_edges = {
        "dep:aws-config",
        "dep:aws-credential-types",
        "dep:aws-sdk-sts",
        "dep:warp_multi_agent_client",
    }
    assert expected_feature_edges <= agent_runtime
    assert "warp_agent_runtime" in agent_mode
    assert not expected_feature_edges & local_only
    assert "agent_mode" not in local_only
    assert "warp_agent_runtime" not in local_only
    assert '#[cfg(all(feature = "local_only", feature = "warp_agent_runtime"))]' in app_root
    assert (
        '#[cfg(feature = "local_only")]\n'
        "    let persisted_data_scope = persistence::PersistedDataScope::TerminalLocal;"
        in app_root
    )
    assert "pub struct TerminalPersistedData" in persistence_root
    assert "pub enum TerminalModelEvent" in persistence_root
    terminal_scope_exit = sqlite.index(
        "if matches!(data_scope, PersistedDataScope::TerminalLocal)"
    )
    agent_conversation_load = sqlite.index("read_agent_conversation_metadata(conn)?")
    assert terminal_scope_exit < agent_conversation_load
    assert (
        '#[cfg(all(feature = "local_fs", not(feature = "local_only")))]\n'
        "    db_connection: Option<Arc<Mutex<SqliteConnection>>>"
        in restored_conversations
    )
    assert "pub enum NavigationKey" in core_input
    assert "pub use warp_core::input::NavigationKey;" in editor
    assert "pub enum NavigationKey" not in editor
    assert "use warp_core::input::NavigationKey;" in terminal_input
    assert "use warp_core::input::NavigationKey;" in terminal_block_filter
    for source in (REPO_ROOT / "app" / "src").rglob("*.rs"):
        assert "warp_editor::editor::NavigationKey" not in source.read_text(), source
    assert "pub enum CLIAgent" in core_cli_agent
    assert "pub enum CLIAgent" not in app_cli_agent
    assert "pub use warp_core::cli_agent_protocol::CLIAgent;" in app_cli_agent
    assert "trait CLIAgentRuntimeExt" in app_cli_agent
    for runtime_dependency in ("AppContext", "SkillProvider", "warp_cli::agent", "Icon"):
        assert runtime_dependency not in core_cli_agent
    assert "use warp_core::cli_agent_protocol::CLIAgent;" in session_config
    assert "use warp_core::cli_agent_protocol::CLIAgent;" in cli_agent_sessions
    local_only_bundle = manifest["package"]["metadata"]["bundle"]["bin"]["warp-local-only"]
    assert local_only_bundle["identifier"] == "dev.warp.Warp-LocalOnly"
    assert local_only_bundle["name"] == "WarpLocalOnly"
    assert 'FEATURES="local_only"' in macos_bundle
    assert "NO_DEFAULT_FEATURES=true" in macos_bundle
    assert 'RELEASE_CHANNEL != "local-only"' in macos_bundle
    assert 'if [ "$CHANNEL" = "local-only" ]' in bundled_resources
    assert "script/bundle --channel local-only --arch aarch64 --debug --adhoc-sign" in local_only_workflow
    assert 'lipo "$executable" -verify_arch arm64' in local_only_workflow
    assert "codesign --verify --deep --strict --verbose=4" in local_only_workflow

    local_only_packages = dependency_packages("local_only")
    unexpected = EXCLUDED_PACKAGES & local_only_packages
    assert not unexpected, f"local-only dependency graph contains: {sorted(unexpected)}"

    agent_packages = dependency_packages("agent_mode")
    missing = EXCLUDED_PACKAGES - agent_packages
    assert not missing, f"Agent dependency graph is missing: {sorted(missing)}"

    persistence_packages = package_dependencies("persistence")
    unexpected = PERSISTENCE_RUNTIME_PACKAGES & persistence_packages
    assert not unexpected, f"persistence dependency graph contains: {sorted(unexpected)}"

    print("local-only dependency boundary verified")


if __name__ == "__main__":
    main()
