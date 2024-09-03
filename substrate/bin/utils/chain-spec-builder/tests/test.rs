use std::fs::File;

use clap::Parser;
use sc_chain_spec::update_code_in_json_chain_spec;
use staging_chain_spec_builder::ChainSpecBuilder;

const RUNTIME_PATH: &str =
	"../../../../target/release/wbuild/substrate-test-runtime/substrate_test_runtime.wasm";

const OUTPUT_FILE: &str = "/tmp/chain_spec_builder.test_output_file.json";

/// Asserts that the JSON in output file matches the JSON in expected file.
///
/// This helper function reads the JSON content from the `OUTPUT_FILE`, updates the output chain
/// specification with a sample code vector `[1, 2, 3]` (to avoid bulky *expected* files), and
/// then compares it against the JSON content from the given `expected_path`.
fn assert_output_eq_expected(output_suffix: &str, expected_path: &str) {
	let path = OUTPUT_FILE.to_string() + output_suffix;
	let mut output: serde_json::Value =
		serde_json::from_reader(File::open(path.clone()).unwrap()).unwrap();

	update_code_in_json_chain_spec(&mut output, &vec![1, 2, 3]);
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
	sp_tracing::try_init_simple();
	let builder = get_builder(SUFFIX, vec!["create", "-r", RUNTIME_PATH, "default"]);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_default.json");
}

#[test]
fn test_create_with_named_preset() {
	const SUFFIX: &str = "01";
	sp_tracing::try_init_simple();
	let builder =
		get_builder(SUFFIX, vec!["create", "-r", RUNTIME_PATH, "named-preset", "staging"]);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_with_named_preset.json");
}

#[test]
fn test_create_with_patch() {
	const SUFFIX: &str = "02";
	sp_tracing::try_init_simple();
	let builder =
		get_builder(SUFFIX, vec!["create", "-r", RUNTIME_PATH, "patch", "tests/input/patch.json"]);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_with_patch.json");
}

#[test]
fn test_create_with_full() {
	const SUFFIX: &str = "03";
	sp_tracing::try_init_simple();
	let builder =
		get_builder(SUFFIX, vec!["create", "-r", RUNTIME_PATH, "full", "tests/input/full.json"]);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_with_full.json");
}

#[test]
fn test_create_with_params() {
	const SUFFIX: &str = "04";
	sp_tracing::try_init_simple();
	let builder = get_builder(
		SUFFIX,
		vec![
			"create",
			"-r",
			RUNTIME_PATH,
			"-n",
			"test_chain",
			"-i",
			"100",
			"-t",
			"live",
			"default",
		],
	);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_with_params.json");
}

#[test]
fn test_create_parachain() {
	const SUFFIX: &str = "05";
	sp_tracing::try_init_simple();
	let builder = get_builder(
		SUFFIX,
		vec![
			"create",
			"-r",
			RUNTIME_PATH,
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
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_parachain.json");
}

#[test]
fn test_create_raw_storage() {
	const SUFFIX: &str = "06";
	sp_tracing::try_init_simple();
	let builder = get_builder(
		SUFFIX,
		vec!["create", "-r", RUNTIME_PATH, "-s", "patch", "tests/input/patch.json"],
	);
	builder.run().unwrap();
	assert_output_eq_expected(SUFFIX, "tests/expected/create_raw_storage.json");
}
