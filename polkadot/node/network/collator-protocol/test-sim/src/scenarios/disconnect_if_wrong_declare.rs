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

//! Peer declares for a para that is *not* in the validator's claim queue, while the
//! validator does have scheduled paras. Validator disconnects the peer. Sanity counterpart
//! pins the assertion to "wrong para" rather than "any declare".

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::ProtocolVersion::V1,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const SCHEDULED: ParaId = ParaId::new(2000);
const WRONG: ParaId = ParaId::new(3000);

#[crate::sim_test]
fn peer_disconnected_after_declaring_for_wrong_para<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), SCHEDULED)]);
	let peer = w.declared_peer(WRONG, V1);
	w.expect_disconnect(&peer);
}

#[crate::sim_test]
fn peer_with_correct_declare_is_not_disconnected<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), SCHEDULED)]);
	let peer = w.declared_peer(SCHEDULED, V1);
	w.expect_no_disconnect(&peer, Duration::from_millis(200));
}
