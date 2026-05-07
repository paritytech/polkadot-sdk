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

//! Auxiliary slot wiring for the real `candidate-backing` subsystem.
//!
//! Spawns the production `CandidateBackingSubsystem` on the harness's executor and registers
//! a slot that:
//!
//! - matches `AllMessages::CandidateBacking(_)` and forwards the inner message to the
//!   subsystem's inbound channel,
//! - forwards `OverseerSignal`s broadcast by [`Sim::signal`].
//!
//! Backing's outbound surface is wide (CandidateValidation, AvailabilityStore,
//! StatementDistribution, Provisioner, ChainApi, RuntimeApi, ProspectiveParachains,
//! CollatorProtocol). Runtime/ChainApi is answered by [`ChainModel`]. ProspectiveParachains
//! by the real subsystem registered in the same harness. The remaining families are
//! handled by stub responders or aux slots — those are added in H.6 as scenarios start
//! exercising the seconding flow.
//!
//! [`Sim`]: crate::harness::Sim
//! [`Sim::signal`]: crate::harness::Sim::signal
//! [`ChainModel`]: crate::chain::ChainModel

use crate::harness::{
	router::{RouteAttempt, SubsystemSlot},
	Sim, SubsystemUnderTest,
};
use futures::{
	channel::mpsc,
	future::BoxFuture,
	FutureExt, SinkExt,
};
use polkadot_node_core_backing::CandidateBackingSubsystem;
use polkadot_node_subsystem::{
	messages::{AllMessages, CandidateBackingMessage},
	overseer::Subsystem,
	FromOrchestra, OverseerSignal,
};
use polkadot_node_subsystem_test_helpers::make_subsystem_context;
use sp_core::testing::TaskExecutor;
use sp_keystore::{Keystore, KeystorePtr};
use std::sync::Arc;

/// Auxiliary slot for the real `candidate-backing` subsystem.
pub struct CandidateBackingAux {
	inbound_tx: mpsc::Sender<FromOrchestra<CandidateBackingMessage>>,
}

impl CandidateBackingAux {
	/// Spawn the real subsystem on `sim`'s executor and return the slot plus the outbound
	/// `AllMessages` receiver. Hand the pair to [`Sim::register_aux`].
	///
	/// The keystore is pre-populated with an Alice sr25519 parachain key. Tests that need a
	/// specific validator identity can construct their own keystore via
	/// [`Self::spawn_with_keystore`].
	///
	/// [`Sim::register_aux`]: crate::harness::Sim::register_aux
	pub fn spawn<S: SubsystemUnderTest>(
		sim: &mut Sim<S>,
	) -> (Self, mpsc::UnboundedReceiver<AllMessages>)
	where
		AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
		AllMessages: From<S::Message>,
	{
		let keystore: KeystorePtr = Arc::new(sc_keystore::LocalKeystore::in_memory());
		Keystore::sr25519_generate_new(
			&*keystore,
			polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
			Some(&sp_keyring::Sr25519Keyring::Alice.to_seed()),
		)
		.expect("keystore accepts inserted key");
		Self::spawn_with_keystore(sim, keystore)
	}

	/// Spawn with an explicit keystore. Useful for scenarios that need a specific validator
	/// public key (e.g. signing as a particular group member).
	pub fn spawn_with_keystore<S: SubsystemUnderTest>(
		sim: &mut Sim<S>,
		keystore: KeystorePtr,
	) -> (Self, mpsc::UnboundedReceiver<AllMessages>)
	where
		AllMessages: From<<S::Message as polkadot_overseer::AssociateOutgoing>::OutgoingMessages>,
		AllMessages: From<S::Message>,
	{
		let pool = TaskExecutor::new();
		let (ctx, handle) = make_subsystem_context::<CandidateBackingMessage, _>(pool);

		let subsystem = CandidateBackingSubsystem::new(keystore, Default::default());
		let spawned = subsystem.start(ctx);
		sim.executor_mut().spawn(spawned.future.map(|_| ()).boxed());
		sim.executor_mut().poll_until_pending();

		let aux = Self { inbound_tx: handle.tx };
		(aux, handle.rx)
	}
}

impl SubsystemSlot for CandidateBackingAux {
	fn name(&self) -> &'static str {
		"candidate-backing"
	}

	fn send_signal(&self, signal: OverseerSignal) -> BoxFuture<'static, ()> {
		let mut tx = self.inbound_tx.clone();
		async move {
			tx.send(FromOrchestra::Signal(signal))
				.await
				.expect("candidate-backing inbound channel still open");
		}
		.boxed()
	}

	fn try_route(&self, msg: AllMessages) -> RouteAttempt {
		match msg {
			AllMessages::CandidateBacking(inner) => {
				let mut tx = self.inbound_tx.clone();
				let fut = async move {
					tx.send(FromOrchestra::Communication { msg: inner })
						.await
						.expect("candidate-backing inbound channel still open");
				}
				.boxed();
				RouteAttempt::Accepted(fut)
			},
			other => RouteAttempt::Declined(other),
		}
	}
}
