use polkadot_sdk::*;
use minimal_template_runtime::WASM_BINARY;
use sc_service::ChainType;
use sc_chain_spec::{
    ChainSpecExtension,
    ChainSpecGroup,
    Properties,
};
use serde::{Deserialize, Serialize};
use core::str;
use std::io::BufReader;

pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
pub struct Extensions {
    #[serde(alias = "relayChain", alias = "RelayChain")]
    pub relay_chain: String,
    #[serde(alias = "paraId", alias = "ParaId")]
    pub para_id: u32,
}

impl Extensions {
    pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
        sc_chain_spec::get_extension(chain_spec.extensions())
    }
}

#[test]
fn test_minimal_dev_chain_spec_rt_validity() {
    let mut properties = Properties::new();
    properties.insert("tokenDecimals".to_string(), 12.into());
    properties.insert("tokenSymbol".to_string(), "UNIT".into());
    let current_wasm = WASM_BINARY.expect("Development wasm not available");

    let test_chain_spec: serde_json::Value = serde_json::from_str(&ChainSpec::builder(
        current_wasm,
        Extensions {
            relay_chain: "dev".into(),
            para_id: 1000,
        })
        .with_name("Custom")
        .with_id("custom")
        .with_chain_type(ChainType::Live)
        .with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
        .with_properties(properties.clone())
        .build()
        .as_json(false)
        .unwrap()).unwrap();

    let existing_chain_spec_file =
		std::fs::File::open("../dev_chain_spec.json").expect("file should open. qed");
	let existing_chain_spec_reader = BufReader::new(existing_chain_spec_file);
	let existing_chain_spec: serde_json::Value =
		serde_json::from_reader(existing_chain_spec_reader).expect("should read proper JSON. qed");

    assert_eq!(existing_chain_spec, test_chain_spec);
}

