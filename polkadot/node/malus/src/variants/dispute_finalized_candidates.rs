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

//! A malicious node variant that attempts to dispute finalized candidates.
//! 
//! Attention: For usage with `zombienet` only!

#![allow(missing_docs)]

use polkadot_cli::{
	prepared_overseer_builder,
	service::{
		AuthorityDiscoveryApi, AuxStore, BabeApi, Block, Error, HeaderBackend, Overseer,
		OverseerConnector, OverseerGen, OverseerGenArgs, OverseerHandle, ParachainHost,
		ProvideRuntimeApi,
	},
	Cli,
};
use polkadot_node_subsystem::{messages::ApprovalVotingMessage, SpawnGlue};
use polkadot_node_subsystem_types::{DefaultSubsystemClient, OverseerSignal};
use sp_core::traits::SpawnNamed;
use futures::channel::oneshot;

// Filter wrapping related types.
use crate::{
	interceptor::*,
	shared::MALUS,
};

use std::sync::Arc;

/// Wraps around ApprovalVotingSubsystem and replaces it.
/// Listens to finalization messages and if possible triggers disputes for their ancestors.
#[derive(Clone)]
struct AncestorDisputer<Spawner> {
	spawner: Spawner, //stores the actual ApprovalVotingSubsystem spawner
	dispute_offset: u32, //relative depth of the disputed block to the finalized block, 0=finalized, 1=parent of finalized
}

impl<Sender, Spawner> MessageInterceptor<Sender> for AncestorDisputer<Spawner>
where
	Sender: overseer::ApprovalVotingSenderTrait + Clone + Send + 'static,
	Spawner: overseer::gen::Spawner + Clone + 'static,
{
	type Message = ApprovalVotingMessage;

	/// Intercept incoming `OverseerSignal::BlockFinalized' and pass the rest as normal.
	fn intercept_incoming(
		&self,
		subsystem_sender: &mut Sender,
		msg: FromOrchestra<Self::Message>,
	) -> Option<FromOrchestra<Self::Message>> {
		match msg {
			FromOrchestra::Communication{msg} => Some(FromOrchestra::Communication{msg}),
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(h, n)) => {

				gum::info!(
					target: MALUS,
					"😈 Block Finalized Interception!"
				);

				//Ensure that the block is actually deep enough to be disputed
				if n <= self.dispute_offset {
					return Some(FromOrchestra::Signal(OverseerSignal::BlockFinalized(h, n)));
				}

				let dispute_offset = self.dispute_offset;
				let mut sender = subsystem_sender.clone();
				self.spawner.spawn_blocking(
					"malus-dispute-finalized-block",
					Some("malus"),
					Box::pin(async move {
						// Query chain for the block header at the disputed depth
						let (tx, rx) = oneshot::channel();
						sender.send_message(ChainApiMessage::FinalizedBlockHash(n - dispute_offset, tx)).await;

						// Fetch hash of the block to be disputed
						let disputable_hash = match rx.await {
							Ok(Ok(Some(hash))) => hash,
							_ => {
								gum::info!(
									target: MALUS,
									"😈 Target ancestor already out of scope!"
								);
								return; // Early return from the async block
							},
						};

						gum::info!(
							target: MALUS,
							"😈 Time to dispute: {:?}", disputable_hash
						);

						// Start dispute
						// subsystem_sender.send_unbounded_message(DisputeCoordinatorMessage::IssueLocalStatement(
						// 	session_index,
						// 	candidate_hash,
						// 	candidate.clone(),
						// 	false, // indicates candidate is invalid -> dispute starts
						// ));
					}),
				);

				// Passthrough the finalization signal as usual (using it as hook only)
				Some(FromOrchestra::Signal(OverseerSignal::BlockFinalized(h, n)))
			},
			FromOrchestra::Signal(signal) => Some(FromOrchestra::Signal(signal)),
		}
	}
}

//----------------------------------------------------------------------------------

#[derive(Debug, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DisputeFinalizedCandidatesOptions {
	/// relative depth of the disputed block to the finalized block, 0=finalized, 1=parent of finalized
	#[clap(long, ignore_case = true, default_value_t = 2, value_parser = clap::value_parser!(u32).range(0..=10))]
	pub dispute_offset: u32,

	#[clap(flatten)]
	pub cli: Cli,
}
/// DisputeFinalizedCandidates implementation wrapper which implements `OverseerGen` glue.
pub(crate) struct DisputeFinalizedCandidates {
	/// relative depth of the disputed block to the finalized block, 0=finalized, 1=parent of finalized
	pub dispute_offset: u32,
}

impl OverseerGen for DisputeFinalizedCandidates {
	fn generate<Spawner, RuntimeClient>(
		&self,
		connector: OverseerConnector,
		args: OverseerGenArgs<'_, Spawner, RuntimeClient>,
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
			"😈 Started Malus node that disputes finalized blocks after they are {:?} finalizations deep.",
			&self.dispute_offset,
		);

		let ancestor_disputer = AncestorDisputer {
			spawner: SpawnGlue(args.spawner.clone()),
			dispute_offset: self.dispute_offset,
		};

		prepared_overseer_builder(args)?
			.replace_approval_voting(move |cb| InterceptedSubsystem::new(cb, ancestor_disputer))
			.build_with_connector(connector)
			.map_err(|e| e.into())
	}
}
