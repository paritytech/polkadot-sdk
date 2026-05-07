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

//! Auxiliary slot wiring for the real `prospective-parachains` subsystem.
//!
//! Spawns `ProspectiveParachainsSubsystem` on the harness's executor and registers a slot
//! that:
//!
//! - matches `AllMessages::ProspectiveParachains(_)` and forwards the inner message to the
//!   subsystem's inbound channel,
//! - forwards `OverseerSignal`s broadcast by [`Sim::signal`].
//!
//! The subsystem's own outbound `AllMessages` (Runtime + ChainApi queries) flow back into
//! the harness's drain loop and are answered by the [`ChainModel`] in the responder chain.
//!
//! [`Sim::signal`]: crate::harness::Sim::signal
//! [`ChainModel`]: crate::chain::ChainModel

use crate::{
	harness::{
		router::{RouteAttempt, SubsystemSlot},
		Sim, SubsystemUnderTest,
	},
};
use futures::{
	channel::mpsc,
	future::BoxFuture,
	FutureExt, SinkExt,
};
use polkadot_node_core_prospective_parachains::ProspectiveParachainsSubsystem;
use polkadot_node_subsystem::{
	messages::{AllMessages, ProspectiveParachainsMessage},
	overseer::Subsystem,
	FromOrchestra, OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::make_subsystem_context;
use sp_core::testing::TaskExecutor;

/// Auxiliary slot for the real `prospective-parachains` subsystem.
pub struct ProspectiveParachainsAux {
	inbound_tx: mpsc::Sender<FromOrchestra<ProspectiveParachainsMessage>>,
}

impl ProspectiveParachainsAux {
	/// Spawn the real subsystem on `sim`'s executor and return the slot plus the outbound
	/// `AllMessages` receiver. Hand the pair to [`Sim::register_aux`].
	///
	/// [`Sim::register_aux`]: crate::harness::Sim::register_aux
	pub fn spawn<S: SubsystemUnderTest>(
		sim: &mut Sim<S>,
	) -> (Self, mpsc::UnboundedReceiver<AllMessages>)
	where
		AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
		AllMessages: From<S::Message>,
	{
		let pool = TaskExecutor::new();
		let (ctx, handle) =
			make_subsystem_context::<ProspectiveParachainsMessage, _>(pool);

		let subsystem = ProspectiveParachainsSubsystem::new(Default::default());
		let spawned = subsystem.start(ctx);
		sim.executor_mut().spawn(spawned.future.map(|_| ()).boxed());
		// Let the subsystem reach its initial parked state.
		sim.executor_mut().poll_until_pending();

		let aux = Self { inbound_tx: handle.tx };
		(aux, handle.rx)
	}
}

impl SubsystemSlot for ProspectiveParachainsAux {
	fn name(&self) -> &'static str {
		"prospective-parachains"
	}

	fn send_signal(&self, signal: OverseerSignal) -> BoxFuture<'static, ()> {
		let mut tx = self.inbound_tx.clone();
		async move {
			tx.send(FromOrchestra::Signal(signal))
				.await
				.expect("prospective-parachains inbound channel still open");
		}
		.boxed()
	}

	fn try_route(&self, msg: AllMessages) -> RouteAttempt {
		match msg {
			AllMessages::ProspectiveParachains(inner) => {
				let mut tx = self.inbound_tx.clone();
				let fut = async move {
					tx.send(FromOrchestra::Communication { msg: inner })
						.await
						.expect("prospective-parachains inbound channel still open");
				}
				.boxed();
				RouteAttempt::Accepted(fut)
			},
			other => RouteAttempt::Declined(other),
		}
	}
}
