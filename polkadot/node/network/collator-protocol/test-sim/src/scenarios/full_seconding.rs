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

//! Full advertise → fetch → respond → second flow end-to-end. Real prospective + real
//! backing + always-valid validation stub. Includes a sanity counterpart that pins the
//! "Reputation::Malicious only on Invalid" semantics.

use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	contract::RepBucket,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn advertise_fetch_respond_yields_second_candidate<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let leaf_n = w.leaf_number();
	let candidate = Candidate::builder()
		.para(PARA)
		.relay_parent(w.leaf())
		.relay_parent_number(leaf_n)
		.build();

	let peer = w.declared_peer(PARA, V2);
	w.full_second(&peer, &candidate);

	// Sanity counterpart for `project_collator_experimental_no_invalid_reputation_event`:
	// a *valid* candidate must NOT produce a Reputation::Malicious. Pairs with the Invalid
	// scenario in `fetch_next_on_invalid` to confirm the Malicious emission is gated on
	// invalidity, not on every fetch outcome.
	w.expect_no_rep(&peer, RepBucket::Malicious);
}
