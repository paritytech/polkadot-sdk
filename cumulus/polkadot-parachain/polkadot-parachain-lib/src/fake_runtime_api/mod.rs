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

use utils::{impl_node_runtime_apis, imports::*};

type CustomBlock = crate::common::types::Block<u32>;
pub mod aura_sr25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(FakeRuntime, CustomBlock, sp_consensus_aura::sr25519::AuthorityId);
}

pub mod aura_ed25519 {
	use super::*;
	#[allow(dead_code)]
	struct FakeRuntime;
	impl_node_runtime_apis!(FakeRuntime, CustomBlock, sp_consensus_aura::ed25519::AuthorityId);
}
