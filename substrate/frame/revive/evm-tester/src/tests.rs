use crate::{
	cli::Args,
	executor::{execute_revive_state_test, execute_revm_state_test},
};
use revm_statetest_types::{SpecName, TestSuite};
use serde_json;

#[test]
fn execute_state_test_with_revm() {
	let json = include_str!("test.json");
	let test_suite: TestSuite = serde_json::from_str(json).expect("Failed to parse test JSON");
	let (test_name, test_case) = test_suite.0.iter().next().expect("No test case found");
	let post_states = test_case.post.get(&SpecName::Prague).expect("Prague fork not found");
	let post_state = &post_states[0];

	let args = Args {
		test_file: None,
		fork: None,
		index: -1,
		run: ".*".to_string(),
		bench: false,
		dump: false,
		human: false,
	};

	let result = execute_revm_state_test(test_name, test_case, "Prague", 0, post_state, &args)
		.expect("Failed to execute state test");

	dbg!(&result);

	assert!(result.pass, "State test should pass, got: {:?}", result.error);
	assert_eq!(result.name, "tests/prague/eip2537_bls_12_381_precompiles/test_bls12_g1mul.py::test_valid[fork_Prague-state_test-bls_g1mul_(0*g1=inf)-]");
	assert_eq!(result.fork, "Prague");
	assert!(result.error.is_none(), "Should have no error: {:?}", result.error);
}

#[test]
fn execute_state_test_with_revive() {
	use env_logger::Env;
	use std::io::Write;
	env_logger::Builder::from_env(Env::default())
		.format(|buf, record| {
			writeln!(
				buf,
				"[{} {}:{}] {} - {}",
				record.level(),
				record.file().unwrap_or("unknown"),
				record.line().unwrap_or(0),
				record.target(),
				record.args()
			)
		})
		.init();
	// env_logger::init();
	let json = include_str!("test.json");
	let test_suite: TestSuite = serde_json::from_str(json).expect("Failed to parse test JSON");
	let (test_name, test_case) = test_suite.0.iter().next().expect("No test case found");
	let post_states = test_case.post.get(&SpecName::Prague).expect("Prague fork not found");
	let post_state = &post_states[0];

	let args = Args {
		test_file: None,
		fork: None,
		index: -1,
		run: ".*".to_string(),
		bench: false,
		dump: false,
		human: false,
	};

	let result = execute_revive_state_test(test_name, test_case, post_state, &args)
		.expect("Failed to execute state test");

	assert!(result.pass, "State test should pass, got: {:?}", result.error);
	assert_eq!(result.name, "tests/prague/eip2537_bls_12_381_precompiles/test_bls12_g1mul.py::test_valid[fork_Prague-state_test-bls_g1mul_(0*g1=inf)-]");
	assert_eq!(result.fork, "Prague");
	assert!(result.error.is_none(), "Should have no error: {:?}", result.error);
}
