// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate. If not, see <https://www.gnu.org/licenses/>.

use crate::{IfDisconnected, NetworkRequest, ProtocolName};

use cid::{multihash::Multihash as CidMultihash, Cid, Version as CidVersion};
use log::{debug, trace, warn};
use prost::Message;
use sc_network_types::PeerId;
use std::collections::{HashMap, HashSet};

use super::{
	is_cid_supported, is_supported_multihash_code,
	schema::bitswap::{
		message::{
			wantlist::{Entry, WantType},
			BlockPresence, BlockPresenceType, Wantlist,
		},
		Message as BitswapMessage,
	},
	Prefix, LOG_TARGET, PROTOCOL_NAME, RAW_CODEC,
};

/// Multihash code for BLAKE2b-256.
pub const BLAKE2B_256_MULTIHASH_CODE: u64 = 0xb220;
/// Multihash code for SHA2-256.
pub const SHA2_256_MULTIHASH_CODE: u64 = 0x12;
/// Multihash code for Keccak-256.
pub const KECCAK_256_MULTIHASH_CODE: u64 = 0x1b;

/// Maximum entries per `WANT-BLOCK` request. Bigger requests get rejected by the peer
/// (see `MAX_WANTED_BLOCKS` in `bitswap/mod.rs`).
pub const MAX_WANTED_BLOCKS_PER_REQUEST: usize = 16;

/// Per-CID outcome from a [`fetch_many`] call.
#[derive(Debug)]
pub enum FetchOutcome {
	/// Peer returned bytes for the requested CID.
	Block(Vec<u8>),
	/// Peer explicitly indicated it does not have this CID.
	DontHave,
	/// Peer didn't acknowledge this CID, or its response was malformed.
	Missing,
}

type Multihash = CidMultihash<64>;

/// Build a raw-codec CID from a 32-byte digest and supported multihash code.
pub fn raw_cid_from_digest(multihash_code: u64, digest: [u8; 32]) -> Result<Cid, BitswapError> {
	if !is_supported_multihash_code(multihash_code) {
		return Err(BitswapError::UnsupportedHashing { multihash_code });
	}

	let multihash = Multihash::wrap(multihash_code, &digest)
		.map_err(|err| BitswapError::DecodeError(err.to_string()))?;
	Ok(Cid::new_v1(RAW_CODEC, multihash))
}

fn validate_wantlist_size(len: usize) -> Result<(), BitswapError> {
	if len == 0 {
		return Err(BitswapError::DecodeError("empty wantlist".into()));
	}
	if len > MAX_WANTED_BLOCKS_PER_REQUEST {
		return Err(BitswapError::DecodeError(format!(
			"wantlist too large: {len} > {MAX_WANTED_BLOCKS_PER_REQUEST}",
		)));
	}
	Ok(())
}

fn validate_cids(cids: &[Cid]) -> Result<(), BitswapError> {
	validate_wantlist_size(cids.len())?;
	for cid in cids {
		if !is_cid_supported(cid) {
			return Err(BitswapError::UnsupportedHashing { multihash_code: cid.hash().code() });
		}
	}
	Ok(())
}

/// Send one `WANT-BLOCK` request for `cids` to `peer` and classify the response.
///
/// Returned blocks are verified by recomputing the CID from the response prefix and bytes.
/// Blocks whose recomputed CID was not requested are ignored.
///
/// Errors if `cids` is empty, larger than [`MAX_WANTED_BLOCKS_PER_REQUEST`], or contains an
/// unsupported CID.
pub async fn fetch_many<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<HashMap<Cid, FetchOutcome>, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	validate_cids(cids)?;

	let wanted: HashSet<Cid> = cids.iter().copied().collect();
	let response = send_request(network, peer, cids).await?;
	Ok(classify_response(response, &wanted, peer))
}

/// Like [`fetch_many`], but does NOT recompute or verify the hash of received bytes.
///
/// Use this when the requester must fetch by CID-shaped identifiers before it can verify the
/// returned bytes through an external authority. The response is matched by request order and CID
/// prefix only; integrity verification is delegated to the caller.
pub async fn fetch_many_unverified<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<HashMap<Cid, FetchOutcome>, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	validate_cids(cids)?;

	let response = send_request(network, peer, cids).await?;
	Ok(classify_response_unverified(response, cids, peer))
}

async fn send_request<N>(
	network: &N,
	peer: PeerId,
	cids: &[Cid],
) -> Result<BitswapMessage, BitswapError>
where
	N: NetworkRequest + ?Sized,
{
	let entries: Vec<Entry> = cids
		.iter()
		.map(|cid| Entry {
			block: cid.to_bytes(),
			want_type: WantType::Block as i32,
			send_dont_have: true,
			..Default::default()
		})
		.collect();
	let request =
		BitswapMessage { wantlist: Some(Wantlist { entries, full: false }), ..Default::default() };

	trace!(
		target: LOG_TARGET,
		"client: sending WANT-BLOCK for {} CIDs to {peer}, protocol {PROTOCOL_NAME}",
		cids.len(),
	);

	let payload = match network
		.request(
			peer,
			ProtocolName::from(PROTOCOL_NAME),
			request.encode_to_vec(),
			None,
			IfDisconnected::TryConnect,
		)
		.await
	{
		Ok((payload, _)) => payload,
		Err(err) => {
			debug!(target: LOG_TARGET, "client: batch request to {peer} rejected by network: {err:?}");
			return Err(BitswapError::RequestFailed(err.to_string()));
		},
	};

	BitswapMessage::decode(&payload[..]).map_err(|err| {
		debug!(target: LOG_TARGET, "client: failed to decode batch response from {peer}: {err}");
		BitswapError::DecodeError(err.to_string())
	})
}

fn classify_response(
	response: BitswapMessage,
	wanted: &HashSet<Cid>,
	peer: PeerId,
) -> HashMap<Cid, FetchOutcome> {
	let mut result: HashMap<Cid, FetchOutcome> = HashMap::with_capacity(wanted.len());

	for block in response.payload {
		let Ok(cid) = cid_from_block_prefix(&block.prefix, &block.data).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed block prefix from {peer}: {err:?}");
		}) else {
			continue;
		};
		if !wanted.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited block for CID {cid}");
			continue;
		}
		debug!(target: LOG_TARGET, "client: {peer} returned {} bytes for CID {cid}", block.data.len());
		result.insert(cid, FetchOutcome::Block(block.data));
	}

	apply_presences_and_fill_missing(response.block_presences, wanted, peer, &mut result);

	result
}

/// Classify an unverified response via order-based correlation.
fn classify_response_unverified(
	response: BitswapMessage,
	wanted: &[Cid],
	peer: PeerId,
) -> HashMap<Cid, FetchOutcome> {
	let mut result: HashMap<Cid, FetchOutcome> = HashMap::with_capacity(wanted.len());
	let wanted_set: HashSet<Cid> = wanted.iter().copied().collect();
	let mut dont_have_cids: HashSet<Cid> = HashSet::with_capacity(wanted.len());

	for presence in response.block_presences {
		let Ok(cid) = Cid::read_bytes(presence.cid.as_slice()).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed presence CID from {peer}: {err}");
		}) else {
			continue;
		};
		if !wanted_set.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited presence for CID {cid}");
			continue;
		}
		if presence.r#type == BlockPresenceType::DontHave as i32 {
			debug!(target: LOG_TARGET, "client: {peer} DONT_HAVE for CID {cid}");
			dont_have_cids.insert(cid);
			result.insert(cid, FetchOutcome::DontHave);
		} else {
			warn!(target: LOG_TARGET, "client: {peer} unexpected presence type {} for CID {cid}", presence.r#type);
			result.insert(cid, FetchOutcome::Missing);
		}
	}

	let mut expected_payload_order = wanted.iter().filter(|cid| !dont_have_cids.contains(cid));

	for block in response.payload {
		let Some(expected_cid) = expected_payload_order.next() else {
			debug!(target: LOG_TARGET, "client: {peer} returned more payload blocks than expected; dropping extras");
			break;
		};
		let Ok(prefix) = decode_prefix(&block.prefix).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed block prefix from {peer}: {err:?}");
		}) else {
			break;
		};
		if !prefix_matches_cid(&prefix, expected_cid) {
			debug!(
				target: LOG_TARGET,
				"client: {peer} returned block with prefix {:?} but expected CID {expected_cid}; \
				 stopping payload attribution",
				prefix,
			);
			break;
		}
		debug!(
			target: LOG_TARGET,
			"client: {peer} returned {} unverified bytes for CID {expected_cid}",
			block.data.len(),
		);
		result.entry(*expected_cid).or_insert(FetchOutcome::Block(block.data.clone()));
	}

	for cid in wanted {
		result.entry(*cid).or_insert(FetchOutcome::Missing);
	}

	result
}

fn apply_presences_and_fill_missing(
	presences: Vec<BlockPresence>,
	wanted: &HashSet<Cid>,
	peer: PeerId,
	result: &mut HashMap<Cid, FetchOutcome>,
) {
	for presence in presences {
		let Ok(cid) = Cid::read_bytes(presence.cid.as_slice()).inspect_err(|err| {
			debug!(target: LOG_TARGET, "client: malformed presence CID from {peer}: {err}");
		}) else {
			continue;
		};
		if !wanted.contains(&cid) {
			debug!(target: LOG_TARGET, "client: {peer} returned unsolicited presence for CID {cid}");
			continue;
		}
		if result.contains_key(&cid) {
			continue;
		}
		let outcome = if presence.r#type == BlockPresenceType::DontHave as i32 {
			debug!(target: LOG_TARGET, "client: {peer} DONT_HAVE for CID {cid}");
			FetchOutcome::DontHave
		} else {
			warn!(target: LOG_TARGET, "client: {peer} unexpected presence type {} for CID {cid}", presence.r#type);
			FetchOutcome::Missing
		};
		result.insert(cid, outcome);
	}

	for cid in wanted {
		result.entry(*cid).or_insert(FetchOutcome::Missing);
	}
}

fn prefix_matches_cid(prefix: &Prefix, cid: &Cid) -> bool {
	prefix.version == cid.version() &&
		prefix.codec == cid.codec() &&
		prefix.mh_type == cid.hash().code() &&
		prefix.mh_len == cid.hash().size()
}

fn cid_from_block_prefix(prefix: &[u8], data: &[u8]) -> Result<Cid, BitswapError> {
	let prefix = decode_prefix(prefix)?;
	let hash = hash_for_multihash_code(prefix.mh_type, data)
		.ok_or(BitswapError::UnsupportedHashing { multihash_code: prefix.mh_type })?;
	let multihash = Multihash::wrap(prefix.mh_type, &hash)
		.map_err(|err| BitswapError::DecodeError(err.to_string()))?;

	match prefix.version {
		CidVersion::V1 => Ok(Cid::new_v1(prefix.codec, multihash)),
		CidVersion::V0 => {
			Err(BitswapError::DecodeError("bitswap block prefix used unsupported CIDv0".into()))
		},
	}
}

fn hash_for_multihash_code(multihash_code: u64, data: &[u8]) -> Option<[u8; 32]> {
	match multihash_code {
		BLAKE2B_256_MULTIHASH_CODE => Some(sp_crypto_hashing::blake2_256(data)),
		SHA2_256_MULTIHASH_CODE => Some(sp_crypto_hashing::sha2_256(data)),
		KECCAK_256_MULTIHASH_CODE => Some(sp_crypto_hashing::keccak_256(data)),
		_ => None,
	}
}

fn decode_prefix(mut bytes: &[u8]) -> Result<Prefix, BitswapError> {
	let mut read_varint = || -> Result<u64, BitswapError> {
		let (v, rest) = unsigned_varint::decode::u64(bytes)
			.map_err(|err| BitswapError::DecodeError(err.to_string()))?;
		bytes = rest;
		Ok(v)
	};

	let version = read_varint()?;
	let codec = read_varint()?;
	let mh_type = read_varint()?;
	let mh_len = read_varint()?;

	if !bytes.is_empty() {
		return Err(BitswapError::DecodeError("bitswap block prefix had trailing bytes".into()));
	}

	let version = CidVersion::try_from(version)
		.map_err(|_| BitswapError::DecodeError(format!("unsupported CID version {version}")))?;
	let mh_len = u8::try_from(mh_len).map_err(|_| {
		BitswapError::DecodeError(format!("multihash length {mh_len} does not fit into u8"))
	})?;

	Ok(Prefix { version, codec, mh_type, mh_len })
}

/// Bitswap client errors.
#[derive(Debug)]
pub enum BitswapError {
	/// Failed to decode or validate a bitswap payload.
	DecodeError(String),
	/// Request/response exchange failed.
	RequestFailed(String),
	/// Block prefix declared an unsupported multihash code.
	UnsupportedHashing {
		/// The unrecognised IPFS multihash code.
		multihash_code: u64,
	},
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{OutboundFailure, RequestFailure};
	use futures::channel::oneshot;
	use sc_network_types::PeerId;
	use std::{collections::VecDeque, sync::Mutex};

	use super::super::schema::bitswap::message::{
		Block as MessageBlock, BlockPresence, BlockPresenceType,
	};

	struct StubSender(Mutex<VecDeque<Result<Vec<u8>, RequestFailure>>>);

	impl StubSender {
		fn new(responses: impl IntoIterator<Item = Result<Vec<u8>, RequestFailure>>) -> Self {
			Self(Mutex::new(responses.into_iter().collect()))
		}
	}

	#[async_trait::async_trait]
	impl NetworkRequest for StubSender {
		async fn request(
			&self,
			_target: PeerId,
			_protocol: ProtocolName,
			_request: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			_connect: IfDisconnected,
		) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
			self.0
				.lock()
				.unwrap()
				.pop_front()
				.expect("StubSender: no canned response queued")
				.map(|bytes| (bytes, ProtocolName::from(PROTOCOL_NAME)))
		}

		fn start_request(
			&self,
			_peer: PeerId,
			_protocol: ProtocolName,
			_payload: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
			_connect: IfDisconnected,
		) {
			let resp = self
				.0
				.lock()
				.unwrap()
				.pop_front()
				.expect("StubSender: no canned response queued");
			let _ = tx.send(resp.map(|bytes| (bytes, ProtocolName::from(PROTOCOL_NAME))));
		}
	}

	fn prefix_for(multihash_code: u64) -> Vec<u8> {
		Prefix { version: CidVersion::V1, codec: RAW_CODEC, mh_type: multihash_code, mh_len: 32 }
			.to_bytes()
	}

	fn cid_for_data(multihash_code: u64, data: &[u8]) -> Cid {
		raw_cid_from_digest(multihash_code, hash_for_multihash_code(multihash_code, data).unwrap())
			.unwrap()
	}

	fn cid_for_digest(multihash_code: u64, digest: [u8; 32]) -> Cid {
		raw_cid_from_digest(multihash_code, digest).unwrap()
	}

	fn encode_response(blocks: &[(u64, Vec<u8>)], presences: &[(Cid, i32)]) -> Vec<u8> {
		let payload = blocks
			.iter()
			.map(|(multihash_code, data)| MessageBlock {
				prefix: prefix_for(*multihash_code),
				data: data.clone(),
			})
			.collect();
		let block_presences = presences
			.iter()
			.map(|(cid, ptype)| BlockPresence { cid: cid.to_bytes(), r#type: *ptype })
			.collect();
		BitswapMessage { payload, block_presences, ..Default::default() }.encode_to_vec()
	}

	#[tokio::test]
	async fn fetch_many_returns_blocks_for_all_wanted() {
		let data_a = b"hash-a-payload".to_vec();
		let data_b = b"hash-b-payload".to_vec();
		let data_c = b"hash-c-payload".to_vec();
		let cid_a = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_a);
		let cid_b = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b);
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_c);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_c.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &[cid_a, cid_b, cid_c])
			.await
			.expect("fetch_many should succeed");

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(d)) if *d == data_b));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::Block(d)) if *d == data_c));
	}

	#[tokio::test]
	async fn fetch_many_partial_dont_have() {
		let data_a = b"a".to_vec();
		let data_b = b"b".to_vec();
		let cid_a = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_a);
		let cid_b = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b);
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"c-not-served");

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
			],
			&[(cid_c, BlockPresenceType::DontHave as i32)],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &[cid_a, cid_b, cid_c]).await.unwrap();

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(_))));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(_))));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::DontHave)));
	}

	#[tokio::test]
	async fn fetch_many_corrupted_data_dropped_as_unsolicited() {
		let real_data = b"real-payload".to_vec();
		let wanted_cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &real_data);
		let corrupted_data = b"i-am-not-the-real-payload".to_vec();
		let response = encode_response(&[(BLAKE2B_256_MULTIHASH_CODE, corrupted_data)], &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &[wanted_cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&wanted_cid), Some(FetchOutcome::Missing)));
	}

	#[tokio::test]
	async fn fetch_many_unverified_accepts_bytes_without_hash_recompute() {
		let data = b"sha2-digest-but-blake2b-request-prefix".to_vec();
		let cid = cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::sha2_256(&data));
		let response = encode_response(&[(BLAKE2B_256_MULTIHASH_CODE, data.clone())], &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many_unverified(&stub, PeerId::random(), &[cid])
			.await
			.expect("unverified fetch should not recompute hashes");

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Block(d)) if *d == data));
	}

	#[tokio::test]
	async fn fetch_many_unverified_dont_have_returned_as_missing() {
		let cid = cid_for_digest(
			BLAKE2B_256_MULTIHASH_CODE,
			sp_crypto_hashing::sha2_256(b"pruned-unverified-payload"),
		);
		let response = encode_response(&[], &[(cid, BlockPresenceType::DontHave as i32)]);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many_unverified(&stub, PeerId::random(), &[cid])
			.await
			.expect("unverified DONT_HAVE should classify successfully");

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::DontHave)));
	}

	#[tokio::test]
	async fn fetch_many_unverified_empty_wants_errors() {
		let stub = StubSender::new(std::iter::empty());

		let err = fetch_many_unverified(&stub, PeerId::random(), &[])
			.await
			.expect_err("empty wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(msg) if msg == "empty wantlist"));
	}

	#[tokio::test]
	async fn fetch_many_unverified_multi_want_all_served_in_request_order() {
		let data_a = b"first-unverified-payload".to_vec();
		let data_b = b"second-unverified-payload".to_vec();
		let data_c = b"third-unverified-payload".to_vec();
		let cid_a =
			cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::sha2_256(&data_a));
		let cid_b =
			cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, sp_crypto_hashing::keccak_256(&data_b));
		let cid_c = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_c);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_a.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_b.clone()),
				(BLAKE2B_256_MULTIHASH_CODE, data_c.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many_unverified(&stub, PeerId::random(), &[cid_a, cid_b, cid_c])
			.await
			.expect("multi-want unverified must succeed via positional correlation");

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_a), Some(FetchOutcome::Block(d)) if *d == data_a));
		assert!(matches!(result.get(&cid_b), Some(FetchOutcome::Block(d)) if *d == data_b));
		assert!(matches!(result.get(&cid_c), Some(FetchOutcome::Block(d)) if *d == data_c));
	}

	#[tokio::test]
	async fn fetch_many_dispatches_per_entry_multihash() {
		let data_b2 = b"blake2b-payload".to_vec();
		let data_sha = b"sha2-256-payload".to_vec();
		let data_kec = b"keccak-256-payload".to_vec();
		let cid_b2 = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data_b2);
		let cid_sha = cid_for_data(SHA2_256_MULTIHASH_CODE, &data_sha);
		let cid_kec = cid_for_data(KECCAK_256_MULTIHASH_CODE, &data_kec);

		let response = encode_response(
			&[
				(BLAKE2B_256_MULTIHASH_CODE, data_b2.clone()),
				(SHA2_256_MULTIHASH_CODE, data_sha.clone()),
				(KECCAK_256_MULTIHASH_CODE, data_kec.clone()),
			],
			&[],
		);
		let stub = StubSender::new([Ok(response)]);

		let result =
			fetch_many(&stub, PeerId::random(), &[cid_b2, cid_sha, cid_kec]).await.unwrap();

		assert_eq!(result.len(), 3);
		assert!(matches!(result.get(&cid_b2), Some(FetchOutcome::Block(d)) if *d == data_b2));
		assert!(matches!(result.get(&cid_sha), Some(FetchOutcome::Block(d)) if *d == data_sha));
		assert!(matches!(result.get(&cid_kec), Some(FetchOutcome::Block(d)) if *d == data_kec));
	}

	#[tokio::test]
	async fn fetch_many_over_cap_errors() {
		let wants: Vec<_> = (0..(MAX_WANTED_BLOCKS_PER_REQUEST + 1) as u8)
			.map(|i| {
				let mut h = [0u8; 32];
				h[0] = i;
				cid_for_digest(BLAKE2B_256_MULTIHASH_CODE, h)
			})
			.collect();
		let stub = StubSender::new(std::iter::empty());

		let err = fetch_many(&stub, PeerId::random(), &wants)
			.await
			.expect_err("over-cap wantlist must error");
		assert!(matches!(err, BitswapError::DecodeError(_)));
	}

	#[tokio::test]
	async fn fetch_many_at_exactly_max_wanted_blocks_succeeds() {
		let mut wants = Vec::with_capacity(MAX_WANTED_BLOCKS_PER_REQUEST);
		let mut blocks = Vec::with_capacity(MAX_WANTED_BLOCKS_PER_REQUEST);
		for i in 0..MAX_WANTED_BLOCKS_PER_REQUEST {
			let data = format!("payload-{i}").into_bytes();
			wants.push(cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data));
			blocks.push((BLAKE2B_256_MULTIHASH_CODE, data));
		}

		let response = encode_response(&blocks, &[]);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &wants)
			.await
			.expect("exactly MAX_WANTED_BLOCKS_PER_REQUEST must succeed");

		assert_eq!(result.len(), MAX_WANTED_BLOCKS_PER_REQUEST);
		for cid in &wants {
			assert!(matches!(result.get(cid), Some(FetchOutcome::Block(_))));
		}
	}

	#[tokio::test]
	async fn fetch_many_block_beats_presence_for_same_cid() {
		let data = b"both-block-and-presence".to_vec();
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &data);

		let response = encode_response(
			&[(BLAKE2B_256_MULTIHASH_CODE, data.clone())],
			&[(cid, BlockPresenceType::DontHave as i32)],
		);
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &[cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&cid), Some(FetchOutcome::Block(d)) if *d == data));
	}

	#[tokio::test]
	async fn fetch_many_response_decode_failure() {
		let stub = StubSender::new([Ok(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]);
		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"any");

		let err = fetch_many(&stub, PeerId::random(), &[cid])
			.await
			.expect_err("malformed response bytes must surface as DecodeError");
		assert!(matches!(err, BitswapError::DecodeError(_)));
	}

	#[tokio::test]
	async fn fetch_many_request_failure_propagates() {
		struct FailingSender;
		#[async_trait::async_trait]
		impl NetworkRequest for FailingSender {
			async fn request(
				&self,
				_target: PeerId,
				_protocol: ProtocolName,
				_request: Vec<u8>,
				_fallback_request: Option<(Vec<u8>, ProtocolName)>,
				_connect: IfDisconnected,
			) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
				Err(RequestFailure::Network(OutboundFailure::ConnectionClosed))
			}

			fn start_request(
				&self,
				_peer: PeerId,
				_protocol: ProtocolName,
				_payload: Vec<u8>,
				_fallback_request: Option<(Vec<u8>, ProtocolName)>,
				tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
				_connect: IfDisconnected,
			) {
				drop(tx);
			}
		}

		let cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, b"any");
		let err = fetch_many(&FailingSender, PeerId::random(), &[cid])
			.await
			.expect_err("request failure must surface as RequestFailed");
		assert!(matches!(err, BitswapError::RequestFailed(_)));
	}

	#[tokio::test]
	async fn fetch_many_unsupported_multihash_in_block_dropped() {
		let wanted_data = b"wanted".to_vec();
		let wanted_cid = cid_for_data(BLAKE2B_256_MULTIHASH_CODE, &wanted_data);
		const UNSUPPORTED_MH_CODE: u64 = 0x99;
		let bad_prefix = Prefix {
			version: CidVersion::V1,
			codec: RAW_CODEC,
			mh_type: UNSUPPORTED_MH_CODE,
			mh_len: 32,
		}
		.to_bytes();

		let mut payload_msg = BitswapMessage::default();
		payload_msg.payload =
			vec![MessageBlock { prefix: bad_prefix, data: b"some-bytes".to_vec() }];
		let response = payload_msg.encode_to_vec();
		let stub = StubSender::new([Ok(response)]);

		let result = fetch_many(&stub, PeerId::random(), &[wanted_cid]).await.unwrap();

		assert_eq!(result.len(), 1);
		assert!(matches!(result.get(&wanted_cid), Some(FetchOutcome::Missing)));
	}
}
