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

//! Peer advertises; CanSecond stub answers `false` → drop. Peer re-advertises the same
//! candidate; validator does NOT fetch the duplicate.
//!
//! Both impls drop the duplicate — that's the shared spec asserted here. Legacy
//! additionally penalises with `COST_UNEXPECTED_MESSAGE` (Performance bucket on the
//! bus); experimental does not slash on advertisement spam (see comment at
//! `validator_side_experimental/state.rs:261-262` — "advertisements are cheap … not
//! worth affecting reputations"). The rep emission divergence is documented in
//! [`crate::scenarios::divergent::reputation_emission`].

use crate::scenarios::shared::WorldExt as _;
use crate::{
	builders::{Candidate, ProtocolVersion::V2},
	chain::CoreSchedule,
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{build_with_ancestors_world_with_config, ChainConfig},
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA: ParaId = ParaId::new(2000);

#[crate::sim_test]
fn re_advertising_after_can_second_false_does_not_refetch<S: CollatorSut>() {
	let config = ChainConfig::default()
		.with_schedule(CoreIndex(0), CoreSchedule::always(PARA))
		.with_can_second_stub(false);
	let mut w = build_with_ancestors_world_with_config::<S>(0, config);

	let candidate = Candidate::for_para_at(PARA, w.leaf());
	let peer = w.declared_peer(PARA, V2);

	// First advertisement: CanSecond=false → drop.
	w.advertise_with_parent_head(
		&peer,
		w.leaf(),
		candidate.hash(),
		polkadot_primitives::HeadData(Vec::new()).hash(),
	);
	w.base.sim.advance(Duration::from_millis(100));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after CanSecond=false (must be zero)",
	);

	// Duplicate advertisement → must remain dropped. Both impls agree.
	w.advertise_with_parent_head(
		&peer,
		w.leaf(),
		candidate.hash(),
		polkadot_primitives::HeadData(Vec::new()).hash(),
	);
	w.base.sim.advance(Duration::from_millis(200));
	w.base.sim.expect_count(
		|e| matches!(e, Effect::SendRequest { .. }),
		0,
		"SendRequest after duplicate advertisement (must be zero — first dropped, second too)",
	);
}
