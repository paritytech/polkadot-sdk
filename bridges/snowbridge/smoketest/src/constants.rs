use hex_literal::hex;
use lazy_static::lazy_static;
use std::{env, string::ToString};

// Todo: load all configs from env in consistent with set-env.sh
pub const ASSET_HUB_PARA_ID: u32 = 1000;
pub const BRIDGE_HUB_PARA_ID: u32 = 1013;
pub const PENPAL_PARA_ID: u32 = 2000;

pub const ETHEREUM_API: &str = "ws://localhost:8546";
pub const ETHEREUM_HTTP_API: &str = "http://localhost:8545";

pub const ASSET_HUB_WS_URL: &str = "ws://127.0.0.1:12144";
pub const BRIDGE_HUB_WS_URL: &str = "ws://127.0.0.1:11144";
pub const PENPAL_WS_URL: &str = "ws://127.0.0.1:13144";
pub const RELAY_CHAIN_WS_URL: &str = "ws://127.0.0.1:9944";
pub const TEMPLATE_NODE_WS_URL: &str = "ws://127.0.0.1:13144";

pub const ETHEREUM_CHAIN_ID: u64 = 11155111;
pub const ETHEREUM_KEY: &str = "0x5e002a1af63fd31f1c25258f3082dc889762664cb8f218d86da85dff8b07b342";
pub const ETHEREUM_ADDRESS: [u8; 20] = hex!("90A987B944Cb1dCcE5564e5FDeCD7a54D3de27Fe");

// The deployment addresses of the following contracts are stable in our E2E env, unless we modify
// the order in contracts are deployed in DeployScript.sol.
pub const GATEWAY_PROXY_CONTRACT: [u8; 20] = hex!("EDa338E4dC46038493b885327842fD3E301CaB39");
pub const WETH_CONTRACT: [u8; 20] = hex!("87d1f7fdfEe7f651FaBc8bFCB6E086C278b77A7d");

// Agent for bridge hub parachain 1013
pub const BRIDGE_HUB_AGENT_ID: [u8; 32] =
	hex!("03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314");
// Agent for asset hub parachain 1000
pub const ASSET_HUB_AGENT_ID: [u8; 32] =
	hex!("72456f48efed08af20e5b317abf8648ac66e86bb90a411d9b0b713f7364b75b4");
// Agent for penpal parachain 2000
pub const SIBLING_AGENT_ID: [u8; 32] =
	hex!("5097ee1101e90c3aadb882858c59a22108668021ec81bce9f4930155e5c21e59");

pub const ASSET_HUB_SOVEREIGN: [u8; 32] =
	hex!("7369626ce8030000000000000000000000000000000000000000000000000000");
pub const SNOWBRIDGE_SOVEREIGN: [u8; 32] =
	hex!("ce796ae65569a670d0c1cc1ac12515a3ce21b5fbf729d63d7b289baad070139d");
pub const PENPAL_SOVEREIGN: [u8; 32] =
	hex!("7369626cd0070000000000000000000000000000000000000000000000000000");

// SS58: DE14BzQ1bDXWPKeLoAqdLAm1GpyAWaWF1knF74cEZeomTBM
pub const FERDIE: [u8; 32] =
	hex!("1cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c");

lazy_static! {
	pub static ref REGISTER_TOKEN_FEE: u64 = env::var("REGISTER_TOKEN_FEE")
		.unwrap_or("200000000000000000".to_string())
		.parse()
		.unwrap();
	pub static ref CREATE_ASSET_FEE: u128 = env::var("CREATE_ASSET_FEE")
		.unwrap_or("10000000000000".to_string())
		.parse()
		.unwrap();
	pub static ref RESERVE_TRANSFER_FEE: u128 = env::var("RESERVE_TRANSFER_FEE")
		.unwrap_or("20000000000".to_string())
		.parse()
		.unwrap();
	pub static ref EXCHANGE_RATE: u128 = env::var("EXCHANGE_RATE")
		.unwrap_or("2500000000000000".to_string())
		.parse()
		.unwrap();
	pub static ref FEE_PER_GAS: u64 =
		env::var("FEE_PER_GAS").unwrap_or("20000000000".to_string()).parse().unwrap();
	pub static ref LOCAL_REWARD: u128 =
		env::var("LOCAL_REWARD").unwrap_or("1000000000000".to_string()).parse().unwrap();
	pub static ref REMOTE_REWARD: u64 = env::var("REMOTE_REWARD")
		.unwrap_or("1000000000000000".to_string())
		.parse()
		.unwrap();
}
