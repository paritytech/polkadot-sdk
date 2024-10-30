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
use sc_chain_spec::update_code_in_json_chain_spec;
use serde_json::{from_reader, from_str, Value};
use staging_chain_spec_builder::ChainSpecBuilder;
use std::{collections::HashMap, sync::LazyLock};

// note: the runtime path will not be read, runtime code will be set directly, to avoid hassle with
// creating the wasm file or providing a valid existing path during test execution.
const DUMMY_PATH: &str = "fake-runtime-path";

const OUTPUT_FILE: &str = "/tmp/chain_spec_builder.test_output_file.json";

const SUBSTRATE_TEST_RUNTIME_PATH: &str =
	unwrap_option_str(substrate_test_runtime::WASM_BINARY_PATH);

const COMMANDS: LazyLock<HashMap<&str, (&str, bool)>> = LazyLock::new(|| {
	[(
			r#"chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH default"#,
			("tests/expected/doc/create_default.json", true),
		),
		(
			"chain-spec-builder display-preset -r $SUBSTRATE_TEST_RUNTIME_PATH",
			("tests/expected/doc/display_preset.json", false),
		),
		(
			r#"chain-spec-builder display-preset -r $SUBSTRATE_TEST_RUNTIME_PATH -p "staging""#,
			("tests/expected/doc/display_preset_staging.json", false),
		),
		(
			"chain-spec-builder list-presets -r $SUBSTRATE_TEST_RUNTIME_PATH",
			("tests/expected/doc/list_presets.json", false),
		),
		(
			r#"chain-spec-builder -c "/dev/stdout" create --relay-chain "dev" --para-id 1000 -r $SUBSTRATE_TEST_RUNTIME_PATH named-preset "staging""#,
			("tests/expected/doc/create_with_named_preset_staging.json", true)
		),
		(
			r#"chain-spec-builder -c "/dev/stdout" create -s -r $SUBSTRATE_TEST_RUNTIME_PATH patch "tests/input/patch.json""#,
			("tests/expected/doc/create_with_patch_raw.json", false)
		),
		(
		    r#"chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH patch "tests/input/patch.json""#,
			("tests/expected/doc/create_with_patch_plain.json", true),
		),
		(
			r#"chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH full "tests/input/full.json""#,
			("tests/expected/doc/create_full_plain.json", true),
		),
		(
			r#"chain-spec-builder -c "/dev/stdout" create -s -r $SUBSTRATE_TEST_RUNTIME_PATH full "tests/input/full.json""#,
			("tests/expected/doc/create_full_raw.json", false),
	)]
	.iter()
	.cloned()
	.collect()
});

// Used for running commands visually pleasing in doc tests.
macro_rules! exe(
	( chain-spec-builder $($a:tt)* ) => {{
		let bin_path = env!("CARGO_BIN_EXE_chain-spec-builder");
		let mut cmd = bin_path.to_string();
		$(
			cmd.push(' ');
			cmd.push_str(stringify!($a));
		)*

		let output = spawn_with_output!(
			$bin_path $($a)*
		)
		.expect("a process running. qed")
		.wait_with_output()
		.expect("to get output. qed.");
		if COMMANDS.contains_key(cmd.as_str()) {
			doc_assert(output, COMMANDS[cmd.as_str()].0, COMMANDS[cmd.as_str()].1);
		}
	}}
);

pub const fn unwrap_option_str(opt: Option<&'static str>) -> &'static str {
	match opt {
		Some(val) => val,
		None => panic!("Expected a value, but found None. qed."),
	}
}

// Used specifically in docs tests.
fn doc_assert(output: String, expected_output_path: &str, remove_code: bool) {
	let expected: Value =
		from_reader(File::open(expected_output_path).unwrap()).expect("a valid JSON. qed.");
	let output = if remove_code {
		let mut output: Value = from_str(output.as_str()).expect("a valid JSON. qed.");
		output
			.get_mut("genesis")
			.unwrap()
			.get_mut("runtimeGenesis")
			.unwrap()
			.as_object_mut()
			.unwrap()
			.remove("code");
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
			"--para-id",
			"10101",
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
#[docify::export]
fn cmd_create_default() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH default
	);
}

#[test]
#[docify::export]
fn cmd_display_default_preset() {
	exe!(
		// Example
		chain-spec-builder display-preset -r $SUBSTRATE_TEST_RUNTIME_PATH
	);
}

#[test]
#[docify::export]
fn cmd_display_preset() {
	exe!(
		// Example
		chain-spec-builder display-preset -r $SUBSTRATE_TEST_RUNTIME_PATH -p "staging"
	);
}

#[test]
#[docify::export]
fn cmd_list_presets() {
	exe!(
		// Example
		chain-spec-builder list-presets -r $SUBSTRATE_TEST_RUNTIME_PATH
	);
}

#[test]
#[docify::export]
fn cmd_create_with_named_preset() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create --relay-chain "dev" --para-id 1000 -r $SUBSTRATE_TEST_RUNTIME_PATH named-preset "staging"
	);
}

#[test]
#[docify::export]
fn cmd_create_with_patch_raw() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create -s -r $SUBSTRATE_TEST_RUNTIME_PATH patch "tests/input/patch.json"
	);
}

#[test]
#[docify::export]
fn cmd_create_with_patch_plain() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH patch "tests/input/patch.json"
	);
}

#[test]
#[docify::export]
fn cmd_create_full_plain() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create -r $SUBSTRATE_TEST_RUNTIME_PATH full "tests/input/full.json"
	);
}

#[test]
#[docify::export]
fn cmd_create_full_raw() {
	exe!(
		// Example
		chain-spec-builder -c "/dev/stdout" create -s -r $SUBSTRATE_TEST_RUNTIME_PATH full "tests/input/full.json"
	);
}
