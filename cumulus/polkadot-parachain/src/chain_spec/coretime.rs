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
use parachains_common::Balance as CoretimeBalance;
use sc_chain_spec::ChainSpec;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq)]
pub enum CoretimeRuntimeType {
	Kusama,
	KusamaLocal,
	KusamaDevelopment, // used by benchmarks

	Polkadot,
	PolkadotLocal,
	PolkadotDevelopment, // used by benchmarks

	Westend,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			polkadot::CORETIME_POLKADOT => Ok(CoretimeRuntimeType::Polkadot),
			polkadot::CORETIME_POLKADOT_LOCAL => Ok(CoretimeRuntimeType::PolkadotLocal),
			polkadot::CORETIME_POLKADOT_DEVELOPMENT => Ok(CoretimeRuntimeType::PolkadotDevelopment),
			kusama::CORETIME_KUSAMA => Ok(CoretimeRuntimeType::Kusama),
			kusama::CORETIME_KUSAMA_LOCAL => Ok(CoretimeRuntimeType::KusamaLocal),
			kusama::CORETIME_KUSAMA_DEVELOPMENT => Ok(CoretimeRuntimeType::KusamaDevelopment),
			westend::CORETIME_WESTEND => Ok(CoretimeRuntimeType::Westend),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn chain_spec_from_json_file(&self, path: PathBuf) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Polkadot |
			CoretimeRuntimeType::PolkadotLocal |
			CoretimeRuntimeType::PolkadotDevelopment =>
				Ok(Box::new(polkadot::CoretimeChainSpec::from_json_file(path)?)),
			CoretimeRuntimeType::Kusama |
			CoretimeRuntimeType::KusamaLocal |
			CoretimeRuntimeType::KusamaDevelopment =>
				Ok(Box::new(kusama::CoretimeChainSpec::from_json_file(path)?)),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_file(path)?)),
		}
	}

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Polkadot =>
				Ok(Box::new(polkadot::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-polkadot.json")[..],
				)?)),
			CoretimeRuntimeType::PolkadotLocal => Ok(Box::new(polkadot::local_config(
				polkadot::CORETIME_POLKADOT_LOCAL,
				"Polkadot Coretime Local",
				"polkadot-local",
				ParaId::new(1010),
			))),
			CoretimeRuntimeType::PolkadotDevelopment => Ok(Box::new(polkadot::local_config(
				polkadot::CORETIME_POLKADOT_DEVELOPMENT,
				"Polkadot Coretime Development",
				"polkadot-dev",
				ParaId::new(1010),
			))),
			CoretimeRuntimeType::Kusama =>
				Ok(Box::new(kusama::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-kusama.json")[..],
				)?)),
			CoretimeRuntimeType::KusamaLocal => Ok(Box::new(kusama::local_config(
				kusama::CORETIME_KUSAMA_LOCAL,
				"Kusama Coretime Local",
				"kusama-local",
				ParaId::new(1010),
			))),
			CoretimeRuntimeType::KusamaDevelopment => Ok(Box::new(kusama::local_config(
				kusama::CORETIME_KUSAMA_DEVELOPMENT,
				"Kusama Coretime Development",
				"kusama-dev",
				ParaId::new(1010),
			))),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-westend.json")[..],
				)?)),
		}
	}
}

/// Check if `id` satisfies Coretime-like format.
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(CoretimeRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			CoretimeRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Westend setup (uses Polkadot runtime)
pub mod westend {
	use crate::chain_spec::coretime::polkadot;

	pub(crate) const CORETIME_WESTEND: &str = "coretime-westend";
	pub type CoretimeChainSpec = polkadot::CoretimeChainSpec;
	pub type RuntimeApi = coretime_polkadot_runtime::RuntimeApi;
}

/// Sub-module for Kusama setup
pub mod kusama {
	use super::{CoretimeBalance, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{constants::kusama_currency, AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const CORETIME_KUSAMA: &str = "coretime-kusama";
	pub(crate) const CORETIME_KUSAMA_LOCAL: &str = "coretime-kusama-local";
	pub(crate) const CORETIME_KUSAMA_DEVELOPMENT: &str = "coretime-kusama-dev";
	const CORETIME_KUSAMA_ED: CoretimeBalance = kusama_currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_kusama_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_kusama_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> CoretimeChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 2.into());
		properties.insert("tokenSymbol".into(), "KSM".into());
		properties.insert("tokenDecimals".into(), 12.into());

		CoretimeChainSpec::from_genesis(
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
	) -> coretime_kusama_runtime::RuntimeGenesisConfig {
		coretime_kusama_runtime::RuntimeGenesisConfig {
			system: coretime_kusama_runtime::SystemConfig {
				code: coretime_kusama_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!")
					.to_vec(),
				..Default::default()
			},
			balances: coretime_kusama_runtime::BalancesConfig {
				balances: endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, CORETIME_KUSAMA_ED * 4096))
					.collect(),
			},
			parachain_info: coretime_kusama_runtime::ParachainInfoConfig {
				parachain_id: id,
				..Default::default()
			},
			collator_selection: coretime_kusama_runtime::CollatorSelectionConfig {
				invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
				candidacy_bond: CORETIME_KUSAMA_ED * 16,
				..Default::default()
			},
			session: coretime_kusama_runtime::SessionConfig {
				keys: invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                   // account id
							acc,                                           // validator id
							coretime_kusama_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect(),
			},
			aura: Default::default(),
			aura_ext: Default::default(),
			parachain_system: Default::default(),
			polkadot_xcm: coretime_kusama_runtime::PolkadotXcmConfig {
				safe_xcm_version: Some(SAFE_XCM_VERSION),
				..Default::default()
			},
		}
	}
}

/// Sub-module for Polkadot setup
pub mod polkadot {
	use super::{CoretimeBalance, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{constants::polkadot_currency, AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const CORETIME_POLKADOT: &str = "coretime-polkadot";
	pub(crate) const CORETIME_POLKADOT_LOCAL: &str = "coretime-polkadot-local";
	pub(crate) const CORETIME_POLKADOT_DEVELOPMENT: &str = "coretime-polkadot-dev";
	const CORETIME_POLKADOT_ED: CoretimeBalance = polkadot_currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_polkadot_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_polkadot_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> CoretimeChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 0.into());
		properties.insert("tokenSymbol".into(), "DOT".into());
		properties.insert("tokenDecimals".into(), 10.into());

		CoretimeChainSpec::from_genesis(
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
	) -> coretime_polkadot_runtime::RuntimeGenesisConfig {
		coretime_polkadot_runtime::RuntimeGenesisConfig {
			system: coretime_polkadot_runtime::SystemConfig {
				code: coretime_polkadot_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!")
					.to_vec(),
				..Default::default()
			},
			balances: coretime_polkadot_runtime::BalancesConfig {
				balances: endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, CORETIME_POLKADOT_ED * 4096))
					.collect(),
			},
			parachain_info: coretime_polkadot_runtime::ParachainInfoConfig {
				parachain_id: id,
				..Default::default()
			},
			collator_selection: coretime_polkadot_runtime::CollatorSelectionConfig {
				invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
				candidacy_bond: CORETIME_POLKADOT_ED * 16,
				..Default::default()
			},
			session: coretime_polkadot_runtime::SessionConfig {
				keys: invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                     // account id
							acc,                                             // validator id
							coretime_polkadot_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect(),
			},
			aura: Default::default(),
			aura_ext: Default::default(),
			parachain_system: Default::default(),
			polkadot_xcm: coretime_polkadot_runtime::PolkadotXcmConfig {
				safe_xcm_version: Some(SAFE_XCM_VERSION),
				..Default::default()
			},
		}
	}
}
