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
