// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate chain configurations.

use primitives::{Ed25519AuthorityId as AuthorityId, ed25519};
use node_primitives::AccountId;
use node_runtime::{ConsensusConfig, CouncilSeatsConfig, CouncilVotingConfig, DemocracyConfig,
	SessionConfig, StakingConfig, TimestampConfig, BalancesConfig, TreasuryConfig,
	SudoConfig, ContractConfig, GrandpaConfig, IndicesConfig, FeesConfig, Permill, Perbill};
pub use node_runtime::GenesisConfig;
use substrate_service;
use hex_literal::{hex, hex_impl};

use substrate_keystore::pad_seed;
use substrate_telemetry::TelemetryEndpoints;

const STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";

/// Specialised `ChainSpec`.
pub type ChainSpec = substrate_service::ChainSpec<GenesisConfig>;

/// Dried Danta testnet generator
pub fn dried_danta_config() -> Result<ChainSpec, String> {
	ChainSpec::from_embedded(include_bytes!("../res/dried-danta.json"))
}

fn staging_testnet_config_genesis() -> GenesisConfig {
	// stash, controller, session-key
	let initial_authorities: Vec<(AccountId, AccountId, AuthorityId)> = vec![(
		hex!["fbecf7767fc63a6f9fa8094bbc5751d7269cd8e619cfdd9edfbe1fbc716b173e"].into(),	// 5Hm2GcbuUct7sWX8d56zRktxr9D9Lw5hTFjSUhUoVHwFNmYW TODO: change once we switch to sr25519
		hex!["6ed35e632190b9c795f019030e6c5cff1508655db28c83577e0a4366c9bd5773"].into(),	// 5Ea1uyGz6H5WHZhWvPDxxLXWyiUkzWDwx54Hcn8LJ5dbFawH TODO: change once we switch to sr25519
		hex!["82c39b31a2b79a90f8e66e7a77fdb85a4ed5517f2ae39f6a80565e8ecae85cf5"].into(),
	),(
		hex!["30b76ef977b84a575992ef52f561db315221123c68074269d3d51ce211c4a3dc"].into(),	// 5DAaeTwVuyUmTyLBR5vKEDWeDJ75nhLutDuCJH58it7EHDM2 TODO: change once we switch to sr25519
		hex!["a270edf24cb2a472b0e913fc43bfd4da0ef337cc715eaf94073d5198f7659f0c"].into(),	// 5FjhAKgzpuzt1dYWE7H7Jb1sEHSuG5hcyZdPtfX829gmFVXh TODO: change once we switch to sr25519
		hex!["4de37a07567ebcbf8c64568428a835269a566723687058e017b6d69db00a77e7"].into(),
	),(
		hex!["7b9e79c1bfc71ad0c4389565c01e79269dc512cb9bd856489671662481355417"].into(),	// 5ErnpkRUbmM3WdbQwnVwfZeYs3iKmggEQceyB9db9ft18dSn TODO: change once we switch to sr25519
		hex!["9ffec660c4d328306cf5e38faf4b132fb5c9f38287af95d9b25629fc29de3945"].into(),	// 5FgV9vxNpdCXMUmHCLQcsN4mUUUG6ZpFuvAMrm5X4BUnFhie TODO: change once we switch to sr25519
		hex!["063d7787ebca768b7445dfebe7d62cbb1625ff4dba288ea34488da266dd6dca5"].into(),
	),(
		hex!["7e58b096b95c4b3b271f27fedd9f2c51edd48b9d37046240e601180c9dcc8c27"].into(),	// 5EvNEhYYd4b9giczuCo2o8bfLZoKW9jnTeUukfL1NWsAAeEx TODO: change once we switch to sr25519
		hex!["36dfc933bb0848d8addf16a961369b2e122633a5819a19e43c8142381a1280e3"].into(),	// 5DJevPKpz4EEvmSpK7W6KemS3i5JYPq5FEuEewgRY2cZCxNg TODO: change once we switch to sr25519
		hex!["8101764f45778d4980dadaceee6e8af2517d3ab91ac9bec9cd1714fa5994081c"].into(),
	)];
	let endowed_accounts: Vec<AccountId> = vec![
		hex!["f295940fa750df68a686fcf4abd4111c8a9c5a5a5a83c4c8639c451a94a7adfd"].into(),	// 5HYmsxGRAmZMjyZYmf7uGPL2YDQGHEt6NjGrfUuxNEgeGBRN TODO: change once we switch to sr25519
	];
	const MILLICENTS: u128 = 1_000_000_000;
	const CENTS: u128 = 1_000 * MILLICENTS;    // assume this is worth about a cent.
	const DOLLARS: u128 = 100 * CENTS;

	const SECS_PER_BLOCK: u64 = 6;
	const MINUTES: u64 = 60 / SECS_PER_BLOCK;
	const HOURS: u64 = MINUTES * 60;
	const DAYS: u64 = HOURS * 24;

	const ENDOWMENT: u128 = 10_000_000 * DOLLARS;
	const STASH: u128 = 100 * DOLLARS;

	GenesisConfig {
		consensus: Some(ConsensusConfig {
			code: include_bytes!("../../runtime/wasm/target/wasm32-unknown-unknown/release/node_runtime.compact.wasm").to_vec(),    // FIXME change once we have #1252
			authorities: initial_authorities.iter().map(|x| x.2.clone()).collect(),
		}),
		system: None,
		balances: Some(BalancesConfig {
			balances: endowed_accounts.iter()
				.map(|&k| (k, ENDOWMENT))
				.chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
				.collect(),
			existential_deposit: 1 * DOLLARS,
			transfer_fee: 1 * CENTS,
			creation_fee: 1 * CENTS,
			vesting: vec![],
		}),
		indices: Some(IndicesConfig {
			ids: endowed_accounts.iter().cloned()
				.chain(initial_authorities.iter().map(|x| x.0.clone()))
				.collect::<Vec<_>>(),
		}),
		session: Some(SessionConfig {
			validators: initial_authorities.iter().map(|x| x.1.into()).collect(),
			session_length: 5 * MINUTES,
			keys: initial_authorities.iter().map(|x| (x.1.clone(), x.2.clone())).collect::<Vec<_>>(),
		}),
		staking: Some(StakingConfig {
			current_era: 0,
			offline_slash: Perbill::from_billionths(1_000_000),
			session_reward: Perbill::from_billionths(2_065),
			current_offline_slash: 0,
			current_session_reward: 0,
			validator_count: 7,
			sessions_per_era: 12,
			bonding_duration: 60 * MINUTES,
			offline_slash_grace: 4,
			minimum_validator_count: 4,
			stakers: initial_authorities.iter().map(|x| (x.0.into(), x.1.into(), STASH)).collect(),
			invulnerables: initial_authorities.iter().map(|x| x.1.into()).collect(),
		}),
		democracy: Some(DemocracyConfig {
			launch_period: 10 * MINUTES,    // 1 day per public referendum
			voting_period: 10 * MINUTES,    // 3 days to discuss & vote on an active referendum
			minimum_deposit: 50 * DOLLARS,    // 12000 as the minimum deposit for a referendum
			public_delay: 10 * MINUTES,
			max_lock_periods: 6,
		}),
		council_seats: Some(CouncilSeatsConfig {
			active_council: vec![],
			candidacy_bond: 10 * DOLLARS,
			voter_bond: 1 * DOLLARS,
			present_slash_per_voter: 1 * CENTS,
			carry_count: 6,
			presentation_duration: 1 * DAYS,
			approval_voting_period: 2 * DAYS,
			term_duration: 28 * DAYS,
			desired_seats: 0,
			inactive_grace_period: 1,    // one additional vote should go by before an inactive voter can be reaped.
		}),
		council_voting: Some(CouncilVotingConfig {
			cooloff_period: 4 * DAYS,
			voting_period: 1 * DAYS,
			enact_delay_period: 0,
		}),
		timestamp: Some(TimestampConfig {
			period: SECS_PER_BLOCK / 2, // due to the nature of aura the slots are 2*period
		}),
		treasury: Some(TreasuryConfig {
			proposal_bond: Permill::from_percent(5),
			proposal_bond_minimum: 1 * DOLLARS,
			spend_period: 1 * DAYS,
			burn: Permill::from_percent(50),
		}),
		contract: Some(ContractConfig {
			contract_fee: 1 * CENTS,
			call_base_fee: 1000,
			create_base_fee: 1000,
			gas_price: 1 * MILLICENTS,
			max_depth: 1024,
			block_gas_limit: 10_000_000,
			current_schedule: Default::default(),
		}),
		sudo: Some(SudoConfig {
			key: endowed_accounts[0].clone(),
		}),
		grandpa: Some(GrandpaConfig {
			authorities: initial_authorities.iter().map(|x| (x.2.clone(), 1)).collect(),
		}),
		fees: Some(FeesConfig {
			transaction_base_fee: 1 * CENTS,
			transaction_byte_fee: 10 * MILLICENTS,
		}),
	}
}

/// Staging testnet config.
pub fn staging_testnet_config() -> ChainSpec {
	let boot_nodes = vec![];
	ChainSpec::from_genesis(
		"Staging Testnet",
		"staging_testnet",
		staging_testnet_config_genesis,
		boot_nodes,
		Some(TelemetryEndpoints::new(vec![(STAGING_TELEMETRY_URL.to_string(), 0)])),
		None,
		None,
		None,
	)
}

/// Helper function to generate AuthorityID from seed
pub fn get_account_id_from_seed(seed: &str) -> AccountId {
	let padded_seed = pad_seed(seed);
	// NOTE from ed25519 impl:
	// prefer pkcs#8 unless security doesn't matter -- this is used primarily for tests.
	ed25519::Pair::from_seed(&padded_seed).public().0.into()
}

/// Helper function to generate stash, controller and session key from seed
pub fn get_authority_keys_from_seed(seed: &str) -> (AccountId, AccountId, AuthorityId) {
	let padded_seed = pad_seed(seed);
	// NOTE from ed25519 impl:
	// prefer pkcs#8 unless security doesn't matter -- this is used primarily for tests.
	(
		get_account_id_from_seed(&format!("{}-stash", seed)),
		get_account_id_from_seed(seed),
		ed25519::Pair::from_seed(&padded_seed).public().0.into()
	)
}

/// Helper function to create GenesisConfig for testing
pub fn testnet_genesis(
	initial_authorities: Vec<(AccountId, AccountId, AuthorityId)>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
) -> GenesisConfig {
	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(|| {
		vec![
			get_account_id_from_seed("Alice"),
			get_account_id_from_seed("Bob"),
			get_account_id_from_seed("Charlie"),
			get_account_id_from_seed("Dave"),
			get_account_id_from_seed("Eve"),
			get_account_id_from_seed("Ferdie"),
		]
	});

	const STASH: u128 = 1 << 20;
	const ENDOWMENT: u128 = 1 << 20;

	GenesisConfig {
		consensus: Some(ConsensusConfig {
			code: include_bytes!("../../runtime/wasm/target/wasm32-unknown-unknown/release/node_runtime.compact.wasm").to_vec(),
			authorities: initial_authorities.iter().map(|x| x.2.clone()).collect(),
		}),
		system: None,
		indices: Some(IndicesConfig {
			ids: endowed_accounts.clone(),
		}),
		balances: Some(BalancesConfig {
			existential_deposit: 500,
			transfer_fee: 0,
			creation_fee: 0,
			balances: endowed_accounts.iter().map(|&k| (k.into(), ENDOWMENT)).collect(),
			vesting: vec![],
		}),
		session: Some(SessionConfig {
			validators: initial_authorities.iter().map(|x| x.1.into()).collect(),
			session_length: 10,
			keys: initial_authorities.iter().map(|x| (x.1.clone(), x.2.clone())).collect::<Vec<_>>(),
		}),
		staking: Some(StakingConfig {
			current_era: 0,
			minimum_validator_count: 1,
			validator_count: 2,
			sessions_per_era: 5,
			bonding_duration: 2 * 60 * 12,
			offline_slash: Perbill::zero(),
			session_reward: Perbill::zero(),
			current_offline_slash: 0,
			current_session_reward: 0,
			offline_slash_grace: 0,
			stakers: initial_authorities.iter().map(|x| (x.0.into(), x.1.into(), STASH)).collect(),
			invulnerables: initial_authorities.iter().map(|x| x.1.into()).collect(),
		}),
		democracy: Some(DemocracyConfig {
			launch_period: 9,
			voting_period: 18,
			minimum_deposit: 10,
			public_delay: 0,
			max_lock_periods: 6,
		}),
		council_seats: Some(CouncilSeatsConfig {
			active_council: endowed_accounts.iter()
				.filter(|&endowed| initial_authorities.iter().find(|&(_, controller, _)| controller == endowed).is_none())
				.map(|a| (a.clone().into(), 1000000)).collect(),
			candidacy_bond: 10,
			voter_bond: 2,
			present_slash_per_voter: 1,
			carry_count: 4,
			presentation_duration: 10,
			approval_voting_period: 20,
			term_duration: 1000000,
			desired_seats: (endowed_accounts.len() - initial_authorities.len()) as u32,
			inactive_grace_period: 1,
		}),
		council_voting: Some(CouncilVotingConfig {
			cooloff_period: 75,
			voting_period: 20,
			enact_delay_period: 0,
		}),
		timestamp: Some(TimestampConfig {
			period: 2,                    // 2*2=4 second block time.
		}),
		treasury: Some(TreasuryConfig {
			proposal_bond: Permill::from_percent(5),
			proposal_bond_minimum: 1_000_000,
			spend_period: 12 * 60 * 24,
			burn: Permill::from_percent(50),
		}),
		contract: Some(ContractConfig {
			contract_fee: 21,
			call_base_fee: 135,
			create_base_fee: 175,
			gas_price: 1,
			max_depth: 1024,
			block_gas_limit: 10_000_000,
			current_schedule: Default::default(),
		}),
		sudo: Some(SudoConfig {
			key: root_key,
		}),
		grandpa: Some(GrandpaConfig {
			authorities: initial_authorities.iter().map(|x| (x.2.clone(), 1)).collect(),
		}),
		fees: Some(FeesConfig {
			transaction_base_fee: 1,
			transaction_byte_fee: 0,
		}),
	}
}

fn development_config_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
		],
		get_account_id_from_seed("Alice").into(),
		None,
	)
}

/// Development config (single validator Alice)
pub fn development_config() -> ChainSpec {
	ChainSpec::from_genesis("Development", "dev", development_config_genesis, vec![], None, None, None, None)
}

fn local_testnet_genesis() -> GenesisConfig {
	testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
			get_authority_keys_from_seed("Bob"),
		],
		get_account_id_from_seed("Alice").into(),
		None,
	)
}

/// Local testnet config (multivalidator Alice + Bob)
pub fn local_testnet_config() -> ChainSpec {
	ChainSpec::from_genesis("Local Testnet", "local_testnet", local_testnet_genesis, vec![], None, None, None, None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use service_test;
	use crate::service::Factory;

	fn local_testnet_genesis_instant() -> GenesisConfig {
		let mut genesis = local_testnet_genesis();
		genesis.timestamp = Some(TimestampConfig { period: 0 });
		genesis
	}

	/// Local testnet config (multivalidator Alice + Bob)
	pub fn integration_test_config() -> ChainSpec {
		ChainSpec::from_genesis("Integration Test", "test", local_testnet_genesis_instant, vec![], None, None, None, None)
	}

	#[test]
	fn test_connectivity() {
		service_test::connectivity::<Factory>(integration_test_config());
	}
}
