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

//! Peer declares for a para that is NOT in the validator's claim queue. Validator drops
//! the connection (immediate `DisconnectPeers`; the rep hit is the deferred CostMinor flush
//! and is not asserted here).
//!
//! Distinct from `unneeded_para`: a real chain is set up with claim queue containing
//! `scheduled_para`; peer declares for `unscheduled_para`. Exercises the view-update →
//! unrelated-declare path.

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V2,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};

const SCHEDULED: ParaId = ParaId::new(2000);
const UNSCHEDULED: ParaId = ParaId::new(3000);

#[crate::sim_test]
fn declare_for_unscheduled_para_disconnects_peer<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), SCHEDULED)]);
	let peer = w.declared_peer(UNSCHEDULED, V2);
	w.expect_disconnect(&peer);
}
