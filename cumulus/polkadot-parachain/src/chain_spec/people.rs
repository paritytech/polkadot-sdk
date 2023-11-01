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
use parachains_common::Balance as PeopleBalance;
use sc_chain_spec::ChainSpec;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported People configurations.
#[derive(Debug, PartialEq)]
pub enum PeopleRuntimeType {
	Kusama,
	KusamaLocal,
	KusamaDevelopment, // used by benchmarks

	Polkadot,
	PolkadotLocal,
	PolkadotDevelopment, // used by benchmarks

	Rococo,
	Westend,
}

impl FromStr for PeopleRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			polkadot::PEOPLE_POLKADOT => Ok(PeopleRuntimeType::Polkadot),
			polkadot::PEOPLE_POLKADOT_LOCAL => Ok(PeopleRuntimeType::PolkadotLocal),
			polkadot::PEOPLE_POLKADOT_DEVELOPMENT => Ok(PeopleRuntimeType::PolkadotDevelopment),
			kusama::PEOPLE_KUSAMA => Ok(PeopleRuntimeType::Kusama),
			kusama::PEOPLE_KUSAMA_LOCAL => Ok(PeopleRuntimeType::KusamaLocal),
			kusama::PEOPLE_KUSAMA_DEVELOPMENT => Ok(PeopleRuntimeType::KusamaDevelopment),
			rococo::PEOPLE_ROCOCO => Ok(PeopleRuntimeType::Rococo),
			westend::PEOPLE_WESTEND => Ok(PeopleRuntimeType::Westend),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl PeopleRuntimeType {
	pub const ID_PREFIX: &'static str = "people";

	pub fn chain_spec_from_json_file(&self, path: PathBuf) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			PeopleRuntimeType::Polkadot |
			PeopleRuntimeType::PolkadotLocal |
			PeopleRuntimeType::PolkadotDevelopment =>
				Ok(Box::new(polkadot::PeopleChainSpec::from_json_file(path)?)),
			PeopleRuntimeType::Kusama |
			PeopleRuntimeType::KusamaLocal |
			PeopleRuntimeType::KusamaDevelopment =>
				Ok(Box::new(kusama::PeopleChainSpec::from_json_file(path)?)),
			PeopleRuntimeType::Rococo =>
				Ok(Box::new(rococo::PeopleChainSpec::from_json_file(path)?)),
			PeopleRuntimeType::Westend =>
				Ok(Box::new(westend::PeopleChainSpec::from_json_file(path)?)),
		}
	}

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			PeopleRuntimeType::Polkadot =>
				Ok(Box::new(polkadot::PeopleChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/people-polkadot.json")[..],
				)?)),
			PeopleRuntimeType::PolkadotLocal => Ok(Box::new(polkadot::local_config(
				polkadot::PEOPLE_POLKADOT_LOCAL,
				"Polkadot People Local",
				"polkadot-local",
				ParaId::new(1010),
			))),
			PeopleRuntimeType::PolkadotDevelopment => Ok(Box::new(polkadot::local_config(
				polkadot::PEOPLE_POLKADOT_DEVELOPMENT,
				"Polkadot People Development",
				"polkadot-dev",
				ParaId::new(1010),
			))),
			PeopleRuntimeType::Kusama => Ok(Box::new(kusama::PeopleChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/people-kusama.json")[..],
			)?)),
			PeopleRuntimeType::KusamaLocal => Ok(Box::new(kusama::local_config(
				kusama::PEOPLE_KUSAMA_LOCAL,
				"Kusama People Local",
				"kusama-local",
				ParaId::new(1010),
			))),
			PeopleRuntimeType::KusamaDevelopment => Ok(Box::new(kusama::local_config(
				kusama::PEOPLE_KUSAMA_DEVELOPMENT,
				"Kusama People Development",
				"kusama-dev",
				ParaId::new(1010),
			))),
			PeopleRuntimeType::Rococo => Ok(Box::new(rococo::PeopleChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/people-rococo.json")[..],
			)?)),
			PeopleRuntimeType::Westend => Ok(Box::new(westend::PeopleChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/people-westend.json")[..],
			)?)),
		}
	}
}

/// Check if `id` satisfies People-like format.
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(PeopleRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			PeopleRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Rococo setup.
pub mod rococo {
	use super::{ParaId, PeopleBalance};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{constants::rococo_currency, AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const PEOPLE_ROCOCO: &str = "people-rococo";
	const PEOPLE_ROCOCO_ED: PeopleBalance = rococo_currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type PeopleChainSpec =
		sc_service::GenericChainSpec<people_rococo_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = people_rococo_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> PeopleChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());

		PeopleChainSpec::from_genesis(
			// Name
			chain_name,
			// ID
			super::ensure_id(id).expect("invalid id"),
			ChainType::Local,
			move || {
				genesis(
					// initial collators.
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
						get_account_id_from_seed::<sr25519::Public>("Charlie"),
						get_account_id_from_seed::<sr25519::Public>("Dave"),
						get_account_id_from_seed::<sr25519::Public>("Eve"),
						get_account_id_from_seed::<sr25519::Public>("Ferdie"),
						get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
						get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
						get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
						get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
						get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
						get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
					],
					para_id,
				)
			},
			Vec::new(),
			None,
			None,
			None,
			Some(properties),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
	}

	fn genesis(
		invulnerables: Vec<(AccountId, AuraId)>,
		endowed_accounts: Vec<AccountId>,
		id: ParaId,
	) -> people_rococo_runtime::RuntimeGenesisConfig {
		people_rococo_runtime::RuntimeGenesisConfig {
			system: people_rococo_runtime::SystemConfig {
				code: people_rococo_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!")
					.to_vec(),
				..Default::default()
			},
			balances: people_rococo_runtime::BalancesConfig {
				balances: endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, PEOPLE_ROCOCO_ED * 4096))
					.collect(),
			},
			parachain_info: people_rococo_runtime::ParachainInfoConfig {
				parachain_id: id,
				..Default::default()
			},
			collator_selection: people_rococo_runtime::CollatorSelectionConfig {
				invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
				candidacy_bond: PEOPLE_ROCOCO_ED * 16,
				..Default::default()
			},
			session: people_rococo_runtime::SessionConfig {
				keys: invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                 // account id
							acc,                                         // validator id
							people_rococo_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect(),
			},
			aura: Default::default(),
			aura_ext: Default::default(),
			parachain_system: Default::default(),
			polkadot_xcm: people_rococo_runtime::PolkadotXcmConfig {
				safe_xcm_version: Some(SAFE_XCM_VERSION),
				..Default::default()
			},
		}
	}
}

/// Sub-module for Westend setup.
pub mod westend {
	use super::{ParaId, PeopleBalance};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{constants::westend_currency, AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const PEOPLE_WESTEND: &str = "people-westend";
	const PEOPLE_WESTEND_ED: PeopleBalance = westend_currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type PeopleChainSpec =
		sc_service::GenericChainSpec<people_westend_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = people_westend_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> PeopleChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		PeopleChainSpec::from_genesis(
			// Name
			chain_name,
			// ID
			super::ensure_id(id).expect("invalid id"),
			ChainType::Local,
			move || {
				genesis(
					// initial collators.
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
						get_account_id_from_seed::<sr25519::Public>("Charlie"),
						get_account_id_from_seed::<sr25519::Public>("Dave"),
						get_account_id_from_seed::<sr25519::Public>("Eve"),
						get_account_id_from_seed::<sr25519::Public>("Ferdie"),
						get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
						get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
						get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
						get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
						get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
						get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
					],
					para_id,
				)
			},
			Vec::new(),
			None,
			None,
			None,
			Some(properties),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
	}

	fn genesis(
		invulnerables: Vec<(AccountId, AuraId)>,
		endowed_accounts: Vec<AccountId>,
		id: ParaId,
	) -> people_westend_runtime::RuntimeGenesisConfig {
		people_westend_runtime::RuntimeGenesisConfig {
			system: people_westend_runtime::SystemConfig {
				code: people_westend_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!")
					.to_vec(),
				..Default::default()
			},
			balances: people_westend_runtime::BalancesConfig {
				balances: endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, PEOPLE_WESTEND_ED * 4096))
					.collect(),
			},
			parachain_info: people_westend_runtime::ParachainInfoConfig {
				parachain_id: id,
				..Default::default()
			},
			collator_selection: people_westend_runtime::CollatorSelectionConfig {
				invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
				candidacy_bond: PEOPLE_WESTEND_ED * 16,
				..Default::default()
			},
			session: people_westend_runtime::SessionConfig {
				keys: invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                  // account id
							acc,                                          // validator id
							people_westend_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect(),
			},
			aura: Default::default(),
			aura_ext: Default::default(),
			parachain_system: Default::default(),
			polkadot_xcm: people_westend_runtime::PolkadotXcmConfig {
				safe_xcm_version: Some(SAFE_XCM_VERSION),
				..Default::default()
			},
		}
	}
}
