// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The para head in JAM state, read through logged JAM reads, and the head streams heads
//! following runs on.

use crate::nodes::jam::LOG_TARGET;
use codec::{Decode, DecodeAll};
use futures::{Stream, StreamExt};
use jam_interface::{HeaderHash, JamStateSource, ServiceId, Slot as JamSlot};
use parachain_service_core::{para_info_key, ParaInfo};
use sc_client_api::BlockchainEvents;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;
use std::{fmt::Debug, future::Future, sync::Arc, time::Instant};

/// Run one JAM read, logging the method, the block it was read at, what came back and the
/// round-trip: latency is a first-class debugging signal on this path.
pub(crate) async fn jam_read<T, F>(
	method: &'static str,
	at: HeaderHash,
	read: F,
) -> Result<T, String>
where
	F: Future<Output = jam_interface::Result<T>>,
	T: std::fmt::Debug,
{
	let started = Instant::now();
	let result = read.await;
	let elapsed_ms = started.elapsed().as_millis();
	match &result {
		Ok(value) => tracing::debug!(
			target: LOG_TARGET,
			method,
			?at,
			?value,
			elapsed_ms,
			"JAM read.",
		),
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			method,
			?at,
			?error,
			elapsed_ms,
			"JAM read failed.",
		),
	}
	result.map_err(|error| format!("{method}: {error}"))
}

/// The head hash a SCALE-encoded `ParaInfo` holds: `head_data` is the header hash of the para's
/// last included block.
pub(crate) fn head_hash_of<Hash: Decode>(para_info: &[u8]) -> Result<Hash, String> {
	let info = ParaInfo::decode(&mut &para_info[..]).map_err(|e| format!("ParaInfo: {e}"))?;
	Hash::decode_all(&mut &info.head_data[..]).map_err(|e| format!("head hash: {e}"))
}

/// Stream of para-head hashes for heads following: every change of the para's `ParaInfo` entry
/// in the parachain service's state yields the new head's header hash together with the slot of
/// the JAM block whose state changed it.
///
/// Absent values (para not registered yet) and undecodable values are dropped, matching the
/// relay-chain streams' behavior of dropping absent heads.
pub(crate) async fn para_head_stream<Hash: Decode + Debug + Send + 'static>(
	jam: &impl JamStateSource,
	parachain_service: ServiceId,
	para_id: u32,
	finalized: bool,
) -> jam_interface::Result<impl Stream<Item = (Hash, JamSlot)> + Send> {
	let key = para_info_key(para_id.into());
	let stream = jam.service_value_stream(parachain_service, &key, finalized).await?;
	Ok(stream.filter_map(move |update| {
		futures::future::ready(match update.value {
			Some(bytes) => match head_hash_of::<Hash>(&bytes) {
				Ok(head) => {
					tracing::debug!(
						target: LOG_TARGET,
						para_id,
						finalized,
						at = ?update.header_hash,
						slot = update.slot,
						?head,
						"Para head changed in JAM state.",
					);
					Some((head, update.slot))
				},
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						para_id,
						?error,
						"Failed to decode the para head from JAM state; dropping update.",
					);
					None
				},
			},
			None => {
				tracing::debug!(
					target: LOG_TARGET,
					para_id,
					finalized,
					at = ?update.header_hash,
					"Para not (yet) present in the parachain service's state.",
				);
				None
			},
		})
	}))
}

/// Resolve head hashes to headers: a head whose header `lookup` finds is yielded at once, any
/// other waits until its block is imported. Whatever `Extra` travels with a head (the JAM slot
/// it changed at) comes out beside its header.
///
/// A newer head replaces the one waiting, so only the latest head is reported. The stream ends
/// when `heads` ends.
pub(crate) fn resolve_heads<Hash, Header, Extra>(
	heads: impl Stream<Item = (Hash, Extra)> + Send + Unpin + 'static,
	imports: impl Stream<Item = (Hash, Header)> + Send + Unpin + 'static,
	lookup: impl Fn(&Hash) -> Option<Header> + Send + 'static,
) -> impl Stream<Item = (Header, Extra)> + Send
where
	Hash: PartialEq + Send + 'static,
	Header: Send + 'static,
	Extra: Send + 'static,
{
	enum Event<Hash, Header, Extra> {
		Head(Option<(Hash, Extra)>),
		Import(Option<(Hash, Header)>),
	}

	futures::stream::unfold(
		(heads.fuse(), imports.fuse(), lookup, None::<(Hash, Extra)>),
		|(mut heads, mut imports, lookup, mut pending)| async move {
			loop {
				let event = futures::select! {
					head = heads.next() => Event::Head(head),
					import = imports.next() => Event::Import(import),
				};
				match event {
					Event::Head(head) => {
						let (head, extra) = head?;
						match lookup(&head) {
							Some(header) => {
								return Some(((header, extra), (heads, imports, lookup, None)));
							},
							None => pending = Some((head, extra)),
						}
					},
					Event::Import(Some((hash, header)))
						if pending.as_ref().is_some_and(|(waiting, _)| *waiting == hash) =>
					{
						let (_, extra) = pending.take().expect("checked just above; qed");
						return Some(((header, extra), (heads, imports, lookup, None)));
					},
					Event::Import(_) => {},
				}
			}
		},
	)
}

/// The para's head headers, resolved from the head hashes in JAM state through the local
/// database, each with the slot of the JAM block that made it the head; see [`resolve_heads`].
pub(crate) async fn para_header_stream<Block, Client>(
	jam: &(impl JamStateSource + 'static),
	client: Arc<Client>,
	parachain_service: ServiceId,
	para_id: u32,
	finalized: bool,
) -> jam_interface::Result<impl Stream<Item = (Block::Header, JamSlot)> + Send>
where
	Block: BlockT,
	Client: HeaderBackend<Block> + BlockchainEvents<Block> + Send + Sync + 'static,
{
	// Subscribe before opening the JAM stream, so an import racing the first lookup is seen.
	let imports = client.every_import_notification_stream().map(|n| (n.hash, n.header)).boxed();
	let heads = para_head_stream::<Block::Hash>(jam, parachain_service, para_id, finalized).await?;
	Ok(resolve_heads(heads.boxed(), imports, move |hash| client.header(*hash).ok().flatten()))
}
