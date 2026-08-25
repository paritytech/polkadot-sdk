// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Builder for synthesizing dynamically-sized inbound/outbound benchmark fixtures.
//!
//! This is the inverse of `snowbridge_pallet_ethereum_client::Pallet::verify`: it produces
//! `EventFixture`s that the verifier accepts, by mirroring the verifier's SSZ + merkle +
//! receipts-trie logic. Three crates currently consume this:
//! - `snowbridge_pallet_inbound_queue_fixtures::dynamic` (v1 inbound)
//! - `snowbridge_pallet_inbound_queue_v2_fixtures::dynamic`
//! - `snowbridge_pallet_outbound_queue_v2::dynamic_fixture`
//!
//! Each caller supplies a [`LogTemplate`] describing its event log shape; the synthesis
//! machinery here does the rest.
//!
//! Construction layers, bottom-up:
//!
//! 1. Build a real receipts trie via `alloy_trie::HashBuilder` whose inclusion proof for the
//!    retained leaf has exactly `n` nodes. The retained leaf holds an Eip1559 receipt whose primary
//!    log matches the verifier's expected event log; the log's `data` field is sized so the encoded
//!    envelope is `s` bytes. To make the proof path cross `n` distinct nodes we add `n - 1` cheap
//!    sibling leaves, each diverging from the retained key at a successive nibble depth so a branch
//!    node is forced at every depth `0..n-1` (see `build_receipts_trie`).
//! 2. Construct an `ExecutionPayloadHeader` whose `receipts_root` is the trie root from (1).
//!    Compute its SSZ `hash_tree_root` — call it `execution_header_root`.
//! 3. Pick `execution_branch` as `EXECUTION_HEADER_DEPTH` zero hashes. Compute the `body_root` by
//!    climbing the merkle tree from `execution_header_root` against that branch at
//!    `subtree_index(EXECUTION_HEADER_INDEX)`.
//! 4. Build a `BeaconHeader` whose `body_root` is the value from (3) at slot `BENCH_SLOT`. Compute
//!    its `hash_tree_root` — call it `beacon_block_root`.
//! 5. Pick `header_branch` as `BLOCK_ROOT_AT_INDEX_DEPTH` zero hashes. Compute `block_roots_root`
//!    by climbing from `beacon_block_root` against that branch at the leaf index
//!    `verify_ancestry_proof` will compute.
//! 6. Pick a deterministic `finalized_block_root` (any non-zero hash). The caller stores
//!    `CompactBeaconState { slot: BENCH_SLOT + 1, block_roots_root }` under it and points
//!    `LatestFinalizedBlockRoot` at the same root before submitting the event.
#![cfg(feature = "runtime-benchmarks")]

extern crate alloc;

use alloc::{boxed::Box, vec, vec::Vec};
use alloy_consensus::{Eip658Value, Receipt, ReceiptEnvelope, ReceiptWithBloom};
use alloy_primitives::{Bloom, Bytes, LogData, B256};
use alloy_rlp::Encodable;
use alloy_trie::{hash_builder::HashBuilder, proof::ProofRetainer, Nibbles};
use snowbridge_beacon_primitives::{
	merkle_proof::{generalized_index_length, subtree_index},
	types::deneb,
	AncestryProof, BeaconHeader, ExecutionProof, VersionedExecutionPayloadHeader,
};
use snowbridge_verification_primitives::{EventFixture, EventProof, Log, Proof};
use sp_core::{H160, H256, U256};
use sp_io::hashing::sha2_256;

/// `get_generalized_index(BeaconBlockBody, 'execution_payload')` for the post-Bellatrix
/// beacon block body. Mirrors `ethereum-client::config::altair::EXECUTION_HEADER_INDEX`.
const EXECUTION_HEADER_INDEX: usize = 25;
/// `BLOCK_ROOT_AT_INDEX_DEPTH` from `ethereum-client::config`.
const BLOCK_ROOT_AT_INDEX_DEPTH: usize = 13;
/// `SLOTS_PER_HISTORICAL_ROOT` from the consensus spec. The ancestry-proof leaf index is
/// `SLOTS_PER_HISTORICAL_ROOT + (block_slot mod SLOTS_PER_HISTORICAL_ROOT)`.
const SLOTS_PER_HISTORICAL_ROOT: u64 = 8192;

/// The slot we synthesize the proof against. Any value that fits in u64 works; we keep it
/// well above the boundary so the index lands inside the second half of the merkle tree.
pub const BENCH_SLOT: u64 = 8_000_000;

/// Lower bound on the receipt size — the encoded Eip1559 envelope around an empty-data log
/// is already this large because of the 256-byte bloom and topic overhead.
pub const MIN_RECEIPT_SIZE: u32 = 320;

/// Transaction index of the retained receipt. A receipt-trie proof can have at most
/// `key_nibbles + 1` nodes, so the key — `rlp(BENCH_TARGET_TX_INDEX)` — must carry at least
/// `MaxProofNodes - 1` nibbles for the sibling-divergence construction in
/// `build_receipts_trie` to reach the benchmarked worst case. This necessarily makes the
/// index synthetic: a realistic block tops out around a few thousand receipts (a ~2-byte
/// index, 4-6 nibbles), far too short.
///
/// `0x00FF_FFFF_FFFF_FFFF` is a 7-byte value, so its RLP key is `0x87` followed by seven
/// `0xff` bytes = 8 bytes = 16 nibbles, supporting proof paths of up to 17 nodes — one above
/// the current `MaxProofNodes` of 16.
///
/// This couples the constant to `MaxProofNodes`: a runtime that raises `MaxProofNodes` above 17
/// MUST widen `BENCH_TARGET_TX_INDEX` to carry at least `MaxProofNodes - 1` nibbles, otherwise
/// `build_receipts_trie` cannot synthesize a proof that deep. The `target_nibbles.len() >= n - 1`
/// debug-assert in `build_receipts_trie` catches that — but only in debug builds, so treat this
/// constant as the hard cap on the benchmarked `n` range.
const BENCH_TARGET_TX_INDEX: u64 = 0x00FF_FFFF_FFFF_FFFF;

/// Output of [`build_dynamic_fixture_with_log`].
pub struct DynamicFixture {
	/// The fully-formed `EventFixture` to submit through the benchmarked extrinsic.
	pub event_fixture: EventFixture,
	/// Block root that the caller must store as `LatestFinalizedBlockRoot` (and to which
	/// the `CompactBeaconState` must be associated) before submitting the event.
	pub finalized_block_root: H256,
}

/// Per-version Ethereum log shape. All inbound/outbound queue versions reuse the same SSZ
/// / merkle / receipts-trie machinery; they only differ in the `Log` they expect inside
/// the receipt (gateway address, topic vector, and the bytes that must round-trip
/// through their respective `try_from(&Log)` after envelope decode).
pub struct LogTemplate {
	/// Gateway contract address that emitted the log.
	pub gateway: [u8; 20],
	/// Topics on the log — for v1 inbound these are `[topic0, channel_id, message_id]`;
	/// for v2 inbound it is just `[OutboundMessageAccepted::SIGNATURE_HASH]`; for v2
	/// outbound `submit_delivery_receipt` it is
	/// `[InboundMessageDispatched::SIGNATURE_HASH, padded_nonce]`.
	pub topics: Vec<H256>,
	/// Build the log's `data` bytes for a given target length. The dynamic builder calls
	/// this in a convergence loop because RLP envelope overhead grows non-uniformly with
	/// data size, so the function is asked for several sizes near the requested `s`.
	///
	/// V1 inbound implementation appends zero bytes to a fixed prefix (the verifier just
	/// byte-compares `Log::data` against the receipt's log). V2 inbound must produce a
	/// valid ABI-encoded `IGatewayV2::OutboundMessageAccepted` payload, so it grows a
	/// variable-length `bytes` field inside the payload to reach the target length.
	///
	/// When the matching log's `data` is fixed-size (e.g. v2 outbound's
	/// `InboundMessageDispatched`, whose data is exactly 96 bytes), set this to a closure
	/// that always returns those exact bytes regardless of `target_len`, and use
	/// [`LogTemplate::filler_log_data_builder`] to grow the receipt instead.
	pub data_builder: Box<dyn Fn(usize) -> Vec<u8>>,
	/// Optional filler log data builder. When `Some`, the dynamic builder appends a
	/// second log to the receipt envelope (with a zero address and no topics) whose
	/// `data` is sized by this builder so the final envelope hits the target receipt
	/// size. Used when the matching log's `data` is fixed-size and cannot be grown.
	pub filler_log_data_builder: Option<Box<dyn Fn(usize) -> Vec<u8>>>,
}

/// Build a synthetic `EventFixture` whose receipt proof has `n` trie nodes and whose
/// receipt body is approximately `s` bytes. The caller is responsible for storing the
/// returned `finalized_block_root` and matching `CompactBeaconState` (slot
/// `BENCH_SLOT + 1`, `block_roots_root` derived from the proof) in ethereum-client storage.
pub fn build_dynamic_fixture_with_log(n: u32, s: u32, log: LogTemplate) -> DynamicFixture {
	let n = n.max(1);
	let s = s.max(MIN_RECEIPT_SIZE);

	// 1. Build the receipts trie.
	let (receipts_root, receipt_proof, receipt_log) = build_receipts_trie(n, s, &log);

	// 2. ExecutionPayloadHeader rooted at the trie root.
	let execution_header = build_execution_header(receipts_root);
	let execution_header_versioned = VersionedExecutionPayloadHeader::Deneb(execution_header);
	let execution_header_root = execution_header_versioned
		.hash_tree_root()
		.expect("synthesized execution header is well-formed; qed");

	// 3. Compute body_root from execution_header_root + zero execution_branch.
	let execution_branch_depth = generalized_index_length(EXECUTION_HEADER_INDEX);
	let execution_branch_index = subtree_index(EXECUTION_HEADER_INDEX);
	let execution_branch: Vec<H256> = vec![H256::zero(); execution_branch_depth];
	let body_root =
		compute_merkle_root(execution_header_root, &execution_branch, execution_branch_index);

	// 4. BeaconHeader rooted at body_root.
	let header = BeaconHeader {
		slot: BENCH_SLOT,
		proposer_index: 0,
		parent_root: H256::zero(),
		state_root: H256::zero(),
		body_root,
	};
	let beacon_block_root =
		header.hash_tree_root().expect("synthesized beacon header is well-formed; qed");

	// 5. Compute block_roots_root from beacon_block_root + zero header_branch.
	let ancestry_index_in_array = BENCH_SLOT % SLOTS_PER_HISTORICAL_ROOT;
	let ancestry_leaf_index = (SLOTS_PER_HISTORICAL_ROOT + ancestry_index_in_array) as usize;
	let header_branch: Vec<H256> = vec![H256::zero(); BLOCK_ROOT_AT_INDEX_DEPTH];
	let block_roots_root =
		compute_merkle_root(beacon_block_root, &header_branch, ancestry_leaf_index);

	// 6. Pick a deterministic finalized_block_root.
	let finalized_block_root: H256 = sha2_256(b"snowbridge-bench-finalized-root").into();

	let execution_proof = ExecutionProof {
		header,
		ancestry_proof: Some(AncestryProof { header_branch, finalized_block_root }),
		execution_header: execution_header_versioned,
		execution_branch,
	};

	let event =
		EventProof { event_log: receipt_log, proof: Proof { receipt_proof, execution_proof } };

	DynamicFixture {
		event_fixture: EventFixture {
			event,
			finalized_header: BeaconHeader {
				slot: BENCH_SLOT + 1,
				proposer_index: 0,
				parent_root: H256::zero(),
				state_root: H256::zero(),
				body_root: H256::zero(),
			},
			block_roots_root,
		},
		finalized_block_root,
	}
}

/// Build a real receipts trie whose inclusion proof for the retained leaf has exactly `n`
/// nodes and whose retained receipt envelope is `s` bytes. Returns
/// `(receipts_root, proof_for_target_tx_index, event_log)`.
///
/// The runtime charges `submit` weight against `(event.proof.receipt_proof.len(), <leaf
/// size>)`, where the `n` axis prices the verifier's per-node RLP-decode + keccak work. So the
/// benchmark's `n` component must equal the produced proof length AND each proof node must be
/// the realistic worst case, otherwise the fitted per-node slope undercharges real proofs.
///
/// Note the `s` axis and the runtime charge use slightly different quantities: the benchmark
/// sizes `s` to the encoded *receipt envelope* length, whereas dispatch charges
/// `receipt_proof.last().len()` — the *trie leaf node*, i.e. the envelope wrapped in the leaf's
/// RLP path/value framing, so `leaf.len() >= s`. Charging the (larger) leaf length against a
/// per-byte slope fitted on envelope length therefore overcharges by the leaf framing overhead
/// and is conservative; it never undercharges.
///
/// A naive trie of `n` sequential `rlp(0..n)` leaves achieves neither: sequential RLP indices
/// diverge at the first nibble, leaving a shallow ~2-node path regardless of `n`. Instead we
/// keep a single target key and force a *full-width* branch node at every depth along its path:
///
/// - The retained key is `rlp(BENCH_TARGET_TX_INDEX)`, chosen so the key has enough nibbles (16) to
///   force proof paths above `MaxProofNodes`; see [`BENCH_TARGET_TX_INDEX`].
/// - For each depth `i` in `0..n-1` we add a sibling leaf for every nibble value other than the
///   target's at position `i` (15 siblings). This makes the branch node at depth `i` full (16
///   children), so the target path crosses `n - 1` ~530-byte branch nodes plus the terminal leaf =
///   `n` nodes. A full branch is the worst case the verifier can encounter, so the per-node slope
///   is a safe upper bound: a caller cannot make any proof node more expensive to hash.
///
/// This upper-bound property still holds when the runtime extrapolates past the benchmarked
/// `n`/`s` range: the `receipts_root` is consensus-committed (verified through the execution +
/// ancestry proof before the receipt proof is checked), so a caller cannot craft pathological
/// trie nodes — real receipts-trie nodes are bounded by the full-width branch calibrated here —
/// and the receipt decode is linear in `s`, so the fitted slopes never undercharge out of range.
/// - Sibling values are 32 zero bytes, large enough that each sibling leaf is referenced by hash in
///   its parent branch (as in a real receipts trie) rather than inlined, which is what makes the
///   branch full-width. The sibling leaves never appear in the retained proof themselves.
fn build_receipts_trie(n: u32, s: u32, log: &LogTemplate) -> (H256, Vec<Vec<u8>>, Log) {
	let n = (n as usize).max(1);

	// Pad the log's `data` field so the encoded receipt envelope is exactly `s` bytes
	// (within +/- a few bytes of overhead — the verifier does not enforce a size).
	let receipt_envelope = encode_sized_receipt_envelope(s as usize, log);
	// The `Log` we pass into `submit` reproduces what the verifier sees on Ethereum: the
	// gateway address, the same topics, and the same `data` we baked into the receipt. Its
	// `tx_index` is what the verifier feeds into `receipt_trie_key`, so it must match the
	// retained leaf's key.
	let receipt_log = Log {
		address: H160(log.gateway),
		topics: log.topics.clone(),
		data: receipt_data_from_envelope_log(&receipt_envelope),
		tx_index: BENCH_TARGET_TX_INDEX,
	};

	// Receipt-trie keys are `rlp(tx_index)`. The target key holds the sized receipt; at each
	// depth on its path we add a sibling for every other nibble value so the branch node there
	// is full-width (16 children) — the verifier's per-node worst case.
	let target_key = rlp_index_nibbles(BENCH_TARGET_TX_INDEX);
	let target_nibbles: Vec<u8> = target_key.to_vec();
	// `assert!` (not `debug_assert!`): benchmarks may run in release, and a too-short target key
	// would silently produce a shallower-than-`n` proof and miscalibrate the per-node slope. See
	// [`BENCH_TARGET_TX_INDEX`] for the coupling to `MaxProofNodes`.
	assert!(
		target_nibbles.len() >= n - 1,
		"target key has too few nibbles to force an {n}-node proof path",
	);

	// 32 bytes is large enough that a sibling leaf's RLP exceeds 32 bytes and is therefore
	// referenced by hash in its parent branch (not inlined), making the branch full-width.
	let sibling_value = vec![0u8; 32];
	let mut keyed: Vec<(Nibbles, Vec<u8>)> = Vec::with_capacity(n + 15 * (n - 1));
	keyed.push((target_key, receipt_envelope));
	for i in 0..n - 1 {
		for nibble in 0u8..16 {
			if nibble == target_nibbles[i] {
				continue; // the target itself occupies this child slot
			}
			let mut sibling = target_nibbles[..i].to_vec();
			sibling.push(nibble);
			keyed.push((Nibbles::from_nibbles(&sibling), sibling_value.clone()));
		}
	}
	// alloy_trie requires sorted key insertion.
	keyed.sort_by(|a, b| a.0.cmp(&b.0));

	let retainer = ProofRetainer::new(vec![target_key]);
	let mut hb = HashBuilder::default().with_proof_retainer(retainer);
	for (key, value) in &keyed {
		hb.add_leaf(*key, value.as_slice());
	}
	let root: B256 = hb.root();
	let proof_nodes = hb.take_proof_nodes();
	let proof: Vec<Vec<u8>> = proof_nodes
		.matching_nodes_sorted(&target_key)
		.into_iter()
		.map(|(_, bytes)| bytes.to_vec())
		.collect();
	// `assert_eq!` (not `debug_assert_eq!`): the produced proof length is exactly the `n` the
	// weight is charged against, so a release benchmark must fail loudly rather than fit a slope
	// against the wrong node count.
	assert_eq!(proof.len(), n, "receipt proof should have exactly n nodes");

	(root.0.into(), proof, receipt_log)
}

fn rlp_index_nibbles(index: u64) -> Nibbles {
	let mut buf = Vec::new();
	index.encode(&mut buf);
	Nibbles::unpack(buf.as_slice())
}

/// Encode an Eip1559 `ReceiptEnvelope` whose primary log matches `log` and whose total
/// RLP-encoded length equals `target_size`. The variable-size knob is either the matching
/// log's `data` field (when `log.filler_log_data_builder` is `None`) or a second
/// zero-address filler log appended after the matching one (when it is `Some`).
fn encode_sized_receipt_envelope(target_size: usize, log: &LogTemplate) -> Vec<u8> {
	let target_size = target_size.max(MIN_RECEIPT_SIZE as usize);
	// Initial guess: target_data_len = target_size - constant overhead. Iterate to
	// converge because RLP overhead grows non-uniformly with data length, and the v2
	// inbound data_builder may produce ABI-encoded payloads whose size jumps in 32-byte
	// chunks.
	let mut data_len = target_size.saturating_sub(MIN_RECEIPT_SIZE as usize);
	let mut last = encode_envelope(log, data_len);
	for _ in 0..16 {
		if last.len() == target_size {
			return last;
		}
		// Nudge by the difference, but never below 0.
		if last.len() < target_size {
			data_len = data_len.saturating_add(target_size - last.len());
		} else {
			data_len = data_len.saturating_sub(last.len() - target_size);
		}
		last = encode_envelope(log, data_len);
	}
	last
}

fn encode_envelope(log: &LogTemplate, data_len: usize) -> Vec<u8> {
	// Matching-log data: when a filler builder is present, the primary data is the fixed
	// matching-log payload (so the verifier's matching-log scan succeeds); the filler
	// log's data is what scales with `data_len`. Otherwise the primary data scales.
	let (primary_data, filler_data) = match &log.filler_log_data_builder {
		Some(filler_builder) => ((log.data_builder)(0), Some((filler_builder)(data_len))),
		None => ((log.data_builder)(data_len), None),
	};

	let primary_log = alloy_primitives::Log {
		address: alloy_primitives::Address::from(log.gateway),
		data: LogData::new_unchecked(
			log.topics.iter().map(|t| B256::from(t.0)).collect(),
			Bytes::from(primary_data),
		),
	};
	let mut logs = vec![primary_log];
	if let Some(filler) = filler_data {
		logs.push(alloy_primitives::Log {
			address: alloy_primitives::Address::ZERO,
			data: LogData::new_unchecked(vec![], Bytes::from(filler)),
		});
	}

	let receipt = Receipt { status: Eip658Value::Eip658(true), cumulative_gas_used: 21_000, logs };
	let envelope = ReceiptEnvelope::Eip1559(ReceiptWithBloom { receipt, logs_bloom: Bloom::ZERO });
	let mut out = Vec::new();
	envelope.encode(&mut out);
	out
}

/// Decode the envelope just enough to recover the primary log's `data` field. The
/// verifier compares the submitted log's `data` byte-for-byte against the receipt's log,
/// so the `Log` we synthesize must carry the exact padded data we stored in the envelope.
fn receipt_data_from_envelope_log(envelope_bytes: &[u8]) -> Vec<u8> {
	use alloy_rlp::Decodable;
	let env = ReceiptEnvelope::decode(&mut &envelope_bytes[..]).expect("envelope round-trips; qed");
	let receipt = env.as_receipt().expect("Eip1559 receipt; qed");
	receipt.logs[0].data.data.to_vec()
}

fn build_execution_header(receipts_root: H256) -> deneb::ExecutionPayloadHeader {
	deneb::ExecutionPayloadHeader {
		parent_hash: H256::zero(),
		fee_recipient: H160::zero(),
		state_root: H256::zero(),
		receipts_root,
		logs_bloom: vec![0u8; 256],
		prev_randao: H256::zero(),
		block_number: BENCH_SLOT,
		gas_limit: 30_000_000,
		gas_used: 21_000,
		timestamp: 0,
		extra_data: vec![],
		base_fee_per_gas: U256::from(7u64),
		block_hash: H256::zero(),
		transactions_root: H256::zero(),
		withdrawals_root: H256::zero(),
		blob_gas_used: 0,
		excess_blob_gas: 0,
	}
}

/// Climb the merkle tree from `leaf` against `branch` at `index`. Mirrors the algorithm
/// in `ethereum-client::merkle_proof::compute_merkle_root` (the helper is pallet-private,
/// so we replicate it here).
fn compute_merkle_root(leaf: H256, branch: &[H256], index: usize) -> H256 {
	let mut value: [u8; 32] = leaf.into();
	for (i, node) in branch.iter().enumerate() {
		let mut data = [0u8; 64];
		if (index >> i) & 1 == 1 {
			data[0..32].copy_from_slice(node.as_bytes());
			data[32..64].copy_from_slice(&value);
		} else {
			data[0..32].copy_from_slice(&value);
			data[32..64].copy_from_slice(node.as_bytes());
		}
		value = sha2_256(&data);
	}
	value.into()
}
