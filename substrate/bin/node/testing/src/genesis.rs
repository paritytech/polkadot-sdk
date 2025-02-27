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
	genesis_config_presets::{kitchen_sink_genesis, validator},
	AccountId, AssetsConfig, BalancesConfig, IndicesConfig, RuntimeGenesisConfig, SessionConfig,
	SocietyConfig, StakingConfig,
};
use sp_keyring::Sr25519Keyring::Alice;

/// Create genesis runtime configuration for tests.
pub fn config() -> RuntimeGenesisConfig {
	config_endowed(Default::default())
}

/// Create genesis runtime configuration for tests with some extra
/// endowed accounts.
pub fn config_endowed(extra_endowed: Vec<AccountId>) -> RuntimeGenesisConfig {
	let initial_authorities = vec![
		(alice(), dave(), session_keys_from_seed(Alice.into())),
		(bob(), eve(), session_keys_from_seed(Alice.into())),
		(alice(), ferdie(), session_keys_from_seed(Alice.into())),
	];

	let mut endowed = vec![alice(), bob(), charlie(), dave(), eve(), ferdie()];
	endowed.extend(extra_endowed);

	kitchen_sink_genesis(
		initial_authorities,
		alice(),
		endowed,
		vec![validator(dave()), validator(eve()), validator(ferdie())],
		None,
	)
}
