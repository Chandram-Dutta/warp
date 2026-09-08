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
LOCAL_ONLY_PRODUCT_SERVICE_PACKAGES = {
    "app-installation-detection",
    "http_server",
    "opentelemetry-http",
    "opentelemetry-otlp",
    "opentelemetry_sdk",
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
    app_menus = (REPO_ROOT / "app" / "src" / "app_menus.rs").read_text()
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
    classic_input = (
        REPO_ROOT / "app" / "src" / "terminal" / "input" / "classic.rs"
    ).read_text()
    terminal_view = (REPO_ROOT / "app" / "src" / "terminal" / "view.rs").read_text()
    block_list_element = (
        REPO_ROOT / "app" / "src" / "terminal" / "block_list_element.rs"
    ).read_text()
    terminal_actions = (
        REPO_ROOT / "app" / "src" / "terminal" / "view" / "action.rs"
    ).read_text()
    header_toolbar_items = (
        REPO_ROOT / "app" / "src" / "workspace" / "header_toolbar_item.rs"
    ).read_text()
    tab_settings = (
        REPO_ROOT / "app" / "src" / "workspace" / "tab_settings.rs"
    ).read_text()
    vertical_tabs = (
        REPO_ROOT / "app" / "src" / "workspace" / "view" / "vertical_tabs.rs"
    ).read_text()
    features_page = (
        REPO_ROOT / "app" / "src" / "settings_view" / "features_page.rs"
    ).read_text()
    appearance_page = (
        REPO_ROOT / "app" / "src" / "settings_view" / "appearance_page.rs"
    ).read_text()
    keybindings_page = (
        REPO_ROOT / "app" / "src" / "settings_view" / "keybindings.rs"
    ).read_text()
    command_palette_actions = (
        REPO_ROOT / "app" / "src" / "search" / "action" / "data_source.rs"
    ).read_text()
    workspace_state = (
        REPO_ROOT / "app" / "src" / "workspace" / "util.rs"
    ).read_text()
    ai_settings = (REPO_ROOT / "app" / "src" / "settings" / "ai.rs").read_text()
    drive_settings = (
        REPO_ROOT / "app" / "src" / "drive" / "settings.rs"
    ).read_text()
    editor_view = (
        REPO_ROOT / "app" / "src" / "editor" / "view" / "mod.rs"
    ).read_text()
    command_palette_sources = (
        REPO_ROOT / "app" / "src" / "search" / "command_palette" / "data_sources.rs"
    ).read_text()
    command_search = (
        REPO_ROOT / "app" / "src" / "search" / "command_search" / "view.rs"
    ).read_text()
    root_view = (REPO_ROOT / "app" / "src" / "root_view.rs").read_text()
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
    assert "release_bundle" in local_only
    assert '#[cfg(all(feature = "local_only", feature = "warp_agent_runtime"))]' in app_root
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "    ctx.add_singleton_model(GlobalBufferModel::new);"
        in app_root
    )
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "            lsp::LspManagerModel::handle(ctx).update(ctx, |manager, ctx| {"
        in app_root
    )
    go_to_line_menu = app_menus.index("CustomAction::GoToLine")
    assert app_menus.rfind('#[cfg(not(feature = "local_only"))]', 0, go_to_line_menu) > (
        app_menus.rfind("let mut group_4", 0, go_to_line_menu)
    )
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
    assert "cargo build --locked --release -p warp --bin warp-local-only" in local_only_workflow
    assert "script/bundle --channel local-only --arch aarch64 --adhoc-sign" in local_only_workflow
    assert "target/aarch64-apple-darwin/release-lto/bundle/osx/WarpLocalOnly.app" in local_only_workflow
    assert 'lipo "$executable" -verify_arch arm64' in local_only_workflow
    assert "codesign --verify --deep --strict --verbose=4" in local_only_workflow
    assert "script/macos/smoke_test_app" in local_only_workflow
    assert "fn input_enter_local_only" in terminal_input
    assert (
        '#[cfg(feature = "local_only")]\n'
        "        self.input_enter_local_only(ctx);"
        in terminal_input
    )
    assert "fn local_only_enter_executes_shell_command_even_when_agent_flags_and_state_are_enabled" in (
        REPO_ROOT / "app" / "src" / "terminal" / "input_tests.rs"
    ).read_text()
    local_classic_render = classic_input.index('#[cfg(feature = "local_only")]')
    agent_classic_render = classic_input.index('#[cfg(not(feature = "local_only"))]', local_classic_render)
    assert local_classic_render < agent_classic_render
    assert "is_inline_history_menu" in classic_input[local_classic_render:agent_classic_render]
    assert "is_slash_commands" not in classic_input[local_classic_render:agent_classic_render]
    assert '#[cfg(not(feature = "local_only"))]\nmod zero_state_block;' in terminal_view
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "        ctx.subscribe_to_model(&agent_view_controller"
        in terminal_view
    )
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "        if FeatureFlag::AgentView.is_enabled()"
        in terminal_view
    )
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "        if AISettings::as_ref(app).is_any_ai_enabled(app)"
        in block_list_element
    )
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "        if WarpDriveSettings::is_warp_drive_enabled(app)"
        in block_list_element
    )
    assert "fn is_available_in_product(&self) -> bool" in terminal_actions
    assert "ContextMenu(action) if !action.is_available_in_product()" in terminal_actions
    assert "InputContextMenuItem(action) if !action.is_available_in_product()" in terminal_actions
    assert "pub fn is_available_in_product(&self) -> bool" in header_toolbar_items
    assert ".filter(HeaderToolbarItemKind::is_available_in_product)" in tab_settings
    assert "return SummaryPaneKind::Terminal;" in vertical_tabs
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "            Box::new(InputTypeWidget::default()),"
        in appearance_page
    )
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        "        if FeatureFlag::AgentView.is_enabled()"
        in features_page
    )
    assert ".filter(binding_is_available_in_product)" in keybindings_page
    assert ".filter(binding_is_available_in_product)" in command_palette_actions
    assert (
        '#[cfg(feature = "local_only")]\n'
        "        return self.is_any_modal_open(app) || self.is_theme_chooser_open;"
        in workspace_state
    )
    assert (
        '#[cfg(feature = "local_only")]\n'
        "        return false;"
        in workspace_state
    )
    assert (
        'pub fn is_any_ai_enabled(&self, app: &AppContext) -> bool {\n'
        '        #[cfg(feature = "local_only")]'
        in ai_settings
    )
    assert ai_settings.count('let _ = app;\n            return false;') >= 1
    assert drive_settings.count('let _ = app;\n            return false;') >= 2
    assert 'let should_show_at_context_menu = !cfg!(feature = "local_only")' in editor_view
    assert command_palette_sources.count('#[cfg(not(feature = "local_only"))]') >= 12
    assert command_search.count('#[cfg(not(feature = "local_only"))]') >= 3
    assert (
        '#[cfg(not(feature = "local_only"))]\n'
        '    app.add_action("root_view:log_out", RootView::log_out);'
        in root_view
    )
    assert '"root_view:open_cloud_conversation_in_existing_window"' in root_view
    assert '"root_view:open_drive_object_existing_window"' in root_view
    assert '"root_view:open_mcp_settings_in_existing_window"' in root_view
    assert '"root_view:open_linear_issue_work_in_existing_window"' in root_view

    local_only_packages = dependency_packages("local_only")
    unexpected = (EXCLUDED_PACKAGES | LOCAL_ONLY_PRODUCT_SERVICE_PACKAGES) & local_only_packages
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
