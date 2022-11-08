// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]

pub use bp_polkadot_core::*;

use sp_version::RuntimeVersion;

/// Statemine Chain.
pub type Statemine = PolkadotLike;

/// Known Statemine runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: sp_version::create_runtime_str!("statemine"),
	impl_name: sp_version::create_runtime_str!("statemine"),
	authoring_version: 1,
	spec_version: 9300,
	impl_version: 0,
	apis: sp_version::create_apis_vec![[]],
	transaction_version: 8,
	state_version: 1,
};
