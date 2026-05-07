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

//! `Declare` signature verification.
//!
//! * [`declare_with_bad_signature_yields_malicious_reputation`] — peer connects after
//!   ActiveLeaves and `Declare`s with a bogus signature → validator reports Malicious.
//! * [`declare_with_valid_signature_does_not_get_malicious_reputation`] — sanity
//!   counterpart that pins the assertion to "bad signature" rather than "any declare".
//!
//! KNOWN-FAILING (experimental): per
//! `project_collator_experimental_skips_declare_sig.md`, experimental destructures
//! the signature into `_signature` and never verifies it. This is the canonical
//! divergence test that surfaces the auth bypass.

use crate::{
	builders::ProtocolVersion::V1,
	contract::RepBucket,
	harness::CollatorSut,
	scenarios::shared::activated_world,
};
use polkadot_primitives::{CoreIndex, Id as ParaId};

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn declare_with_bad_signature_yields_malicious_reputation<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let peer = w.connected_peer(PARA, V1);
	w.sim.send(peer.declare_with_bad_signature());
	w.expect_rep(&peer, RepBucket::Malicious);
}

#[crate::sim_test]
fn declare_with_valid_signature_does_not_get_malicious_reputation<S: CollatorSut>() {
	let mut w = activated_world::<S>(&[(CoreIndex(0), PARA)]);
	let _peer = w.declared_peer(PARA, V1);
	// Sanity counterpart: a *valid* declare must NOT trip the malicious bucket. Pairs with
	// the bad-signature scenario above to rule out "any declare in this setup yields a
	// Reputation::Malicious" as a false positive.
	w.expect_no_rep(&_peer, RepBucket::Malicious);
}
