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
//!
//! # Assertion naming convention
//!
//! `expect_*` assertions may **advance the simulated clock**, bounded by an explicit window
//! (`expect`, `expect_from`, `expect_no`). `assert_*` and `count_*` helpers never advance
//! time — they evaluate the recorder as it stands (`assert_count`, `assert_count_after`,
//! `assert_at_least_after`, `count_effects`). Pick deliberately: a time-driving assertion
//! after an `advance` can step past timeouts and other deadlines, changing what the
//! scenario observes.

use crate::{
	contract::{Effect, RequestId},
	harness::{
		dispatcher::AnswerQuery,
		pending_fetches::{PendingFetches, RawResponse},
		router::{self, RouteAttempt, SubsystemSlot, UutRoute, UutSlot},
		Barrier, Recorder,
	},
	report::TimelineReport,
	runtime::{Executor, LocalPoolSpawner, MockClock},
};
use futures::future::BoxFuture;
use polkadot_node_clock::Clock;
use polkadot_node_subsystem::{messages::AllMessages, FromOrchestra, OverseerSignal, SpawnGlue};
use polkadot_node_subsystem_test_helpers::{
	make_subsystem_context, TestSubsystemContext, TestSubsystemContextHandle,
};
use polkadot_overseer::AssociateOutgoing;
use std::{sync::Arc, time::Duration};

/// Configuration for a single simulation run.
#[derive(Default)]
pub struct SimConfig {}

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
		ctx: TestSubsystemContext<Self::Message, SpawnGlue<LocalPoolSpawner>>,
		clock: Arc<MockClock>,
	) -> BoxFuture<'static, ()>;

	/// Try to extract `Self::Message` from an `AllMessages` value addressed to this subsystem.
	///
	/// Used by the router to deliver outbound messages from one auxiliary subsystem (e.g.
	/// `candidate-backing` emitting `CollatorProtocolMessage::Seconded`) into the UUT's
	/// inbound channel.
	///
	/// Returns `Ok(inner)` if the message targets this subsystem, or `Err(msg)` to let the
	/// router try other slots / fall through to classification.
	fn try_extract_inbound(msg: AllMessages) -> Result<Self::Message, AllMessages>;

	/// Build the subsystem-message envelope this subsystem would receive from the
	/// production network bridge when the local node's view changes to `view`.
	///
	/// Mirrors the production fan-out: one `NetworkBridgeEvent::OurViewChange(view)`
	/// gets wrapped per-subsystem as `<Self::Message>::NetworkBridgeUpdate(focused
	/// event)`. The adapter (which knows the subsystem's wire-protocol type) does the
	/// wrapping.
	///
	/// Default returns `None` — subsystems that don't consume network-bridge events
	/// (e.g. `prospective-parachains`) leave this. Adapters for subsystems that DO
	/// consume them override it; the framework's [`crate::world_base::BlockBuilder`]
	/// automatically publishes the view on `.activate()` when this returns `Some`.
	fn our_view_change(_view: polkadot_node_network_protocol::OurView) -> Option<Self::Message> {
		None
	}
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
	/// Shared spawner used by every subsystem context the harness builds. Aux subsystem
	/// constructors clone this so their spawned tasks land on the same `LocalPool`.
	spawner: LocalPoolSpawner,
	uut: UutSlot<S::Message>,
	/// Outbound `AllMessages` channels: the UUT's plus one per registered auxiliary
	/// subsystem. Drained round-robin in registration order on every settle pass.
	outbound_rxs: Vec<futures::channel::mpsc::UnboundedReceiver<AllMessages>>,
	/// Subsystem slots registered with the harness. Index 0 corresponds to the UUT outbound
	/// rx at `outbound_rxs[0]`; index `i+1` corresponds to `outbound_rxs[i+1]`. The UUT slot
	/// itself is stored in `uut`; it does not consume `AllMessages` (test code injects
	/// typed stimuli directly), so it does not appear in this vector.
	aux: Vec<Box<dyn SubsystemSlot>>,
	/// Side table of `oneshot::Sender`s extracted from outbound fetch requests. Tests
	/// resolve them via `Sim::respond_fetch`.
	pending_fetches: PendingFetches,
}

/// Install a process-wide `tracing` subscriber the first time any `Sim` starts. Without
/// this, `gum::trace!` / `tracing::*` events fired by the real subsystems (prospective,
/// backing) go nowhere — debugging a "fetch fired but no second" failure becomes
/// guesswork.
///
/// Driven by `RUST_LOG`; defaults to `off` so unrelated test runs aren't spammed. Typical
/// usage: `RUST_LOG=parachain=trace cargo test ...`.
fn install_tracing_subscriber() {
	use std::sync::Once;
	use tracing_subscriber::{fmt, EnvFilter};
	static INIT: Once = Once::new();
	INIT.call_once(|| {
		let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("off"));
		let _ = fmt::Subscriber::builder()
			.with_env_filter(filter)
			.with_test_writer()
			.with_target(true)
			.try_init();
	});
}

impl<S: SubsystemUnderTest> Sim<S>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	/// Spin up the simulation. Constructs a `MockClock`, a single-threaded executor, a
	/// `TestSubsystemContext`, and spawns the subsystem's main loop. Returns a handle the test
	/// uses to drive stimuli and observe effects.
	pub fn start<R>(_cfg: SimConfig, responder: R) -> Self
	where
		R: AnswerQuery + 'static,
	{
		install_tracing_subscriber();
		let clock = Arc::new(MockClock::default());
		let mut executor = Executor::new();

		let spawner = LocalPoolSpawner::new();
		executor.set_spawn_drain(spawner.drain_handle());
		let (ctx, handle) = make_subsystem_context::<S::Message, _>(spawner.clone());

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
			spawner,
			uut,
			outbound_rxs: vec![uut_outbound_rx],
			aux: Vec::new(),
			pending_fetches: PendingFetches::new(),
		}
	}

	/// The shared `LocalPoolSpawner` used to build subsystem contexts. Aux constructors
	/// clone this so their spawned background tasks land on the same `LocalPool`.
	pub fn spawner(&self) -> LocalPoolSpawner {
		self.spawner.clone()
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

	/// Advance simulated time by `dur`. Iteratively resolves scheduled events (executor timer
	/// wakeups and pending-fetch timeouts) until either the time budget is exhausted or no
	/// further event falls inside the remaining window.
	///
	/// Plain `MockClock::advance(d)` only fires wakeups whose deadline already exists at the
	/// time of the call. Tick streams (e.g. `tick_stream`) re-register a new wakeup every
	/// time the previous one fires; without iteration `Sim::advance(25s)` would only
	/// surface one tick. Settling between sub-steps lets every wakeup land.
	pub fn advance(&mut self, dur: Duration) {
		let target = self.clock.now() + dur;
		loop {
			let now = self.clock.now();
			if now >= target {
				break;
			}
			let remaining = target - now;
			self.step_clock(remaining);
			self.executor.poll_until_pending();
			self.drain();
		}
	}

	/// Duration until the next scheduled event the sim must stop at: the earliest executor
	/// timer wakeup or pending-fetch timeout deadline, whichever comes first. `None` when
	/// neither exists.
	fn next_event_in(&self) -> Option<Duration> {
		let now = self.clock.now();
		let fetch_timeout =
			self.pending_fetches.next_deadline().map(|d| d.saturating_duration_since(now));
		match (self.clock.next_wakeup_in(), fetch_timeout) {
			(Some(a), Some(b)) => Some(a.min(b)),
			(a, b) => a.or(b),
		}
	}

	/// Step the clock to the next scheduled event ([`Self::next_event_in`]), capped at
	/// `remaining`. Stopping *at* fetch-timeout deadlines (rather than overshooting to the
	/// next executor wakeup or window end) keeps the subsystem's reaction to a timeout
	/// correctly interleaved with its own timers, exactly as on a real network.
	fn step_clock(&mut self, remaining: Duration) {
		match self.next_event_in() {
			Some(d) if d <= remaining => self.clock.advance(d),
			// Either the next event is past the cap, or none is pending. Step the clock
			// by the full cap in one go.
			Some(_) | None => self.clock.advance(remaining),
		}
	}

	/// Wait for an effect matching `predicate` to appear in the recorder, advancing the clock
	/// as needed up to `within`. Searches the entire observation log so a stimulus that
	/// produced its effect synchronously before this call still matches. Panics with a
	/// [`TimelineReport`] on timeout.
	///
	/// Searching the whole log means an effect from an *earlier scenario step* can satisfy
	/// the expectation. When the same kind of effect legitimately occurs more than once in
	/// a scenario (multi-round tests), use [`Self::expect_from`] with a barrier instead.
	#[track_caller]
	pub fn expect<F>(&mut self, predicate: F, within: Duration, expected: &str) -> Effect
	where
		F: Fn(&Effect) -> bool,
	{
		self.expect_from(Barrier::START, predicate, within, expected)
	}

	/// Like [`Self::expect`], but only effects recorded at or after `barrier` match. Snapshot
	/// the barrier via [`Recorder::barrier`] *before* the stimulus to express "this step must
	/// produce a fresh effect" — an identical effect recorded by an earlier step cannot
	/// satisfy the expectation.
	#[track_caller]
	pub fn expect_from<F>(
		&mut self,
		barrier: Barrier,
		predicate: F,
		within: Duration,
		expected: &str,
	) -> Effect
	where
		F: Fn(&Effect) -> bool,
	{
		let location = std::panic::Location::caller();
		let at_str = format!("{}:{}", location.file(), location.line());
		let start_sim_t = self.now_sim_t();

		self.drain();
		if let Some(eff) = self.recorder.find_effect_from(barrier.index(), &predicate).cloned() {
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
			self.step_clock(remaining);
			self.executor.poll_until_pending();
			self.drain();
			if let Some(eff) = self.recorder.find_effect_from(barrier.index(), &predicate).cloned()
			{
				return eff;
			}
		}
	}

	/// Assert that an effect matching `predicate` is *already* recorded at or after `barrier`,
	/// returning it. Unlike [`Self::expect_from`], this never advances the clock — use it when a
	/// preceding step (e.g. an explicit `advance` past a timeout) has already driven the effect
	/// out, so there is nothing to wait for. Panics with a [`TimelineReport`] if not present.
	#[track_caller]
	pub fn assert_from<F>(&mut self, barrier: Barrier, predicate: F, expected: &str) -> Effect
	where
		F: Fn(&Effect) -> bool,
	{
		self.drain();
		self.recorder.find_effect_from(barrier.index(), &predicate).cloned().unwrap_or_else(|| {
			let location = std::panic::Location::caller();
			let report = TimelineReport {
				expected: expected.to_string(),
				actual: format!("no matching effect recorded at sim_t = {}ms", self.now_sim_t().as_millis()),
				window_start: self.now_sim_t(),
				window: Duration::ZERO,
				recorder: &self.recorder,
				replay_seed: None,
				at: Some(&format!("{}:{}", location.file(), location.line())),
				hint: None,
			};
			panic!("assertion failed:\n{}", report);
		})
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

	/// Resolve an outstanding fetch by [`RequestId`] with `response`. The corresponding
	/// `oneshot::Sender` (parked by the harness when the subsystem fired
	/// `NetworkBridgeTxMessage::SendRequests`) is consumed and the subsystem's await unblocks.
	///
	/// Settles the executor afterwards so the subsystem can react to the response.
	///
	/// Panics if `request_id` is unknown (already responded, or no fetch with this id).
	///
	/// [`RequestId`]: crate::contract::RequestId
	pub fn respond_fetch(&mut self, request_id: RequestId, response: RawResponse) {
		let sender = self.pending_fetches.take(request_id).unwrap_or_else(|| {
			panic!(
				"Sim::respond_fetch: no outstanding fetch for {:?} (already responded? unknown id?)",
				request_id
			)
		});
		// `send` consumes the sender and may fail if the receiver was dropped — the
		// subsystem giving up on the fetch is a legitimate outcome the test may want to
		// observe via a subsequent effect, so don't panic on send failure.
		let _ = sender.send(response);
		self.executor.poll_until_pending();
		self.drain();
	}

	/// Number of outstanding fetches awaiting a response. Useful for assertions like "exactly
	/// one fetch was fired" before delivering the response.
	pub fn pending_fetches(&self) -> usize {
		self.pending_fetches.len()
	}

	/// Drop the response sender for `request_id`, which makes the awaiting `oneshot`
	/// receiver resolve with `Canceled`. From the subsystem's POV this is the equivalent
	/// of a network-level timeout / cancellation. Experimental's collation-fetch path
	/// classifies this as `RequestError::Canceled` (`is_timed_out() == true`) and applies
	/// `FAILED_FETCH_SLASH` to the responding peer's reputation.
	///
	/// Panics if `request_id` is unknown.
	pub fn cancel_fetch(&mut self, request_id: RequestId) {
		let sender = self.pending_fetches.take(request_id).unwrap_or_else(|| {
			panic!(
				"Sim::cancel_fetch: no outstanding fetch for {:?} (already responded? unknown id?)",
				request_id
			)
		});
		// Dropping the sender resolves the receiver's await with `Canceled`.
		drop(sender);
		self.executor.poll_until_pending();
		self.drain();
	}

	/// Assert that NO effect matching `predicate` is observed within `within` from this call.
	/// Panics with a [`TimelineReport`] showing the offending effect if one is found.
	///
	/// "From this call" is enforced by an entry-index barrier, not a `sim_t` cutoff: only
	/// effects recorded after `expect_no` is entered count. Effects already in the log —
	/// including ones recorded earlier in the very same simulated instant — are ignored, so
	/// `expect_no` never false-fails on a prior step's effect that happens to share the
	/// current `sim_t`.
	#[track_caller]
	pub fn expect_no<F>(&mut self, predicate: F, within: Duration, expected_absence: &str)
	where
		F: Fn(&Effect) -> bool,
	{
		let location = std::panic::Location::caller();
		let at_str = format!("{}:{}", location.file(), location.line());
		let start_sim_t = self.now_sim_t();

		// Drain anything already pending, then snapshot the log length. Everything recorded
		// from here on is what this assertion is about.
		self.drain();
		let barrier = self.recorder.len();

		let panic_on_match = |this: &Self, eff: &Effect| -> ! {
			let report = TimelineReport {
				expected: format!("absence of: {}", expected_absence),
				actual: format!(
					"found a matching effect at sim_t = {}ms: {}",
					this.now_sim_t().as_millis(),
					crate::report::format_effect(eff),
				),
				window_start: start_sim_t,
				window: within,
				recorder: &this.recorder,
				replay_seed: None,
				at: Some(&at_str),
				hint: None,
			};
			panic!("expect_no failed:\n{}", report);
		};

		if let Some(eff) = self.recorder.find_effect_from(barrier, &predicate).cloned() {
			panic_on_match(self, &eff);
		}

		// Advance through the window; bail at the first newly-recorded match.
		loop {
			let elapsed = self.now_sim_t().saturating_sub(start_sim_t);
			if elapsed >= within {
				return;
			}
			let remaining = within - elapsed;
			self.step_clock(remaining);
			self.executor.poll_until_pending();
			self.drain();
			if let Some(eff) = self.recorder.find_effect_from(barrier, &predicate).cloned() {
				panic_on_match(self, &eff);
			}
		}
	}

	/// Count the number of recorded effects matching `predicate`. Useful for "exactly N
	/// fetches in flight" assertions.
	pub fn count_effects<F: Fn(&Effect) -> bool>(&self, predicate: F) -> usize {
		self.recorder
			.entries()
			.iter()
			.filter(|o| match o {
				crate::harness::observation::Observation::Effect(s) => predicate(&s.value),
			})
			.count()
	}

	/// Convenience: assert exactly `expected` effects matching `predicate` are recorded
	/// right now. Never advances time. Panics with timeline on mismatch.
	#[track_caller]
	pub fn assert_count<F: Fn(&Effect) -> bool>(
		&self,
		predicate: F,
		expected: usize,
		description: &str,
	) {
		let actual = self.count_effects(predicate);
		assert_eq!(
			actual,
			expected,
			"expected exactly {} {} (got {}):\n\n{}",
			expected,
			description,
			actual,
			crate::report::format_timeline(&self.recorder),
		);
	}

	/// Like [`Self::assert_count_after`], but asserts `actual >= at_least` instead of
	/// equality. Use when the contract specifies a lower bound — e.g. "after the timeout
	/// at least one new fetch fires" — and the upper bound depends on subsystem-internal
	/// scheduling decisions tests shouldn't lock to. Never advances time.
	#[track_caller]
	pub fn assert_at_least_after<F: Fn(&Effect) -> bool>(
		&self,
		since: Duration,
		predicate: F,
		at_least: usize,
		description: &str,
	) {
		let actual = self
			.recorder
			.entries()
			.iter()
			.filter(|o| match o {
				crate::harness::observation::Observation::Effect(s) => {
					s.sim_t >= since && predicate(&s.value)
				},
			})
			.count();
		assert!(
			actual >= at_least,
			"expected at least {} {} since sim_t={}ms (got {}):\n\n{}",
			at_least,
			description,
			since.as_millis(),
			actual,
			crate::report::format_timeline(&self.recorder),
		);
	}

	/// Variant of [`Self::assert_count`] that only counts effects with `sim_t >= since`.
	/// Tests use this with [`Self::now_sim_t`] to bound a count to a specific window
	/// — e.g. "exactly 1 SendRequest fired between this point and end of test."
	/// Never advances time.
	#[track_caller]
	pub fn assert_count_after<F: Fn(&Effect) -> bool>(
		&self,
		since: Duration,
		predicate: F,
		expected: usize,
		description: &str,
	) {
		let actual = self
			.recorder
			.entries()
			.iter()
			.filter(|o| match o {
				crate::harness::observation::Observation::Effect(s) => {
					s.sim_t >= since && predicate(&s.value)
				},
			})
			.count();
		assert_eq!(
			actual,
			expected,
			"expected exactly {} {} since sim_t={}ms (got {}):\n\n{}",
			expected,
			description,
			since.as_millis(),
			actual,
			crate::report::format_timeline(&self.recorder),
		);
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

	/// Current simulation time as a `Duration` since the sim started. Tests use this as a
	/// barrier to filter the recorder for effects that fire after a known point.
	pub fn now_sim_t(&self) -> Duration {
		Duration::from_millis(self.clock.duration_since_epoch().as_millis() as u64)
	}

	fn drain(&mut self) {
		loop {
			// Stamp effects with simulated time elapsed since sim start — the same origin
			// `now_sim_t` reports — so windowed assertions and recorded `sim_t` agree.
			let sim_t = self.now_sim_t();
			let now = self.clock.now();
			let mut progressed = false;

			// Time out any fetch whose deadline has passed. Dropping the response sender makes
			// the subsystem's awaiting receiver resolve with `Canceled` — the same signal a
			// real network-level request timeout produces — so the subsystem runs its
			// timeout-fallback path. Count it as progress so the resulting effects settle in
			// this same pass.
			if self.pending_fetches.drain_timed_out(now) > 0 {
				progressed = true;
				self.executor.poll_until_pending();
			}

			for idx in 0..self.outbound_rxs.len() {
				match self.outbound_rxs[idx].try_next() {
					Ok(Some(msg)) => {
						progressed = true;
						let uut_route = UutRouteFor::<S> { uut: &self.uut };
						let aux = self.aux.as_slice();
						let recorder = &mut self.recorder;
						let responder = &mut *self.responder;
						let pending = &mut self.pending_fetches;
						self.executor.run_until(router::route(
							sim_t,
							now,
							msg,
							Some(&uut_route),
							aux,
							recorder,
							responder,
							pending,
						));
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

/// Type-tagged adapter that lets the router call back into the UUT slot for inbound delivery.
struct UutRouteFor<'a, S: SubsystemUnderTest>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	uut: &'a UutSlot<S::Message>,
}

impl<'a, S: SubsystemUnderTest> UutRoute for UutRouteFor<'a, S>
where
	AllMessages: From<<S::Message as AssociateOutgoing>::OutgoingMessages>,
	AllMessages: From<S::Message>,
{
	fn try_route(&self, msg: AllMessages) -> RouteAttempt {
		match S::try_extract_inbound(msg) {
			Ok(inner) => {
				let fut =
					self.uut.send_typed(polkadot_node_subsystem::FromOrchestra::Communication {
						msg: inner,
					});
				RouteAttempt::Accepted(fut)
			},
			Err(other) => RouteAttempt::Declined(other),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::contract::Effect;
	use futures::FutureExt;
	use polkadot_node_network_protocol::{
		peer_set::PeerSet,
		request_response::{v2 as request_v2, OutgoingRequest, Protocol, Recipient, Requests},
	};
	use polkadot_node_subsystem::messages::{
		CollatorProtocolMessage, IfDisconnected, NetworkBridgeTxMessage,
	};
	use polkadot_overseer::SubsystemContext;
	use polkadot_primitives::Hash;

	/// How long after the fetch's network timeout the probe's own timer fires. The probe's
	/// two markers bracket the timeout deadline, so the tests below can assert the timeout
	/// is delivered at its exact instant and in the right order relative to subsystem
	/// timers.
	const TIMER_AFTER_TIMEOUT: Duration = Duration::from_millis(400);

	/// Minimal subsystem fixture pinning the harness's request-timeout model. On spawn it:
	///
	/// 1. fires a single `CollationFetchingV2` request (never answered by the test),
	/// 2. arms its own `clock.delay` at `request_timeout() + TIMER_AFTER_TIMEOUT`,
	/// 3. emits a `DisconnectPeers(_, Collation)` marker when the request resolves (`Canceled` —
	///    the harness-modelled network timeout), and
	/// 4. emits a `DisconnectPeers(_, Validation)` marker when its own timer fires.
	///
	/// The marker `sim_t` stamps are the assertion currency: stamps — unlike marker order,
	/// which the probe's sequential awaits would force anyway — reveal *when* the harness
	/// delivered each event.
	struct TimeoutProbe;

	impl SubsystemUnderTest for TimeoutProbe {
		type Message = CollatorProtocolMessage;

		fn spawn(
			mut ctx: TestSubsystemContext<Self::Message, SpawnGlue<LocalPoolSpawner>>,
			clock: Arc<MockClock>,
		) -> BoxFuture<'static, ()> {
			async move {
				// Arm the timer before awaiting the response so both are pending from t=0.
				let timer = clock
					.delay(Protocol::CollationFetchingV2.request_timeout() + TIMER_AFTER_TIMEOUT);

				let (req, response_recv) = OutgoingRequest::new(
					Recipient::Peer(sc_network_types::PeerId::random()),
					request_v2::CollationFetchingRequest {
						scheduling_parent: Hash::zero(),
						para_id: 100.into(),
						candidate_hash: Default::default(),
					},
				);
				ctx.send_message(NetworkBridgeTxMessage::SendRequests(
					vec![Requests::CollationFetchingV2(req)],
					IfDisconnected::ImmediateError,
				))
				.await;

				let _ = response_recv.await;
				ctx.send_message(NetworkBridgeTxMessage::DisconnectPeers(
					Vec::new(),
					PeerSet::Collation,
				))
				.await;

				timer.await;
				ctx.send_message(NetworkBridgeTxMessage::DisconnectPeers(
					Vec::new(),
					PeerSet::Validation,
				))
				.await;

				// Park until the harness concludes (or the test drops the context).
				loop {
					match ctx.recv().await {
						Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => return,
						_ => {},
					}
				}
			}
			.boxed()
		}

		fn try_extract_inbound(msg: AllMessages) -> Result<Self::Message, AllMessages> {
			match msg {
				AllMessages::CollatorProtocol(inner) => Ok(inner),
				other => Err(other),
			}
		}
	}

	/// The probe makes no queries; any query reaching the responder is a test bug.
	struct NoQueries;
	impl AnswerQuery for NoQueries {}

	fn start_probe() -> Sim<TimeoutProbe> {
		let mut sim = Sim::<TimeoutProbe>::start(SimConfig::default(), NoQueries);
		// Sync on the probe's fetch: classification registers the request (and anchors its
		// timeout deadline) at drain time, so make the harness see it at t=0.
		let _ = sim.expect(
			|e| matches!(e, Effect::SendRequest { .. }),
			Duration::from_millis(10),
			"the probe's fetch",
		);
		assert_eq!(sim.now_sim_t(), Duration::ZERO, "fetch must be registered at t=0");
		sim
	}

	fn marker_sim_t(sim: &Sim<TimeoutProbe>, set: PeerSet) -> Duration {
		sim.recorder()
			.entries()
			.iter()
			.find_map(|o| {
				let crate::harness::observation::Observation::Effect(s) = o;
				match &s.value {
					Effect::DisconnectPeers { peer_set, .. } if *peer_set == set => Some(s.sim_t),
					_ => None,
				}
			})
			.unwrap_or_else(|| panic!("marker for {:?} not recorded", set))
	}

	/// One `advance` across both deadlines: the request timeout must be delivered at its
	/// exact instant — *before* the probe's own later timer — not lazily at the next
	/// executor wakeup. Guards the `next_event_in` wiring: without it, both markers would
	/// be stamped at the timer's wakeup (the only clock event the loop would stop at).
	#[test]
	fn fetch_timeout_fires_at_exact_deadline_before_later_subsystem_timer() {
		let mut sim = start_probe();
		sim.advance(Protocol::CollationFetchingV2.request_timeout() + TIMER_AFTER_TIMEOUT * 2);

		let timeout = Protocol::CollationFetchingV2.request_timeout();
		assert_eq!(marker_sim_t(&sim, PeerSet::Collation), timeout);
		assert_eq!(marker_sim_t(&sim, PeerSet::Validation), timeout + TIMER_AFTER_TIMEOUT);
	}

	/// The timeout must not fire before its deadline, and must fire once the deadline is
	/// reached.
	#[test]
	fn fetch_timeout_does_not_fire_early() {
		let mut sim = start_probe();
		let timeout = Protocol::CollationFetchingV2.request_timeout();

		sim.advance(timeout - Duration::from_millis(1));
		sim.assert_count(
			|e| matches!(e, Effect::DisconnectPeers { .. }),
			0,
			"no timeout reaction before the deadline",
		);

		sim.advance(Duration::from_millis(1));
		sim.assert_count(
			|e| matches!(e, Effect::DisconnectPeers { peer_set: PeerSet::Collation, .. }),
			1,
			"timeout reaction exactly at the deadline",
		);
	}

	/// A time-driving `expect` stops at the fetch deadline (where the awaited effect is
	/// produced) instead of overshooting toward its window end.
	#[test]
	fn expect_stops_at_fetch_deadline() {
		let mut sim = start_probe();
		let _ = sim.expect(
			|e| matches!(e, Effect::DisconnectPeers { peer_set: PeerSet::Collation, .. }),
			Duration::from_secs(10),
			"the probe's timeout marker",
		);
		assert_eq!(sim.now_sim_t(), Protocol::CollationFetchingV2.request_timeout());
	}

	/// `expect_from` must not be satisfied by an effect recorded before the barrier — the
	/// probe's only `SendRequest` predates it, so the expectation times out.
	#[test]
	#[should_panic(expected = "expectation failed")]
	fn expect_from_ignores_effects_before_barrier() {
		let mut sim = start_probe();
		let barrier = sim.recorder().barrier();
		let _ = sim.expect_from(
			barrier,
			|e| matches!(e, Effect::SendRequest { .. }),
			Duration::from_millis(100),
			"a fresh SendRequest after the barrier (none is coming)",
		);
	}
}
