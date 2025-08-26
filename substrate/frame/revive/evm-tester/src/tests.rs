use crate::{cli::Args, executor::execute_state_test};
use revm_statetest_types::{SpecName, TestSuite as StateTestSuite};
use serde_json;

#[test]
fn test_execute_state_test() {
	let json = r#"{
        "test1": {
            "env": {
                "currentCoinbase": "b94f5374fce5edbc8e2a8697c15331677e6ebf0b",
                "currentDifficulty": "0x200000",
                "currentGasLimit": "0x26e1f476fe1e22",
                "currentNumber": "0x1",
                "currentTimestamp": "0x3e8",
                "previousHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "currentBaseFee": "0x10"
            },
            "pre": {
                "0x00000000000000000000000000000000000000f1": {
                    "code": "0x4660015500",
                    "storage": {},
                    "balance": "0x0",
                    "nonce": "0x0"
                },
                "0xa94f5374fce5edbc8e2a8697c15331677e6ebf0b": {
                    "code": "0x",
                    "storage": {},
                    "balance": "0xffffffffff",
                    "nonce": "0x0"
                }
            },
            "transaction": {
                "sender": "0xa94f5374fce5edbc8e2a8697c15331677e6ebf0b",
                "gasPrice": "0x10",
                "nonce": "0x0",
                "to": "0x00000000000000000000000000000000000000f1",
                "data": ["0x"],
                "gasLimit": ["0xb9a0b"],
                "value": ["0x01"],
                "secretKey": "0x45a915e4d060149eb4365960e6a7a45f334393093061116b197e3240065ff2d8"
            },
            "out": "0x",
            "post": {
                "London": [{
                    "hash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "logs": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "indexes": {
                        "data": 0,
                        "gas": 0,
                        "value": 0
                    }
                }]
            }
        }
    }"#;

	// Parse the test suite
	let test_suite: StateTestSuite = serde_json::from_str(json).expect("Failed to parse test JSON");
	let (test_name, test_case) = test_suite.0.iter().next().expect("No test case found");

	// Get the London fork post state
	let post_states = test_case.post.get(&SpecName::London).expect("London fork not found");
	let post_state = &post_states[0];

	// Create test args
	let args = Args {
		test_file: None,
		fork: None,
		index: -1,
		run: ".*".to_string(),
		bench: false,
		dump: false,
		human: false,
	};

	// Execute the state test
	let result = execute_state_test(test_name, test_case, "London", 0, post_state, &args)
		.expect("Failed to execute state test");

	// Verify the result
	assert!(result.pass, "State test should pass, got: {:?}", result.error);
	assert_eq!(result.name, "test1");
	assert_eq!(result.fork, "London");
	assert!(result.error.is_none(), "Should have no error: {:?}", result.error);
}
