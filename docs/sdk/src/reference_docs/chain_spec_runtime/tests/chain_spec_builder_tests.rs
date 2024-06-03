use serde_json::{json, Value};
use std::{process::Command, str};

const WASM_FILE_PATH: &str =
	"../../../../../target/release/wbuild/chain-spec-guide-runtime/chain_spec_guide_runtime.wasm";

const CHAIN_SPEC_BUILDER_PATH: &str = "../../../../../target/release/chain-spec-builder";

#[test]
#[docify::export]
fn list_presets() {
	let output = Command::new(CHAIN_SPEC_BUILDER_PATH)
		.arg("list-presets")
		.arg("-r")
		.arg(WASM_FILE_PATH)
		.output()
		.expect("Failed to execute command");

	let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

	let expected_output = json!({
		"presets":[
			"preset_1",
			"preset_2",
			"preset_3",
			"preset_4"
		]
	});
	assert_eq!(output, expected_output, "Output did not match expected");
}

#[test]
#[docify::export]
fn get_preset() {
	let output = Command::new(CHAIN_SPEC_BUILDER_PATH)
		.arg("display-preset")
		.arg("-r")
		.arg(WASM_FILE_PATH)
		.arg("-p")
		.arg("preset_1")
		.output()
		.expect("Failed to execute command");

	let output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

	//note: copy of chain_spec_guide_runtime::preset_1
	let expected_output = json!({
		"bar": {
			"initialAccount": "5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL",
		},
		"foo": {
			"someEnum": {
				"Data2": {
					"v": "0x0c0f"
				}
			},
			"someInteger": 100
		},
	});
	assert_eq!(output, expected_output, "Output did not match expected");
}

#[test]
#[docify::export]
fn generate_chain_spec() {
	let output = Command::new(CHAIN_SPEC_BUILDER_PATH)
		.arg("-c")
		.arg("/dev/stdout")
		.arg("create")
		.arg("-r")
		.arg(WASM_FILE_PATH)
		.arg("named-preset")
		.arg("preset_1")
		.output()
		.expect("Failed to execute command");

	let mut output: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

	//remove code field for better readability
	if let Some(code) = output["genesis"]["runtimeGenesis"].as_object_mut().unwrap().get_mut("code")
	{
		*code = Value::String("0x123".to_string());
	}

	let expected_output = json!({
	  "name": "Custom",
	  "id": "custom",
	  "chainType": "Live",
	  "bootNodes": [],
	  "telemetryEndpoints": null,
	  "protocolId": null,
	  "properties": null,
	  "codeSubstitutes": {},
	  "genesis": {
		"runtimeGenesis": {
		  "code": "0x123",
		  "patch": {
			"bar": {
			  "initialAccount": "5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL"
			},
			"foo": {
			  "someEnum": {
				"Data2": {
				  "v": "0x0c0f"
				}
			  },
			  "someInteger": 100
			}
		  }
		}
	  }
	});
	assert_eq!(output, expected_output, "Output did not match expected");
}
