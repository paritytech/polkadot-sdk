// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::chain_spec::{get_account_id_from_seed, Extensions};
use cumulus_primitives_core::ParaId;
use parachains_common::AccountId;
use sc_service::ChainType;
use sp_core::sr25519;

/// Specialized `ChainSpec` for the seedling parachain runtime.
pub type SeedlingChainSpec =
	sc_service::GenericChainSpec<seedling_runtime::RuntimeGenesisConfig, Extensions>;

pub fn get_seedling_chain_spec() -> SeedlingChainSpec {
	SeedlingChainSpec::from_genesis(
		"Seedling Local Testnet",
		"seedling_local_testnet",
		ChainType::Local,
		move || {
			seedling_testnet_genesis(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				2000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend".into(), para_id: 2000 },
	)
}

fn seedling_testnet_genesis(
	root_key: AccountId,
	parachain_id: ParaId,
) -> seedling_runtime::RuntimeGenesisConfig {
	seedling_runtime::RuntimeGenesisConfig {
		system: seedling_runtime::SystemConfig {
			code: seedling_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
			..Default::default()
		},
		sudo: seedling_runtime::SudoConfig { key: Some(root_key) },
		parachain_info: seedling_runtime::ParachainInfoConfig {
			parachain_id,
			..Default::default()
		},
		parachain_system: Default::default(),
	}
}
