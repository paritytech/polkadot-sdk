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

//! Per-block hydration for the V3 unincluded segment.
//!
//! The block builder sends just headers across; the collation task calls [`hydrate_segment`] to
//! re-assemble each unincluded parablock into a [`SegmentEntry`] (body + storage proof + relay
//! data + validation-code hash) which the collator service can turn into a `SegmentCollation`.

use super::relay_chain_data_cache::RelayChainDataCache;
use codec::Encode;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_unincluded_segment_store::UnincludedSegmentStore;
use cumulus_primitives_core::{
	extract_relay_parent, relay_chain::Hash as RelayHash, PersistedValidationData,
};
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_primitives::ValidationCodeHash;
use sc_client_api::{backend::AuxStore, Backend};
use sp_api::StorageProof;
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};

/// One unincluded parablock's collation-ready payload.
pub struct SegmentEntry<Block: BlockT> {
	pub relay_parent: RelayHash,
	pub parent_header: Block::Header,
	pub blocks: Vec<Block>,
	pub proof: StorageProof,
	pub validation_code_hash: ValidationCodeHash,
	pub validation_data: PersistedValidationData,
}

const LOG_TARGET: &str = "consensus::slot_based::unincluded_segment";

/// Hydrate a list of unincluded-segment headers into [`SegmentEntry`]s by calling
/// [`build_entry`] on each. Headers that fail to hydrate locally are logged and skipped — the
/// rest of the segment is preserved.
pub(super) async fn hydrate_segment<Block, B, Client, RClient, CHP>(
	headers: Vec<Block::Header>,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &UnincludedSegmentStore<Block, Client>,
	relay_chain_data_cache: &mut RelayChainDataCache<RClient>,
) -> Vec<SegmentEntry<Block>>
where
	Block: BlockT,
	B: Backend<Block>,
	Client: AuxStore,
	RClient: RelayChainInterface + Clone + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	let mut entries = Vec::with_capacity(headers.len());
	for header in headers {
		match build_entry(header, para_backend, code_hash_provider, store, relay_chain_data_cache)
			.await
		{
			Some(entry) => entries.push(entry),
			None => tracing::warn!(
				target: LOG_TARGET,
				"Skipping unincluded-segment entry: could not hydrate header (missing body/proof/relay data).",
			),
		}
	}
	entries
}

/// Rebuild a [`SegmentEntry`] for one unincluded parablock by combining its header with the
/// locally-stored body and storage proof, the relay-chain data at the block's relay parent, and
/// the validation-code hash at the block's para parent.
///
/// Returns `None` when any input is missing locally (no body, no stored proof, no relay data,
/// no code hash) — the caller skips that entry rather than aborting the whole segment.
pub(super) async fn build_entry<Block, B, Client, RClient, CHP>(
	header: Block::Header,
	para_backend: &B,
	code_hash_provider: &CHP,
	store: &UnincludedSegmentStore<Block, Client>,
	relay_chain_data_cache: &mut RelayChainDataCache<RClient>,
) -> Option<SegmentEntry<Block>>
where
	Block: BlockT,
	B: Backend<Block>,
	Client: AuxStore,
	RClient: RelayChainInterface + Clone + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	let block_hash = header.hash();
	let parent_hash = *header.parent_hash();

	let relay_parent = extract_relay_parent(header.digest())?;

	let parent_header = para_backend.blockchain().header(parent_hash).ok().flatten()?;
	let body = para_backend.blockchain().body(block_hash).ok().flatten()?;
	let block = Block::new(header, body);

	let stored = store.load(block_hash).ok().flatten()?;

	let relay_data = relay_chain_data_cache.get_by_hash(relay_parent).await.ok()?;
	let validation_data = PersistedValidationData {
		parent_head: parent_header.encode().into(),
		relay_parent_number: *relay_data.relay_header.number(),
		relay_parent_storage_root: *relay_data.relay_header.state_root(),
		max_pov_size: relay_data.max_pov_size,
	};

	let validation_code_hash = code_hash_provider.code_hash_at(parent_hash)?;

	Some(SegmentEntry {
		relay_parent,
		parent_header,
		blocks: vec![block],
		proof: stored.proof,
		validation_code_hash,
		validation_data,
	})
}
