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

fn decode_runtime_info(ch_spec_data: &ChainSpec) -> 
    Result<(String, u32, u32), Box<dyn Error>> {
    let chain_spec_as_json = ch_spec_data.as_json(false)
        .expect("Failed to serialize existing chain spec");
    let json_value: Value = serde_json::from_str(&chain_spec_as_json)?; // Deserialize the chain spec JSON string into a usable serde_Json Value object.

    let runtime_hx_cde = json_value
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
    ).unwrap();

    let existing_chain_spec = ChainSpec::from_json_file(
            "../dev_chain_spec.json".into()
        ).expect("failed to find development chain spec");

    let test_chain_data:(String, u32, u32) = decode_runtime_info(&test_chain_spec).expect("failed to retrieve test chain runtime info");
    let current_chain_data:(String, u32, u32) = decode_runtime_info(&existing_chain_spec).expect("failed to retrieve current chain runtime info");

    assert_eq!(test_chain_data, current_chain_data);
}

