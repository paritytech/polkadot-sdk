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

//! Smoke tests covering harness wiring (`Sim::start`/`finish`, aux registration, signal
//! broadcast). Live in the collator consumer crate because they need a real
//! `SubsystemUnderTest` (here: `LegacyValidator`); the test-sim core has no SUT of its own.

use futures::{future::BoxFuture, FutureExt};
use polkadot_collator_protocol_test_sim::{
	contract::Query,
	harness::{AnswerQuery, RouteAttempt, Sim, SimConfig, SubsystemSlot},
	impls::LegacyValidator,
};
use polkadot_node_subsystem::{messages::AllMessages, OverseerSignal};
use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};

struct PanicResponder;
impl AnswerQuery for PanicResponder {
	fn answer(&mut self, query: Query) {
		panic!("unexpected query in smoke test: {:?}", query);
	}
}

#[test]
fn legacy_validator_starts_and_concludes() {
	let sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let recorder = sim.finish();
	assert_eq!(recorder.len(), 0);
}

#[test]
fn legacy_validator_smoke_via_explicit_test_attr() {
	let sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let _ = sim.finish();
}

struct CountingAux {
	signals: Arc<AtomicUsize>,
}

impl SubsystemSlot for CountingAux {
	fn name(&self) -> &'static str {
		"counting-aux"
	}
	fn send_signal(&self, _signal: OverseerSignal) -> BoxFuture<'static, ()> {
		let signals = self.signals.clone();
		async move {
			signals.fetch_add(1, Ordering::SeqCst);
		}
		.boxed()
	}
	fn try_route(&self, msg: AllMessages) -> RouteAttempt {
		RouteAttempt::Declined(msg)
	}
}

#[test]
fn prospective_parachains_aux_concludes_cleanly() {
	use polkadot_collator_protocol_test_sim::aux::ProspectiveParachainsAux;
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let (slot, outbound_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	sim.register_aux(slot, outbound_rx);

	let recorder = sim.finish();
	assert_eq!(recorder.len(), 0);
}

#[test]
fn candidate_backing_aux_concludes_cleanly() {
	use polkadot_collator_protocol_test_sim::aux::CandidateBackingAux;
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let (slot, outbound_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(slot, outbound_rx);

	let recorder = sim.finish();
	assert_eq!(recorder.len(), 0);
}

#[test]
fn prospective_and_backing_aux_concludes_cleanly_together() {
	use polkadot_collator_protocol_test_sim::aux::{CandidateBackingAux, ProspectiveParachainsAux};
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let (psp, psp_rx) = ProspectiveParachainsAux::spawn(&mut sim);
	let (cb, cb_rx) = CandidateBackingAux::spawn(&mut sim);
	sim.register_aux(psp, psp_rx);
	sim.register_aux(cb, cb_rx);

	let recorder = sim.finish();
	assert_eq!(recorder.len(), 0);
}

#[test]
fn aux_slot_receives_signal_broadcast() {
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let signals = Arc::new(AtomicUsize::new(0));
	sim.register_aux_slot_only(CountingAux { signals: signals.clone() });

	sim.signal(OverseerSignal::BlockFinalized(Default::default(), 1));
	assert_eq!(signals.load(Ordering::SeqCst), 1);

	let _ = sim.finish();
}

#[test]
fn always_valid_stub_concludes() {
	use polkadot_collator_protocol_test_sim::aux::{CandidateOutputs, CandidateValidationStub};
	let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
	let stub = CandidateValidationStub::always_valid(&mut sim, CandidateOutputs::default());
	sim.register_aux_slot_only(stub);
	let _ = sim.finish();
}
