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
//! This malus variant behaves honestly in backing and approval voting.
//! The maliciousness comes from emitting an extra dispute statement on top of the other ones.
//!
//! Some extra quirks which generally should be insignificant:
//! - The malus node will not dispute at session boundaries
//! - The malus node will not dispute blocks it backed itself
//! - Be cautious about the size of the network to make sure disputes are not auto-confirmed
//! (7 validators is the smallest network size as it needs [(7-1)//3]+1 = 3 votes to get
//! confirmed but it only gets 1 from backing and 1 from malus so 2 in total)
//!
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
use polkadot_node_subsystem_types::{ChainApiBackend, OverseerSignal, RuntimeApiSubsystemClient};
use polkadot_node_subsystem_util::request_candidate_events;
use polkadot_primitives::CandidateEvent;
use sp_core::traits::SpawnNamed;

// Filter wrapping related types.
use crate::{interceptor::*, shared::MALUS};

use std::sync::Arc;

/// Wraps around ApprovalVotingSubsystem and replaces it.
/// Listens to finalization messages and if possible triggers disputes for their ancestors.
#[derive(Clone)]
struct AncestorDisputer<Spawner> {
	spawner: Spawner, //stores the actual ApprovalVotingSubsystem spawner
	dispute_offset: u32, /* relative depth of the disputed block to the finalized block,
	                   * 0=finalized, 1=parent of finalized etc */
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
			FromOrchestra::Communication { msg } => Some(FromOrchestra::Communication { msg }),
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(
				finalized_hash,
				finalized_height,
			)) => {
				gum::debug!(
					target: MALUS,
					"😈 Block Finalization Interception! Block: {:?}", finalized_hash,
				);

				//Ensure that the chain is long enough for the target ancestor to exist
				if finalized_height <= self.dispute_offset {
					return Some(FromOrchestra::Signal(OverseerSignal::BlockFinalized(
						finalized_hash,
						finalized_height,
					)))
				}

				let dispute_offset = self.dispute_offset;
				let mut sender = subsystem_sender.clone();
				self.spawner.spawn_blocking(
					"malus-dispute-finalized-block",
					Some("malus"),
					Box::pin(async move {
						// Query chain for the block hash at the target depth
						let (tx, rx) = oneshot::channel();
						sender
							.send_message(ChainApiMessage::FinalizedBlockHash(
								finalized_height - dispute_offset,
								tx,
							))
							.await;
						let disputable_hash = match rx.await {
							Ok(Ok(Some(hash))) => {
								gum::debug!(
									target: MALUS,
									"😈 Time to search {:?}`th ancestor! Block: {:?}", dispute_offset, hash,
								);
								hash
							},
							_ => {
								gum::debug!(
									target: MALUS,
									"😈 Seems the target is not yet finalized! Nothing to dispute."
								);
								return // Early return from the async block
							},
						};

						// Fetch all candidate events for the target ancestor
						let events =
							request_candidate_events(disputable_hash, &mut sender).await.await;
						let events = match events {
							Ok(Ok(events)) => events,
							Ok(Err(e)) => {
								gum::error!(
									target: MALUS,
									"😈 Failed to fetch candidate events: {:?}", e
								);
								return // Early return from the async block
							},
							Err(e) => {
								gum::error!(
									target: MALUS,
									"😈 Failed to fetch candidate events: {:?}", e
								);
								return // Early return from the async block
							},
						};

						// Extract a token candidate from the events to use for disputing
						let event = events.iter().find(|event| {
							matches!(event, CandidateEvent::CandidateIncluded(_, _, _, _))
						});
						let candidate = match event {
							Some(CandidateEvent::CandidateIncluded(candidate, _, _, _)) =>
								candidate,
							_ => {
								gum::error!(
									target: MALUS,
									"😈 No candidate included event found! Nothing to dispute."
								);
								return // Early return from the async block
							},
						};

						// Extract the candidate hash from the candidate
						let candidate_hash = candidate.hash();

						// Fetch the session index for the candidate
						let (tx, rx) = oneshot::channel();
						sender
							.send_message(RuntimeApiMessage::Request(
								disputable_hash,
								RuntimeApiRequest::SessionIndexForChild(tx),
							))
							.await;
						let session_index = match rx.await {
							Ok(Ok(session_index)) => session_index,
							_ => {
								gum::error!(
									target: MALUS,
									"😈 Failed to fetch session index for candidate."
								);
								return // Early return from the async block
							},
						};
						gum::info!(
							target: MALUS,
							"😈 Disputing candidate with hash: {:?} in session {:?}", candidate_hash, session_index,
						);

						// Start dispute
						sender.send_unbounded_message(
							DisputeCoordinatorMessage::IssueLocalStatement(
								session_index,
								candidate_hash,
								candidate.clone(),
								false, // indicates candidate is invalid -> dispute starts
							),
						);
					}),
				);

				// Passthrough the finalization signal as usual (using it as hook only)
				Some(FromOrchestra::Signal(OverseerSignal::BlockFinalized(
					finalized_hash,
					finalized_height,
				)))
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
	/// relative depth of the disputed block to the finalized block, 0=finalized, 1=parent of
	/// finalized etc
	#[clap(long, ignore_case = true, default_value_t = 2, value_parser = clap::value_parser!(u32).range(0..=50))]
	pub dispute_offset: u32,

	#[clap(flatten)]
	pub cli: Cli,
}

/// DisputeFinalizedCandidates implementation wrapper which implements `OverseerGen` glue.
pub(crate) struct DisputeFinalizedCandidates {
	/// relative depth of the disputed block to the finalized block, 0=finalized, 1=parent of
	/// finalized etc
	pub dispute_offset: u32,
}

impl OverseerGen for DisputeFinalizedCandidates {
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
			"😈 Started Malus node that disputes finalized blocks after they are {:?} finalizations deep.",
			&self.dispute_offset,
		);

		let ancestor_disputer = AncestorDisputer {
			spawner: SpawnGlue(args.spawner.clone()),
			dispute_offset: self.dispute_offset,
		};

		validator_overseer_builder(
			args,
			ext_args.expect("Extended arguments required to build validator overseer are provided"),
		)?
		.replace_approval_voting(move |cb| InterceptedSubsystem::new(cb, ancestor_disputer))
		.build_with_connector(connector)
		.map_err(|e| e.into())
	}
}
