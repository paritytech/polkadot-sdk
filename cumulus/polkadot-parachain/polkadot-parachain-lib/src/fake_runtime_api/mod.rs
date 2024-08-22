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

//! In an ideal world this would be one runtime which would simplify the code massively.
//! This is not an ideal world - Polkadot Asset Hub has a different key type.

mod utils;

use crate::common::types::CustomBlock;
use utils::{impl_node_runtime_apis, imports::*};

pub mod asset_hub_polkadot {
	use super::{utils::imports::*, *};
	use parachains_common::AssetHubPolkadotAuraId;

	type Block = CustomBlock<u32>;
	pub struct FakeRuntime;
	impl_node_runtime_apis!(FakeRuntime, Block, AssetHubPolkadotAuraId);
}

pub mod u32_block {
	use super::*;

	type Block = CustomBlock<u32>;
	pub mod aura_sr25519 {
		use super::*;
		struct FakeRuntime;
		impl_node_runtime_apis!(FakeRuntime, Block, sp_consensus_aura::sr25519::AuthorityId);
	}

	pub mod aura_ed25519 {
		use super::*;
		struct FakeRuntime;
		impl_node_runtime_apis!(FakeRuntime, Block, sp_consensus_aura::ed25519::AuthorityId);
	}
}

pub mod u64_block {
	use super::*;

	type Block = CustomBlock<u64>;
	pub mod aura_sr25519 {
		use super::*;
		struct FakeRuntime;
		impl_node_runtime_apis!(FakeRuntime, Block, sp_consensus_aura::sr25519::AuthorityId);
	}

	pub mod aura_ed25519 {
		use super::*;
		struct FakeRuntime;
		impl_node_runtime_apis!(FakeRuntime, Block, sp_consensus_aura::ed25519::AuthorityId);
	}
}
