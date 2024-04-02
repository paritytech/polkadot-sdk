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

//! A malicious node variant that attempts spam statement requests.
//!
//! This malus variant behaves honestly in everything except when propagating statement distribution requests through the network bridge subsystem.
//! Instead of sending a single request when it needs something it attempts to spam the peer with multiple requests.
//!
//! Attention: For usage with `zombienet` only!

#![allow(missing_docs)]

use polkadot_cli::{
	service::{
		AuthorityDiscoveryApi, AuxStore, BabeApi, Block, Error, ExtendedOverseerGenArgs, HeaderBackend, Overseer, OverseerConnector, OverseerGen, OverseerGenArgs, OverseerHandle, ParachainHost, ProvideRuntimeApi
	},
	validator_overseer_builder, Cli,
};
use polkadot_node_subsystem::{messages::NetworkBridgeTxMessage, SpawnGlue};
use polkadot_node_subsystem_types::DefaultSubsystemClient;
use polkadot_node_network_protocol::request_response::{outgoing::Requests, OutgoingRequest};
use sp_core::traits::SpawnNamed;

// Filter wrapping related types.
use crate::{interceptor::*, shared::MALUS};

use std::sync::Arc;

/// Wraps around network bridge and replaces it.
#[derive(Clone)]
struct RequestSpammer<Spawner> {
	spawner: Spawner, //stores the actual network bridge subsystem spawner
	spam_factor: u32, // How many statement distribution requests to send.
}

impl<Sender, Spawner> MessageInterceptor<Sender> for RequestSpammer<Spawner>
where
	Sender: overseer::NetworkBridgeTxSenderTrait + Clone + Send + 'static,
	Spawner: overseer::gen::Spawner + Clone + 'static,
{
	type Message = NetworkBridgeTxMessage;

	/// Intercept NetworkBridgeTxMessage::SendRequests with Requests::AttestedCandidateV2 inside and duplicate that request
	fn intercept_incoming(
		&self,
		_subsystem_sender: &mut Sender,
		msg: FromOrchestra<Self::Message>,
	) -> Option<FromOrchestra<Self::Message>> {
		match msg {
			FromOrchestra::Communication {
				msg: NetworkBridgeTxMessage::SendRequests (mut requests, if_disconnected),
			} => {
				// AttestedCandidateV2 requests arrive 1 by 1
				if requests.len() == 1 {
					// Check if the request is of the type AttestedCandidateV2
					if let Requests::AttestedCandidateV2(req) = &requests[0] {
						// Temporarily store peer and payload for duplication
						let peer_to_duplicate = req.peer.clone();
						let payload_to_duplicate = req.payload.clone();

						// Duplicate the request spam_factor times and append to the list
						for _ in 0..self.spam_factor-1 {
							let (new_outgoing_request, _) = OutgoingRequest::new(peer_to_duplicate.clone(), payload_to_duplicate.clone());
							let new_request = Requests::AttestedCandidateV2(new_outgoing_request);
							requests.push(new_request);
						}

						gum::info!(
							target: MALUS,
							"😈 Duplicating AttestedCandidateV2 request extra {:?} times to peer: {:?}.", self.spam_factor, peer_to_duplicate,
						);
					}
				}

				// Passthrough the message with a potentially modified number of requests
				Some(FromOrchestra::Communication {
					msg: NetworkBridgeTxMessage::SendRequests(requests, if_disconnected),
				})
			},
			FromOrchestra::Communication { msg } => Some(FromOrchestra::Communication { msg }),
			FromOrchestra::Signal(signal) => Some(FromOrchestra::Signal(signal)),
		}
	}
}

//----------------------------------------------------------------------------------

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct SpamStatementRequestsOptions {
/// How many statement distribution requests to send.
	#[clap(long, ignore_case = true, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(0..=10000000))]
	pub spam_factor: u32,

	#[clap(flatten)]
	pub cli: Cli,
}

/// SpamStatementRequests implementation wrapper which implements `OverseerGen` glue.
pub(crate) struct SpamStatementRequests {
	/// How many statement distribution requests to send.
	pub spam_factor: u32,
}

impl OverseerGen for SpamStatementRequests {
	fn generate<Spawner, RuntimeClient>(
		&self,
		connector: OverseerConnector,
		args: OverseerGenArgs<'_, Spawner, RuntimeClient>,
		ext_args: Option<ExtendedOverseerGenArgs>,
	) -> Result<
		(Overseer<SpawnGlue<Spawner>, Arc<DefaultSubsystemClient<RuntimeClient>>>, OverseerHandle),
		Error,
	>
	where
		RuntimeClient: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block> + AuxStore,
		RuntimeClient::Api: ParachainHost<Block> + BabeApi<Block> + AuthorityDiscoveryApi<Block>,
		Spawner: 'static + SpawnNamed + Clone + Unpin,
	{
		gum::info!(
			target: MALUS,
			"😈 Started Malus node that sends {:?} statement distribution requests instead of 1.",
			&self.spam_factor,
		);

		let request_spammer = RequestSpammer {
			spawner: SpawnGlue(args.spawner.clone()),
			spam_factor: self.spam_factor,
		};

		validator_overseer_builder(
			args,
			ext_args.expect("Extended arguments required to build validator overseer are provided"),
		)?
		.replace_network_bridge_tx(move |cb| InterceptedSubsystem::new(cb, request_spammer))
		.build_with_connector(connector)
		.map_err(|e| e.into())
	}
}
