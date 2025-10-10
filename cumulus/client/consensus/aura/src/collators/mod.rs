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

//! Stock, pure Aura collators.
//!
//! This includes the [`basic`] collator, which only builds on top of the most recently
//! included parachain block, as well as the [`lookahead`] collator, which prospectively
//! builds on parachain blocks which have not yet been included in the relay chain.

use crate::collator::SlotClaim;
use codec::Codec;
use cumulus_client_consensus_common::{self as consensus_common, ParentSearchParams};
use cumulus_primitives_aura::{AuraUnincludedSegmentApi, Slot};
use cumulus_primitives_core::{relay_chain::Header as RelayHeader, BlockT};
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::prelude::*;
use polkadot_node_subsystem::messages::{CollatorProtocolMessage, RuntimeApiRequest};
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	Hash as RelayHash, Id as ParaId, OccupiedCoreAssumption, ValidationCodeHash,
	DEFAULT_SCHEDULING_LOOKAHEAD,
};
use sc_consensus_aura::{standalone as aura_internal, AuraApi};
use sp_api::{ApiExt, ProvideRuntimeApi, RuntimeApiInfo};
use sp_consensus_aura::SlotDuration;
use sp_core::Pair;
use sp_keystore::KeystorePtr;
use sp_timestamp::Timestamp;
use std::time::Duration;

pub mod basic;
pub mod lookahead;
pub mod slot_based;

// This is an arbitrary value which is guaranteed to exceed the required depth for 500ms blocks
// built with a relay parent offset of 1. It must be larger than the unincluded segment capacity.
//
// The formula we use to compute the capacity of the unincluded segment in the parachain runtime
// is:
// UNINCLUDED_SEGMENT_CAPACITY = (2 + RELAY_PARENT_OFFSET) * BLOCK_PROCESSING_VELOCITY + 1.
//
// Since we only search for parent blocks which have already been imported,
// we can guarantee that all imported blocks respect the unincluded segment
// rules specified by the parachain's runtime and thus will never be too deep. This is just an extra
// sanity check.
const PARENT_SEARCH_DEPTH: usize = 40;

/// The slot offset to start pre-connecting to backing groups. Represented as number
/// of seconds before own slot starts.
const PRE_CONNECT_SLOT_OFFSET: Duration = Duration::from_secs(6);

/// Task name for the collator protocol helper.
pub const COLLATOR_PROTOCOL_HELPER_TASK_GROUP: &str = "collator-protocol-helper";

/// Helper for triggering backing group connections early.
///
/// Returns the updated `our_slot` value.
pub(crate) async fn update_backing_group_connections<Block, Client, P, Spawner>(
	client: &std::sync::Arc<Client>,
	keystore: &sp_keystore::KeystorePtr,
	overseer_handle: &mut cumulus_relay_chain_interface::OverseerHandle,
	spawn_handle: &Spawner,
	best_block: Block::Hash,
	slot_duration: SlotDuration,
	current_slot: Slot,
	our_slot: Option<Slot>,
) -> Option<Slot>
where
	Block: sp_runtime::traits::Block,
	Client: sc_client_api::HeaderBackend<Block> + Send + Sync + ProvideRuntimeApi<Block> + 'static,
	Client::Api: AuraApi<Block, P::Public>,
	P: sp_core::Pair + Send + Sync,
	P::Public: Codec,
	Spawner: sp_core::traits::SpawnNamed + Clone,
{
	let authorities = client.runtime_api().authorities(best_block).ok()?;

	// Check if our slot has passed and we are not expected to author again in next slot.
	match (
		our_slot,
		aura_internal::claim_slot::<P>(current_slot + 1, &authorities, &keystore)
			.await
			.is_none(),
	) {
		(Some(last_slot), true) if current_slot > last_slot => {
			tracing::debug!(target: crate::LOG_TARGET, "Our slot {} has passed, current slot is {}, sending disconnect message", last_slot, current_slot);

			// Send a message to the collator protocol to stop pre-connecting to backing
			// groups
			overseer_handle
				.send_msg(
					CollatorProtocolMessage::DisconnectFromBackingGroups,
					"CollatorProtocolHelper",
				)
				.await;

			return None;
		},
		(Some(_), false) => {
			// `our_slot` is `Some` means we alredy sent pre-connect message, no need to
			// proceed further.
			return our_slot
		},
		_ => {},
	}

	// Check if our slot is coming up next. This means that there is still another slot
	// before our turn.
	let target_slot = current_slot + 2;
	if aura_internal::claim_slot::<P>(target_slot, &authorities, &keystore)
		.await
		.is_none()
	{
		return our_slot
	}

	tracing::debug!(target: crate::LOG_TARGET, "Our slot {} is due soon", target_slot );

	// Determine our own slot timestamp.
	let Some(own_slot_ts) = target_slot.timestamp(slot_duration) else {
		tracing::warn!(target: crate::LOG_TARGET, "Failed to get own slot timestamp");

		return our_slot;
	};

	let pre_connect_delay = own_slot_ts
		.saturating_sub(*Timestamp::current())
		.saturating_sub(PRE_CONNECT_SLOT_OFFSET.as_millis() as u64);

	tracing::debug!(target: crate::LOG_TARGET, "Pre-connecting to backing groups in {}ms", pre_connect_delay);

	let mut overseer_handle_clone = overseer_handle.clone();
	spawn_handle.spawn(
		"send-pre-connect-message",
		Some(COLLATOR_PROTOCOL_HELPER_TASK_GROUP),
		async move {
			futures_timer::Delay::new(std::time::Duration::from_millis(pre_connect_delay)).await;

			tracing::debug!(target: crate::LOG_TARGET, "Sending pre-connect message");

			// Send a message to the collator protocol to pre-connect to backing groups
			overseer_handle_clone
				.send_msg(CollatorProtocolMessage::ConnectToBackingGroups, "CollatorProtocolHelper")
				.await;
		}
		.boxed(),
	);
	Some(target_slot)
}

/// Check the `local_validation_code_hash` against the validation code hash in the relay chain
/// state.
///
/// If the code hashes do not match, it prints a warning.
async fn check_validation_code_or_log(
	local_validation_code_hash: &ValidationCodeHash,
	para_id: ParaId,
	relay_client: &impl RelayChainInterface,
	relay_parent: RelayHash,
) {
	let state_validation_code_hash = match relay_client
		.validation_code_hash(relay_parent, para_id, OccupiedCoreAssumption::Included)
		.await
	{
		Ok(hash) => hash,
		Err(error) => {
			tracing::debug!(
				target: super::LOG_TARGET,
				%error,
				?relay_parent,
				%para_id,
				"Failed to fetch validation code hash",
			);
			return
		},
	};

	match state_validation_code_hash {
		Some(state) =>
			if state != *local_validation_code_hash {
				tracing::warn!(
					target: super::LOG_TARGET,
					%para_id,
					?relay_parent,
					?local_validation_code_hash,
					relay_validation_code_hash = ?state,
					"Parachain code doesn't match validation code stored in the relay chain state.",
				);
			},
		None => {
			tracing::warn!(
				target: super::LOG_TARGET,
				%para_id,
				?relay_parent,
				"Could not find validation code for parachain in the relay chain state.",
			);
		},
	}
}

/// Fetch scheduling lookahead at given relay parent.
async fn scheduling_lookahead(
	relay_parent: RelayHash,
	relay_client: &impl RelayChainInterface,
) -> Option<u32> {
	let runtime_api_version = relay_client
		.version(relay_parent)
		.await
		.map_err(|e| {
			tracing::error!(
				target: super::LOG_TARGET,
				error = ?e,
				"Failed to fetch relay chain runtime version.",
			)
		})
		.ok()?;

	let parachain_host_runtime_api_version = runtime_api_version
		.api_version(
			&<dyn polkadot_primitives::runtime_api::ParachainHost<polkadot_primitives::Block>>::ID,
		)
		.unwrap_or_default();

	if parachain_host_runtime_api_version <
		RuntimeApiRequest::SCHEDULING_LOOKAHEAD_RUNTIME_REQUIREMENT
	{
		return None
	}

	match relay_client.scheduling_lookahead(relay_parent).await {
		Ok(scheduling_lookahead) => Some(scheduling_lookahead),
		Err(err) => {
			tracing::error!(
				target: crate::LOG_TARGET,
				?err,
				?relay_parent,
				"Failed to fetch scheduling lookahead from relay chain",
			);
			None
		},
	}
}

// Returns the claim queue at the given relay parent.
async fn claim_queue_at(
	relay_parent: RelayHash,
	relay_client: &impl RelayChainInterface,
) -> ClaimQueueSnapshot {
	// Get `ClaimQueue` from runtime
	match relay_client.claim_queue(relay_parent).await {
		Ok(claim_queue) => claim_queue.into(),
		Err(error) => {
			tracing::error!(
				target: crate::LOG_TARGET,
				?error,
				?relay_parent,
				"Failed to query claim queue runtime API",
			);
			Default::default()
		},
	}
}

// Checks if we own the slot at the given block and whether there
// is space in the unincluded segment.
async fn can_build_upon<Block: BlockT, Client, P>(
	para_slot: Slot,
	relay_slot: Slot,
	timestamp: Timestamp,
	parent_hash: Block::Hash,
	included_block: Block::Hash,
	client: &Client,
	keystore: &KeystorePtr,
) -> Option<SlotClaim<P::Public>>
where
	Client: ProvideRuntimeApi<Block>,
	Client::Api: AuraApi<Block, P::Public> + AuraUnincludedSegmentApi<Block> + ApiExt<Block>,
	P: Pair,
	P::Public: Codec,
	P::Signature: Codec,
{
	let runtime_api = client.runtime_api();
	let authorities = runtime_api.authorities(parent_hash).ok()?;
	let author_pub = aura_internal::claim_slot::<P>(para_slot, &authorities, keystore).await?;

	// This function is typically called when we want to build block N. At that point, the
	// unincluded segment in the runtime is unaware of the hash of block N-1. If the unincluded
	// segment in the runtime is full, but block N-1 is the included block, the unincluded segment
	// should have length 0 and we can build. Since the hash is not available to the runtime
	// however, we need this extra check here.
	if parent_hash == included_block {
		return Some(SlotClaim::unchecked::<P>(author_pub, para_slot, timestamp));
	}

	let api_version = runtime_api
		.api_version::<dyn AuraUnincludedSegmentApi<Block>>(parent_hash)
		.ok()
		.flatten()?;

	let slot = if api_version > 1 { relay_slot } else { para_slot };

	runtime_api
		.can_build_upon(parent_hash, included_block, slot)
		.ok()?
		.then(|| SlotClaim::unchecked::<P>(author_pub, para_slot, timestamp))
}

/// Use [`cumulus_client_consensus_common::find_potential_parents`] to find parachain blocks that
/// we can build on. Once a list of potential parents is retrieved, return the last one of the
/// longest chain.
async fn find_parent<Block>(
	relay_parent: RelayHash,
	para_id: ParaId,
	para_backend: &impl sc_client_api::Backend<Block>,
	relay_client: &impl RelayChainInterface,
) -> Option<(<Block as BlockT>::Header, consensus_common::PotentialParent<Block>)>
where
	Block: BlockT,
{
	let parent_search_params = ParentSearchParams {
		relay_parent,
		para_id,
		ancestry_lookback: scheduling_lookahead(relay_parent, relay_client)
			.await
			.unwrap_or(DEFAULT_SCHEDULING_LOOKAHEAD)
			.saturating_sub(1) as usize,
		max_depth: PARENT_SEARCH_DEPTH,
		ignore_alternative_branches: true,
	};

	let potential_parents = cumulus_client_consensus_common::find_potential_parents::<Block>(
		parent_search_params,
		para_backend,
		relay_client,
	)
	.await;

	let potential_parents = match potential_parents {
		Err(e) => {
			tracing::error!(
				target: crate::LOG_TARGET,
				?relay_parent,
				err = ?e,
				"Could not fetch potential parents to build upon"
			);

			return None
		},
		Ok(x) => x,
	};

	let included_block = potential_parents.iter().find(|x| x.depth == 0)?.header.clone();
	potential_parents
		.into_iter()
		.max_by_key(|a| a.depth)
		.map(|parent| (included_block, parent))
}

#[cfg(test)]
mod tests {
	use crate::collators::can_build_upon;
	use codec::Encode;
	use cumulus_primitives_aura::Slot;
	use cumulus_primitives_core::BlockT;
	use cumulus_relay_chain_interface::PHash;
	use cumulus_test_client::{
		runtime::{Block, Hash},
		Client, DefaultTestClientBuilderExt, InitBlockBuilder, TestClientBuilder,
		TestClientBuilderExt,
	};
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use polkadot_primitives::HeadData;
	use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy};
	use sp_consensus::BlockOrigin;
	use sp_keystore::{Keystore, KeystorePtr};
	use sp_timestamp::Timestamp;
	use std::sync::Arc;

	async fn import_block<I: BlockImport<Block>>(
		importer: &I,
		block: Block,
		origin: BlockOrigin,
		import_as_best: bool,
	) {
		let (header, body) = block.deconstruct();

		let mut block_import_params = BlockImportParams::new(origin, header);
		block_import_params.fork_choice = Some(ForkChoiceStrategy::Custom(import_as_best));
		block_import_params.body = Some(body);
		importer.import_block(block_import_params).await.unwrap();
	}

	fn sproof_with_parent_by_hash(client: &Client, hash: PHash) -> RelayStateSproofBuilder {
		let header = client.header(hash).ok().flatten().expect("No header for parent block");
		let included = HeadData(header.encode());
		let mut builder = RelayStateSproofBuilder::default();
		builder.para_id = cumulus_test_client::runtime::PARACHAIN_ID.into();
		builder.included_para_head = Some(included);

		builder
	}
	async fn build_and_import_block(client: &Client, included: Hash) -> Block {
		let sproof = sproof_with_parent_by_hash(client, included);

		let block_builder = client.init_block_builder(None, sproof).block_builder;

		let block = block_builder.build().unwrap().block;

		let origin = BlockOrigin::NetworkInitialSync;
		import_block(client, block.clone(), origin, true).await;
		block
	}

	fn set_up_components() -> (Arc<Client>, KeystorePtr) {
		let keystore = Arc::new(sp_keystore::testing::MemoryKeystore::new()) as Arc<_>;
		for key in sp_keyring::Sr25519Keyring::iter() {
			Keystore::sr25519_generate_new(
				&*keystore,
				sp_application_crypto::key_types::AURA,
				Some(&key.to_seed()),
			)
			.expect("Can insert key into MemoryKeyStore");
		}
		(Arc::new(TestClientBuilder::new().build()), keystore)
	}

	/// This tests a special scenario where the unincluded segment in the runtime
	/// is full. We are calling `can_build_upon`, passing the last built block as the
	/// included one. In the runtime we will not find the hash of the included block in the
	/// unincluded segment. The `can_build_upon` runtime API would therefore return `false`, but
	/// we are ensuring on the node side that we are are always able to build on the included block.
	#[tokio::test]
	async fn test_can_build_upon() {
		let (client, keystore) = set_up_components();

		let genesis_hash = client.chain_info().genesis_hash;
		let mut last_hash = genesis_hash;

		// Fill up the unincluded segment tracker in the runtime.
		while can_build_upon::<_, _, sp_consensus_aura::sr25519::AuthorityPair>(
			Slot::from(u64::MAX),
			Slot::from(u64::MAX),
			Timestamp::default(),
			last_hash,
			genesis_hash,
			&*client,
			&keystore,
		)
		.await
		.is_some()
		{
			let block = build_and_import_block(&client, genesis_hash).await;
			last_hash = block.header().hash();
		}

		// Blocks were built with the genesis hash set as included block.
		// We call `can_build_upon` with the last built block as the included block.
		let result = can_build_upon::<_, _, sp_consensus_aura::sr25519::AuthorityPair>(
			Slot::from(u64::MAX),
			Slot::from(u64::MAX),
			Timestamp::default(),
			last_hash,
			last_hash,
			&*client,
			&keystore,
		)
		.await;
		assert!(result.is_some());
	}
}

/// Holds a relay parent and its descendants.
pub struct RelayParentData {
	/// The relay parent block header
	relay_parent: RelayHeader,
	/// Ordered collection of descendant block headers, from oldest to newest
	descendants: Vec<RelayHeader>,
}

impl RelayParentData {
	/// Creates a new instance with the given relay parent and no descendants.
	pub fn new(relay_parent: RelayHeader) -> Self {
		Self { relay_parent, descendants: Default::default() }
	}

	/// Creates a new instance with the given relay parent and descendants.
	pub fn new_with_descendants(relay_parent: RelayHeader, descendants: Vec<RelayHeader>) -> Self {
		Self { relay_parent, descendants }
	}

	/// Returns a reference to the relay parent header.
	pub fn relay_parent(&self) -> &RelayHeader {
		&self.relay_parent
	}

	/// Returns the number of descendants.
	#[cfg(test)]
	pub fn descendants_len(&self) -> usize {
		self.descendants.len()
	}

	/// Consumes the structure and returns a vector containing the relay parent followed by its
	/// descendants in chronological order. The resulting list should be provided to the parachain
	/// inherent data.
	pub fn into_inherent_descendant_list(self) -> Vec<RelayHeader> {
		let Self { relay_parent, mut descendants } = self;

		if descendants.is_empty() {
			return Default::default()
		}

		let mut result = vec![relay_parent];
		result.append(&mut descendants);
		result
	}
}
