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

//! Chain specifications for the test runtime.

use pallet_staking::Forcing;
use polkadot_primitives::{
	AccountId, AssignmentId, SchedulerParams, ValidatorId, MAX_CODE_SIZE, MAX_POV_SIZE,
};
use polkadot_service::chain_spec::Extensions;
use polkadot_test_runtime::BABE_GENESIS_EPOCH_CONFIG;
use sc_chain_spec::{ChainSpec, ChainType};
use sc_consensus_grandpa::AuthorityId as GrandpaId;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{crypto::get_public_from_string_or_panic, sr25519};
use sp_keyring::Sr25519Keyring;
use sp_runtime::Perbill;
use test_runtime_constants::currency::DOTS;

const DEFAULT_PROTOCOL_ID: &str = "dot";

/// The `ChainSpec` parameterized for polkadot test runtime.
pub type PolkadotChainSpec = sc_service::GenericChainSpec<Extensions>;

/// Returns the properties for the [`PolkadotChainSpec`].
pub fn polkadot_chain_spec_properties() -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"tokenDecimals": 10,
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn polkadot_local_testnet_config() -> PolkadotChainSpec {
	PolkadotChainSpec::builder(
		polkadot_test_runtime::WASM_BINARY.expect("Wasm binary must be built for testing"),
		Default::default(),
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(polkadot_local_testnet_genesis())
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.with_properties(polkadot_chain_spec_properties())
	.build()
}

/// Local testnet genesis config (multivalidator Alice + Bob)
pub fn polkadot_local_testnet_genesis() -> serde_json::Value {
	polkadot_testnet_genesis(
		vec![get_authority_keys_from_seed("Alice"), get_authority_keys_from_seed("Bob")],
		Sr25519Keyring::Alice.to_account_id(),
		None,
	)
}

/// Helper function to generate stash, controller and session key from seed
fn get_authority_keys_from_seed(
	seed: &str,
) -> (AccountId, AccountId, BabeId, GrandpaId, ValidatorId, AssignmentId, AuthorityDiscoveryId) {
	(
		get_public_from_string_or_panic::<sr25519::Public>(&format!("{}//stash", seed)).into(),
		get_public_from_string_or_panic::<sr25519::Public>(seed).into(),
		get_public_from_string_or_panic::<BabeId>(seed),
		get_public_from_string_or_panic::<GrandpaId>(seed),
		get_public_from_string_or_panic::<ValidatorId>(seed),
		get_public_from_string_or_panic::<AssignmentId>(seed),
		get_public_from_string_or_panic::<AuthorityDiscoveryId>(seed),
	)
}

fn testnet_accounts() -> Vec<AccountId> {
	Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect()
}

/// Helper function to create polkadot `RuntimeGenesisConfig` for testing
fn polkadot_testnet_genesis(
	initial_authorities: Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
	)>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
) -> serde_json::Value {
	use polkadot_test_runtime as runtime;

	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(testnet_accounts);

	const ENDOWMENT: u128 = 1_000_000 * DOTS;
	const STASH: u128 = 100 * DOTS;

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().map(|k| (k.clone(), ENDOWMENT)).collect::<Vec<_>>(),
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						runtime::SessionKeys {
							babe: x.2.clone(),
							grandpa: x.3.clone(),
							para_validator: x.4.clone(),
							para_assignment: x.5.clone(),
							authority_discovery: x.6.clone(),
						},
					)
				})
				.collect::<Vec<_>>(),
		},
		"staking": {
			"minimumValidatorCount": 1,
			"validatorCount": 2,
			"stakers": initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), STASH, runtime::StakerStatus::<AccountId>::Validator))
				.collect::<Vec<_>>(),
			"invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
			"forceEra": Forcing::NotForcing,
			"slashRewardFraction": Perbill::from_percent(10),
		},
		"babe": {
			"epochConfig": Some(BABE_GENESIS_EPOCH_CONFIG),
		},
		"sudo": { "key": Some(root_key) },
		"configuration": {
			"config": polkadot_runtime_parachains::configuration::HostConfiguration {
				validation_upgrade_cooldown: 10u32,
				validation_upgrade_delay: 5,
				code_retention_period: 1200,
				max_code_size: MAX_CODE_SIZE,
				max_pov_size: MAX_POV_SIZE,
				max_head_data_size: 32 * 1024,
				no_show_slots: 10,
				minimum_validation_upgrade_delay: 5,
				max_downward_message_size: 1024,
				scheduler_params: SchedulerParams {
					group_rotation_frequency: 20,
					paras_availability_period: 4,
					..Default::default()
				},
				..Default::default()
			},
		}
	})
}

/// Can be called for a `Configuration` to check if it is a configuration for the `Test` network.
pub trait IdentifyVariant {
	/// Returns if this is a configuration for the `Test` network.
	fn is_test(&self) -> bool;
}

impl IdentifyVariant for Box<dyn ChainSpec> {
	fn is_test(&self) -> bool {
		self.id().starts_with("test")
	}
}
