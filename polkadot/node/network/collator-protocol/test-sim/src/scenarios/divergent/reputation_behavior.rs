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

//! Behavioural consequences of reputation — experimental-only.
//!
//! Legacy reputation is fire-and-forget to the network bridge: the test framework
//! mocks the bridge, so legacy rep has no behavioural feedback you can observe in this
//! framework. Experimental's rep, by contrast, drives:
//!
//! - **Fetch ranking**: when two peers advertise valid candidates at the same RP, the
//!   higher-scored peer's request goes out first. Ordering is `(score DESC, timestamp
//!   ASC, advertisement ord)` — see `validator_side_experimental/collation_manager
//!   /mod.rs:1009-1017`.
//! - **Penalty box for fresh peers**: a peer with score 0 (no past inclusions) waits
//!   `MAX_FETCH_DELAY = 300ms` after scheduling-parent activation before its fetch
//!   fires. A peer with score ≥ 1 (from `VALID_INCLUDED_CANDIDATE_BUMP`) bypasses the
//!   delay. See `calculate_delay` in `collation_manager/mod.rs:664-670`.
//! - **Slot eviction by score**: per-para slot cap is 60 (`CONNECTED_PEERS_PARA_LIMIT`).
//!   When full, a connecting peer with strictly higher score evicts the lowest-scored
//!   incumbent (`peer_manager/connected.rs:257-269`). Score `0` does *not* trigger
//!   floor-disconnect on its own.
//!
//! These tests are `only = "experimental"` — legacy has no equivalent observable
//! mechanism. For the *spec contract* both impls share (penalise misbehaviour, reward
//! good citizenship), see [`super::reputation_emission`] for the bus-event vs silent
//! divergence.
//!
//! # Pending
//!
//! These tests are intentionally aspirational — they're the gem the user asked for
//! ("nasty peer cannot starve a high-rep peer"). Filling them in needs a way to
//! pre-seed an experimental peer's score (e.g. by having the test drive a finalized
//! block with the peer's candidate, which triggers `VALID_INCLUDED_CANDIDATE_BUMP`).
//! That requires the framework to model finalization — a reasonable extension once
//! the scaffolding above is committed.
//!
//! Sketches kept here so the design is recorded in code, not just chat.

#![allow(dead_code, unused_imports)]

use crate::{
	builders::{Candidate, Peer, ProtocolVersion::V2},
	contract::Effect,
	harness::CollatorSut,
	scenarios::shared::{activated_world, World},
};
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::time::Duration;

const PARA_A: ParaId = ParaId::new(2000);

// TODO: implement once `World::seed_score(peer, score)` exists.
//
// /// A previously-included peer (score ≥ 1) fetches immediately. A fresh peer (score 0)
// /// waits 300ms before its fetch fires. Asserts `MAX_FETCH_DELAY = 300ms` is observable
// /// on the timeline.
// #[crate::sim_test(only = "experimental")]
// fn fresh_peer_waits_in_penalty_box_then_fetches<S: CollatorSut>() {
//     unimplemented!()
// }

// TODO: implement once `World::seed_score(peer, score)` exists AND once
// `World::register_validator_set(...)` lets us construct a legitimate
// `MAX_AUTHORITY_INCOMING_STREAMS`-bounded slot-fill scenario for the experimental side.
//
// /// `nasty_peer_cannot_starve_high_rep_peer`: high-rep peer A and 60+ low-rep peers
// /// (the per-para cap is 60). When the cap is full and a new low-rep peer connects,
// /// it must NOT evict A. When a higher-scored peer connects, *another* low-rep peer
// /// is evicted instead of A.
// #[crate::sim_test(only = "experimental")]
// fn high_rep_peer_not_evicted_under_capacity_pressure<S: CollatorSut>() {
//     unimplemented!()
// }

// TODO: implement once `World::seed_score(peer, score)` exists.
//
// /// Two peers advertise valid candidates at the same RP; both have legitimate slots.
// /// Higher-rep peer's fetch fires first. Asserts `(score DESC, timestamp ASC)` ordering.
// #[crate::sim_test(only = "experimental")]
// fn higher_rep_peer_fetched_first<S: CollatorSut>() {
//     unimplemented!()
// }
