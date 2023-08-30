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

//! A collator for Aura that looks ahead of the most recently included parachain block
//! when determining what to build upon.
//!
//! This collator also builds additional blocks when the maximum backlog is not saturated.
//! The size of the backlog is determined by invoking a runtime API. If that runtime API
//! is not supported, this assumes a maximum backlog size of 1.
//!
//! This takes more advantage of asynchronous backing, though not complete advantage.
//! When the backlog is not saturated, this approach lets the backlog temporarily 'catch up'
//! with periods of higher throughput. When the backlog is saturated, we typically
//! fall back to the limited cadence of a single parachain block per relay-chain block.
//!
//! Despite this, the fact that there is a backlog at all allows us to spend more time
//! building the block, as there is some buffer before it can get posted to the relay-chain.
//! The main limitation is block propagation time - i.e. the new blocks created by an author
//! must be propagated to the next author before their turn.

use codec::{Codec, Encode};
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{
	self as consensus_common, ParachainBlockImportMarker, ParentSearchParams,
};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{
	relay_chain::Hash as PHash, CollectCollationInfo, PersistedValidationData,
};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::SubmitCollationParams;
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{CollatorPair, Id as ParaId, OccupiedCoreAssumption};

use futures::prelude::*;
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf};
use sc_consensus::BlockImport;
use sc_consensus_aura::standalone as aura_internal;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_consensus_aura::{AuraApi, Slot, SlotDuration};
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member};
use sp_timestamp::Timestamp;
use std::{convert::TryFrom, sync::Arc, time::Duration};

use crate::collator::{self as collator_util, SlotClaim};

/// Parameters for [`run`].
pub struct Params<BI, CIDP, Client, Backend, RClient, CHP, SO, Proposer, CS> {
	/// Inherent data providers. Only non-consensus inherent data should be provided, i.e.
	/// the timestamp, slot, and paras inherents should be omitted, as they are set by this
	/// collator.
	pub create_inherent_data_providers: CIDP,
	/// Used to actually import blocks.
	pub block_import: BI,
	/// The underlying para client.
	pub para_client: Arc<Client>,
	/// The para client's backend, used to access the database.
	pub para_backend: Arc<Backend>,
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// A validation code hash provider, used to get the current validation code hash.
	pub code_hash_provider: CHP,
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
}

/// Run async-backing-friendly Aura.
pub fn run<Block, P, BI, CIDP, Client, Backend, RClient, CHP, SO, Proposer, CS>(
	mut params: Params<BI, CIDP, Client, Backend, RClient, CHP, SO, Proposer, CS>,
) -> impl Future<Output = ()> + Send + 'static
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ AuxStore
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api:
		AuraApi<Block, P::Public> + CollectCollationInfo<Block> + AuraUnincludedSegmentApi<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RClient: RelayChainInterface + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	SO: SyncOracle + Send + Sync + Clone + 'static,
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	// This is an arbitrary value which is likely guaranteed to exceed any reasonable
	// limit, as it would correspond to 10 non-included blocks.
	//
	// Since we only search for parent blocks which have already been imported,
	// we can guarantee that all imported blocks respect the unincluded segment
	// rules specified by the parachain's runtime and thus will never be too deep.
	const PARENT_SEARCH_DEPTH: usize = 10;

	async move {
		cumulus_client_collator::initialize_collator_subsystems(
			&mut params.overseer_handle,
			params.collator_key,
			params.para_id,
		)
		.await;

		let mut import_notifications = match params.relay_client.import_notification_stream().await
		{
			Ok(s) => s,
			Err(err) => {
				tracing::error!(
					target: crate::LOG_TARGET,
					?err,
					"Failed to initialize consensus: no relay chain import notification stream"
				);

				return
			},
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

		while let Some(relay_parent_header) = import_notifications.next().await {
			let relay_parent = relay_parent_header.hash();

			let max_pov_size = match params
				.relay_client
				.persisted_validation_data(
					relay_parent,
					params.para_id,
					OccupiedCoreAssumption::Included,
				)
				.await
			{
				Ok(None) => continue,
				Ok(Some(pvd)) => pvd.max_pov_size,
				Err(err) => {
					tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to gather information from relay-client");
					continue
				},
			};

			let (slot_now, timestamp) = match consensus_common::relay_slot_and_timestamp(
				&relay_parent_header,
				params.relay_chain_slot_duration,
			) {
				None => continue,
				Some((r_s, t)) => {
					let our_slot = Slot::from_timestamp(t, params.slot_duration);
					tracing::debug!(
						target: crate::LOG_TARGET,
						relay_slot = ?r_s,
						para_slot = ?our_slot,
						timestamp = ?t,
						slot_duration = ?params.slot_duration,
						relay_chain_slot_duration = ?params.relay_chain_slot_duration,
						"Adjusted relay-chain slot to parachain slot"
					);
					(our_slot, t)
				},
			};

			let parent_search_params = ParentSearchParams {
				relay_parent,
				para_id: params.para_id,
				ancestry_lookback: max_ancestry_lookback(relay_parent, &params.relay_client).await,
				max_depth: PARENT_SEARCH_DEPTH,
				ignore_alternative_branches: true,
			};

			let potential_parents =
				cumulus_client_consensus_common::find_potential_parents::<Block>(
					parent_search_params,
					&*params.para_backend,
					&params.relay_client,
				)
				.await;

			let mut potential_parents = match potential_parents {
				Err(e) => {
					tracing::error!(
						target: crate::LOG_TARGET,
						?relay_parent,
						err = ?e,
						"Could not fetch potential parents to build upon"
					);

					continue
				},
				Ok(x) => x,
			};

			let included_block = match potential_parents.iter().find(|x| x.depth == 0) {
				None => continue, // also serves as an `is_empty` check.
				Some(b) => b.hash,
			};

			let para_client = &*params.para_client;
			let keystore = &params.keystore;
			let can_build_upon = |block_hash| {
				can_build_upon::<_, _, P>(
					slot_now,
					timestamp,
					block_hash,
					included_block,
					para_client,
					&keystore,
				)
			};

			// Sort by depth, ascending, to choose the longest chain.
			//
			// If the longest chain has space, build upon that. Otherwise, don't
			// build at all.
			potential_parents.sort_by_key(|a| a.depth);
			let initial_parent = match potential_parents.pop() {
				None => continue,
				Some(p) => p,
			};

			// Build in a loop until not allowed. Note that the authorities can change
			// at any block, so we need to re-claim our slot every time.
			let mut parent_hash = initial_parent.hash;
			let mut parent_header = initial_parent.header;
			let overseer_handle = &mut params.overseer_handle;

			// This needs to change to support elastic scaling, but for continuously
			// scheduled chains this ensures that the backlog will grow steadily.
			for n_built in 0..2 {
				let slot_claim = match can_build_upon(parent_hash).await {
					None => break,
					Some(c) => c,
				};

				tracing::debug!(
					target: crate::LOG_TARGET,
					?relay_parent,
					unincluded_segment_len = initial_parent.depth + n_built,
					"Slot claimed. Building"
				);

				let validation_data = PersistedValidationData {
					parent_head: parent_header.encode().into(),
					relay_parent_number: *relay_parent_header.number(),
					relay_parent_storage_root: *relay_parent_header.state_root(),
					max_pov_size,
				};

				// Build and announce collations recursively until
				// `can_build_upon` fails or building a collation fails.
				let (parachain_inherent_data, other_inherent_data) = match collator
					.create_inherent_data(
						relay_parent,
						&validation_data,
						parent_hash,
						slot_claim.timestamp(),
					)
					.await
				{
					Err(err) => {
						tracing::error!(target: crate::LOG_TARGET, ?err);
						break
					},
					Ok(x) => x,
				};

				let validation_code_hash = match params.code_hash_provider.code_hash_at(parent_hash)
				{
					None => {
						tracing::error!(target: crate::LOG_TARGET, ?parent_hash, "Could not fetch validation code hash");
						break
					},
					Some(v) => v,
				};

				match collator
					.collate(
						&parent_header,
						&slot_claim,
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
				{
					Ok((collation, block_data, new_block_hash)) => {
						// Here we are assuming that the import logic protects against equivocations
						// and provides sybil-resistance, as it should.
						collator.collator_service().announce_block(new_block_hash, None);

						// Send a submit-collation message to the collation generation subsystem,
						// which then distributes this to validators.
						//
						// Here we are assuming that the leaf is imported, as we've gotten an
						// import notification.
						overseer_handle
							.send_msg(
								CollationGenerationMessage::SubmitCollation(
									SubmitCollationParams {
										relay_parent,
										collation,
										parent_head: parent_header.encode().into(),
										validation_code_hash,
										result_sender: None,
									},
								),
								"SubmitCollation",
							)
							.await;

						parent_hash = new_block_hash;
						parent_header = block_data.into_header();
					},
					Err(err) => {
						tracing::error!(target: crate::LOG_TARGET, ?err);
						break
					},
				}
			}
		}
	}
}

// Checks if we own the slot at the given block and whether there
// is space in the unincluded segment.
async fn can_build_upon<Block: BlockT, Client, P>(
	slot: Slot,
	timestamp: Timestamp,
	parent_hash: Block::Hash,
	included_block: Block::Hash,
	client: &Client,
	keystore: &KeystorePtr,
) -> Option<SlotClaim<P::Public>>
where
	Client: ProvideRuntimeApi<Block>,
	Client::Api: AuraApi<Block, P::Public> + AuraUnincludedSegmentApi<Block>,
	P: Pair,
	P::Public: Codec,
	P::Signature: Codec,
{
	let runtime_api = client.runtime_api();
	let authorities = runtime_api.authorities(parent_hash).ok()?;
	let author_pub = aura_internal::claim_slot::<P>(slot, &authorities, keystore).await?;

	// Here we lean on the property that building on an empty unincluded segment must always
	// be legal. Skipping the runtime API query here allows us to seamlessly run this
	// collator against chains which have not yet upgraded their runtime.
	if parent_hash != included_block {
		if !runtime_api.can_build_upon(parent_hash, included_block, slot).ok()? {
			return None
		}
	}

	Some(SlotClaim::unchecked::<P>(author_pub, slot, timestamp))
}

async fn max_ancestry_lookback(
	_relay_parent: PHash,
	_relay_client: &impl RelayChainInterface,
) -> usize {
	// TODO [https://github.com/paritytech/cumulus/issues/2706]
	// We need to read the relay-chain state to know what the maximum
	// age truly is, but that depends on those pallets existing.
	//
	// For now, just provide the conservative value of '2'.
	// Overestimating can cause problems, as we'd be building on forks of the
	// chain that can never get included. Underestimating is less of an issue.
	2
}
