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

//! Inflates the messaging bandwidth returned by the `backing_constraints` runtime API (UMP queue
//! space and HRMP channel capacity), so the node ignores those limits when deciding what to
//! second, back and provision.
//!
//! Used by the zombienet HRMP-capacity-bypass regression test: every validator runs this so the
//! block author's provisioner is inflated too, letting over-limit candidates reach the on-chain
//! acceptance check.
//!
//! Attention: For usage with `zombienet` only!

#![allow(missing_docs)]

use futures::channel::oneshot;
use polkadot_cli::{
	service::{
		AuxStore, Error, ExtendedOverseerGenArgs, Overseer, OverseerConnector, OverseerGen,
		OverseerGenArgs, OverseerHandle,
	},
	validator_overseer_builder, Cli,
};
use polkadot_node_subsystem::SpawnGlue;
use polkadot_node_subsystem_types::{ChainApiBackend, RuntimeApiSubsystemClient};
use sp_core::traits::SpawnNamed;

use crate::{interceptor::*, shared::MALUS};

use std::sync::Arc;

/// `1 << 30` rather than `u32::MAX`: `apply_modifications` subtracts real per-candidate usage from
/// this across the fragment chain, and a few candidates' worth of messages is many orders of
/// magnitude below it, so it cannot underflow — while staying clear of the type maximum.
const LARGE_CAPACITY: u32 = 1 << 30;

/// Overwrite the messaging-bandwidth fields, leaving all others untouched.
fn inflate_constraints(constraints: &mut polkadot_primitives::async_backing::Constraints) {
	constraints.ump_remaining = LARGE_CAPACITY;
	constraints.ump_remaining_bytes = LARGE_CAPACITY;
	for (_para_id, channel) in constraints.hrmp_channels_out.iter_mut() {
		channel.bytes_remaining = LARGE_CAPACITY;
		channel.messages_remaining = LARGE_CAPACITY;
	}
}

/// Intercepts `BackingConstraints` responses and inflates them before forwarding.
#[derive(Clone)]
struct InflateConstraints<Spawner> {
	spawner: Spawner,
}

impl<Sender, Spawner> MessageInterceptor<Sender> for InflateConstraints<Spawner>
where
	Sender: overseer::RuntimeApiSenderTrait + Clone + Send + 'static,
	Spawner: overseer::gen::Spawner + Clone + 'static,
{
	type Message = RuntimeApiMessage;

	fn intercept_incoming(
		&self,
		_subsystem_sender: &mut Sender,
		msg: FromOrchestra<Self::Message>,
	) -> Option<FromOrchestra<Self::Message>> {
		match msg {
			FromOrchestra::Communication {
				msg:
					RuntimeApiMessage::Request(
						relay_parent,
						RuntimeApiRequest::BackingConstraints(para_id, tx),
					),
			} => {
				gum::debug!(
					target: MALUS,
					?para_id,
					?relay_parent,
					"Intercepted BackingConstraints request — inflating messaging limits",
				);

				// Replacement channel for the real subsystem's answer; we keep the original `tx`
				// for the caller (prospective-parachains) and rewrite what goes down it.
				let (new_tx, new_rx) = oneshot::channel();

				self.spawner.spawn(
					"malus-inflate-constraints",
					Some("malus"),
					Box::pin(async move {
						match new_rx.await {
							Ok(Ok(Some(mut constraints))) => {
								inflate_constraints(&mut constraints);
								gum::trace!(
									target: MALUS,
									?para_id,
									"Forwarding inflated constraints for para",
								);
								let _ = tx.send(Ok(Some(constraints)));
							},
							// `None` or an inner `Err` — forward unchanged.
							Ok(other) => {
								let _ = tx.send(other);
							},
							// Sender dropped on shutdown; drop `tx` so the caller sees a
							// cancellation rather than hanging.
							Err(_cancelled) => {},
						}
					}),
				);

				// Forward with the fresh sender so the real subsystem writes to `new_tx`.
				Some(FromOrchestra::Communication {
					msg: RuntimeApiMessage::Request(
						relay_parent,
						RuntimeApiRequest::BackingConstraints(para_id, new_tx),
					),
				})
			},
			// Everything else passes through unmodified.
			FromOrchestra::Communication { msg } => Some(FromOrchestra::Communication { msg }),
			FromOrchestra::Signal(signal) => Some(FromOrchestra::Signal(signal)),
		}
	}
}

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct IgnoreMessageConstraintsOptions {
	#[clap(flatten)]
	pub cli: Cli,
}

/// Overseer whose runtime-API subsystem reports effectively unlimited HRMP/UMP bandwidth.
pub(crate) struct IgnoreMessageConstraints;

impl OverseerGen for IgnoreMessageConstraints {
	fn generate<Spawner, RuntimeClient>(
		&self,
		connector: OverseerConnector,
		args: OverseerGenArgs<'_, Spawner, RuntimeClient>,
		ext_args: Option<ExtendedOverseerGenArgs>,
	) -> Result<(Overseer<SpawnGlue<Spawner>, Arc<RuntimeClient>>, OverseerHandle), Error>
	where
		RuntimeClient: RuntimeApiSubsystemClient + ChainApiBackend + AuxStore + 'static,
		Spawner: 'static + SpawnNamed + Clone + Unpin,
	{
		gum::info!(
			target: MALUS,
			"Started Malus node that ignores HRMP/UMP messaging constraints \
			 (BackingConstraints bandwidth inflated to {})",
			LARGE_CAPACITY,
		);

		let filter = InflateConstraints { spawner: SpawnGlue(args.spawner.clone()) };

		validator_overseer_builder(
			args,
			ext_args.expect("Extended arguments required to build validator overseer are provided"),
		)?
		.replace_runtime_api(move |ra| InterceptedSubsystem::new(ra, filter))
		.build_with_connector(connector)
		.map_err(|e| e.into())
	}
}
