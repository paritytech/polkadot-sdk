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

//! The archiver: keeps the [`Archive`] current.
//!
//! - **New best block:** rewind past retracted blocks, then archive each enacted block's sends.
//! - **First start:** replay from the first block whose runtime has `SpecMsgApi`. This needs that
//!   history's state; without it the node logs and stays idle.
//! - **Finality:** apply retention with watermarks read at the finalized block.

use crate::{archive::Archive, store::ArchiveStore, LOG_TARGET};
use cumulus_primitives_core::SpecMsgApi;
use cumulus_primitives_spec_messaging::{streams_root::read_streams_root, StreamId};
use futures::{select, StreamExt};
use parking_lot::RwLock;
use sc_client_api::BlockchainEvents;
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, Zero},
	SaturatedConversion,
};
use std::{
	collections::BTreeMap,
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

/// How long block boundaries, and the proof material of streams without a watermark, are kept:
/// the design's serving horizon (§ Liftability), mirroring relay-chain availability retention.
pub const SERVING_HORIZON: Duration = Duration::from_secs(25 * 60 * 60);

fn now() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Keep `archive` in step with `client` until the client's notification streams end. Calls the
/// runtime and the store synchronously; spawn it as a blocking task.
pub async fn run_archiver<Block, C, S>(
	client: Arc<C>,
	archive: Arc<RwLock<Archive<Block::Hash, S>>>,
) where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>
		+ BlockchainEvents<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>
		+ Send
		+ Sync
		+ 'static,
	C::Api: SpecMsgApi<Block>,
	S: ArchiveStore,
{
	let mut imports = client.import_notification_stream().fuse();
	let mut finality = client.finality_notification_stream().fuse();
	let archiver = Archiver { client, archive };
	archiver.follow(archiver.client.info().best_hash);
	loop {
		select! {
			import = imports.next() => match import {
				Some(import) if import.is_new_best => archiver.follow(import.hash),
				Some(_) => {},
				None => return,
			},
			finalized = finality.next() => match finalized {
				Some(finalized) => archiver.retain(finalized.hash),
				None => return,
			},
		}
	}
}

struct Archiver<Block: BlockT, C, S> {
	client: Arc<C>,
	archive: Arc<RwLock<Archive<Block::Hash, S>>>,
}

impl<Block, C, S> Archiver<Block, C, S>
where
	Block: BlockT,
	C: ProvideRuntimeApi<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>,
	C::Api: SpecMsgApi<Block>,
	S: ArchiveStore,
{
	fn has_api(&self, hash: Block::Hash) -> Result<bool, sp_api::ApiError> {
		self.client.runtime_api().has_api::<dyn SpecMsgApi<Block>>(hash)
	}

	/// Bring the archive's tip to `target`.
	fn follow(&self, target: Block::Hash) {
		let tip = self.archive.read().tip();
		match tip {
			Some(tip) if tip.hash == target => {},
			Some(tip) => {
				let route = match sp_blockchain::tree_route(&*self.client, tip.hash, target) {
					Ok(route) => route,
					Err(e) => {
						log::warn!(target: LOG_TARGET, "No route from the archived tip: {e}");
						return;
					},
				};
				if !route.retracted().is_empty() {
					let common = route.common_block();
					if let Err(e) =
						self.archive.write().rewind_to(common.hash, common.number.saturated_into())
					{
						log::warn!(target: LOG_TARGET, "Archive rewind failed: {e:?}");
						return;
					}
				}
				for block in route.enacted() {
					if !self.import(block.hash) {
						return;
					}
				}
			},
			None => {
				let Some(origin) = self.find_origin(target) else { return };
				let mut chain = vec![target];
				while chain.last() != Some(&origin) {
					let Some(parent) = chain
						.last()
						.and_then(|hash| self.client.header(*hash).ok().flatten())
						.map(|header| *header.parent_hash())
					else {
						return;
					};
					chain.push(parent);
				}
				log::info!(
					target: LOG_TARGET,
					"Building the spec-msg archive from {origin:?}: {} blocks",
					chain.len(),
				);
				for hash in chain.into_iter().rev() {
					if !self.import(hash) {
						return;
					}
				}
			},
		}
	}

	/// The first block of the run of API-bearing blocks that ends at `target`, or `None` if
	/// `target` has no API or that run's start cannot be read.
	///
	/// TODO(spec-msg): with pruned state, this and `import` fail for good: a fresh node,
	/// or one down longer than the pruning window, never serves again. Seed the archive at the
	/// first block with state from an `outbound_frontiers()` runtime API (peaks and leaf count per
	/// stream; both floors at the leaf count), then replay from there.
	fn find_origin(&self, target: Block::Hash) -> Option<Block::Hash> {
		if !self.has_api(target).unwrap_or(false) {
			return None;
		}
		let mut origin = target;
		loop {
			let header = self.client.header(origin).ok().flatten()?;
			if header.number().is_zero() {
				return Some(origin);
			}
			let parent = *header.parent_hash();
			match self.has_api(parent) {
				Ok(true) => origin = parent,
				Ok(false) => return Some(origin),
				Err(e) => {
					log::warn!(
						target: LOG_TARGET,
						"Cannot build the spec-msg archive: state below #{} is unavailable ({e}). \
						 It needs a node that executed the chain since spec-msg was enabled.",
						header.number(),
					);
					return None;
				},
			}
		}
	}

	/// Archive one block. `false` stops the current walk; the next best block retries.
	fn import(&self, hash: Block::Hash) -> bool {
		let Ok(Some(header)) = self.client.header(hash) else {
			log::warn!(target: LOG_TARGET, "Missing header {hash:?}");
			return false;
		};
		let sends = match self.has_api(hash) {
			Ok(true) => match self.client.runtime_api().outbound_messages(hash) {
				Ok(sends) => sends,
				Err(e) => {
					log::warn!(target: LOG_TARGET, "outbound_messages at {hash:?} failed: {e}");
					return false;
				},
			},
			Ok(false) => Vec::new(),
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Runtime version at {hash:?} unavailable: {e}");
				return false;
			},
		};
		let committed = read_streams_root(header.digest());
		let sent: usize = sends.iter().map(|(_, payloads)| payloads.len()).sum();
		let result = self.archive.write().import_block(
			hash,
			*header.parent_hash(),
			(*header.number()).saturated_into(),
			now(),
			sends,
			committed,
		);
		if let Err(e) = result {
			log::warn!(target: LOG_TARGET, "Archiving {hash:?} failed: {e:?}");
			return false;
		}
		if sent > 0 {
			log::debug!(
				target: LOG_TARGET,
				"Archived block #{} {hash:?}: {sent} sends, committing {committed:?}",
				header.number(),
			);
		}
		true
	}

	/// Apply retention at `finalized`, if archived.
	fn retain(&self, finalized: Block::Hash) {
		if !self.archive.read().contains(&finalized) {
			return;
		}
		let mut watermarks = BTreeMap::new();
		if self.has_api(finalized).unwrap_or(false) {
			match self.client.runtime_api().out_channels(finalized) {
				Ok(channels) => {
					for (channel, state) in channels {
						if let Some(read) = state.register {
							let stream = StreamId::Channel {
								recipient: channel.peer,
								domain: channel.domain,
								num: channel.num,
							};
							watermarks.insert(stream, read.register.up_to.0);
						}
					}
				},
				Err(e) => {
					log::warn!(target: LOG_TARGET, "out_channels at {finalized:?} failed: {e}");
					return;
				},
			}
		}
		let cutoff = now().saturating_sub(SERVING_HORIZON.as_secs());
		if let Err(e) = self.archive.write().retain(finalized, &watermarks, cutoff) {
			log::warn!(target: LOG_TARGET, "Archive retention failed: {e:?}");
		}
	}
}
