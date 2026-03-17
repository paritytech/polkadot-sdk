// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Outbound message distributor.
//!
//! Watches best (not finalized) parachain blocks and distributes pending
//! outgoing messages to destination relay peers. This is the "speculative"
//! part — messages are forwarded before they are finalized, achieving ~1
//! relay block latency.

use std::sync::Arc;

use codec::{Decode, Encode};
use futures::StreamExt;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives_speculative_messaging::{MessageBatch, OutgoingMessage, StoredMerkleTree};
use sc_client_api::{Backend, BlockchainEvents, StorageProvider};
use sp_core::storage::StorageKey;
use sp_runtime::traits::Block as BlockT;

use crate::{
	registry::PeerRegistry,
	service::{NetworkTransport, SpeculativeMessagingWorker},
};

const LOG_TARGET: &str = "spec-msg::outbound";

/// Run the outbound distribution loop.
///
/// Subscribes to the parachain client's block import notifications and,
/// for each new best block, reads `PendingOutgoing` storage, constructs
/// [`MessageBatch`]es with Merkle proofs, and distributes them via the
/// worker's [`SpeculativeMessagingWorker::distribute_outgoing`].
pub async fn run<Block, BE, Client, R, N>(
	client: Arc<Client>,
	worker: Arc<SpeculativeMessagingWorker<R, N>>,
	para_id: ParaId,
) where
	Block: BlockT,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + BlockchainEvents<Block> + 'static,
	R: PeerRegistry + 'static,
	N: NetworkTransport + 'static,
{
	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		"Starting outbound message distributor (best-block mode)",
	);

	let mut import_stream = client.import_notification_stream();

	while let Some(notification) = import_stream.next().await {
		if !notification.is_new_best {
			continue;
		}
		let block_hash = notification.hash;

		if let Err(e) = process_block::<Block, BE, Client, R, N>(
			&client,
			&worker,
			para_id,
			block_hash,
		)
		.await
		{
			tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				?e,
				"Failed to process block for outbound messages",
			);
		}
	}

	tracing::info!(target: LOG_TARGET, "Outbound message distributor exiting");
}

/// Storage key helpers using Substrate's Twox128 prefix scheme.
mod storage_keys {
	use sp_core::storage::StorageKey;

	fn twox_128(data: &[u8]) -> [u8; 16] {
		sp_io::hashing::twox_128(data)
	}

	/// Return the storage prefix for `SpeculativeMessaging::PendingOutgoing`.
	pub fn pending_outgoing_prefix() -> StorageKey {
		let mut key = Vec::with_capacity(32);
		key.extend_from_slice(&twox_128(b"SpeculativeMessaging"));
		key.extend_from_slice(&twox_128(b"PendingOutgoing"));
		StorageKey(key)
	}

	/// Return the storage key for `SpeculativeMessaging::TopLevelTree`.
	pub fn top_level_tree_key() -> StorageKey {
		let mut key = Vec::with_capacity(32);
		key.extend_from_slice(&twox_128(b"SpeculativeMessaging"));
		key.extend_from_slice(&twox_128(b"TopLevelTree"));
		StorageKey(key)
	}

	/// Return the storage prefix for `SpeculativeMessaging::RelayPeers`.
	pub fn relay_peers_prefix() -> StorageKey {
		let mut key = Vec::with_capacity(32);
		key.extend_from_slice(&twox_128(b"SpeculativeMessaging"));
		key.extend_from_slice(&twox_128(b"RelayPeers"));
		StorageKey(key)
	}

	/// Decode a `ParaId` from a `PendingOutgoing` map storage key.
	///
	/// Key format: `pallet_prefix(16) + storage_prefix(16) + twox_64(dest)(8) + dest_encoded`
	pub fn decode_destination(key_bytes: &[u8]) -> Option<polkadot_parachain_primitives::primitives::Id> {
		// 32 bytes prefix + 8 bytes twox_64 hash
		if key_bytes.len() <= 40 {
			return None;
		}
		let dest_encoded = &key_bytes[40..];
		codec::Decode::decode(&mut &dest_encoded[..]).ok()
	}
}

async fn process_block<Block, BE, Client, R, N>(
	client: &Arc<Client>,
	worker: &Arc<SpeculativeMessagingWorker<R, N>>,
	para_id: ParaId,
	block_hash: Block::Hash,
) -> Result<(), Box<dyn std::error::Error>>
where
	Block: BlockT,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
	R: PeerRegistry + 'static,
	N: NetworkTransport + 'static,
{
	let prefix = storage_keys::pending_outgoing_prefix();

	// Read all PendingOutgoing storage keys
	let keys: Vec<StorageKey> =
		client.storage_keys(block_hash, Some(&prefix), None)?.collect();

	if keys.is_empty() {
		return Ok(());
	}

	// Read TopLevelTree for proof generation
	let tree_key = storage_keys::top_level_tree_key();
	let top_level_tree: StoredMerkleTree = client
		.storage(block_hash, &tree_key)?
		.and_then(|data| Decode::decode(&mut &data.0[..]).ok())
		.ok_or("TopLevelTree not found in storage")?;

	let provides_root = top_level_tree.provides_commitment().root;

	// Sync RelayPeers from storage into the worker's registry so
	// distribute_outgoing can look up destination peers.
	sync_relay_peers::<Block, BE, Client, R, N>(client, worker, block_hash)?;

	let mut batches = Vec::new();

	for storage_key in &keys {
		let destination = match storage_keys::decode_destination(&storage_key.0) {
			Some(id) => id,
			None => continue,
		};

		// Read the messages for this destination
		let messages: Vec<OutgoingMessage> = match client.storage(block_hash, storage_key)? {
			Some(data) => match Decode::decode(&mut &data.0[..]) {
				Ok(msgs) => msgs,
				Err(_) => continue,
			},
			None => continue,
		};

		if messages.is_empty() {
			continue;
		}

		// Generate subtree proof for this destination
		let (subtree_root, subtree_proof) =
			match top_level_tree.generate_proof(destination) {
				Ok(proof) => proof,
				Err(e) => {
					tracing::warn!(
						target: LOG_TARGET,
						?destination,
						?e,
						"Failed to generate subtree proof",
					);
					continue;
				},
			};

		let batch = MessageBatch {
			source: para_id,
			source_block: block_hash.using_encoded(|b| {
				sp_core::H256::from_slice(&b[..32])
			}),
			provides_root,
			subtree_root,
			subtree_inclusion_proof: subtree_proof,
			messages,
		};

		tracing::info!(
			target: LOG_TARGET,
			?destination,
			msg_count = batch.messages.len(),
			"Distributing spec-msg batch",
		);

		batches.push((destination, batch));
	}

	if !batches.is_empty() {
		let results = worker.distribute_outgoing(batches).await;
		for (dest, result) in results {
			match result {
				Ok(()) => tracing::debug!(
					target: LOG_TARGET,
					?dest,
					"Successfully distributed batch",
				),
				Err(e) => tracing::warn!(
					target: LOG_TARGET,
					?dest,
					?e,
					"Failed to distribute batch",
				),
			}
		}
	}

	Ok(())
}

/// Read `RelayPeers` storage and populate the worker's peer registry.
fn sync_relay_peers<Block, BE, Client, R, N>(
	client: &Arc<Client>,
	worker: &Arc<SpeculativeMessagingWorker<R, N>>,
	block_hash: Block::Hash,
) -> Result<(), Box<dyn std::error::Error>>
where
	Block: BlockT,
	BE: Backend<Block> + 'static,
	Client: StorageProvider<Block, BE> + 'static,
	R: PeerRegistry + 'static,
	N: NetworkTransport + 'static,
{
	let prefix = storage_keys::relay_peers_prefix();
	let keys: Vec<StorageKey> =
		client.storage_keys(block_hash, Some(&prefix), None)?.collect();

	for key in &keys {
		let para_id = match storage_keys::decode_destination(&key.0) {
			Some(id) => id,
			None => continue,
		};

		let peer_id_bytes: Vec<u8> = match client.storage(block_hash, key)? {
			Some(data) => match Decode::decode(&mut &data.0[..]) {
				Ok(bytes) => bytes,
				Err(_) => continue,
			},
			None => continue,
		};

		worker.registry().set_peer(para_id, peer_id_bytes);
	}

	Ok(())
}
