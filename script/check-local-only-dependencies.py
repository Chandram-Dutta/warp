#!/usr/bin/env python3

"""Check physical product removal, including target-specific build dependencies."""

import pathlib
import subprocess
import sys
import tomllib


REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
EXCLUDED_PACKAGES = {
    "ai",
    "app-installation-detection",
    "aws-config",
    "aws-credential-types",
    "aws-sdk-sts",
    "cloud_object_client",
    "cloud_object_models",
    "cloud_object_persistence",
    "cloud_objects",
    "computer_use",
    "http_server",
    "languages",
    "lsp",
    "mcp",
    "node_runtime",
    "opentelemetry-http",
    "opentelemetry-otlp",
    "opentelemetry_sdk",
    "remote_server",
    "rmcp",
    "syntax_tree",
    "warp_graphql",
    "warp_graphql_schema",
    "warp_multi_agent_api",
    "warp_multi_agent_client",
    "warp_server_auth",
    "warp_server_client",
    "warp_tui",
}
EXCLUDED_APP_MODULES = {
    "ai", "ai_assistant", "auth", "billing", "cloud_object", "code", "code_review",
    "drive", "notebooks", "remote_server", "server", "tui",
}
EXCLUDED_SOURCE_PATHS = {
    "crates/lsp", "crates/node_runtime", "crates/warp_tui",
    "crates/warpui_core/src/elements/tui",
}


def is_excluded(package: str) -> bool:
    return package in EXCLUDED_PACKAGES or package.startswith("arborium")


def main() -> int:
    result = subprocess.run(
        [
            "cargo", "tree", "--locked", "-p", "warp", "--no-default-features",
            "--features", "local_only", "--target", "all", "--edges", "normal,build",
            "--prefix", "none", "--format", "{p}",
        ],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    packages = {line.split()[0] for line in result.stdout.splitlines() if line}
    failures = []
    unexpected = sorted(filter(is_excluded, packages))
    if unexpected:
        failures.append(f"Normal/build graph still contains: {', '.join(unexpected)}")
    if "warp_local_ai" not in packages:
        failures.append("Direct-provider Next Command dependency warp_local_ai is missing")

    for manifest_path in sorted((REPO_ROOT / "crates").glob("*/Cargo.toml")):
        manifest = tomllib.loads(manifest_path.read_text())
        if is_excluded(manifest.get("package", {}).get("name", "")):
            failures.append(f"Excluded crate still exists: {manifest_path.parent.relative_to(REPO_ROOT)}")
    for name in sorted(EXCLUDED_APP_MODULES):
        for path in (REPO_ROOT / "app/src" / name, REPO_ROOT / "app/src" / f"{name}.rs"):
            if path.exists():
                failures.append(f"Excluded app module still exists: {path.relative_to(REPO_ROOT)}")
    for path in sorted(EXCLUDED_SOURCE_PATHS):
        if (REPO_ROOT / path).exists():
            failures.append(f"Excluded source still exists: {path}")

    manifest = tomllib.loads((REPO_ROOT / "app/Cargo.toml").read_text())
    binaries = {binary["name"] for binary in manifest.get("bin", [])}
    product_binaries = binaries - {"generate_settings_schema", "integration"}
    if product_binaries != {"warp-local-only"}:
        failures.append(f"Expected sole product binary warp-local-only, found: {sorted(product_binaries)}")

    if failures:
        print("Physical removal incomplete:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1
    print("Configured source/package exclusions verified across all target normal/build graphs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
