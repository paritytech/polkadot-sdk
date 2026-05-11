#!/usr/bin/env python3
# Copyright (C) Parity Technologies (UK) Ltd.
# SPDX-License-Identifier: GPL-3.0-only

"""Check that tooling dependency worlds do not leak into production defaults.

The workspace intentionally contains a few external testing/tooling stacks
(`zombienet-sdk`, `substrate-txtesttool`, and their transitive Substrate crates)
whose versions cannot safely be unified with the in-tree SDK stack. This check
keeps those worlds separated: default production members must not reach them via
normal/build dependencies, and pinned tooling entry points require an explicit
update to this file when they move.
"""

from __future__ import annotations

import json
import subprocess
import sys
from collections import deque
from pathlib import Path


TOOLING_ONLY_PACKAGES = {
    "substrate-txtesttool": "0.7.0",
    "zombienet-configuration": "0.4.9",
    "zombienet-orchestrator": "0.4.9",
    "zombienet-sdk": "0.4.9",
}

SDK_STACK_PACKAGE_NAMES = {
    "sc-chain-spec",
    "sc-client-api",
    "sp-api",
    "sp-core",
    "sp-io",
    "sp-runtime",
    "sp-state-machine",
}


def run_metadata(workspace: Path) -> dict:
    output = subprocess.check_output(
        ["cargo", "metadata", "--format-version=1", "--locked"],
        cwd=workspace,
        text=True,
    )
    return json.loads(output)


def normal_dependency_ids(node: dict) -> list[str]:
    normal_deps = []
    for dep in node.get("deps", []):
        dep_kinds = dep.get("dep_kinds", [])
        if dep_kinds and all(kind.get("kind") == "dev" for kind in dep_kinds):
            continue
        normal_deps.append(dep["pkg"])
    return normal_deps


def normal_closure(seed_ids: list[str], node_by_id: dict[str, dict]) -> set[str]:
    seen: set[str] = set()
    queue = deque(seed_ids)
    while queue:
        package_id = queue.popleft()
        if package_id in seen:
            continue
        seen.add(package_id)
        node = node_by_id.get(package_id)
        if node is None:
            continue
        queue.extend(normal_dependency_ids(node))
    return seen


def package_label(package: dict) -> str:
    source = package.get("source") or "path"
    return f'{package["name"]} {package["version"]} ({source})'


def main() -> int:
    workspace = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    metadata = run_metadata(workspace)
    package_by_id = {package["id"]: package for package in metadata["packages"]}
    node_by_id = {node["id"]: node for node in metadata["resolve"]["nodes"]}

    default_members = metadata.get("workspace_default_members") or metadata["workspace_members"]
    default_normal = normal_closure(default_members, node_by_id)
    default_packages = [package_by_id[package_id] for package_id in default_normal]

    errors: list[str] = []

    leaked_tooling = sorted(
        package_label(package)
        for package in default_packages
        if package["name"] in TOOLING_ONLY_PACKAGES
    )
    if leaked_tooling:
        errors.append(
            "tooling-only packages are reachable from workspace default members via "
            f"normal/build dependencies: {', '.join(leaked_tooling)}"
        )

    leaked_external_sdk_stack = sorted(
        package_label(package)
        for package in default_packages
        if package["name"] in SDK_STACK_PACKAGE_NAMES and package.get("source") is not None
    )
    if leaked_external_sdk_stack:
        errors.append(
            "external SDK stack crates are reachable from workspace default members: "
            f"{', '.join(leaked_external_sdk_stack)}"
        )

    packages_by_name: dict[str, list[dict]] = {}
    for package in metadata["packages"]:
        packages_by_name.setdefault(package["name"], []).append(package)

    for package_name, expected_version in sorted(TOOLING_ONLY_PACKAGES.items()):
        actual_versions = sorted(
            {package["version"] for package in packages_by_name.get(package_name, [])}
        )
        if actual_versions and actual_versions != [expected_version]:
            errors.append(
                f"{package_name} tooling world moved from {expected_version} to "
                f"{', '.join(actual_versions)}; update the dependency-world review policy"
            )

    if errors:
        print("Dependency world check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    pinned = ", ".join(
        f"{name} {version}" for name, version in sorted(TOOLING_ONLY_PACKAGES.items())
    )
    print(
        "Dependency world check passed: workspace defaults are free of tooling-only "
        f"SDK stacks; pinned tooling worlds: {pinned}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
