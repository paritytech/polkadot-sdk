// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use sp_core::{Pair, Public, sr25519};
use bridge_node_runtime::{
	AccountId, AuraConfig, BalancesConfig, BridgeEthPoAConfig,
	GenesisConfig, GrandpaConfig, SudoConfig, SystemConfig,
	WASM_BINARY, Signature
};
use sp_consensus_aura::sr25519::{AuthorityId as AuraId};
use grandpa_primitives::{AuthorityId as GrandpaId};
use sc_service;
use sp_runtime::traits::{Verify, IdentifyAccount};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::ChainSpec<GenesisConfig>;

/// The chain specification option. This is expected to come in from the CLI and
/// is little more than one of a number of alternatives which can easily be converted
/// from a string (`--chain=...`) into a `ChainSpec`.
#[derive(Clone, Debug)]
pub enum Alternative {
	/// Whatever the current runtime is, with just Alice as an auth.
	Development,
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate an authority key for Aura
pub fn get_authority_keys_from_seed(s: &str) -> (AuraId, GrandpaId) {
	(
		get_from_seed::<AuraId>(s),
		get_from_seed::<GrandpaId>(s),
	)
}

impl Alternative {
	/// Get an actual chain config from one of the alternatives.
	pub(crate) fn load(self) -> Result<ChainSpec, String> {
		Ok(match self {
			Alternative::Development => ChainSpec::from_genesis(
				"Development",
				"dev",
				|| testnet_genesis(
					vec![
						get_authority_keys_from_seed("Alice"),
					],
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					vec![
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
						get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
					],
					true,
				),
				vec![],
				None,
				None,
				None,
				None
			),
		})
	}

	pub(crate) fn from(s: &str) -> Option<Self> {
		match s {
			"" | "dev" => Some(Alternative::Development),
			_ => None,
		}
	}
}

fn testnet_genesis(initial_authorities: Vec<(AuraId, GrandpaId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	_enable_println: bool) -> GenesisConfig {
	GenesisConfig {
		frame_system: Some(SystemConfig {
			code: WASM_BINARY.to_vec(),
			changes_trie_config: Default::default(),
		}),
		pallet_balances: Some(BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k|(k, 1 << 60)).collect(),
		}),
		pallet_aura: Some(AuraConfig {
			authorities: initial_authorities.iter().map(|x| (x.0.clone())).collect(),
		}),
		pallet_bridge_eth_poa: load_kovan_config(),
		pallet_grandpa: Some(GrandpaConfig {
			authorities: initial_authorities.iter().map(|x| (x.1.clone(), 1)).collect(),
		}),
		pallet_sudo: Some(SudoConfig {
			key: root_key,
		}),
	}
}

pub fn load_spec(id: &str) -> Result<Option<ChainSpec>, String> {
	Ok(match Alternative::from(id) {
		Some(spec) => Some(spec.load()?),
		None => None,
	})
}

fn load_kovan_config() -> Option<BridgeEthPoAConfig> {
	Some(BridgeEthPoAConfig {
			initial_header: sp_bridge_eth_poa::Header {
				parent_hash: Default::default(),
				timestamp: 0,
				number: 0,
				author: Default::default(),
				transactions_root: "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".parse().unwrap(),
				uncles_hash: "1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347".parse().unwrap(),
				extra_data: vec![],
				state_root: "2480155b48a1cea17d67dbfdfaafe821c1d19cdd478c5358e8ec56dec24502b2".parse().unwrap(),
				receipts_root: "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".parse().unwrap(),
				log_bloom: Default::default(),
				gas_used: Default::default(),
				gas_limit: 6000000.into(),
				difficulty: 131072.into(),
				seal: vec![
					vec![128].into(),
					vec![184, 65, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into(),
				],
			},
			initial_difficulty: 0.into(),
			initial_validators: vec![
				[0x00, 0xD6, 0xCc, 0x1B, 0xA9, 0xcf, 0x89, 0xBD, 0x2e, 0x58,
					0x00, 0x97, 0x41, 0xf4, 0xF7, 0x32, 0x5B, 0xAd, 0xc0, 0xED].into(),
				[0x00, 0x42, 0x7f, 0xea, 0xe2, 0x41, 0x9c, 0x15, 0xb8, 0x9d,
					0x1c, 0x21, 0xaf, 0x10, 0xd1, 0xb6, 0x65, 0x0a, 0x4d, 0x3d].into(),
				[0x4E, 0xd9, 0xB0, 0x8e, 0x63, 0x54, 0xC7, 0x0f, 0xE6, 0xF8,
					0xCB, 0x04, 0x11, 0xb0, 0xd3, 0x24, 0x6b, 0x42, 0x4d, 0x6c].into(),
				[0x00, 0x20, 0xee, 0x4B, 0xe0, 0xe2, 0x02, 0x7d, 0x76, 0x60,
					0x3c, 0xB7, 0x51, 0xeE, 0x06, 0x95, 0x19, 0xbA, 0x81, 0xA1].into(),
				[0x00, 0x10, 0xf9, 0x4b, 0x29, 0x6a, 0x85, 0x2a, 0xaa, 0xc5,
					0x2e, 0xa6, 0xc5, 0xac, 0x72, 0xe0, 0x3a, 0xfd, 0x03, 0x2d].into(),
				[0x00, 0x77, 0x33, 0xa1, 0xFE, 0x69, 0xCF, 0x3f, 0x2C, 0xF9,
					0x89, 0xF8, 0x1C, 0x7b, 0x4c, 0xAc, 0x16, 0x93, 0x38, 0x7A].into(),
				[0x00, 0xE6, 0xd2, 0xb9, 0x31, 0xF5, 0x5a, 0x3f, 0x17, 0x01,
					0xc7, 0x38, 0x9d, 0x59, 0x2a, 0x77, 0x78, 0x89, 0x78, 0x79].into(),
				[0x00, 0xe4, 0xa1, 0x06, 0x50, 0xe5, 0xa6, 0xD6, 0x00, 0x1C,
					0x38, 0xff, 0x8E, 0x64, 0xF9, 0x70, 0x16, 0xa1, 0x64, 0x5c].into(),
				[0x00, 0xa0, 0xa2, 0x4b, 0x9f, 0x0e, 0x5e, 0xc7, 0xaa, 0x4c,
					0x73, 0x89, 0xb8, 0x30, 0x2f, 0xd0, 0x12, 0x31, 0x94, 0xde].into(),
			],
		})
}
