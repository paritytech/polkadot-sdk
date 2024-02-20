// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! This provides the option to run a basic relay-chain driven Aura implementation.
//!
//! This collator only builds on top of the most recently included block, limiting the
//! block time to a maximum of two times the relay-chain block time, and requiring the
//! block to be built and distributed to validators between two relay-chain blocks.
//!
//! For more information about AuRa, the Substrate crate should be checked.

use codec::{Codec, Decode};
use cumulus_client_collator::{
	relay_chain_driven::CollationRequest, service::ServiceInterface as CollatorServiceInterface,
};
use cumulus_client_consensus_common::ParachainBlockImportMarker;
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_core::{relay_chain::BlockId as RBlockId, CollectCollationInfo};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::CollationResult;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Id as ParaId};

use futures::{channel::mpsc::Receiver, prelude::*};
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf};
use sc_consensus::BlockImport;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_consensus_aura::{AuraApi, SlotDuration};
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member};
use std::{convert::TryFrom, sync::Arc, time::Duration};

use crate::collator as collator_util;

/// Parameters for [`run`].
pub struct Params<BI, CIDP, Client, RClient, SO, Proposer, CS> {
	/// Inherent data providers. Only non-consensus inherent data should be provided, i.e.
	/// the timestamp, slot, and paras inherents should be omitted, as they are set by this
	/// collator.
	pub create_inherent_data_providers: CIDP,
	/// Used to actually import blocks.
	pub block_import: BI,
	/// The underlying para client.
	pub para_client: Arc<Client>,
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// A chain synchronization oracle.
	pub sync_oracle: SO,
	/// The underlying keystore, which should contain Aura consensus keys.
	pub keystore: KeystorePtr,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// A handle to the relay-chain client's "Overseer" or task orchestrator.
	pub overseer_handle: OverseerHandle,
	/// The length of slots in this chain.
	pub slot_duration: SlotDuration,
	/// The length of slots in the relay chain.
	pub relay_chain_slot_duration: Duration,
	/// The underlying block proposer this should call into.
	pub proposer: Proposer,
	/// The generic collator service used to plug into this consensus engine.
	pub collator_service: CS,
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
	/// Receiver for collation requests. If `None`, Aura consensus will establish a new receiver.
	/// Should be used when a chain migrates from a different consensus algorithm and was already
	/// processing collation requests before initializing Aura.
	pub collation_request_receiver: Option<Receiver<CollationRequest>>,
}

/// Run bare Aura consensus as a relay-chain-driven collator.
pub fn run<Block, P, BI, CIDP, Client, RClient, SO, Proposer, CS>(
	params: Params<BI, CIDP, Client, RClient, SO, Proposer, CS>,
) -> impl Future<Output = ()> + Send + 'static
where
	Block: BlockT + Send,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ AuxStore
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public> + CollectCollationInfo<Block>,
	RClient: RelayChainInterface + Send + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + Send + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	async move {
		let mut collation_requests = match params.collation_request_receiver {
			Some(receiver) => receiver,
			None =>
				cumulus_client_collator::relay_chain_driven::init(
					params.collator_key,
					params.para_id,
					params.overseer_handle,
				)
				.await,
		};

		let mut collator = {
			let params = collator_util::Params {
				create_inherent_data_providers: params.create_inherent_data_providers,
				block_import: params.block_import,
				relay_client: params.relay_client.clone(),
				keystore: params.keystore.clone(),
				para_id: params.para_id,
				proposer: params.proposer,
				collator_service: params.collator_service,
			};

			collator_util::Collator::<Block, P, _, _, _, _, _>::new(params)
		};

		let mut last_processed_slot = 0;

		while let Some(request) = collation_requests.next().await {
			macro_rules! reject_with_error {
				($err:expr) => {{
					request.complete(None);
					tracing::error!(target: crate::LOG_TARGET, err = ?{ $err });
					continue;
				}};
			}

			macro_rules! try_request {
				($x:expr) => {{
					match $x {
						Ok(x) => x,
						Err(e) => reject_with_error!(e),
					}
				}};
			}

			let validation_data = request.persisted_validation_data();

			let parent_header =
				try_request!(Block::Header::decode(&mut &validation_data.parent_head.0[..]));

			let parent_hash = parent_header.hash();

			if !collator.collator_service().check_block_status(parent_hash, &parent_header) {
				continue
			}

			let relay_parent_header =
				match params.relay_client.header(RBlockId::hash(*request.relay_parent())).await {
					Err(e) => reject_with_error!(e),
					Ok(None) => continue, // sanity: would be inconsistent to get `None` here
					Ok(Some(h)) => h,
				};

			let claim = match collator_util::claim_slot::<_, _, P>(
				&*params.para_client,
				parent_hash,
				&relay_parent_header,
				params.slot_duration,
				params.relay_chain_slot_duration,
				&params.keystore,
			)
			.await
			{
				Ok(None) => continue,
				Ok(Some(c)) => c,
				Err(e) => reject_with_error!(e),
			};

			// With async backing this function will be called every relay chain block.
			//
			// Most parachains currently run with 12 seconds slots and thus, they would try to
			// produce multiple blocks per slot which very likely would fail on chain. Thus, we have
			// this "hack" to only produce on block per slot.
			//
			// With https://github.com/paritytech/polkadot-sdk/issues/3168 this implementation will be
			// obsolete and also the underlying issue will be fixed.
			if last_processed_slot >= *claim.slot() {
				continue
			}

			let (parachain_inherent_data, other_inherent_data) = try_request!(
				collator
					.create_inherent_data(
						*request.relay_parent(),
						&validation_data,
						parent_hash,
						claim.timestamp(),
					)
					.await
			);

			let maybe_collation = try_request!(
				collator
					.collate(
						&parent_header,
						&claim,
						None,
						(parachain_inherent_data, other_inherent_data),
						params.authoring_duration,
						// Set the block limit to 50% of the maximum PoV size.
						//
						// TODO: If we got benchmarking that includes the proof size,
						// we should be able to use the maximum pov size.
						(validation_data.max_pov_size / 2) as usize,
					)
					.await
			);

			if let Some((collation, _, post_hash)) = maybe_collation {
				let result_sender =
					Some(collator.collator_service().announce_with_barrier(post_hash));
				request.complete(Some(CollationResult { collation, result_sender }));
			} else {
				request.complete(None);
				tracing::debug!(target: crate::LOG_TARGET, "No block proposal");
			}

			last_processed_slot = *claim.slot();
		}
	}
}
