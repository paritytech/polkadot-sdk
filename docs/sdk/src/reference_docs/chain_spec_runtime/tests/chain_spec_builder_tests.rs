use cmd_lib::*;
use serde_json::{json, Value};
use std::str;

fn wasm_file_path() -> &'static str {
	chain_spec_guide_runtime::runtime::WASM_BINARY_PATH
		.expect("chain_spec_guide_runtime wasm should exist. qed")
}

const CHAIN_SPEC_BUILDER_PATH: &str = "../../../../../target/release/chain-spec-builder";

macro_rules! bash(
	( chain-spec-builder $x:tt, $($a:tt)* ) => {{
		let path = get_chain_spec_builder_path();
		let x: serde_json::Value = json!($x);
		let output = spawn_with_output!(
			$path $($a)*
		)
		.expect("a process running. qed")
		.wait_with_output()
		.expect("to get output. qed.");
		let mut output0: serde_json::Value = serde_json::from_slice(&output.as_bytes()).unwrap();
		assert_eq!(output0, x, "Output did not match expected");
	}}
);

macro_rules! bash2(
	( chain-spec-builder $x:tt, $($a:tt)* ) => {{
		let path = get_chain_spec_builder_path();
		let x: serde_json::Value = json!($x);
		let output = spawn_with_output!(
			$path $($a)*
		)
		.expect("a process running. qed")
		.wait_with_output()
		.expect("to get output. qed.");
		let mut output0: serde_json::Value = serde_json::from_slice(&output.as_bytes()).unwrap();
		//remove code field for better readability
	if let Some(code) = output0["genesis"]["runtimeGenesis"].as_object_mut().unwrap().get_mut("code")
	{
		*code = Value::String("0x123".to_string());
	}
		assert_eq!(output0, x, "Output did not match expected");
	}}
);

fn get_chain_spec_builder_path() -> &'static str {
	run_cmd!(
		cargo build --release -p staging-chain-spec-builder --bin chain-spec-builder
	)
	.expect("Failed to execute command");
	CHAIN_SPEC_BUILDER_PATH
}

#[docify::export]
fn cmd_list_presets(path: &str) {
	let path = wasm_file_path();
	bash!(
		chain-spec-builder {
			"presets":[
				"preset_1",
				"preset_2",
				"preset_3",
				"preset_4",
				"preset_invalid"
			]
		}, list-presets -r $path
	);
}

#[test]
fn list_presets() {
	cmd_list_presets(wasm_file_path());
}

#[docify::export]
fn cmd_get_preset(path: &str) {
	bash!(
		chain-spec-builder {
			"bar": {
				"initialAccount": "5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL",
			},
			"foo": {
				"someEnum": {
					"Data2": {
						"values": "0x0c10"
					}
				},
				"someInteger": 200
			},
		}, display-preset -r $path -p preset_2
	);
}

#[test]
fn get_preset() {
	cmd_get_preset(wasm_file_path());
}

#[docify::export]
fn cmd_generate_chain_spec(path: &str) {
	bash2!(
		chain-spec-builder {
			"name": "Custom",
			"id": "custom",
			"chainType": "Live",
			"bootNodes": [],
			"telemetryEndpoints": null,
			"protocolId": null,
			"properties": { "tokenDecimals": 12, "tokenSymbol": "UNIT" },
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
						"values": "0x0c10"
					  }
					},
					"someInteger": 200
				  }
				}
			  }
			}
		  }, -c /dev/stdout create -r $path named-preset preset_2
	);
}

#[test]
fn generate_chain_spec() {
	cmd_generate_chain_spec(wasm_file_path());
}

#[docify::export]
fn cmd_generate_para_chain_spec(path: &str) {
	let path = wasm_file_path();
	bash2!(
		chain-spec-builder {
			"name": "Custom",
			"id": "custom",
			"chainType": "Live",
			"bootNodes": [],
			"telemetryEndpoints": null,
			"protocolId": null,
			"relay_chain": "polkadot",
			"para_id": 1000,
			"properties": { "tokenDecimals": 12, "tokenSymbol": "UNIT" },
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
						"values": "0x0c10"
					  }
					},
					"someInteger": 200
				  }
				}
			  }
			}}
		  , -c /dev/stdout create -c polkadot -p 1000 -r $path named-preset preset_2
	);
}

#[test]
fn generate_para_chain_spec() {
	cmd_generate_para_chain_spec(wasm_file_path());
}

#[test]
#[docify::export]
fn preset_4_json() {
	assert_eq!(
		chain_spec_guide_runtime::presets::preset_4(),
		json!({
			"foo": {
				"someEnum": {
					"Data2": {
						"values": "0x0c10"
					}
				},
			},
		})
	);
}
