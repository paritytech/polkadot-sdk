use codec::Decode;
use sp_version::RuntimeVersion;
use polkadot_sdk::*;
use minimal_template_runtime::WASM_BINARY;
use sc_service::ChainType;
use sc_chain_spec::{
    ChainSpecExtension,
    ChainSpecGroup,
    Properties,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use core::str;
use std::error::Error;
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


fn decode_runtime_info(spec_data: Value) -> 
    Result<(String, u32, u32), Box<dyn Error>> {
    let runtime_hx_cde = spec_data
        .get("genesis").expect("failed to get genesis")
        .get("runtimeGenesis").expect("failed to get runtimeGenesis")
        .get("code").expect("failed to get code")
        .as_str().expect("failed to turn to str");

    let clean_u8 = runtime_hx_cde.trim_start_matches("0x").as_bytes(); // normalize runtime hex cde remove '0x' and convert to bytes.
    let version_info: RuntimeVersion = RuntimeVersion::decode(&mut &clean_u8[..])
    .map_err(|_| "Failed to decode runtime version")?;

    let rt_spec_name = version_info.spec_name.into_owned();
    let rt_spec_version = version_info.spec_version;
    let rt_authoring_version = version_info.authoring_version;

    Ok((rt_spec_name, rt_spec_version, rt_authoring_version))
}

#[test]
fn test_minimal_dev_chain_spec_rt_validity() {
    let mut properties = Properties::new();
    properties.insert("tokenDecimals".to_string(), 12.into());
    properties.insert("tokenSymbol".to_string(), "UNIT".into());
    let current_wasm = WASM_BINARY.expect("Development wasm not available");

    let test_chain_spec: serde_json::Value = serde_json::from_str(ChainSpec::builder(
        current_wasm,
        Extensions {
            relay_chain: "dev".into(),
            para_id: 1000,
        })
        .with_name("test_Development")
        .with_id("dev")
        .with_chain_type(ChainType::Local)
        .with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
        .with_properties(properties.clone())
        .build()
        .as_json(false)
        .unwrap()
        .as_str()
    ).unwrap();

    let existing_chain_spec_file =
		std::fs::File::open("../dev_chain_spec.json").expect("file should open. qed");
	let existing_chain_spec_reader = BufReader::new(existing_chain_spec_file);
	let existing_chain_spec: serde_json::Value =
		serde_json::from_reader(existing_chain_spec_reader).expect("should read proper JSON. qed");

    let data1 = decode_runtime_info(existing_chain_spec).unwrap();
    let data2 = decode_runtime_info(test_chain_spec).unwrap(); 
	assert_eq!(data1, data2);
}

