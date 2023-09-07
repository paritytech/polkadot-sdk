// Copyright Parity Technologies (UK) Ltd.
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

use cumulus_primitives_core::ParaId;
use sp_core::sr25519;

use super::{get_account_id_from_seed, get_collator_keys_from_seed};
use crate::chain_spec::{Extensions, SAFE_XCM_VERSION};
use parachains_common::{AccountId, AuraId};
use sc_chain_spec::ChainType;

const GLUTTON_ED: glutton_runtime::Balance = glutton_runtime::EXISTENTIAL_DEPOSIT;

/// Specialized `ChainSpec` for the Glutton parachain runtime.
pub type GluttonChainSpec =
	sc_service::GenericChainSpec<glutton_runtime::RuntimeGenesisConfig, Extensions>;

pub type RuntimeApi = glutton_runtime::RuntimeApi;

pub fn glutton_development_config(para_id: ParaId) -> GluttonChainSpec {
	GluttonChainSpec::from_genesis(
		// Name
		"Glutton Development",
		// ID
		"glutton_dev",
		ChainType::Local,
		move || {
			glutton_genesis(
				para_id,
				vec![(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Alice"),
				)],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				],
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "kusama-dev".into(), para_id: para_id.into() },
	)
}

pub fn glutton_local_config(para_id: ParaId) -> GluttonChainSpec {
	GluttonChainSpec::from_genesis(
		// Name
		"Glutton Local",
		// ID
		"glutton_local",
		ChainType::Local,
		move || {
			glutton_genesis(
				para_id,
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<AuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<AuraId>("Bob"),
					),
				],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "kusama-local".into(), para_id: para_id.into() },
	)
}

pub fn glutton_config(para_id: ParaId) -> GluttonChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	GluttonChainSpec::from_genesis(
		// Name
		format!("Glutton {}", para_id).as_str(),
		// ID
		format!("glutton-kusama-{}", para_id).as_str(),
		ChainType::Live,
		move || {
			glutton_genesis(
				para_id,
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<AuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<AuraId>("Bob"),
					),
				],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
			)
		},
		Vec::new(),
		None,
		// Protocol ID
		Some(format!("glutton-kusama-{}", para_id).as_str()),
		None,
		Some(properties),
		Extensions { relay_chain: "kusama".into(), para_id: para_id.into() },
	)
}

fn glutton_genesis(
	parachain_id: ParaId,
	collators: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
) -> glutton_runtime::RuntimeGenesisConfig {
	glutton_runtime::RuntimeGenesisConfig {
		system: glutton_runtime::SystemConfig {
			code: glutton_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
			..Default::default()
		},
		balances: glutton_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		parachain_info: glutton_runtime::ParachainInfoConfig { parachain_id, ..Default::default() },
		parachain_system: Default::default(),
		aura: Default::default(),
		aura_ext: Default::default(),
		glutton: glutton_runtime::GluttonConfig {
			compute: Default::default(),
			storage: Default::default(),
			trash_data_count: Default::default(),
			..Default::default()
		},
		session: glutton_runtime::SessionConfig {
			keys: collators
				.iter()
				.cloned()
				.map(|(acc, aura)| {
					(
						acc.clone(),                           // account id
						acc,                                   // validator id
						glutton_runtime::SessionKeys { aura }, // session keys
					)
				})
				.collect(),
		},
		collator_selection: glutton_runtime::CollatorSelectionConfig {
			invulnerables: collators.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: GLUTTON_ED * 16,
			..Default::default()
		},
		polkadot_xcm: glutton_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		},
		transaction_payment: Default::default(),
		sudo: glutton_runtime::SudoConfig {
			key: Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		},
	}
}
