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
//! [`AnswerQuery`] and the subsystem under test into a single test harness.

use crate::{
	contract::Effect,
	harness::{
		dispatcher::{AnswerQuery, Dispatcher},
		Recorder,
	},
	report::TimelineReport,
	runtime::{Executor, MockClock},
};
use polkadot_collator_protocol::Clock;
use futures::{future::BoxFuture, FutureExt};
use polkadot_node_subsystem::{
	messages::AllMessages, FromOrchestra, OverseerSignal, SpawnGlue,
};
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
	handle: TestSubsystemContextHandle<S::Message>,
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
		let (ctx, handle) =
			make_subsystem_context::<S::Message, _>(pool);

		let fut = S::spawn(ctx, clock.clone());
		executor.spawn(fut);
		// First poll lets the subsystem reach its initial parked state before any stimulus.
		executor.poll_until_pending();

		Self { clock, executor, recorder: Recorder::new(), responder: Box::new(responder), handle }
	}

	/// Access to the deterministic clock.
	pub fn clock(&self) -> &Arc<MockClock> {
		&self.clock
	}

	/// Access to the recorder. Tests can inspect entries directly when convenient.
	pub fn recorder(&self) -> &Recorder {
		&self.recorder
	}

	/// Inject an inbound message and settle. Drives the subsystem until it parks, draining any
	/// outbound messages produced into recorder/responder.
	pub fn inject(&mut self, msg: FromOrchestra<S::Message>) {
		self.executor.run_until(self.handle.tx.clone().send_message(msg));
		self.drain();
	}

	/// Inject an `OverseerSignal` (e.g. `ActiveLeaves`) and settle.
	pub fn signal(&mut self, signal: OverseerSignal) {
		self.inject(FromOrchestra::Signal(signal));
	}

	/// Inject a regular subsystem message and settle.
	pub fn send(&mut self, msg: S::Message) {
		self.inject(FromOrchestra::Communication { msg });
	}

	/// Advance simulated time. After advancing, the executor settles so any tasks waiting on the
	/// clock can make progress; outbound messages they produce are drained into the harness.
	pub fn advance(&mut self, dur: Duration) {
		self.clock.advance(dur);
		self.executor.poll_until_pending();
		self.drain();
	}

	/// Wait for an effect matching `predicate` to appear in the recorder, advancing the clock as
	/// needed up to `within`. Panics with a [`TimelineReport`] on timeout.
	#[track_caller]
	pub fn expect<F>(&mut self, predicate: F, within: Duration, expected: &str) -> Effect
	where
		F: Fn(&Effect) -> bool,
	{
		let location = std::panic::Location::caller();
		let at_str = format!("{}:{}", location.file(), location.line());
		let start_sim_t = self.now_sim_t();
		let initial_search_from = self.recorder.entries().len();

		// Drain anything currently sitting in the channel before searching.
		self.drain();
		if let Some(eff) = self.find_from(initial_search_from, &predicate) {
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

			// Advance to next pending wakeup, but never past the window.
			let remaining = within - elapsed_in_window;
			let advanced = match self.clock.advance_to_next_wakeup() {
				Some(d) if d <= remaining => d,
				Some(_) | None => {
					// Either the next wakeup is past the window, or there are no wakeups.
					// Step time to the window's edge and re-check the recorder one last time.
					self.clock.advance(remaining);
					self.executor.poll_until_pending();
					self.drain();
					if let Some(eff) = self.find_from(initial_search_from, &predicate) {
						return eff;
					}
					continue; // loop will time out next iteration.
				},
			};
			// `advanced` was applied by `advance_to_next_wakeup`; settle and drain.
			let _ = advanced;
			self.executor.poll_until_pending();
			self.drain();
			if let Some(eff) = self.find_from(initial_search_from, &predicate) {
				return eff;
			}
		}
	}

	/// Conclude the subsystem, drain remaining work, return all recorded observations.
	pub fn finish(mut self) -> Recorder {
		self.executor
			.run_until(self.handle.tx.clone().send_message(FromOrchestra::Signal(OverseerSignal::Conclude)));
		// Drive everything to completion, including the subsystem's clean-up.
		self.executor.poll_until_pending();
		self.drain();
		self.recorder
	}

	fn now_sim_t(&self) -> Duration {
		// `MockClock` starts wall-clock-ms at zero and increments lockstep with `Instant`
		// advances; use that as the sim_t.
		Duration::from_millis(self.clock.wall_clock_ms() as u64)
	}

	fn find_from<F: Fn(&Effect) -> bool>(&self, from: usize, predicate: &F) -> Option<Effect> {
		self.recorder
			.entries()
			.iter()
			.skip(from)
			.find_map(|o| match o {
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
			match self.handle.rx.try_next() {
				Ok(Some(msg)) => {
					Dispatcher::new(&mut self.recorder, &mut *self.responder).dispatch(now, msg);
					// Responder side-effects (oneshot sends) may have unblocked the subsystem.
					self.executor.poll_until_pending();
				},
				Ok(None) => break,
				Err(_) => break,
			}
		}
	}
}

/// Helper trait used internally to send a single message into a `mpsc::Sender` via an awaitable
/// closure friendly to `Executor::run_until`.
trait SendMessage<M> {
	fn send_message(self, msg: FromOrchestra<M>) -> BoxFuture<'static, ()>;
}

impl<M: Send + 'static> SendMessage<M> for futures::channel::mpsc::Sender<FromOrchestra<M>> {
	fn send_message(self, msg: FromOrchestra<M>) -> BoxFuture<'static, ()> {
		let mut tx = self;
		async move {
			use futures::SinkExt;
			tx.send(msg).await.expect("test subsystem channel still open");
		}
		.boxed()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		contract::Query,
		harness::dispatcher::AnswerQuery,
		impls::LegacyValidator,
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
		// No stimuli were sent — the validator should not have produced any observable
		// effects. (It also should not have queried anything; the panic-responder enforces.)
		assert_eq!(recorder.len(), 0);
	}

	#[crate::sim_test]
	fn sim_test_attribute_runs_as_a_regular_test() {
		// Sanity: #[sim_test] expands to a registered test that the runner picks up.
		let sim = Sim::<LegacyValidator>::start(SimConfig::default(), PanicResponder);
		let _ = sim.finish();
	}
}
