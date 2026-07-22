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

//! Speculative Messaging node parts: the sender-side archive, the
//! `/spec-msg/exchange` request-response protocol and the receiver-side
//! relay chain monitor.
//!
//! A sending parachain's block commits to all its outbound message streams
//! with one hash — the `StreamsRoot` — while the payloads travel off-chain
//! between collators. This crate is the off-chain half around that hash,
//! starting with the sender's side:
//!
//! - [`SpecMsgArchive`] persists every block's sends keyed by `(stream, position)` (payloads plus
//!   leaf hashes — proofs need hashes even where payloads were pruned) together with per-block
//!   frontier boundaries, and serves the two root-keyed requests of the fetch protocol
//!   ([`MessagesRequest`] / [`EventRequest`]): every request names the `StreamsRoot` it is willing
//!   to depend on (`under`), every response is proven under exactly that root.
//! - [`run_spec_msg_archiver`] follows the chain's best blocks, extracts each block's sends via the
//!   `SpecMsgApi` runtime API (version-gated: runtimes without the API keep the archive idle) and
//!   maintains the archive's retention: channel payloads below the peers' confirmation watermarks,
//!   extension material to the 25 h serving horizon ([`SERVING_HORIZON`]).
//! - [`SpecMsgRequestHandler`] answers incoming `/spec-msg/exchange` requests from the archive;
//!   [`spec_msg_protocol_config`] is the protocol registration for
//!   `net_config.add_request_response_protocol`.
//! - [`verify_messages_response`] / [`verify_event_response`] are the requester's side: responses
//!   are untrusted, verification recomputes the frontier, walks extension and tree proofs and
//!   compares against the root the request named — a mismatch discards response and peer. The
//!   receiver-side fetch loop and inherent provider build on these.
//!
//! And the receiver's side:
//!
//! - [`run_relay_provides_monitor`] watches imported relay chain blocks for the consumed sources'
//!   newly *included* `StreamsRoot`s (the inclusion-tier trust anchor: the relay pushes each
//!   enacted `Provides` root into the source's `RecentProvides` ring) and offers each exactly once
//!   as a [`RelayProvidesEvent`] to the fetch/verify pipeline, alongside pending-availability
//!   prefetch hints. [`read_recent_provides`] is the underlying relay-state read, shared with the
//!   inherent provider.
//! - [`run_spec_msg_fetcher`] consumes those events: per included root it fetches the source's
//!   consumed streams (chunked, resumable) and ack register head reads from the source's peers
//!   ([`SourcePeers`]; the MVP mechanism is the [`PeerRegistry`] the wiring fills), verifies
//!   everything against exactly that root and lands it in the [`SpecMsgPool`] — a poisoned response
//!   discards response and peer and is refetched.
//! - [`inherent_data_at`] builds the block's `specmsg0` inherent data from the pool at the
//!   authoring parent (include the result in `create_inherent_data_providers` — it IS an
//!   `sp_inherents::InherentDataProvider`); [`assemble_lifts`] / [`lift_assembler`] build the
//!   candidate's POV lifts from the built blocks' consumption records, for
//!   `CollatorService::with_spec_msg_lift_assembler`.
//!
//! MVP scope: request-response only — no notification/announce protocol,
//! no DA, no live push; triggers come from included roots only (the
//! speculative tier's header-digest anchors are post-MVP, and
//! pending-availability prefetch hints are ignored by the fetcher).
//!
//! [`MessagesRequest`]: cumulus_primitives_spec_messaging::MessagesRequest
//! [`EventRequest`]: cumulus_primitives_spec_messaging::EventRequest

#![warn(missing_docs)]

pub mod archive;
pub mod authoring;
pub mod exchange;
pub mod fetch;
pub mod monitor;
mod nodes;
pub mod pool;
pub mod protocol;
pub mod verify;
pub mod worker;

pub use archive::{ArchiveError, ServeError, SpecMsgArchive, SERVING_HORIZON};
pub use authoring::{
	assemble_collation, assemble_lifts, inherent_data_at, lift_assembler, AssembleError,
	ROUND_GRACE_WINDOW,
};
pub use exchange::{ExchangeError, ExchangeNetwork, PeerRegistry, SourcePeers};
pub use fetch::{fetch_source, run_spec_msg_fetcher, FetchError, FETCH_CHUNK_BYTES};
pub use monitor::{
	pending_provides, read_recent_provides, run_relay_provides_monitor, RelayProvidesEvent,
	SourceProvides,
};
pub use pool::{InherentBudget, SpecMsgPool};
pub use protocol::{
	spec_msg_protocol_config, SpecMsgRequestHandler, MAX_RESPONSE_SIZE, PROTOCOL_NAME,
};
pub use verify::{
	verify_event_response, verify_messages_response, VerifiedEvent, VerifiedMessages, VerifyError,
};
pub use worker::run_spec_msg_archiver;

const LOG_TARGET: &str = "spec-msg";

#[cfg(test)]
mod test_support {
	//! Shared fixtures: an in-memory `AuxStore`, a minimal test block type
	//! and an archive builder over deterministic sends.

	use crate::archive::SpecMsgArchive;
	use cumulus_primitives_spec_messaging::StreamId;
	use polkadot_core_primitives::Hash;
	use sp_runtime::traits::{BlakeTwo256, Hash as HashT};
	use std::{collections::HashMap, sync::Arc};

	/// The archive's block context in tests: a plain header chain.
	pub type TestBlock = sp_runtime::generic::Block<
		sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
		sp_runtime::OpaqueExtrinsic,
	>;

	pub type TestArchive = SpecMsgArchive<TestBlock, TestAux>;

	/// In-memory `AuxStore` shared between archive incarnations — dropping
	/// the archive and reloading over the same store is the restart test.
	#[derive(Default)]
	pub struct TestAux(parking_lot::Mutex<HashMap<Vec<u8>, Vec<u8>>>);

	impl sc_client_api::AuxStore for TestAux {
		fn insert_aux<
			'a,
			'b: 'a,
			'c: 'a,
			I: IntoIterator<Item = &'a (&'c [u8], &'c [u8])>,
			D: IntoIterator<Item = &'a &'b [u8]>,
		>(
			&self,
			insert: I,
			delete: D,
		) -> sp_blockchain::Result<()> {
			let mut map = self.0.lock();
			for (key, value) in insert {
				map.insert(key.to_vec(), value.to_vec());
			}
			for key in delete {
				map.remove(*key);
			}
			Ok(())
		}

		fn get_aux(&self, key: &[u8]) -> sp_blockchain::Result<Option<Vec<u8>>> {
			Ok(self.0.lock().get(key).cloned())
		}
	}

	/// Deterministic block hash for block `number` (0 = "genesis").
	pub fn block_hash(number: u32) -> Hash {
		BlakeTwo256::hash_of(&(*b"test block", number))
	}

	/// Deterministic payload for `stream`'s message at `position`.
	pub fn payload(stream: &StreamId, position: u64) -> Vec<u8> {
		let mut payload = stream.to_bytes().to_vec();
		payload.extend_from_slice(&position.to_le_bytes());
		payload
	}

	/// Imports blocks `1..=blocks` into `archive`, each carrying
	/// `per_block` messages on every stream of `streams`, at timestamp
	/// `base_time + number`. Payloads are [`payload`]-deterministic and
	/// positioned densely across blocks. Returns the per-block roots.
	pub fn import_blocks(
		archive: &mut TestArchive,
		streams: &[StreamId],
		blocks: u32,
		per_block: u64,
		base_time: u64,
	) -> Vec<cumulus_primitives_spec_messaging::StreamsRoot> {
		(1..=blocks)
			.map(|number| {
				let sends = streams
					.iter()
					.map(|stream| {
						let start = (number as u64 - 1) * per_block;
						let payloads =
							(start..start + per_block).map(|i| payload(stream, i)).collect();
						(*stream, payloads)
					})
					.collect();
				archive
					.import_block_at(
						block_hash(number),
						block_hash(number - 1),
						number,
						sends,
						base_time + number as u64,
					)
					.expect("test blocks are imported in order")
					.expect("every test block carries sends")
			})
			.collect()
	}

	/// A fresh archive over a fresh in-memory store.
	pub fn new_archive() -> (Arc<TestAux>, TestArchive) {
		let aux = Arc::new(TestAux::default());
		let archive = SpecMsgArchive::load(aux.clone()).expect("empty store loads");
		(aux, archive)
	}

	pub fn channel(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}

	pub fn ack(recipient: u32) -> StreamId {
		StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
	}

	pub use exchange_mock::{MockExchange, PeerBehavior};

	mod exchange_mock {
		//! A mock `/spec-msg/exchange` counterparty: per-peer behavior over a
		//! shared sender-side [`TestArchive`] — the receiver-side pipeline's
		//! test double.

		use super::TestArchive;
		use crate::exchange::{ExchangeError, ExchangeNetwork};
		use codec::{DecodeAll, Encode};
		use cumulus_primitives_spec_messaging::{ExchangeRequest, ExchangeResponse};
		use parking_lot::{Mutex, RwLock};
		use sc_network::PeerId;
		use std::collections::HashMap;

		/// How one mock peer answers.
		#[derive(Clone, Copy, Debug, Eq, PartialEq)]
		pub enum PeerBehavior {
			/// Serves faithfully from the archive.
			Honest,
			/// Serves archive responses with tampered content.
			Poison,
			/// Refuses everything at the transport level.
			Refuse,
			/// Refuses the first `n` requests at the transport level, then
			/// serves faithfully — the transient-failure double.
			RefuseFirst(u32),
		}

		/// Routes exchange requests to mock peers over one archive.
		pub struct MockExchange {
			pub archive: RwLock<TestArchive>,
			pub behavior: HashMap<PeerId, PeerBehavior>,
			/// Every served/attempted request's peer, in order.
			pub hits: Mutex<Vec<PeerId>>,
		}

		#[async_trait::async_trait]
		impl ExchangeNetwork for MockExchange {
			async fn exchange(
				&self,
				peer: PeerId,
				request: Vec<u8>,
			) -> Result<Vec<u8>, ExchangeError> {
				self.hits.lock().push(peer);
				let behavior = self.behavior.get(&peer).copied().unwrap_or(PeerBehavior::Refuse);
				match behavior {
					PeerBehavior::Refuse => return Err(ExchangeError::Network("refused".into())),
					PeerBehavior::RefuseFirst(refusals) => {
						// `hits` already contains the current attempt.
						let attempts = self.hits.lock().iter().filter(|hit| **hit == peer).count();
						if attempts <= refusals as usize {
							return Err(ExchangeError::Network("refused".into()));
						}
					},
					PeerBehavior::Honest | PeerBehavior::Poison => (),
				}
				let request = ExchangeRequest::decode_all(&mut &request[..])
					.expect("tests send valid requests");
				let archive = self.archive.read();
				let mut response = match request {
					ExchangeRequest::Messages(request) => {
						archive.serve_messages(&request).map(ExchangeResponse::Messages)
					},
					ExchangeRequest::Event(request) => {
						archive.serve_event(&request).map(ExchangeResponse::Event)
					},
				}
				.map_err(|_| ExchangeError::Network("unservable".into()))?;
				if behavior == PeerBehavior::Poison {
					tamper(&mut response);
				}
				Ok(response.encode())
			}
		}

		/// Corrupts a valid response so that it still decodes but cannot
		/// verify under the requested root.
		fn tamper(response: &mut ExchangeResponse) {
			match response {
				ExchangeResponse::Messages(response) => match response.payloads.first_mut() {
					Some(payload) => payload.push(0xFF),
					// Payload-free responses: break the tree walk instead.
					None => response
						.tree_proof
						.steps
						.push((0, polkadot_core_primitives::Hash::repeat_byte(0xAA))),
				},
				ExchangeResponse::Event(response) => response.payload.push(0xFF),
			}
		}
	}
}
