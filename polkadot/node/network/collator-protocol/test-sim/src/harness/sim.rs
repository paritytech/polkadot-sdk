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

//! The simulation driver: stitches together [`MockClock`], [`Executor`], [`Recorder`],
//! [`AnswerQuery`] and one or more spawned subsystems into a single test harness.
//!
//! In Phase H.1 only the unit under test is spawned; auxiliary subsystem slots are an empty
//! vector. Real `prospective-parachains` and `candidate-backing` are wired into the same
//! infrastructure in later phases (H.3 / H.4) without changing the public `Sim` API.

use crate::{
	contract::Effect,
	harness::{
		dispatcher::AnswerQuery,
		router::{self, SubsystemSlot, UutSlot},
		Recorder,
	},
	report::TimelineReport,
	runtime::{Executor, MockClock},
};
use futures::{future::BoxFuture, FutureExt};
use polkadot_collator_protocol::Clock;
use polkadot_node_subsystem::{messages::AllMessages, FromOrchestra, OverseerSignal, SpawnGlue};
use polkadot_node_subsystem_test_helpers::{
	make_subsystem_context, TestSubsystemContext, TestSubsystemContextHandle,
};
use polkadot_overseer::AssociateOutgoing;
use sp_core::testing::TaskExecutor;
use std::{
	sync::Arc,
	time::{Duration, Instant},
};

/// Configuration for a single simulation run.
pub struct SimConfig {
	/// Wall-clock instant the simulation starts at. Defaults to a fresh `Instant::now()`.
	pub epoch: Instant,
}

impl Default for SimConfig {
	fn default() -> Self {
		Self { epoch: Instant::now() }
	}
}

/// A subsystem that can be driven by the test framework.
///
/// Implementations construct the subsystem (via its public API) and return its main-loop future
/// boxed for the executor to drive.
pub trait SubsystemUnderTest: 'static
where
	AllMessages: From<<Self::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<Self::Message>,
{
	/// The subsystem's incoming message type. Drives the type of the test context handle.
	type Message: AssociateOutgoing + std::fmt::Debug + Send + 'static;

	/// Construct the subsystem and return its main-loop future, ready to be spawned on the
	/// pool. The provided `clock` is the deterministic clock the framework drives.
	fn spawn(
		ctx: TestSubsystemContext<Self::Message, SpawnGlue<TaskExecutor>>,
		clock: Arc<MockClock>,
	) -> BoxFuture<'static, ()>;
}

/// A running simulation around `S`.
pub struct Sim<S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	clock: Arc<MockClock>,
	executor: Executor,
	recorder: Recorder,
	responder: Box<dyn AnswerQuery>,
	uut: UutSlot<S::Message>,
	/// Outbound `AllMessages` channels: the UUT's plus one per registered auxiliary
	/// subsystem. Drained round-robin in registration order on every settle pass.
	outbound_rxs: Vec<futures::channel::mpsc::UnboundedReceiver<AllMessages>>,
	/// Subsystem slots registered with the harness. Index 0 corresponds to the UUT outbound
	/// rx at `outbound_rxs[0]`; index `i+1` corresponds to `outbound_rxs[i+1]`. The UUT slot
	/// itself is stored in `uut`; it does not consume `AllMessages` (test code injects
	/// typed stimuli directly), so it does not appear in this vector.
	aux: Vec<Box<dyn SubsystemSlot>>,
}

impl<S: SubsystemUnderTest> Sim<S>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// Spin up the simulation. Constructs a `MockClock`, a single-threaded executor, a
	/// `TestSubsystemContext`, and spawns the subsystem's main loop. Returns a handle the test
	/// uses to drive stimuli and observe effects.
	pub fn start<R>(cfg: SimConfig, responder: R) -> Self
	where
		R: AnswerQuery + 'static,
	{
		let clock = Arc::new(MockClock::new(cfg.epoch));
		let mut executor = Executor::new();

		let pool = TaskExecutor::new();
		let (ctx, handle) = make_subsystem_context::<S::Message, _>(pool);

		let uut = UutSlot { name: "uut", inbound_tx: handle.tx.clone() };
		let TestSubsystemContextHandle { rx: uut_outbound_rx, .. } = handle;

		let fut = S::spawn(ctx, clock.clone());
		executor.spawn(fut);
		// First poll lets the subsystem reach its initial parked state before any stimulus.
		executor.poll_until_pending();

		Self {
			clock,
			executor,
			recorder: Recorder::new(),
			responder: Box::new(responder),
			uut,
			outbound_rxs: vec![uut_outbound_rx],
			aux: Vec::new(),
		}
	}

	/// Direct access to the executor, for spawning auxiliary subsystem futures during
	/// registration helpers. Tests don't usually need this.
	pub fn executor_mut(&mut self) -> &mut Executor {
		&mut self.executor
	}

	/// Access to the deterministic clock.
	pub fn clock(&self) -> &Arc<MockClock> {
		&self.clock
	}

	/// Access to the recorder. Tests can inspect entries directly when convenient.
	pub fn recorder(&self) -> &Recorder {
		&self.recorder
	}

	/// Inject a typed message into the UUT and settle. Drives the subsystem until it parks,
	/// draining any outbound messages produced into recorder/responder/aux slots.
	pub fn inject(&mut self, msg: FromOrchestra<S::Message>) {
		self.executor.run_until(self.uut.send_typed(msg));
		self.drain();
	}

	/// Inject an `OverseerSignal`. The signal is broadcast to the UUT and every registered
	/// auxiliary subsystem. Settles after delivery.
	pub fn signal(&mut self, signal: OverseerSignal) {
		// UUT first so its handler runs before aux subsystems may produce dependent messages.
		self.executor.run_until(self.uut.send_signal(signal.clone()));
		for slot in &self.aux {
			let fut = slot.send_signal(signal.clone());
			self.executor.run_until(fut);
		}
		self.drain();
	}

	/// Inject a regular subsystem message into the UUT and settle.
	pub fn send(&mut self, msg: S::Message) {
		self.inject(FromOrchestra::Communication { msg });
	}

	/// Advance simulated time. After advancing, the executor settles so any tasks waiting on
	/// the clock can make progress; outbound messages they produce are drained.
	pub fn advance(&mut self, dur: Duration) {
		self.clock.advance(dur);
		self.executor.poll_until_pending();
		self.drain();
	}

	/// Wait for an effect matching `predicate` to appear in the recorder, advancing the clock
	/// as needed up to `within`. Searches the entire observation log so a stimulus that
	/// produced its effect synchronously before this call still matches. Panics with a
	/// [`TimelineReport`] on timeout.
	#[track_caller]
	pub fn expect<F>(&mut self, predicate: F, within: Duration, expected: &str) -> Effect
	where
		F: Fn(&Effect) -> bool,
	{
		let location = std::panic::Location::caller();
		let at_str = format!("{}:{}", location.file(), location.line());
		let start_sim_t = self.now_sim_t();

		self.drain();
		if let Some(eff) = self.find_match(&predicate) {
			return eff;
		}

		loop {
			let elapsed_in_window = self.now_sim_t().saturating_sub(start_sim_t);
			if elapsed_in_window >= within {
				let report = TimelineReport {
					expected: expected.to_string(),
					actual: format!("timed out at sim_t = {}ms", self.now_sim_t().as_millis()),
					window_start: start_sim_t,
					window: within,
					recorder: &self.recorder,
					replay_seed: None,
					at: Some(&at_str),
					hint: None,
				};
				panic!("expectation failed:\n{}", report);
			}

			let remaining = within - elapsed_in_window;
			match self.clock.advance_to_next_wakeup() {
				Some(d) if d <= remaining => {
					let _ = d;
				},
				Some(_) | None => {
					self.clock.advance(remaining);
					self.executor.poll_until_pending();
					self.drain();
					if let Some(eff) = self.find_match(&predicate) {
						return eff;
					}
					continue;
				},
			};
			self.executor.poll_until_pending();
			self.drain();
			if let Some(eff) = self.find_match(&predicate) {
				return eff;
			}
		}
	}

	/// Register an auxiliary subsystem slot whose outbound stream the harness should drain.
	///
	/// `slot` is the [`SubsystemSlot`] for routing inbound messages and signals. `outbound_rx`
	/// is the receiver side of the test-context the auxiliary subsystem was constructed
	/// with; the harness polls it on every settle pass and feeds outbound messages back into
	/// the router.
	pub fn register_aux<A: SubsystemSlot + 'static>(
		&mut self,
		slot: A,
		outbound_rx: futures::channel::mpsc::UnboundedReceiver<AllMessages>,
	) {
		self.aux.push(Box::new(slot));
		self.outbound_rxs.push(outbound_rx);
	}

	/// Register an auxiliary subsystem slot only — for use by slots that do not produce
	/// outbound `AllMessages` (e.g. a no-op test fixture).
	pub fn register_aux_slot_only<A: SubsystemSlot + 'static>(&mut self, slot: A) {
		self.aux.push(Box::new(slot));
	}

	/// Conclude every spawned subsystem, drain remaining work, return all recorded
	/// observations.
	pub fn finish(mut self) -> Recorder {
		self.executor.run_until(self.uut.send_signal(OverseerSignal::Conclude));
		for slot in &self.aux {
			let fut = slot.send_signal(OverseerSignal::Conclude);
			self.executor.run_until(fut);
		}
		self.executor.poll_until_pending();
		self.drain();
		self.recorder
	}

	fn now_sim_t(&self) -> Duration {
		Duration::from_millis(self.clock.wall_clock_ms() as u64)
	}

	fn find_match<F: Fn(&Effect) -> bool>(&self, predicate: &F) -> Option<Effect> {
		self.recorder.entries().iter().find_map(|o| match o {
			crate::harness::observation::Observation::Effect(s) =>
				if predicate(&s.value) {
					Some(s.value.clone())
				} else {
					None
				},
		})
	}

	fn drain(&mut self) {
		loop {
			let now = self.clock.now();
			let mut progressed = false;
			for idx in 0..self.outbound_rxs.len() {
				match self.outbound_rxs[idx].try_next() {
					Ok(Some(msg)) => {
						progressed = true;
						let aux = self.aux.as_slice();
						let recorder = &mut self.recorder;
						let responder = &mut *self.responder;
						self.executor.run_until(router::route(now, msg, aux, recorder, responder));
						// Other subsystems may now have work to do (forwarded messages
						// reached their inboxes; oneshot replies unblocked someone).
						self.executor.poll_until_pending();
					},
					Ok(None) | Err(_) => {},
				}
			}
			if !progressed {
				break;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{contract::Query, harness::dispatcher::AnswerQuery, impls::LegacyValidator};

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

	#[crate::sim_test]
	fn sim_test_attribute_runs_as_a_regular_test() {
		let sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
		let _ = sim.finish();
	}

	use crate::harness::router::{RouteAttempt, SubsystemSlot};
	use std::sync::atomic::{AtomicUsize, Ordering};

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
			// H.1's no-op aux: never claims a message; everything falls through.
			RouteAttempt::Declined(msg)
		}
	}

	#[test]
	fn prospective_parachains_aux_concludes_cleanly() {
		use crate::aux::ProspectiveParachainsAux;
		let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
		let (slot, outbound_rx) = ProspectiveParachainsAux::spawn(&mut sim);
		sim.register_aux(slot, outbound_rx);

		// No view-update is sent → prospective fires no Runtime/ChainApi queries; the panic
		// responder stays untouched. `finish` sends Conclude to UUT *and* prospective; both
		// drop their main loops cleanly and no outbound `AllMessages` is left unconsumed.
		let recorder = sim.finish();
		assert_eq!(recorder.len(), 0);
	}

	#[test]
	fn aux_slot_receives_signal_broadcast() {
		let mut sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
		let signals = Arc::new(AtomicUsize::new(0));
		sim.register_aux_slot_only(CountingAux { signals: signals.clone() });

		// `finish` sends Conclude to UUT and (per signal fan-out) also to aux. We don't
		// reach `finish`'s Conclude through `signal()`, so trigger a signal explicitly.
		sim.signal(OverseerSignal::BlockFinalized(Default::default(), 1));
		assert_eq!(signals.load(Ordering::SeqCst), 1);

		let _ = sim.finish();
	}
}
