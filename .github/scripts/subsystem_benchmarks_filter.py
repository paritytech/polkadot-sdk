#!/usr/bin/env python3
"""Report whether a PR affects the subsystem benchmarks; never skip their execution."""

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tomllib


BENCHMARK_PACKAGES = (
	"polkadot-availability-recovery",
	"polkadot-availability-distribution",
	"polkadot-node-core-approval-voting",
	"polkadot-statement-distribution",
	"polkadot-node-core-dispute-coordinator",
)
GLOBAL_PREFIXES = (".github/", ".cargo/", ".config/")
IGNORED_PREFIXES = ("docs/", "prdoc/")
# Chain-spec unit tests include these across crate boundaries. Cover them conservatively.
EXTRA_INPUTS = ("substrate/test-utils/runtime/res/",)
GLOBAL_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}


def result(would_run, reason, **details):
	return {"mode": "report-only", "would_run": would_run, "reason": reason, **details}


def dependency_coverage(root, roots=BENCHMARK_PACKAGES):
	"""Over-approximate local Cargo dependencies without resolving features or targets."""
	root = Path(root).resolve()
	workspace_manifest = tomllib.loads((root / "Cargo.toml").read_text())
	workspace = workspace_manifest["workspace"]
	packages = {}
	names = {}

	def load(directory):
		path = Path(directory).resolve()
		if not path.is_relative_to(root):
			raise ValueError("Local dependency escapes the repository")
		key = path.relative_to(root).as_posix()
		if key not in packages:
			manifest = tomllib.loads((path / "Cargo.toml").read_text())
			packages[key] = manifest
		return key

	for member in workspace["members"]:
		if any(char in member for char in "*?["):
			raise ValueError("Workspace member globs are not supported")
		key = load(root / member)
		name = packages[key]["package"]["name"]
		if name in names:
			raise ValueError("Duplicate workspace package name")
		names[name] = key

	def dependencies(key, include_dev=False):
		manifest = packages[key]
		tables = [manifest, *manifest.get("target", {}).values()]
		groups = ["dependencies", "build-dependencies"]
		if include_dev:
			groups.append("dev-dependencies")
		for table in tables:
			for group in groups:
				for alias, specification in table.get(group, {}).items():
					base = root / key
					if not isinstance(specification, dict):
						continue
					if specification.get("workspace"):
						specification = workspace["dependencies"][alias]
						base = root
					if isinstance(specification, dict) and "path" in specification:
						yield load(base / specification["path"])

	root_paths = {names[name] for name in roots}
	pending = list(root_paths)
	# A local registry patch can substitute an external dependency. Include every such patch.
	for registry in workspace_manifest.get("patch", {}).values():
		for specification in registry.values():
			if isinstance(specification, dict) and "path" in specification:
				pending.append(load(root / specification["path"]))
	if "replace" in workspace_manifest:
		raise ValueError("Cargo dependency replacements are not supported")
	covered = set()
	while pending:
		key = pending.pop()
		if key in covered:
			continue
		covered.add(key)
		pending.extend(dependencies(key, include_dev=key in root_paths))
	return covered, set(packages)


def decide(root, changed_paths, event_name="pull_request"):
	"""Unknown inputs and graph errors propose running the full suite."""
	if event_name != "pull_request":
		return result(True, "full suite for non-PR events")
	paths = sorted(set(changed_paths))
	for path in paths:
		parsed = PurePosixPath(path)
		if parsed.is_absolute() or ".." in parsed.parts:
			return result(True, "invalid changed path")
		if path in GLOBAL_FILES or path.endswith("/Cargo.toml"):
			return result(True, "Cargo or toolchain input changed", input=path)
		if path.startswith(GLOBAL_PREFIXES):
			return result(True, "CI or build configuration changed", input=path)
	try:
		covered, known = dependency_coverage(root)
		owners = sorted(known, key=len, reverse=True)
		affected = set()
		for path in paths:
			if path.startswith(EXTRA_INPUTS):
				return result(True, "cross-crate fixture changed", input=path)
			if path.startswith(IGNORED_PREFIXES):
				continue
			owner = next((key for key in owners if path.startswith(key + "/")), None)
			if owner is None:
				return result(True, "unknown input", input=path)
			if owner in covered:
				affected.add(owner)
		return result(
			bool(affected),
			"dependency changed" if affected else "no benchmark inputs changed",
			dependency_count=len(covered),
			affected_packages=sorted(affected),
			changed_file_count=len(paths),
		)
	except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
		return result(True, "dependency graph unavailable", error=str(error))


def git(root, *arguments):
	return subprocess.check_output(
		["git", "-C", str(root), *arguments], stderr=subprocess.PIPE
	)


def pull_request_changes(root, base=None, payload=None):
	"""Compare the actual synthetic merge checkout to its verified first parent."""
	payload = payload or {}
	pr = payload.get("pull_request", {})
	base = base or pr.get("base", {}).get("sha")
	head = pr.get("head", {}).get("sha")
	if not base or not re.fullmatch(r"[0-9a-f]{40}", base):
		raise ValueError("A full pull-request base SHA is required")
	checkout = git(root, "rev-parse", "HEAD").decode().strip()
	header = git(root, "cat-file", "-p", checkout).split(b"\n\n", 1)[0]
	parents = [
		line.split()[1].decode() for line in header.splitlines() if line.startswith(b"parent ")
	]
	if len(parents) != 2 or parents[0] != base:
		raise ValueError("Checkout is not the expected pull-request merge commit")
	if head and parents[1] != head:
		raise ValueError("Checkout does not contain the expected pull-request head")
	if os.environ.get("GITHUB_SHA", checkout) != checkout:
		raise ValueError("Checkout differs from the workflow event SHA")
	# Disabling rename detection retains both old and new paths as delete/add entries.
	raw = git(
		root, "diff", "--no-ext-diff", "--name-only", "-z", "--no-renames", base, checkout, "--"
	)
	return [path.decode("utf-8") for path in raw.split(b"\0") if path], checkout, base


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--repo", type=Path, default=Path.cwd())
	parser.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME", "local"))
	parser.add_argument("--base", help="Explicit base SHA for local merge-checkout replay")
	arguments = parser.parse_args()
	try:
		if arguments.event != "pull_request":
			report = decide(arguments.repo, [], arguments.event)
		else:
			payload = {}
			if os.environ.get("GITHUB_EVENT_PATH"):
				payload = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
			paths, checkout, base = pull_request_changes(
				arguments.repo, arguments.base, payload
			)
			report = decide(arguments.repo, paths)
			report.update(checkout=checkout, base=base)
	except (
		OSError, ValueError, KeyError, TypeError, AttributeError, subprocess.CalledProcessError
	) as error:
		report = result(True, "pull-request context unavailable", error=str(error))
	print(json.dumps(report, indent=2))
	if os.environ.get("GITHUB_STEP_SUMMARY"):
		with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a") as summary:
			summary.write("## Subsystem benchmark filter: report only\n\n")
			summary.write("All five benchmark jobs still run regardless of this decision.\n\n")
			summary.write(f"Proposed decision: **{'RUN' if report['would_run'] else 'SKIP'}**\n\n")
			summary.write(f"Reason: {report['reason']}.\n")
			if "dependency_count" in report:
				summary.write(f"\nLocal crates covered: {report['dependency_count']}.\n")


if __name__ == "__main__":
	main()
