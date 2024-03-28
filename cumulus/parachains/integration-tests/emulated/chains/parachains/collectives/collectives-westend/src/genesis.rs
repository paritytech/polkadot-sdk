// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// Substrate
use sp_core::storage::Storage;

// Cumulus
use emulated_integration_tests_common::{
	accounts, build_genesis_storage, collators, SAFE_XCM_VERSION,
};
use parachains_common::Balance;

pub const PARA_ID: u32 = 1001;
pub const ED: Balance = testnet_parachains_constants::westend::currency::EXISTENTIAL_DEPOSIT;

pub fn genesis() -> Storage {
	let genesis_config = collectives_westend_runtime::RuntimeGenesisConfig {
		system: collectives_westend_runtime::SystemConfig::default(),
		balances: collectives_westend_runtime::BalancesConfig {
			balances: accounts::init_balances().iter().cloned().map(|k| (k, ED * 4096)).collect(),
		},
		parachain_info: collectives_westend_runtime::ParachainInfoConfig {
			parachain_id: PARA_ID.into(),
			..Default::default()
		},
		collator_selection: collectives_westend_runtime::CollatorSelectionConfig {
			invulnerables: collators::invulnerables().iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ED * 16,
			..Default::default()
		},
		session: collectives_westend_runtime::SessionConfig {
			keys: collators::invulnerables()
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                                       // account id
						acc,                                               // validator id
						collectives_westend_runtime::SessionKeys { aura }, // session keys
					)
				})
				.collect(),
		},
		polkadot_xcm: collectives_westend_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		},
		..Default::default()
	};

	build_genesis_storage(
		&genesis_config,
		collectives_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}
