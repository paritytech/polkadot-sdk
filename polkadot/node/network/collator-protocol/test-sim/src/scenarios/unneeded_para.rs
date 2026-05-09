// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Peer declares for a para not on any assigned core. Validator drops the connection.
//!
//! Empty claim queue (no schedule installed) → every para is unneeded. ActiveLeaves
//! preamble (built into `activated_world`) clears both impls' startup-init guard.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V1,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::Id as ParaId;

#[crate::sim_test]
fn declare_for_unneeded_para_disconnects_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[]); // empty schedule = nothing scheduled
	let peer = w.declared_peer(ParaId::from(2000), V1);
	w.expect_disconnect(&peer);
}
