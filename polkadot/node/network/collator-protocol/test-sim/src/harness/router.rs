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

//! Routing layer between subsystem outbound `AllMessages` streams and their destinations.
//!
//! Auxiliary subsystems (real `prospective-parachains`, `candidate-backing`, …) are added in
//! later phases as `SubsystemSlot` implementations. In Phase H.1 the registry is empty; the
//! harness still always falls through to the classify-into-Effect-or-Query path.
//!
//! # Why two destinations?
//!
//! Today an outbound message is either an [`Effect`] (recorded) or a [`Query`] (mock-answered).
//! Wiring real auxiliary subsystems introduces cases where the same outbound message is *both*
//! an effect tests assert on *and* an input that drives an auxiliary subsystem (e.g.
//! `CandidateBackingMessage::Second{...}`). That dual-delivery design is intentionally
//! deferred until [`H.4`](crate::harness) when a real auxiliary subsystem provides concrete
//! constraints on the cloning / oneshot-ownership tradeoffs.
//!
//! [`Effect`]: crate::contract::Effect
//! [`Query`]: crate::contract::Query

use crate::{
	contract::{classify, Classified},
	harness::{dispatcher::AnswerQuery, Recorder},
};
use futures::{
	channel::mpsc,
	future::BoxFuture,
	FutureExt, SinkExt,
};
use polkadot_node_subsystem::{messages::AllMessages, FromOrchestra, OverseerSignal};
use std::time::Instant;

/// Common surface for any subsystem the harness has spawned.
///
/// Auxiliary subsystems implement this so the [`Sim`] can broadcast signals to them and offer
/// them outbound messages from other subsystems for routing.
///
/// All futures returned are `'static` — implementations clone the inbound channel sender so
/// the future does not borrow `&self`.
///
/// [`Sim`]: crate::harness::Sim
pub trait SubsystemSlot: Send {
	/// Human-readable name. Used for cycle/diagnostic reports.
	fn name(&self) -> &'static str;

	/// Future that delivers an `OverseerSignal` to this subsystem's inbound channel.
	fn send_signal(&self, signal: OverseerSignal) -> BoxFuture<'static, ()>;

	/// If this subsystem owns `msg`, return a future that forwards it. Otherwise return
	/// `None` and the caller continues routing.
	///
	/// Implementations must extract the typed inner message from `AllMessages`; doing so
	/// consumes the message (oneshot senders embedded in it move into the typed value).
	fn try_route(&self, msg: AllMessages) -> RouteAttempt;
}

/// Outcome of an attempt to route a single `AllMessages` to a [`SubsystemSlot`].
pub enum RouteAttempt {
	/// The slot owns this message. The wrapped future delivers it to the slot's inbound
	/// channel; the caller awaits and then continues with the next outbound message.
	Accepted(BoxFuture<'static, ()>),
	/// The slot does not own this message. The original `AllMessages` is returned so the
	/// caller can offer it to the next slot or fall through to classify.
	Declined(AllMessages),
}

/// Slot for the unit under test. Distinct from `SubsystemSlot` because tests inject typed
/// stimuli (`FromOrchestra<M>`), not `AllMessages`. Stored separately on `Sim`.
pub struct UutSlot<M: 'static + Send + std::fmt::Debug> {
	pub(crate) name: &'static str,
	pub(crate) inbound_tx: mpsc::Sender<FromOrchestra<M>>,
}

impl<M: 'static + Send + std::fmt::Debug> UutSlot<M> {
	/// Future that delivers a typed `FromOrchestra<M>` to the UUT's inbound channel.
	pub fn send_typed(&self, msg: FromOrchestra<M>) -> BoxFuture<'static, ()> {
		let mut tx = self.inbound_tx.clone();
		async move {
			tx.send(msg).await.expect("UUT inbound channel still open");
		}
		.boxed()
	}

	/// Future that delivers an `OverseerSignal` to the UUT's inbound channel.
	pub fn send_signal(&self, signal: OverseerSignal) -> BoxFuture<'static, ()> {
		self.send_typed(FromOrchestra::Signal(signal))
	}

	/// Subsystem name. Used for cycle/diagnostic reports.
	pub fn name(&self) -> &'static str {
		self.name
	}
}

/// Run a single outbound `AllMessages` through the routing pipeline:
///
/// 1. Offer it to each registered auxiliary slot in `aux` order; the first slot that accepts
///    consumes the message and the function returns once the typed send completes.
/// 2. If no aux slot accepts, classify the message: effects go to the recorder, queries go
///    to the responder.
///
/// In Phase H.1 `aux` is always empty; the function reduces to the classify path.
pub async fn route<R: AnswerQuery + ?Sized>(
	now: Instant,
	msg: AllMessages,
	aux: &[Box<dyn SubsystemSlot>],
	recorder: &mut Recorder,
	responder: &mut R,
) {
	let mut current = msg;
	for slot in aux {
		match slot.try_route(current) {
			RouteAttempt::Accepted(fut) => {
				fut.await;
				return;
			},
			RouteAttempt::Declined(msg) => {
				current = msg;
			},
		}
	}
	for c in classify(current) {
		match c {
			Classified::Effect(e) => recorder.record_effect(now, e),
			Classified::Query(q) => responder.answer(q),
		}
	}
}
