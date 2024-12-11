// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Polkadot chain configurations.

#[cfg(feature = "rococo-native")]
use rococo_runtime as rococo;
use sc_chain_spec::ChainSpecExtension;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use sc_chain_spec::ChainType;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use sc_telemetry::TelemetryEndpoints;
use serde::{Deserialize, Serialize};
#[cfg(feature = "westend-native")]
use westend_runtime as westend;

#[cfg(feature = "westend-native")]
const WESTEND_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(feature = "rococo-native")]
const ROCOCO_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(feature = "rococo-native")]
const VERSI_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
const DEFAULT_PROTOCOL_ID: &str = "dot";

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<polkadot_primitives::Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<polkadot_primitives::Block>,
	/// The light sync state.
	///
	/// This value will be set by the `sync-state rpc` implementation.
	pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

// Generic chain spec, in case when we don't have the native runtime.
pub type GenericChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
#[cfg(feature = "westend-native")]
pub type WestendChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
// Dummy chain spec, but that is fine when we don't have the native runtime.
#[cfg(not(feature = "westend-native"))]
pub type WestendChainSpec = GenericChainSpec;

/// The `ChainSpec` parameterized for the rococo runtime.
#[cfg(feature = "rococo-native")]
pub type RococoChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the rococo runtime.
// Dummy chain spec, but that is fine when we don't have the native runtime.
#[cfg(not(feature = "rococo-native"))]
pub type RococoChainSpec = GenericChainSpec;

pub fn polkadot_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/polkadot.json")[..])
}

pub fn kusama_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/kusama.json")[..])
}

pub fn westend_config() -> Result<WestendChainSpec, String> {
	WestendChainSpec::from_json_bytes(&include_bytes!("../chain-specs/westend.json")[..])
}

pub fn paseo_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/paseo.json")[..])
}

pub fn rococo_config() -> Result<RococoChainSpec, String> {
	RococoChainSpec::from_json_bytes(&include_bytes!("../chain-specs/rococo.json")[..])
}

/// Westend staging testnet config.
#[cfg(feature = "westend-native")]
pub fn westend_staging_testnet_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::WASM_BINARY.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Westend Staging Testnet")
	.with_id("westend_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("staging_testnet")
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(WESTEND_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Westend Staging telemetry url is valid; qed"),
	)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo staging testnet config.
#[cfg(feature = "rococo-native")]
pub fn rococo_staging_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Rococo Staging Testnet")
	.with_id("rococo_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("staging_testnet")
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(ROCOCO_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Rococo Staging telemetry url is valid; qed"),
	)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

pub fn versi_chain_spec_properties() -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"ss58Format": 42,
		"tokenDecimals": 12,
		"tokenSymbol": "VRS",
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

/// Versi staging testnet config.
#[cfg(feature = "rococo-native")]
pub fn versi_staging_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Versi development wasm not available")?,
		Default::default(),
	)
	.with_name("Versi Staging Testnet")
	.with_id("versi_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("staging_testnet")
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(VERSI_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Versi Staging telemetry url is valid; qed"),
	)
	.with_protocol_id("versi")
	.with_properties(versi_chain_spec_properties())
	.build())
}

/// Westend development config (single validator Alice)
#[cfg(feature = "westend-native")]
pub fn westend_development_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::WASM_BINARY.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("westend_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo development config (single validator Alice)
#[cfg(feature = "rococo-native")]
pub fn rococo_development_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("rococo_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// `Versi` development config (single validator Alice)
#[cfg(feature = "rococo-native")]
pub fn versi_development_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Versi development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("versi_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_protocol_id("versi")
	.build())
}

/// Westend local testnet config (multivalidator Alice + Bob)
#[cfg(feature = "westend-native")]
pub fn westend_local_testnet_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::fast_runtime_binary::WASM_BINARY
			.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Westend Local Testnet")
	.with_id("westend_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo local testnet config (multivalidator Alice + Bob)
#[cfg(feature = "rococo-native")]
pub fn rococo_local_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::fast_runtime_binary::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Rococo Local Testnet")
	.with_id("rococo_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// `Versi` local testnet config (multivalidator Alice + Bob + Charlie + Dave)
#[cfg(feature = "rococo-native")]
pub fn versi_local_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm (used for versi) not available")?,
		Default::default(),
	)
	.with_name("Versi Local Testnet")
	.with_id("versi_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name("versi_local_testnet")
	.with_protocol_id("versi")
	.build())
}
