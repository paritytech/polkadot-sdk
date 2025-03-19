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
//! The block building mechanism consists of two parts:
//! 	1. A block-builder task that builds parachain blocks at each of our slots.
//! 	2. A collator task that transforms the blocks into a collation and submits them to the relay
//!     chain.
//!
//! Blocks are built on every parachain slot if there is a core scheduled on the relay chain. At the
//! beginning of each block building loop, we determine how many blocks we expect to build per relay
//! chain block. The collator implementation then expects that we have that many cores scheduled
//! during the relay chain block. After the block is built, the block builder task sends it to
//! the collation task which compresses it and submits it to the collation-generation subsystem.

use self::{block_builder_task::run_block_builder, collation_task::run_collation_task};
use codec::Codec;
use consensus_common::ParachainCandidate;
use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::{self as consensus_common, ParachainBlockImportMarker};
use cumulus_client_consensus_proposer::ProposerInterface;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{ClaimQueueOffset, CoreSelector, GetCoreSelectorApi};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::FutureExt;
use polkadot_primitives::{
	vstaging::DEFAULT_CLAIM_QUEUE_OFFSET, CollatorPair, CoreIndex, Hash as RelayHash, Id as ParaId,
	ValidationCodeHash,
};
use sc_client_api::{backend::AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus::BlockImport;
use sc_utils::mpsc::tracing_unbounded;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::AuraApi;
use sp_core::{crypto::Pair, traits::SpawnNamed, U256};
use sp_inherents::CreateInherentDataProviders;
use sp_keystore::KeystorePtr;
use sp_runtime::traits::{Block as BlockT, Member, NumberFor, One};
use std::{path::PathBuf, sync::Arc, time::Duration};

pub use block_import::{SlotBasedBlockImport, SlotBasedBlockImportHandle};

mod block_builder_task;
mod block_import;
mod collation_task;
mod relay_chain_data_cache;

/// Parameters for [`run`].
pub struct Params<Block, BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS, Spawner> {
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
	/// The underlying block proposer this should call into.
	pub proposer: Proposer,
	/// The generic collator service used to plug into this consensus engine.
	pub collator_service: CS,
	/// The amount of time to spend authoring each block.
	pub authoring_duration: Duration,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
	/// Drift slots by a fixed duration. This can be used to create more preferrable authoring
	/// timings.
	pub slot_drift: Duration,
	/// The handle returned by [`SlotBasedBlockImport`].
	pub block_import_handle: SlotBasedBlockImportHandle<Block>,
	/// Spawner for spawning futures.
	pub spawner: Spawner,
	/// When set, the collator will export every produced `POV` to this folder.
	pub export_pov: Option<PathBuf>,
}

/// Run aura-based block building and collation task.
pub fn run<Block, P, BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS, Spawner>(
	params: Params<Block, BI, CIDP, Client, Backend, RClient, CHP, Proposer, CS, Spawner>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockOf
		+ AuxStore
		+ HeaderBackend<Block>
		+ BlockBackend<Block>
		+ UsageProvider<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api:
		AuraApi<Block, P::Public> + GetCoreSelectorApi<Block> + AuraUnincludedSegmentApi<Block>,
	Backend: sc_client_api::Backend<Block> + 'static,
	RClient: RelayChainInterface + Clone + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + 'static,
	CIDP::InherentDataProviders: Send,
	BI: BlockImport<Block> + ParachainBlockImportMarker + Send + Sync + 'static,
	Proposer: ProposerInterface<Block> + Send + Sync + 'static,
	CS: CollatorServiceInterface<Block> + Send + Sync + Clone + 'static,
	CHP: consensus_common::ValidationCodeHashProvider<Block::Hash> + Send + 'static,
	P: Pair + 'static,
	P::Public: AppPublic + Member + Codec,
	P::Signature: TryFrom<Vec<u8>> + Member + Codec,
	Spawner: SpawnNamed,
{
	let Params {
		create_inherent_data_providers,
		block_import,
		para_client,
		para_backend,
		relay_client,
		code_hash_provider,
		keystore,
		collator_key,
		para_id,
		proposer,
		collator_service,
		authoring_duration,
		reinitialize,
		slot_drift,
		block_import_handle,
		spawner,
		export_pov,
	} = params;

	let (tx, rx) = tracing_unbounded("mpsc_builder_to_collator", 100);
	let collator_task_params = collation_task::Params {
		relay_client: relay_client.clone(),
		collator_key,
		para_id,
		reinitialize,
		collator_service: collator_service.clone(),
		collator_receiver: rx,
		block_import_handle,
		export_pov,
	};

	let collation_task_fut = run_collation_task::<Block, _, _>(collator_task_params);

	let block_builder_params = block_builder_task::BuilderTaskParams {
		create_inherent_data_providers,
		block_import,
		para_client,
		para_backend,
		relay_client,
		code_hash_provider,
		keystore,
		para_id,
		proposer,
		collator_service,
		authoring_duration,
		collator_sender: tx,
		slot_drift,
	};

	let block_builder_fut =
		run_block_builder::<Block, P, _, _, _, _, _, _, _, _>(block_builder_params);

	spawner.spawn_blocking(
		"slot-based-block-builder",
		Some("slot-based-collator"),
		block_builder_fut.boxed(),
	);
	spawner.spawn_blocking(
		"slot-based-collation",
		Some("slot-based-collator"),
		collation_task_fut.boxed(),
	);
}

/// Message to be sent from the block builder to the collation task.
///
/// Contains all data necessary to submit a collation to the relay chain.
struct CollatorMessage<Block: BlockT> {
	/// The hash of the relay chain block that provides the context for the parachain block.
	pub relay_parent: RelayHash,
	/// The header of the parent block.
	pub parent_header: Block::Header,
	/// The parachain block candidate.
	pub parachain_candidate: ParachainCandidate<Block>,
	/// The validation code hash at the parent block.
	pub validation_code_hash: ValidationCodeHash,
	/// Core index that this block should be submitted on
	pub core_index: CoreIndex,
}

/// Fetch the `CoreSelector` and `ClaimQueueOffset` for `parent_hash`.
fn core_selector<Block: BlockT, Client>(
	para_client: &Client,
	parent_hash: Block::Hash,
	parent_number: NumberFor<Block>,
) -> Result<(CoreSelector, ClaimQueueOffset), sp_api::ApiError>
where
	Client: ProvideRuntimeApi<Block> + Send + Sync,
	Client::Api: GetCoreSelectorApi<Block>,
{
	let runtime_api = para_client.runtime_api();

	if runtime_api.has_api::<dyn GetCoreSelectorApi<Block>>(parent_hash)? {
		Ok(runtime_api.core_selector(parent_hash)?)
	} else {
		let next_block_number: U256 = (parent_number + One::one()).into();

		// If the runtime API does not support the core selector API, fallback to some default
		// values.
		Ok((CoreSelector(next_block_number.byte(0)), ClaimQueueOffset(DEFAULT_CLAIM_QUEUE_OFFSET)))
	}
}
