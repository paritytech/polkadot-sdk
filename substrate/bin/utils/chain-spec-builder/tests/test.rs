// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::fs::File;

use clap::Parser;

use cmd_lib::spawn_with_output;
use pretty_assertions::assert_eq;
use sc_chain_spec::update_code_in_json_chain_spec;
use serde_json::{from_reader, from_str, Value};
use staging_chain_spec_builder::ChainSpecBuilder;

// note: the runtime path will not be read, runtime code will be set directly, to avoid hassle with
// creating the wasm file or providing a valid existing path during test execution.
const DUMMY_PATH: &str = "fake-runtime-path";

const OUTPUT_FILE: &str = "/tmp/chain_spec_builder.test_output_file.json";

const BOOTNODE_0: &str =
	"/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf";
const BOOTNODE_1: &str =
	"/dns/node-1.example.com/tcp/30333/p2p/12D3KooWAb5MyC1UJiEQJk4Hg4B2Vi3AJdqSUhTGYUqSnEqCFMFg";
const BOOTNODE_2: &str =
	"/ip4/198.51.100.19/tcp/30333/p2p/12D3KooWAdyiVAaeGdtBt6vn5zVetwA4z4qfm9Fi2QCSykN1wTBJ";

// Used for running commands visually pleasing in doc tests.
macro_rules! bash(
	( chain-spec-builder $($a:tt)* ) => {{
		let bin_path = env!("CARGO_BIN_EXE_chain-spec-builder");
		spawn_with_output!(
			$bin_path $($a)*
		)
		.expect("a process running. qed")
		.wait_with_output()
		.expect("to get output. qed.")
	}}
);

// Used specifically in docs tests.
fn doc_assert(output: String, expected_output_path: &str, remove_code: bool) {
	let expected: Value =
		from_reader(File::open(expected_output_path).unwrap()).expect("a valid JSON. qed.");
	let output = if remove_code {
		let mut output: Value = from_str(output.as_str()).expect("a valid JSON. qed.");
		// Remove code sections gracefully for both `plain` & `raw`.
		output
			.get_mut("genesis")
			.and_then(|inner| inner.get_mut("runtimeGenesis"))
			.and_then(|inner| inner.as_object_mut())
			.and_then(|inner| inner.remove("code"));
		output
			.get_mut("genesis")
			.and_then(|inner| inner.get_mut("raw"))
			.and_then(|inner| inner.get_mut("top"))
			.and_then(|inner| inner.as_object_mut())
			.and_then(|inner| inner.remove("0x3a636f6465"));
		output
	} else {
		from_str::<Value>(output.as_str()).expect("a valid JSON. qed.")
	};
	assert_eq!(output, expected);
}

/// Asserts that the JSON in output file matches the JSON in expected file.
///
/// This helper function reads the JSON content from the file at `OUTPUT_FILE + suffix` path. If the
/// `overwrite_code` flag is set, it updates the output chain specification with a sample code
/// vector `[1, 2, 3]` (to avoid bulky *expected* files), and then compares it against the JSON
/// content from the given `expected_path`.
fn assert_output_eq_expected(overwrite_code: bool, output_suffix: &str, expected_path: &str) {
	let path = OUTPUT_FILE.to_string() + output_suffix;
	let mut output: serde_json::Value =
		serde_json::from_reader(File::open(path.clone()).unwrap()).unwrap();
	if overwrite_code {
		update_code_in_json_chain_spec(&mut output, &vec![1, 2, 3]);
	}
	let expected: serde_json::Value =
		serde_json::from_reader(File::open(expected_path).unwrap()).unwrap();

	assert_eq!(expected, output);

	std::fs::remove_file(path).expect("Failed to delete file");
}

/// Asserts that the JSON in output file matches the JSON in expected file, except for the boot
/// nodes, which shall match the given ones.
///
/// The boot nodes commands shall not alter any other part of the chain specification, so the
/// expected file is the chain specification they were given as an input.
fn assert_output_eq_expected_with_bootnodes(
	overwrite_code: bool,
	output_suffix: &str,
	expected_path: &str,
	bootnodes: &[&str],
) {
	let path = OUTPUT_FILE.to_string() + output_suffix;
	let mut output: serde_json::Value =
		serde_json::from_reader(File::open(path.clone()).unwrap()).unwrap();
	if overwrite_code {
		update_code_in_json_chain_spec(&mut output, &vec![1, 2, 3]);
	}
	let mut expected: serde_json::Value =
		serde_json::from_reader(File::open(expected_path).unwrap()).unwrap();
	expected["bootNodes"] = serde_json::json!(bootnodes);

	assert_eq!(expected, output);

	std::fs::remove_file(path).expect("Failed to delete file");
}

fn get_builder(suffix: &str, command_args: Vec<&str>) -> ChainSpecBuilder {
	let path = OUTPUT_FILE.to_string() + suffix;
	let mut base_args = vec!["dummy", "-c", path.as_str()];
	base_args.extend(command_args);
	ChainSpecBuilder::parse_from(base_args)
}

#[test]
fn test_create_default() {
	const SUFFIX: &str = "00";
	let mut builder = get_builder(SUFFIX, vec!["create", "-r", DUMMY_PATH, "default"]);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_default.json");
}

#[test]
fn test_create_with_named_preset() {
	const SUFFIX: &str = "01";
	let mut builder =
		get_builder(SUFFIX, vec!["create", "-r", DUMMY_PATH, "named-preset", "staging"]);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_with_named_preset.json");
}

#[test]
fn test_create_with_patch() {
	const SUFFIX: &str = "02";
	let mut builder =
		get_builder(SUFFIX, vec!["create", "-r", DUMMY_PATH, "patch", "tests/input/patch.json"]);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_with_patch.json");
}

#[test]
fn test_create_with_full() {
	const SUFFIX: &str = "03";
	let mut builder =
		get_builder(SUFFIX, vec!["create", "-r", DUMMY_PATH, "full", "tests/input/full.json"]);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_with_full.json");
}

#[test]
fn test_create_with_params() {
	const SUFFIX: &str = "04";
	let mut builder = get_builder(
		SUFFIX,
		vec!["create", "-r", DUMMY_PATH, "-n", "test_chain", "-i", "100", "-t", "live", "default"],
	);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_with_params.json");
}

#[test]
fn test_create_parachain() {
	const SUFFIX: &str = "05";
	let mut builder = get_builder(
		SUFFIX,
		vec![
			"create",
			"-r",
			DUMMY_PATH,
			"-n",
			"test_chain",
			"-i",
			"100",
			"-t",
			"live",
			"--relay-chain",
			"rococo-local",
			"default",
		],
	);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_parachain.json");
}

#[test]
fn test_create_raw_storage() {
	const SUFFIX: &str = "06";
	let mut builder = get_builder(
		SUFFIX,
		vec!["create", "-r", DUMMY_PATH, "-s", "patch", "tests/input/patch.json"],
	);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_raw_storage.json");
}

#[test]
fn test_update_code() {
	const SUFFIX: &str = "07";
	let builder = get_builder(
		SUFFIX,
		vec!["update-code", "tests/input/chain_spec_plain.json", "tests/input/code_040506.blob"],
	);
	builder.run().unwrap();
	assert_output_eq_expected(false, SUFFIX, "tests/expected/update_code.json");
}

#[test]
fn test_update_code_raw() {
	const SUFFIX: &str = "08";
	let builder = get_builder(
		SUFFIX,
		vec!["update-code", "tests/input/chain_spec_raw.json", "tests/input/code_040506.blob"],
	);
	builder.run().unwrap();
	assert_output_eq_expected(false, SUFFIX, "tests/expected/update_code_raw.json");
}

#[test]
fn test_convert_to_raw() {
	const SUFFIX: &str = "09";
	let builder =
		get_builder(SUFFIX, vec!["convert-to-raw", "tests/input/chain_spec_conversion_test.json"]);
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/convert_to_raw.json");
}

#[test]
fn test_add_code_substitute() {
	const SUFFIX: &str = "10";
	let builder = get_builder(
		SUFFIX,
		vec![
			"add-code-substitute",
			"tests/input/chain_spec_plain.json",
			"tests/input/code_040506.blob",
			"100",
		],
	);
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/add_code_substitute.json");
}

#[test]
fn test_create_with_properties() {
	const SUFFIX: &str = "11";
	let mut builder = get_builder(
		SUFFIX,
		vec![
			"create",
			"-r",
			DUMMY_PATH,
			"--properties",
			"tokenSymbol=TEST,tokenDecimals=6",
			"--properties",
			"isEthereum=false",
			"--properties",
			"ss58Prefix=42",
			"default",
		],
	);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected(true, SUFFIX, "tests/expected/create_with_properties.json");
}

#[test]
fn test_add_bootnodes() {
	const SUFFIX: &str = "12";
	let builder = get_builder(
		SUFFIX,
		vec!["bootnodes", "add", "tests/input/chain_spec_plain.json", BOOTNODE_0, BOOTNODE_1],
	);
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		false,
		SUFFIX,
		"tests/input/chain_spec_plain.json",
		&[BOOTNODE_0, BOOTNODE_1],
	);
}

#[test]
fn test_add_bootnodes_raw() {
	const SUFFIX: &str = "13";
	let builder = get_builder(
		SUFFIX,
		vec!["bootnodes", "add", "tests/input/chain_spec_raw.json", BOOTNODE_0],
	);
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		false,
		SUFFIX,
		"tests/input/chain_spec_raw.json",
		&[BOOTNODE_0],
	);
}

#[test]
fn test_add_bootnodes_appends_and_skips_the_existing_ones() {
	const SUFFIX: &str = "14";
	// `BOOTNODE_1` is already stored, so only `BOOTNODE_2` is appended, at the end of the list.
	let builder = get_builder(
		SUFFIX,
		vec![
			"bootnodes",
			"add",
			"tests/input/chain_spec_plain_with_bootnodes.json",
			BOOTNODE_1,
			BOOTNODE_2,
		],
	);
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		false,
		SUFFIX,
		"tests/input/chain_spec_plain_with_bootnodes.json",
		&[BOOTNODE_0, BOOTNODE_1, BOOTNODE_2],
	);
}

#[test]
fn test_remove_bootnodes() {
	const SUFFIX: &str = "15";
	// `BOOTNODE_2` is not stored in the given chain spec, it shall be ignored.
	let builder = get_builder(
		SUFFIX,
		vec![
			"bootnodes",
			"remove",
			"tests/input/chain_spec_plain_with_bootnodes.json",
			BOOTNODE_0,
			BOOTNODE_2,
		],
	);
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		false,
		SUFFIX,
		"tests/input/chain_spec_plain_with_bootnodes.json",
		&[BOOTNODE_1],
	);
}

#[test]
fn test_remove_all_bootnodes() {
	const SUFFIX: &str = "16";
	let builder = get_builder(
		SUFFIX,
		vec!["bootnodes", "remove", "tests/input/chain_spec_plain_with_bootnodes.json", "--all"],
	);
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		false,
		SUFFIX,
		"tests/input/chain_spec_plain_with_bootnodes.json",
		&[],
	);
}

#[test]
fn test_create_with_bootnodes() {
	const SUFFIX: &str = "17";
	let bootnodes = format!("{BOOTNODE_0},{BOOTNODE_1}");
	let mut builder = get_builder(
		SUFFIX,
		vec!["create", "-r", DUMMY_PATH, "--bootnodes", bootnodes.as_str(), "default"],
	);
	builder.set_create_cmd_runtime_code(substrate_test_runtime::WASM_BINARY.unwrap().into());
	builder.run().unwrap();
	assert_output_eq_expected_with_bootnodes(
		true,
		SUFFIX,
		"tests/expected/create_default.json",
		&[BOOTNODE_0, BOOTNODE_1],
	);
}

#[test]
fn test_bootnodes_shall_contain_the_peer_id() {
	assert!(ChainSpecBuilder::try_parse_from(vec![
		"dummy",
		"bootnodes",
		"add",
		"tests/input/chain_spec_plain.json",
		"/dns/node-0.example.com/tcp/30333",
	])
	.is_err());
}

#[test]
fn test_bootnodes_command_is_also_available_in_the_singular() {
	assert!(ChainSpecBuilder::try_parse_from(vec![
		"dummy",
		"bootnode",
		"list",
		"tests/input/chain_spec_plain.json",
	])
	.is_ok());
}

#[docify::export_content]
fn cmd_create_default(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create -r $runtime_path default
	)
}

#[test]
fn create_default() {
	doc_assert(
		cmd_create_default(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_default.json",
		true,
	);
}

#[docify::export_content]
fn cmd_display_default_preset(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder display-preset -r $runtime_path
	)
}

#[test]
fn display_default_preset() {
	doc_assert(
		cmd_display_default_preset(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed."),
		),
		"tests/expected/doc/display_preset.json",
		false,
	);
}

#[docify::export]
fn cmd_display_preset(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder display-preset -r $runtime_path -p "staging"
	)
}

#[test]
fn display_preset() {
	doc_assert(
		cmd_display_preset(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/display_preset_staging.json",
		false,
	);
}

#[docify::export_content]
fn cmd_list_presets(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder list-presets -r $runtime_path
	)
}

#[test]
fn list_presets() {
	doc_assert(
		cmd_list_presets(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/list_presets.json",
		false,
	);
}

#[docify::export_content]
fn cmd_create_with_named_preset(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create --relay-chain "dev" -r $runtime_path named-preset "staging"
	)
}

#[test]
fn create_with_named_preset() {
	doc_assert(
		cmd_create_with_named_preset(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_with_named_preset_staging.json",
		true,
	)
}

#[docify::export_content]
fn cmd_create_with_patch_raw(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create -s -r $runtime_path patch "tests/input/patch.json"
	)
}

#[test]
fn create_with_patch_raw() {
	doc_assert(
		cmd_create_with_patch_raw(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_with_patch_raw.json",
		true,
	);
}

#[docify::export_content]
fn cmd_create_with_patch_plain(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create -r $runtime_path patch "tests/input/patch.json"
	)
}

#[test]
fn create_with_patch_plain() {
	doc_assert(
		cmd_create_with_patch_plain(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_with_patch_plain.json",
		true,
	);
}

#[docify::export_content]
fn cmd_create_full_plain(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create -r $runtime_path full "tests/input/full.json"
	)
}

#[test]
fn create_full_plain() {
	doc_assert(
		cmd_create_full_plain(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_full_plain.json",
		true,
	);
}

#[docify::export_content]
fn cmd_create_full_raw(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" create -s -r $runtime_path full "tests/input/full.json"
	)
}

#[test]
fn create_full_raw() {
	doc_assert(
		cmd_create_full_raw(
			substrate_test_runtime::WASM_BINARY_PATH.expect("to be a valid path. qed"),
		),
		"tests/expected/doc/create_full_raw.json",
		true,
	);
}

#[docify::export_content]
fn cmd_add_bootnodes(chain_spec_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" bootnodes add $chain_spec_path "/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf"
	)
}

#[test]
fn add_bootnodes() {
	doc_assert(
		cmd_add_bootnodes("tests/input/chain_spec_plain.json"),
		"tests/expected/doc/add_bootnodes.json",
		false,
	);
}

#[docify::export_content]
fn cmd_remove_bootnodes(chain_spec_path: &str) -> String {
	bash!(
		chain-spec-builder -c "/dev/stdout" bootnodes remove $chain_spec_path "/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf"
	)
}

#[test]
fn remove_bootnodes() {
	doc_assert(
		cmd_remove_bootnodes("tests/input/chain_spec_plain_with_bootnodes.json"),
		"tests/expected/doc/remove_bootnodes.json",
		false,
	);
}

#[docify::export_content]
fn cmd_list_bootnodes(chain_spec_path: &str) -> String {
	bash!(
		chain-spec-builder bootnodes list $chain_spec_path
	)
}

#[test]
fn list_bootnodes() {
	doc_assert(
		cmd_list_bootnodes("tests/input/chain_spec_plain_with_bootnodes.json"),
		"tests/expected/doc/list_bootnodes.json",
		false,
	);
}
