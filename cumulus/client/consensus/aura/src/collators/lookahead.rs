// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

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
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{ClaimQueueOffset, CollectCollationInfo, PersistedValidationData};
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{PoV, SubmitCollationParams};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	vstaging::DEFAULT_CLAIM_QUEUE_OFFSET, BlockNumber as RBlockNumber, CollatorPair, Hash as RHash,
	HeadData, Id as ParaId, OccupiedCoreAssumption,
};

use futures::prelude::*;
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf};
use sc_consensus::BlockImport;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{AuraApi, Slot};
use sp_core::crypto::Pair;
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, Member, NumberFor};
use std::{
	fs::{self, File},
	path::PathBuf,
	sync::Arc,
	time::Duration,
};

use crate::{collator as collator_util, LOG_TARGET};

/// Export the given `pov` to the file system at `path`.
///
/// The file will be named `block_hash_block_number.pov`.
///
/// The `parent_header`, `relay_parent_storage_root` and `relay_parent_number` will also be
/// stored in the file alongside the `pov`. This enables stateless validation of the `pov`.
fn export_pov_to_path<Block: BlockT>(
	path: PathBuf,
	pov: PoV,
	block_hash: Block::Hash,
	block_number: NumberFor<Block>,
	parent_header: Block::Header,
	relay_parent_storage_root: RHash,
	relay_parent_number: RBlockNumber,
) {
	if let Err(error) = fs::create_dir_all(&path) {
		tracing::error!(target: LOG_TARGET, %error, path = %path.display(), "Failed to create PoV export directory");
		return
	}

	let mut file = match File::create(path.join(format!("{block_hash:?}_{block_number}.pov"))) {
		Ok(f) => f,
		Err(error) => {
			tracing::error!(target: LOG_TARGET, %error, "Failed to export PoV.");
			return
		},
	};

	pov.encode_to(&mut file);
	HeadData(parent_header.encode()).encode_to(&mut file);
	relay_parent_storage_root.encode_to(&mut file);
	relay_parent_number.encode_to(&mut file);
}

/// Parameters for [`run`].
pub struct Params<BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS> {
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
	/// The underlying keystore, which should contain Aura consensus keys.
	pub keystore: KeystorePtr,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// A handle to the relay-chain client's "Overseer" or task orchestrator.
	pub overseer_handle: OverseerHandle,
	/// The length of slots in the relay chain.
	pub relay_chain_slot_duration: Duration,
	/// The underlying block proposer this should call into.
	pub proposer: Proposer,
	/// The generic collator service used to plug into this consensus engine.
	pub collator_service: CS,
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
}

/// Run async-backing-friendly Aura.
pub fn run<Block, P, BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS>(
	params: Params<BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS>,
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
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	run_with_export::<_, P, _, _, _, _, _, _, _, _>(ParamsWithExport { params, export_pov: None })
}

/// Parameters for [`run_with_export`].
pub struct ParamsWithExport<BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS> {
	/// The parameters.
	pub params: Params<BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS>,
	/// When set, the collator will export every produced `POV` to this folder.
	pub export_pov: Option<PathBuf>,
}

/// Run async-backing-friendly Aura.
///
/// This is exactly the same as [`run`], but it supports the optional export of each produced `POV`
/// to the file system.
pub fn run_with_export<Block, P, BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS>(
	ParamsWithExport { mut params, export_pov }: ParamsWithExport<
		BI,
		CIDP,
		Client,
		Backend,
		RClient,
		CHP,
		Proposer,
		CS,
	>,
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
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
	P: Pair,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
	async move {
		cumulus_client_collator::initialize_collator_subsystems(
			&mut params.overseer_handle,
			params.collator_key,
			params.para_id,
			params.reinitialize,
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

			let core_index = if let Some(core_index) = super::cores_scheduled_for_para(
				relay_parent,
				params.para_id,
				&mut params.relay_client,
				ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET),
			)
			.await
			.get(0)
			{
				*core_index
			} else {
				tracing::trace!(
					target: crate::LOG_TARGET,
					?relay_parent,
					?params.para_id,
					"Para is not scheduled on any core, skipping import notification",
				);

				continue
			};

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

			let (included_block, initial_parent) = match crate::collators::find_parent(
				relay_parent,
				params.para_id,
				&*params.para_backend,
				&params.relay_client,
			)
			.await
			{
				Some(value) => value,
				None => continue,
			};

			let para_client = &*params.para_client;
			let keystore = &params.keystore;
			let can_build_upon = |block_hash| {
				let slot_duration = match sc_consensus_aura::standalone::slot_duration_at(
					&*params.para_client,
					block_hash,
				) {
					Ok(sd) => sd,
					Err(err) => {
						tracing::error!(target: crate::LOG_TARGET, ?err, "Failed to acquire parachain slot duration");
						return None
					},
				};
				tracing::debug!(target: crate::LOG_TARGET, ?slot_duration, ?block_hash, "Parachain slot duration acquired");
				let (relay_slot, timestamp) = consensus_common::relay_slot_and_timestamp(
					&relay_parent_header,
					params.relay_chain_slot_duration,
				)?;
				let slot_now = Slot::from_timestamp(timestamp, slot_duration);
				tracing::debug!(
					target: crate::LOG_TARGET,
					?relay_slot,
					para_slot = ?slot_now,
					?timestamp,
					?slot_duration,
					relay_chain_slot_duration = ?params.relay_chain_slot_duration,
					"Adjusted relay-chain slot to parachain slot"
				);
				Some(super::can_build_upon::<_, _, P>(
					slot_now,
					relay_slot,
					timestamp,
					block_hash,
					included_block,
					para_client,
					&keystore,
				))
			};

			// Build in a loop until not allowed. Note that the authorities can change
			// at any block, so we need to re-claim our slot every time.
			let mut parent_hash = initial_parent.hash;
			let mut parent_header = initial_parent.header;
			let overseer_handle = &mut params.overseer_handle;

			// Do not try to build upon an unknown, pruned or bad block
			if !collator.collator_service().check_block_status(parent_hash, &parent_header) {
				continue
			}

			// This needs to change to support elastic scaling, but for continuously
			// scheduled chains this ensures that the backlog will grow steadily.
			for n_built in 0..2 {
				let slot_claim = match can_build_upon(parent_hash) {
					Some(fut) => match fut.await {
						None => break,
						Some(c) => c,
					},
					None => break,
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

				let Some(validation_code_hash) =
					params.code_hash_provider.code_hash_at(parent_hash)
				else {
					tracing::error!(target: crate::LOG_TARGET, ?parent_hash, "Could not fetch validation code hash");
					break
				};

				super::check_validation_code_or_log(
					&validation_code_hash,
					params.para_id,
					&params.relay_client,
					relay_parent,
				)
				.await;

				let allowed_pov_size = if cfg!(feature = "full-pov-size") {
					validation_data.max_pov_size
				} else {
					// Set the block limit to 50% of the maximum PoV size.
					//
					// TODO: If we got benchmarking that includes the proof size,
					// we should be able to use the maximum pov size.
					validation_data.max_pov_size / 2
				} as usize;

				match collator
					.collate(
						&parent_header,
						&slot_claim,
						None,
						(parachain_inherent_data, other_inherent_data),
						params.authoring_duration,
						allowed_pov_size,
					)
					.await
				{
					Ok(Some((collation, block_data, new_block_hash))) => {
						// Here we are assuming that the import logic protects against equivocations
						// and provides sybil-resistance, as it should.
						collator.collator_service().announce_block(new_block_hash, None);

						if let Some(ref export_pov) = export_pov {
							export_pov_to_path::<Block>(
								export_pov.clone(),
								collation.proof_of_validity.clone().into_compressed(),
								new_block_hash,
								*block_data.header().number(),
								parent_header.clone(),
								*relay_parent_header.state_root(),
								*relay_parent_header.number(),
							);
						}

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
										core_index,
									},
								),
								"SubmitCollation",
							)
							.await;

						parent_hash = new_block_hash;
						parent_header = block_data.into_header();
					},
					Ok(None) => {
						tracing::debug!(target: crate::LOG_TARGET, "No block proposal");
						break
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
