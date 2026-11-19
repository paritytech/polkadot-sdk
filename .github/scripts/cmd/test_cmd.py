"""Command-bot behavior, with isolated imports and parametrized command cases."""

import argparse
import json
from pathlib import Path
from unittest.mock import Mock, mock_open, patch

import pytest


RUNTIMES = [
	dict(name="dev", package="kitchensink-runtime", path="substrate/frame",
		header="substrate/HEADER-APACHE2", template="substrate/.maintain/frame-weight-template.hbs",
		bench_features="runtime-benchmarks", bench_flags="--flag1 --flag2"),
	dict(name="westend", package="westend-runtime", path="polkadot/runtime/westend",
		header="polkadot/file_header.txt",
		template="polkadot/xcm/pallet-xcm-benchmarks/template.hbs",
		bench_features="runtime-benchmarks", bench_flags="--flag3 --flag4"),
	dict(name="asset-hub-westend", package="asset-hub-westend-runtime",
		path="cumulus/parachains/runtimes/assets/asset-hub-westend",
		header="cumulus/file_header.txt", template="cumulus/templates/xcm-bench-template.hbs",
		bench_features="runtime-benchmarks", bench_flags="--flag7 --flag8"),
]
LABELS = ["T1-FRAME", "R0-no-crate-publish-required", "D2-substantial"]


@pytest.fixture
def cmd(load_script, monkeypatch, tmp_path):
	with patch("builtins.open", mock_open(read_data=json.dumps(RUNTIMES))), patch("os.makedirs"):
		module = load_script("cmd/cmd.py")
	module.print_and_log.__defaults__ = (tmp_path / "command.log",)
	monkeypatch.setattr(module.parser, "parse_known_args", Mock())
	monkeypatch.setattr(module.os, "system", Mock(return_value=0))
	monkeypatch.setattr(module.os, "popen", Mock())
	monkeypatch.setattr(module.generate_prdoc, "main", Mock(return_value=0))
	return module


def run(cmd, command, **options):
	args = argparse.Namespace(command=command, **options)
	cmd.parser.parse_known_args.return_value = args, []
	cmd.main()
	return args


@pytest.mark.parametrize("runtimes,pallets,listed,manifest", [
	([r["name"] for r in RUNTIMES], ["pallet_balances"],
		["pallet_balances", "pallet_staking", "pallet_something"], "./substrate/frame/balances"),
	(["westend"], ["pallet_balances", "pallet_staking"],
		["pallet_balances", "pallet_staking", "pallet_something"], None),
	(["westend"], ["pallet_xcm_benchmarks::generic"],
		["pallet_balances", "pallet_staking", "pallet_something", "pallet_xcm_benchmarks::generic"],
		None),
	(["westend"], ["pallet_balances", "pallet_staking"],
		["pallet_staking", "pallet_balances"], None),
	(["dev"], ["pallet_balances"], ["pallet_balances", "pallet_something"],
		"substrate/frame/kitchensink"),
	(["asset-hub-westend"], ["pallet_assets"], ["pallet_assets"], None),
	(["asset-hub-westend"], ["pallet_xcm_benchmarks::generic", "pallet_assets"],
		["pallet_assets", "pallet_xcm_benchmarks::generic"], None),
], ids=["all-runtimes", "two-pallets", "relay-xcm", "reversed-listing", "dev", "cumulus",
	"cumulus-xcm"])
def test_bench(cmd, runtimes, pallets, listed, manifest):
	responses = ["\n".join(listed)] * len(runtimes)
	if manifest:
		responses.append(manifest + "/Cargo.toml")
	cmd.os.popen.return_value.read.side_effect = responses
	run(cmd, "bench-omni", runtime=runtimes, pallet=pallets, fail_fast=True,
		quiet=False, clean=False, image=None)
	calls = [call.args[0].split() for call in cmd.os.system.call_args_list]
	assert ["cargo", "install", "-q", "--path", "substrate/utils/frame/omni-bencher",
		"--locked", "--profile", "production"] in calls
	for runtime in (r for r in RUNTIMES if r["name"] in runtimes):
		assert (f"forklift cargo build -q -p {runtime['package']} --profile production "
			"--features=runtime-benchmarks").split() in calls
		for pallet in pallets:
			xcm = pallet.startswith("pallet_xcm_benchmarks")
			output = (manifest + "/src/weights.rs" if runtime["name"] == "dev" else
				f"./{runtime['path']}/src/weights" + ("/xcm" if xcm else ""))
			template = runtime["template"] if runtime["name"] == "dev" or xcm else None
			expected = (
				"frame-omni-bencher v1 benchmark pallet --extrinsic=* "
				f"--runtime=target/production/wbuild/{runtime['package']}/"
				f"{runtime['package'].replace('-', '_')}.wasm --pallet={pallet} "
				f"--header={Path(runtime['header']).resolve()} --output={output} "
				"--wasm-execution=compiled --steps=50 --repeat=20 --heap-pages=4096 "
				+ (f"--template={template} " if template else "")
				+ "--no-storage-info --no-min-squares --no-median-slopes " + runtime["bench_flags"]
			)
			assert expected.split() in calls
	assert len(calls) == 1 + len(runtimes) * (1 + len(pallets))


@pytest.mark.parametrize("command,expected", [
	("fmt", ["cargo +nightly fmt", "taplo format --config .config/taplo.toml"]),
	("update-ui", ["sh ./scripts/update-ui-tests.sh"]),
])
def test_simple_commands(cmd, command, expected):
	run(cmd, command)
	assert [call.args[0] for call in cmd.os.system.call_args_list] == expected


@pytest.mark.parametrize("fails", [False, True], ids=["success", "failure"])
def test_prdoc(cmd, capsys, fails):
	if fails:
		def failing_main(args, log):
			log("Error generating prdoc: boom")
			return 1
		cmd.generate_prdoc.main.side_effect = failing_main
		with pytest.raises(SystemExit) as error:
			run(cmd, "prdoc")
		assert error.value.code == 1
		output = capsys.readouterr().out
		assert "Error generating prdoc: boom" in output
		assert "Failed to generate prdoc" in output
	else:
		args = run(cmd, "prdoc")
		cmd.generate_prdoc.main.assert_called_once_with(args, log=cmd.print_and_log)


@pytest.mark.parametrize("case", [
	dict(labels=LABELS[:2], expected=LABELS[:2]),
	dict(labels=["T1-FRAM", "R0-no-crate-publish"], expected=LABELS[:2], corrected=True),
	dict(labels=["T1-something"], expected=["T1-FRAME"],
		allowed=["T1-FRAME", "T2-pallets", "R0-no-crate-publish-required"]),
	dict(labels=["INVALID-LABEL", "ANOTHER-BAD-LABEL"], error="validation_failed"),
	dict(labels=["T1-FRAME", "INVALID-LABEL", "D2-substantial"], error="validation_failed"),
	dict(fetch_error=True, error="Failed to fetch labels from repository"),
	dict(open=False, error="Cannot modify labels on merged PRs"),
	dict(expected=["T1-FRAME"]),
	dict(org=False, author=False, error="Only the PR author or organization members"),
	dict(org=False, author=True, expected=["T1-FRAME"]),
	dict(org=True, author=False, expected=["T1-FRAME"]),
], ids=["valid", "typos", "prefix", "invalid", "mixed", "fetch-failure", "merged",
	"open", "unauthorized", "author", "org-member"])
def test_labels(cmd, monkeypatch, capsys, case):
	for name, value in dict(PR_NUM="123", IS_ORG_MEMBER=str(case.get("org", True)).lower(),
		IS_PR_AUTHOR=str(case.get("author", False)).lower()).items():
		monkeypatch.setenv(name, value)
	monkeypatch.setattr(cmd, "check_pr_status", Mock(return_value=case.get("open", True)))
	labels = Mock(return_value=case.get("allowed", LABELS))
	if case.get("fetch_error"):
		labels.side_effect = RuntimeError("Failed to fetch labels from repository")
	monkeypatch.setattr(cmd, "get_allowed_labels", labels)
	if "error" in case:
		with pytest.raises(SystemExit) as error:
			run(cmd, "label", labels=case.get("labels", ["T1-FRAME"]))
		assert error.value.code == 1
	else:
		run(cmd, "label", labels=case.get("labels", ["T1-FRAME"]))
	output = capsys.readouterr().out
	prefix = "ERROR_JSON: " if "error" in case else "LABELS_JSON: "
	payload = json.loads(next(line[len(prefix):] for line in output.splitlines()
		if line.startswith(prefix)))
	if "error" in case:
		assert payload["error"] == "validation_failed"
		assert case["error"] in json.dumps(payload)
	else:
		assert payload["labels"] == case["expected"]
		if case.get("corrected"):
			assert "Auto-corrected" in output


def test_auto_correct_labels(cmd):
	valid = LABELS + ["I2-bug"]
	corrections, suggestions = cmd.auto_correct_labels(["T1-FRAM"], valid)
	assert corrections == [("T1-FRAM", "T1-FRAME")]
	corrections, suggestions = cmd.auto_correct_labels(["TOTALLY-WRONG"], valid)
	assert corrections == []
	assert len(suggestions) == 1


def test_find_closest_labels(cmd):
	valid = ["T1-FRAME", "T2-pallets", "R0-no-crate-publish-required"]
	assert "T1-FRAME" in cmd.find_closest_labels("T1-FRAM", valid)
	assert cmd.find_closest_labels("COMPLETELY-DIFFERENT", valid, cutoff=0.8) == []


@pytest.mark.parametrize("exists", [False, True], ids=["new-directory", "existing-directory"])
def test_logging_setup(cmd, exists):
	with patch.object(cmd.os.path, "exists", return_value=exists), \
		patch.object(cmd.os, "makedirs") as mkdir, \
		patch("builtins.open", mock_open()) as opening:
		cmd.setup_logging()
		assert mkdir.call_count == (0 if exists else 1)
		if not exists:
			mkdir.assert_called_once_with("/tmp/cmd")
		opening.assert_called_once_with("/tmp/cmd/command_output.log", "w")
