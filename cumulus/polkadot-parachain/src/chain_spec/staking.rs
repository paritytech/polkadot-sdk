// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::chain_spec::{
	get_account_id_from_seed, get_collator_keys_from_seed, Extensions, GenericChainSpec,
	SAFE_XCM_VERSION,
};
use cumulus_primitives_core::ParaId;
use parachains_common::{AccountId, AuraId, Balance as StakingBalance};
use sc_service::ChainType;
use sp_core::sr25519;

const STAKING_ROCOCO_ED: StakingBalance = parachains_common::westend::currency::EXISTENTIAL_DEPOSIT;

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn staking_rococo_session_keys(keys: AuraId) -> staking_rococo_runtime::SessionKeys {
	staking_rococo_runtime::SessionKeys { aura: keys }
}

pub fn staking_rococo_development_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());
	staking_rococo_like_development_config(
		properties,
		"Rococo Staking Development",
		"staking-rococo-dev",
		2000,
	)
}

fn staking_rococo_like_development_config(
	properties: sc_chain_spec::Properties,
	name: &str,
	chain_id: &str,
	para_id: u32,
) -> GenericChainSpec {
	GenericChainSpec::builder(
		staking_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-dev".into(), para_id },
	)
	.with_name(name)
	.with_id(chain_id)
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(staking_rococo_genesis(
		// initial collators.
		vec![(
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_collator_keys_from_seed::<AuraId>("Alice"),
		)],
		vec![
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_account_id_from_seed::<sr25519::Public>("Bob"),
			get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
		],
		parachains_common::rococo::currency::UNITS * 1_000_000,
		para_id.into(),
	))
	.with_properties(properties)
	.build()
}

fn staking_rococo_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: StakingBalance,
	id: ParaId,
) -> serde_json::Value {
	serde_json::json!({
		"balances": staking_rococo_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, endowment))
				.collect(),
		},
		"parachainInfo": staking_rococo_runtime::ParachainInfoConfig {
			parachain_id: id,
			..Default::default()
		},
		"collatorSelection": staking_rococo_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: STAKING_ROCOCO_ED * 16,
			..Default::default()
		},
		"session": staking_rococo_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                         // account id
						acc,                                 // validator id
						staking_rococo_session_keys(aura),   // session keys
					)
				})
				.collect(),
		},
		"polkadotXcm": staking_rococo_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		}
	})
}
