#!/usr/bin/env python3
"""Conservative dependency and actual-merge-context tests for the report-only selector."""

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import subsystem_benchmarks_filter as selector


def make_workspace(root):
	packages = {f"bench{i}": name for i, name in enumerate(selector.BENCHMARK_PACKAGES)}
	for name in (
		"shared", "shared/independent", "optional", "builder", "dev", "leaf",
		"transitive-dev", "unrelated", "target", "patched",
	):
		packages[name] = name.replace("/", "-")
	(root / "Cargo.toml").write_text(
		"[workspace]\nmembers = " + json.dumps(list(packages))
		+ '\n[workspace.dependencies]\nshared-alias = { path = "shared", package = "shared" }'
		+ '\n[patch.crates-io]\npatched = { path = "patched" }\n'
	)
	for directory, name in packages.items():
		path = root / directory
		path.mkdir(parents=True, exist_ok=True)
		(path / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n')
	with (root / "bench0/Cargo.toml").open("a") as manifest:
		manifest.write(
			'\n[dependencies]\nshared-alias = { workspace = true }\n'
			'optional = { path = "../optional", optional = true }\n'
			'[build-dependencies]\nbuilder = { path = "../builder" }\n'
			'[dev-dependencies]\ndev = { path = "../dev" }\n'
			'[target.\'cfg(target_os = "linux")\'.dependencies]\n'
			'target = { path = "../target" }\n'
		)
	with (root / "shared/Cargo.toml").open("a") as manifest:
		manifest.write(
			'\n[dependencies]\nleaf = { path = "../leaf" }\n'
			'[dev-dependencies]\ntransitive-dev = { path = "../transitive-dev" }\n'
		)


class DependencyTests(unittest.TestCase):
	def setUp(self):
		self.temporary = tempfile.TemporaryDirectory()
		self.addCleanup(self.temporary.cleanup)
		self.root = Path(self.temporary.name)
		make_workspace(self.root)

	def test_direct_transitive_build_dev_optional_target_and_patch_inputs_run(self):
		for package in (
			"bench0", "shared", "leaf", "builder", "dev", "optional", "target", "patched",
		):
			with self.subTest(package=package):
				answer = selector.decide(self.root, [package + "/src/lib.rs"])
				self.assertTrue(answer["would_run"])
				self.assertEqual(answer["reason"], "dependency changed")

	def test_unrelated_crates_and_transitive_dev_dependencies_can_skip(self):
		for package in ("unrelated", "transitive-dev", "shared/independent"):
			with self.subTest(package=package):
				self.assertFalse(selector.decide(self.root, [package + "/src/lib.rs"])["would_run"])

	def test_global_inputs_always_run(self):
		for path in (
			"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
			"unrelated/Cargo.toml", ".github/env", ".cargo/config.toml", ".config/test.toml",
		):
			with self.subTest(path=path):
				self.assertTrue(selector.decide(self.root, [path])["would_run"])

	def test_documentation_and_prdoc_can_skip(self):
		answer = selector.decide(self.root, ["docs/guide.md", "prdoc/pr_1.prdoc"])
		self.assertFalse(answer["would_run"])

	def test_cross_crate_fixtures_run(self):
		path = "substrate/test-utils/runtime/res/default_genesis_config.json"
		self.assertTrue(selector.decide(self.root, [path])["would_run"])

	def test_unknown_or_unsafe_inputs_run(self):
		for path in ("new-root-input.rs", "../outside.rs", "/absolute.rs"):
			with self.subTest(path=path):
				self.assertTrue(selector.decide(self.root, [path])["would_run"])

	def test_missing_or_malformed_dependency_runs(self):
		(self.root / "leaf/Cargo.toml").unlink()
		self.assertTrue(selector.decide(self.root, ["unrelated/lib.rs"])["would_run"])
		(self.root / "leaf/Cargo.toml").write_text("invalid [ toml")
		self.assertTrue(selector.decide(self.root, ["unrelated/lib.rs"])["would_run"])

	def test_dependency_outside_repository_runs(self):
		with (self.root / "bench0/Cargo.toml").open("a") as manifest:
			manifest.write('outside = { path = "../../outside" }\n')
		answer = selector.decide(self.root, ["unrelated/lib.rs"])
		self.assertTrue(answer["would_run"])
		self.assertEqual(answer["reason"], "dependency graph unavailable")

	def test_unsupported_workspace_globs_run(self):
		(self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["bench*"]\n')
		self.assertTrue(selector.decide(self.root, ["unrelated/lib.rs"])["would_run"])

	def test_non_pr_events_always_run(self):
		for event in ("push", "merge_group", "workflow_dispatch"):
			with self.subTest(event=event):
				self.assertTrue(selector.decide(self.root, [], event)["would_run"])

	def test_mode_is_always_report_only(self):
		self.assertEqual(selector.decide(self.root, ["unrelated/lib.rs"])["mode"], "report-only")


class MergeContextTests(unittest.TestCase):
	def setUp(self):
		self.temporary = tempfile.TemporaryDirectory()
		self.addCleanup(self.temporary.cleanup)
		self.root = Path(self.temporary.name)
		make_workspace(self.root)
		self.git("init", "-q", "-b", "main")
		(self.root / "shared/old.rs").write_text("pub fn original() {}\n")
		self.commit("initial")
		self.git("checkout", "-q", "-b", "feature")
		self.git("mv", "shared/old.rs", "unrelated/new.rs")
		self.commit("PR rename")
		self.head = self.git("rev-parse", "HEAD").strip()
		self.git("checkout", "-q", "main")
		(self.root / "base-only.txt").write_text("unrelated base branch change")
		self.commit("advance base")
		self.base = self.git("rev-parse", "HEAD").strip()
		self.git("merge", "--no-ff", "--no-commit", "feature")
		self.commit("synthetic CI merge")
		self.checkout = self.git("rev-parse", "HEAD").strip()
		self.payload = {"pull_request": {"base": {"sha": self.base}, "head": {"sha": self.head}}}

	def git(self, *arguments):
		return subprocess.check_output(
			["git", "-C", str(self.root), "-c", "user.name=CI test",
			 "-c", "user.email=ci@example.invalid", *arguments],
			stderr=subprocess.PIPE, text=True,
		)

	def commit(self, message):
		self.git("add", ".")
		self.git("commit", "-q", "--no-gpg-sign", "-m", message)

	def test_actual_merge_diff_preserves_both_rename_paths(self):
		with patch.dict(os.environ, {"GITHUB_SHA": self.checkout}):
			paths, checkout, base = selector.pull_request_changes(self.root, payload=self.payload)
		self.assertEqual(set(paths), {"shared/old.rs", "unrelated/new.rs"})
		self.assertEqual(checkout, self.checkout)
		self.assertEqual(base, self.base)
		self.assertTrue(selector.decide(self.root, paths)["would_run"])

	def test_wrong_base_or_head_is_rejected(self):
		for field in ("base", "head"):
			with self.subTest(field=field):
				payload = json.loads(json.dumps(self.payload))
				payload["pull_request"][field]["sha"] = "0" * 40
				with self.assertRaises(ValueError):
					selector.pull_request_changes(self.root, payload=payload)

	def test_mismatched_event_sha_is_rejected(self):
		with patch.dict(os.environ, {"GITHUB_SHA": "0" * 40}):
			with self.assertRaises(ValueError):
				selector.pull_request_changes(self.root, payload=self.payload)

	def test_missing_context_reports_run_and_exits_successfully(self):
		environment = {k: v for k, v in os.environ.items() if not k.startswith("GITHUB_")}
		output = subprocess.check_output(
			["python3", selector.__file__, "--repo", str(self.root), "--event", "pull_request"],
			env=environment, text=True,
		)
		answer = json.loads(output)
		self.assertTrue(answer["would_run"])
		self.assertEqual(answer["reason"], "pull-request context unavailable")

	def test_malformed_event_context_reports_run(self):
		event = self.root / "event.json"
		environment = {k: v for k, v in os.environ.items() if not k.startswith("GITHUB_")}
		environment["GITHUB_EVENT_PATH"] = str(event)
		for malformed in (None, []):
			with self.subTest(payload=malformed):
				event.write_text(json.dumps({"pull_request": malformed}))
				output = subprocess.check_output(
					["python3", selector.__file__, "--repo", str(self.root),
					 "--event", "pull_request"], env=environment, text=True,
				)
				self.assertTrue(json.loads(output)["would_run"])


class WorkflowContractTests(unittest.TestCase):
	def test_benchmark_roots_match_the_workflow_matrix(self):
		workflow = Path(__file__).resolve().parents[1] / "workflows/benchmarks-subsystem.yml"
		names = re.findall(r'\bname:\s*["\']([^"\']+)["\']', workflow.read_text())
		self.assertEqual(tuple(names), selector.BENCHMARK_PACKAGES)


if __name__ == "__main__":
	unittest.main()
