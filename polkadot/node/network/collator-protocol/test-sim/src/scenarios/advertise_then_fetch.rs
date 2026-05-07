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

//! Headline scenario: peer connects, declares, and advertises a candidate. CanSecond
//! check (via real candidate-backing) passes; validator fires `CollationFetchingV2`.
//!
//! First scenario that drives the full hybrid harness — chain model + real
//! prospective-parachains + real candidate-backing — through to `SendRequest`.

use crate::{
	builders::ProtocolVersion::V2,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn valid_advertisement_triggers_fetch<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.declared_peer(PARA, V2);
	let cand = w.advertise(&peer, w.leaf(), PARA);
	let _ = w.fetch_request(&cand);
}
