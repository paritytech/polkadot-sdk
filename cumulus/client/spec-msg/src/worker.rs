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

//! The archiver worker: follows the own chain's best blocks and keeps the
//! sender-side archive current.
//!
//! Per best block: extract the block's sends via the `SpecMsgApi` runtime
//! API — version-gated exactly like collation info: runtimes without the
//! API keep the archive idle — append them to the archive, then run
//! retention (watermark pruning from the `out_channels()` register views,
//! horizon sweep).
//!
//! Branch handling: the worker walks the new best branch back to the
//! archived tip (rewinding the archive to the common ancestor on reorgs)
//! or, on a fresh archive, to the chain's spec-msg *origin* — the first
//! block whose runtime exposes the API (everything before it has no
//! streams by definition). A node that cannot reach either (state pruned
//! below the fork/origin) logs and idles: rebuilding the archive from the
//! chain's other nodes via the fetch protocol is the recovery path (not
//! part of the MVP; re-syncing with execution rebuilds it too, via exactly
//! this code path).

use std::sync::Arc;

use futures::StreamExt;
use parking_lot::RwLock;
use sc_client_api::{AuxStore, BlockchainEvents};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, One};

use cumulus_primitives_core::SpecMsgApi;
use cumulus_primitives_spec_messaging::StreamId;
use polkadot_core_primitives::Hash;

use crate::{
	archive::{now_secs, ArchiveError, SpecMsgArchive, SERVING_HORIZON},
	LOG_TARGET,
};

/// Hard bound on blocks imported per notification: deeper gaps mean the
/// node was away for longer than the serving horizon covers anyway.
const MAX_CATCHUP_BLOCKS: usize = 20_480;

/// Errors handling one best-block notification.
#[derive(Debug, thiserror::Error)]
enum WorkerError {
	#[error("runtime api error: {0}")]
	Api(#[from] sp_api::ApiError),
	#[error("blockchain error: {0}")]
	Blockchain(#[from] sp_blockchain::Error),
	#[error("archive error: {0}")]
	Archive(#[from] ArchiveError),
	#[error("missing header for block {0:?}")]
	MissingHeader(Hash),
	#[error("best branch does not connect to the archive (deep reorg or pruned state)")]
	Disconnected,
	#[error("more than {MAX_CATCHUP_BLOCKS} blocks behind; archive needs a rebuild")]
	TooFarBehind,
}

/// Runs the archiver over the client's import notifications until the
/// stream ends. Spawn as an essential task next to the collation subsystems
/// (wired by the parachain service).
pub async fn run_spec_msg_archiver<Block, Client, AUX>(
	client: Arc<Client>,
	archive: Arc<RwLock<SpecMsgArchive<Block, AUX>>>,
) where
	Block: BlockT<Hash = Hash>,
	Client: BlockchainEvents<Block> + ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
	AUX: AuxStore,
{
	let mut notifications = client.import_notification_stream();
	while let Some(notification) = notifications.next().await {
		if !notification.is_new_best {
			continue;
		}
		if let Err(error) =
			archive_best_block(&*client, &archive, notification.hash, &notification.header)
		{
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				block = ?notification.hash,
				"Failed to archive block",
			);
		}
	}
}

/// Brings the archive up to the new best block `hash`.
fn archive_best_block<Block, Client, AUX>(
	client: &Client,
	archive: &RwLock<SpecMsgArchive<Block, AUX>>,
	hash: Block::Hash,
	header: &Block::Header,
) -> Result<(), WorkerError>
where
	Block: BlockT<Hash = Hash>,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
	AUX: AuxStore,
{
	if archive.read().contains_block(&hash) {
		return Ok(());
	}
	// The version gate: no API, no streams — stay idle.
	if !client.runtime_api().has_api::<dyn SpecMsgApi<Block>>(hash)? {
		return Ok(());
	}

	// Walk the best branch backwards to the archived tip or the spec-msg
	// origin, collecting what to import.
	let mut to_import = vec![(hash, header.clone())];
	let ancestor = loop {
		let (_, last) = to_import.last().expect("pushed before the loop; qed");
		let parent = *last.parent_hash();
		if archive.read().contains_block(&parent) {
			break Some(parent);
		}
		if *last.number() <= One::one() ||
			!client.runtime_api().has_api::<dyn SpecMsgApi<Block>>(parent)?
		{
			break None;
		}
		if to_import.len() >= MAX_CATCHUP_BLOCKS {
			return Err(WorkerError::TooFarBehind);
		}
		let parent_header = client.header(parent)?.ok_or(WorkerError::MissingHeader(parent))?;
		to_import.push((parent, parent_header));
	};

	match ancestor {
		// A reorg rewinds to the common ancestor before the new branch goes
		// in; `rewind_to` is a no-op when the ancestor IS the tip.
		Some(ancestor) => archive.write().rewind_to(ancestor)?,
		None if archive.read().tip().is_some() => return Err(WorkerError::Disconnected),
		None => (),
	}

	for (block_hash, block_header) in to_import.into_iter().rev() {
		let sends = client.runtime_api().outbound_messages(block_hash)?;
		let streams = sends.iter().filter(|(_, payloads)| !payloads.is_empty()).count();
		let leaves: usize = sends.iter().map(|(_, payloads)| payloads.len()).sum();
		let root = archive.write().import_block(
			block_hash,
			*block_header.parent_hash(),
			*block_header.number(),
			sends,
		)?;
		tracing::debug!(
			target: LOG_TARGET,
			number = ?block_header.number(),
			streams,
			leaves,
			?root,
			"Archived own block",
		);
	}

	// Retention: channel payloads below the peers' confirmation watermarks
	// (the latest verified register reads served by the `out_channels()`
	// view), then the horizon sweep.
	let out_channels = client.runtime_api().out_channels(hash)?;
	let mut archive = archive.write();
	for (channel, state) in out_channels {
		if let Some(register) = state.register {
			let stream = StreamId::Channel {
				recipient: channel.peer,
				domain: channel.domain,
				num: channel.num,
			};
			archive.prune_payloads(&stream, register.up_to)?;
		}
	}
	archive.prune_horizon(now_secs().saturating_sub(SERVING_HORIZON.as_secs()))?;

	Ok(())
}
