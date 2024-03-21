// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Genesis Configuration.

use crate::keyring::*;
use kitchensink_runtime::{
	constants::currency::*, AccountId, AssetsConfig, BalancesConfig, IndicesConfig,
	RuntimeGenesisConfig, SessionConfig, SocietyConfig, StakerStatus, StakingConfig,
};
use sp_keyring::Ed25519Keyring;
use sp_runtime::Perbill;

/// Create genesis runtime configuration for tests.
pub fn config() -> RuntimeGenesisConfig {
	config_endowed(Default::default())
}

/// Create genesis runtime configuration for tests with some extra
/// endowed accounts.
pub fn config_endowed(extra_endowed: Vec<AccountId>) -> RuntimeGenesisConfig {
	let mut endowed = vec![
		(alice(), 111 * DOLLARS),
		(bob(), 100 * DOLLARS),
		(charlie(), 100_000_000 * DOLLARS),
		(dave(), 111 * DOLLARS),
		(eve(), 101 * DOLLARS),
		(ferdie(), 100 * DOLLARS),
	];

	endowed.extend(extra_endowed.into_iter().map(|endowed| (endowed, 100 * DOLLARS)));

	RuntimeGenesisConfig {
		indices: IndicesConfig { indices: vec![] },
		balances: BalancesConfig { balances: endowed },
		session: SessionConfig {
			keys: vec![
				(alice(), dave(), session_keys_from_seed(Ed25519Keyring::Alice.into())),
				(bob(), eve(), session_keys_from_seed(Ed25519Keyring::Bob.into())),
				(charlie(), ferdie(), session_keys_from_seed(Ed25519Keyring::Charlie.into())),
			],
		},
		staking: StakingConfig {
			stakers: vec![
				(dave(), dave(), 111 * DOLLARS, StakerStatus::Validator),
				(eve(), eve(), 100 * DOLLARS, StakerStatus::Validator),
				(ferdie(), ferdie(), 100 * DOLLARS, StakerStatus::Validator),
			],
			validator_count: 3,
			minimum_validator_count: 0,
			slash_reward_fraction: Perbill::from_percent(10),
			invulnerables: vec![alice(), bob(), charlie()],
			..Default::default()
		},
		society: SocietyConfig { pot: 0 },
		assets: AssetsConfig { assets: vec![(9, alice(), true, 1)], ..Default::default() },
		..Default::default()
	}
}
