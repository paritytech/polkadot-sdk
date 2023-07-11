// Copyright 2023 Parity Technologies (UK) Ltd.
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

use codec::{Decode, Encode};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::ParachainBlockImportMarker;
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_core::{relay_chain::BlockId as RBlockId, CollectCollationInfo};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::CollationResult;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Id as ParaId};

use futures::prelude::*;
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
use std::{convert::TryFrom, hash::Hash, sync::Arc, time::Duration};

use crate::collator as collator_util;

/// Parameters for [`run`].
pub struct Params<BI, CIDP, Client, RClient, SO, Proposer, CS> {
	pub create_inherent_data_providers: CIDP,
	pub block_import: BI,
	pub para_client: Arc<Client>,
	pub relay_client: Arc<RClient>,
	pub sync_oracle: SO,
	pub keystore: KeystorePtr,
	pub key: CollatorPair,
	pub para_id: ParaId,
	pub overseer_handle: OverseerHandle,
	pub slot_duration: SlotDuration,
	pub relay_chain_slot_duration: SlotDuration,
	pub proposer: Proposer,
	pub collator_service: CS,
}

/// Run bare Aura consensus as a relay-chain-driven collator.
pub async fn run<Block, P, BI, CIDP, Client, RClient, SO, Proposer, CS>(
	params: Params<BI, CIDP, Client, RClient, SO, Proposer, CS>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ AuxStore
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: AuraApi<Block, P::Public> + CollectCollationInfo<Block>,
	RClient: RelayChainInterface,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	Proposer: ProposerInterface<Block, Transaction = BI::Transaction>,
	Proposer::Transaction: Sync,
	CS: CollatorServiceInterface<Block>,
	P: Pair + Send + Sync,
	P::Public: AppPublic + Hash + Member + Encode + Decode,
	P::Signature: TryFrom<Vec<u8>> + Hash + Member + Encode + Decode,
{
	let mut collation_requests = cumulus_client_collator::relay_chain_driven::init(
		params.key,
		params.para_id,
		params.overseer_handle,
	)
	.await;

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

		let (collation, _, post_hash) = try_request!(
			collator
				.collate(
					&parent_header,
					&claim,
					None,
					(parachain_inherent_data, other_inherent_data),
					// TODO [https://github.com/paritytech/cumulus/issues/2439]
					// We should call out to a pluggable interface that provides
					// the proposal duration.
					Duration::from_millis(500),
					// Set the block limit to 50% of the maximum PoV size.
					//
					// TODO: If we got benchmarking that includes the proof size,
					// we should be able to use the maximum pov size.
					(validation_data.max_pov_size / 2) as usize,
				)
				.await
		);

		let result_sender = Some(collator.collator_service().announce_with_barrier(post_hash));
		request.complete(Some(CollationResult { collation, result_sender }));
	}
}
