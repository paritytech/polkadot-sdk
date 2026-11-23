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

use crate::nodes::jam::chain::{
	anchor::{
		choose_lookup_anchor, fetch_anchor_state_proof, lookup_anchor_survives_reporting,
		register_jam_state_reader, LOOKUP_ANCHOR_SAFETY_MARGIN, MAX_LOOKUP_ANCHOR_AGE,
	},
	in_flight_reports::{decode_digest_head, push_report, InFlightReport, ReportSource},
	para_head::{head_hash_of, resolve_heads},
	pools::{scan_pools, PoolScan},
	slots::{jam_slot_at, JAM_SLOT_DURATION_MS},
};
use codec::{Decode, Encode};
use cumulus_client_jam_authorizer::testing::authorizer_of;
use cumulus_jam_aura_primitives::timestamp_for_anchor;
use cumulus_jam_state_reader::{JamStateExt, JAM_PROOF_KEY};
use futures::{channel::mpsc, FutureExt, Stream, StreamExt};
use jam_interface::{
	AuthPool, AuthorizerHash, BlockDesc, HeaderHash, JamStateSource, ServiceId, Slot as JamSlot,
	StateRootHash, StorageKey, WorkPackageHash, WorkReport,
};
use jam_types::RefineContext;
use parachain_service_core::{
	para_info_key, service_value_state_key, verify as verify_state_proof, ParaInfo, StateKey,
	StateProof,
};
use sp_additional_data::{hash_commitments, hash_value, AdditionalDataExt};
use sp_core::H256;
use sp_externalities::Extensions;
use sp_runtime::traits::{BlakeTwo256, Header as HeaderT};
use sp_timestamp::Timestamp;

fn para_info(head: Vec<u8>) -> Vec<u8> {
	ParaInfo {
		head_data: head.try_into().expect("heads in these tests are below 4 KiB; qed"),
		validation_code: None,
		announced_upgrade: None,
		total_state_balance: 0,
		used_state_balance: 0,
		is_deregistering: false,
	}
	.encode()
}

#[test]
fn the_head_is_the_hash_in_para_info() {
	let hash = H256::repeat_byte(0xab);
	assert_eq!(head_hash_of::<H256>(&para_info(hash.as_bytes().to_vec())), Ok(hash));
}

#[test]
fn an_encoded_header_is_not_a_head_hash() {
	use sp_runtime::traits::Header as _;
	let header = sp_runtime::generic::Header::<u32, sp_runtime::traits::BlakeTwo256>::new(
		1,
		Default::default(),
		Default::default(),
		Default::default(),
		Default::default(),
	);
	assert!(head_hash_of::<H256>(&para_info(header.encode())).is_err());
}

#[test]
fn bytes_that_are_no_para_info_are_an_error() {
	assert!(head_hash_of::<H256>(&[0xff, 0x00]).is_err());
}

type Resolved = std::pin::Pin<Box<dyn Stream<Item = &'static str> + Send>>;

fn resolver(
	known: Vec<(H256, &'static str)>,
) -> (mpsc::UnboundedSender<H256>, mpsc::UnboundedSender<(H256, &'static str)>, Resolved) {
	let (heads_tx, heads_rx) = mpsc::unbounded();
	let (imports_tx, imports_rx) = mpsc::unbounded();
	let lookup =
		move |hash: &H256| known.iter().find(|(known, _)| known == hash).map(|(_, header)| *header);
	let heads = heads_rx.map(|hash| (hash, ()));
	let resolved = resolve_heads(heads, imports_rx, lookup).map(|(header, ())| header);
	(heads_tx, imports_tx, Box::pin(resolved))
}

/// What travels with a head comes out beside its header, whether the header was known at
/// once or had to be imported first.
#[test]
fn the_heads_extra_travels_with_its_header() {
	let (a, b) = (H256::repeat_byte(1), H256::repeat_byte(2));
	let (heads, heads_rx) = mpsc::unbounded::<(H256, u32)>();
	let (imports, imports_rx) = mpsc::unbounded::<(H256, &'static str)>();
	let lookup = move |hash: &H256| (*hash == a).then_some("a");
	let mut resolved = Box::pin(resolve_heads(heads_rx, imports_rx, lookup));

	heads
		.unbounded_send((a, 10))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), Some(Some(("a", 10))));

	heads
		.unbounded_send((b, 11))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None, "b waits for its import");
	imports
		.unbounded_send((b, "b"))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), Some(Some(("b", 11))));
}

#[test]
fn a_known_head_resolves_at_once() {
	let a = H256::repeat_byte(1);
	let (heads, _imports, mut resolved) = resolver(vec![(a, "a")]);
	heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), Some(Some("a")));
}

#[test]
fn an_unknown_head_resolves_when_imported() {
	let (a, other) = (H256::repeat_byte(1), H256::repeat_byte(9));
	let (heads, imports, mut resolved) = resolver(vec![]);
	heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None);
	imports
		.unbounded_send((other, "other"))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None, "an unrelated import yields nothing");
	imports
		.unbounded_send((a, "a"))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), Some(Some("a")));
}

#[test]
fn a_newer_head_replaces_the_waiting_one() {
	let (a, b) = (H256::repeat_byte(1), H256::repeat_byte(2));
	let (heads, imports, mut resolved) = resolver(vec![]);
	heads.unbounded_send(a).expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None);
	heads.unbounded_send(b).expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None);
	imports
		.unbounded_send((a, "a"))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), None, "the replaced head is not reported");
	imports
		.unbounded_send((b, "b"))
		.expect("the resolver holds the receiving end; qed");
	assert_eq!(resolved.next().now_or_never(), Some(Some("b")));
}

#[test]
fn the_resolver_ends_with_the_heads() {
	let (heads, _imports, mut resolved) = resolver(vec![]);
	drop(heads);
	assert_eq!(resolved.next().now_or_never(), Some(None));
}

/// A finalized JAM chain, newest first, one block per slot ending at `newest_slot`.
fn finalized_chain(newest_slot: JamSlot, length: u32) -> Vec<BlockDesc> {
	(0..length)
		.map(|back| {
			let slot = newest_slot.saturating_sub(back as JamSlot);
			BlockDesc { header_hash: HeaderHash::from([slot as u8; 32]), slot }
		})
		.collect()
}

/// The lookup anchor is the newest finalized block, not a block whose slot names this
/// collator: under slot-bound sets the *anchor* slot selects the collator, and the set is
/// announced a session ahead, so there is nothing left to walk for. What remains is
/// polkajam's freshness rule, applied when the package is reported.
#[test]
fn the_lookup_anchor_is_the_newest_finalized_block() {
	let chain = finalized_chain(20, 9);

	let chosen = choose_lookup_anchor(&chain[0], chain[1]).expect("slot 19 is recent enough");

	assert_eq!(chosen.slot, 19, "no walk, the parent of the finalized block itself");
}

/// A finalized block that would expire before the package carrying it could be reported is
/// not a lookup anchor at all: the tick has to be skipped, loudly.
#[test]
fn a_stale_lookup_anchor_is_refused() {
	let budget = MAX_LOOKUP_ANCHOR_AGE - LOOKUP_ANCHOR_SAFETY_MARGIN;
	let anchor = BlockDesc { header_hash: HeaderHash::from([1u8; 32]), slot: 100 };

	let newest = |slot| BlockDesc { header_hash: HeaderHash::from([2u8; 32]), slot };
	assert!(choose_lookup_anchor(&anchor, newest(100 - budget)).is_some());
	assert!(choose_lookup_anchor(&anchor, newest(100 - budget - 1)).is_none());
}

/// One core's authorizer pool, holding `hashes` as the eight-deep window it is.
fn pool(hashes: &[AuthorizerHash]) -> AuthPool {
	AuthPool::truncate_from(hashes.to_vec())
}

fn other_hash() -> AuthorizerHash {
	AuthorizerHash([0xee; 32])
}

/// Nothing waits on JAM to author, but a package can only be submitted to a core whose pool
/// holds this para's authorizer — anywhere else a guarantor would refuse it. So the scan is
/// what turns "which core?" from a flag into a fact read off the chain.
#[test]
fn a_core_holding_the_paras_authorizer_is_where_its_packages_go() {
	let ours = authorizer_of("alice", "Alice", 1).hash();
	let pools = vec![pool(&[other_hash()]), pool(&[other_hash(), ours])];

	let scan = scan_pools(&pools, ours);

	assert_eq!(scan.target, Some(1));
	assert!(scan.also_on.is_empty());
}

/// A core that holds no copy of our hash is not ours, however full its pool is; with none of
/// them holding it the para has no core at all, which is a state the collator has to survive
/// rather than crash on — it keeps authoring and submits nothing.
#[test]
fn no_core_holding_it_is_no_target_at_all() {
	let ours = authorizer_of("alice", "Alice", 1).hash();

	assert_eq!(scan_pools(&[pool(&[other_hash()]), pool(&[])], ours), PoolScan::default());
	assert_eq!(scan_pools(&[], ours).target, None, "a chain with no cores names none");
}

/// Mid-reassignment both the old and the new core hold our hash while the old one's pool
/// drains. The scan keeps them in index order, which is the order a turn's packages take
/// them: one package per core, lowest index first.
#[test]
fn several_cores_holding_it_go_to_the_lowest_index() {
	let ours = authorizer_of("alice", "Alice", 1).hash();
	let pools = vec![pool(&[ours]), pool(&[other_hash()]), pool(&[ours]), pool(&[ours])];

	let scan = scan_pools(&pools, ours);

	assert_eq!(scan.target, Some(0));
	assert_eq!(scan.also_on, vec![2, 3]);
}

/// A turn puts one package on each core that holds the para's authorizer, so the i-th
/// package needs the i-th core — and the turn has no more packages once they run out.
#[test]
fn a_turn_puts_its_packages_on_different_cores() {
	let ours = authorizer_of("alice", "Alice", 1).hash();
	let pools = vec![pool(&[ours]), pool(&[other_hash()]), pool(&[ours]), pool(&[ours])];

	let scan = scan_pools(&pools, ours);

	assert_eq!(scan.core_count(), 3);
	assert_eq!(scan.core_for_package(0), Some(0));
	assert_eq!(scan.core_for_package(1), Some(2));
	assert_eq!(scan.core_for_package(2), Some(3));
	assert_eq!(scan.core_for_package(3), None, "the turn has no core left");
}

/// With no core holding it there is no package to place and no core to place it on: the
/// builder keeps authoring, but the turn produces nothing to submit.
#[test]
fn a_para_with_no_core_has_no_packages_to_place() {
	let scan = PoolScan::default();

	assert_eq!(scan.core_count(), 0);
	assert_eq!(scan.core_for_package(0), None);
}

/// Two paras on one collator set differ only in their config, so their hashes differ and one
/// para's core must never look like the other's.
#[test]
fn another_paras_core_is_not_ours() {
	let ours = authorizer_of("alice,bob", "Alice", 1).hash();
	let theirs = authorizer_of("alice,bob", "Alice", 2).hash();
	assert_ne!(ours, theirs);

	assert_eq!(scan_pools(&[pool(&[theirs])], ours).target, None);
}

/// polkajam refuses a lookup anchor older than `max_lookup_anchor_age`, and it applies that
/// when the package is *reported*, not when it is built. A package has the report deadline to
/// get there, so the builder has to keep that much of the window in hand — otherwise the
/// package dies in flight and looks exactly like one that was never submitted.
#[test]
fn a_lookup_anchor_must_outlive_the_packages_time_in_flight() {
	let budget = MAX_LOOKUP_ANCHOR_AGE - LOOKUP_ANCHOR_SAFETY_MARGIN;
	assert!(lookup_anchor_survives_reporting(100, 100 - budget));
	assert!(!lookup_anchor_survives_reporting(100, 100 - budget - 1));
	assert!(lookup_anchor_survives_reporting(0, 0), "a chain younger than the window is fine");
}

#[test]
fn the_anchor_timestamp_is_common_era_based() {
	assert_eq!(timestamp_for_anchor(0), jam_types::JAM_COMMON_ERA * 1000);
	assert_eq!(timestamp_for_anchor(10), (jam_types::JAM_COMMON_ERA + 60) * 1000);
}

/// The wall-clock-driven builder derives its JAM slot from the clock and its timestamp from
/// that slot, so the two conversions have to be exact inverses.
#[test]
fn jam_slot_at_inverts_the_anchor_timestamp() {
	for slot in [0, 1, 12_345] {
		assert_eq!(jam_slot_at(Timestamp::new(timestamp_for_anchor(slot))), slot);
	}
}

/// Anything inside a slot belongs to that slot; only the next boundary moves it on.
#[test]
fn jam_slot_at_floors_to_the_slot_start() {
	let slot_start = timestamp_for_anchor(7);
	assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS - 1)), 7);
	assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS)), 8);
}

/// Before the common era there are no slots; clamping to slot 0 keeps a badly-set clock from
/// wrapping the subtraction.
#[test]
fn jam_slot_at_clamps_before_the_common_era() {
	assert_eq!(jam_slot_at(Timestamp::new(0)), 0);
}

pub(crate) type TestHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

const TEST_PARA_ID: u32 = 1000;

/// A chain of headers, each one the child of its predecessor — the shape the segment holds.
pub(crate) fn chain(length: u32) -> Vec<TestHeader> {
	let mut parent = H256::zero();
	(1..=length)
		.map(|number| {
			let header = TestHeader::new(
				number,
				Default::default(),
				Default::default(),
				parent,
				Default::default(),
			);
			parent = header.hash();
			header
		})
		.collect()
}

/// A work output in the shape the parachain service produces: para id, head data (the
/// header hash), the parent head hash, and — in newer services — the block's number.
fn work_output(header: &TestHeader, with_number: bool) -> Vec<u8> {
	let mut output = (0u32, header.hash().as_ref().to_vec(), [7u8; 32]).encode();
	if with_number {
		output.extend(header.number().encode());
	}
	output
}

/// A work report as JAM state holds it, carrying one item refined by `service`.
fn work_report(byte: u8, service: ServiceId) -> WorkReport {
	work_report_with(byte, service, Ok(Default::default()))
}

/// The same, with the refinement result spelled out.
fn work_report_with(
	byte: u8,
	service: ServiceId,
	result: Result<jam_types::WorkOutput, jam_types::WorkError>,
) -> WorkReport {
	let digest = jam_types::WorkDigest {
		service,
		code_hash: Default::default(),
		payload_hash: Default::default(),
		accumulate_gas: 0,
		result,
		refine_load: Default::default(),
	};
	WorkReport {
		package_spec: jam_std_common::WorkPackageSpec {
			hash: WorkPackageHash::from([byte; 32]),
			len: 0,
			erasure_root: Default::default(),
			erasure_shards: Default::default(),
			exports_root: Default::default(),
			exports_count: 0,
		},
		context: RefineContext {
			anchor: Default::default(),
			anchor_slot: 0,
			state_root: Default::default(),
			beefy_root: Default::default(),
			lookup_anchor: Default::default(),
			lookup_anchor_slot: 0,
			lookup_anchor_state_root: Default::default(),
			prerequisites: Default::default(),
		},
		core_index: 0,
		authorizer_hash: Default::default(),
		auth_gas_used: 0,
		auth_output: Default::default(),
		sr_lookup: Default::default(),
		results: vec![digest].try_into().expect("a single digest always fits; qed"),
	}
}

/// Only our service's packages belong in the monitor: another service's report says nothing
/// about this parachain, and logging it as ours would make the pipeline view a lie.
#[test]
fn only_reports_for_our_service_are_kept() {
	let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

	push_report(&mut reports, &work_report(1, 7), ReportSource::Availability, 42);
	push_report(&mut reports, &work_report(2, 42), ReportSource::Availability, 42);

	assert_eq!(
		reports,
		vec![InFlightReport {
			wp_hash: WorkPackageHash::from([2u8; 32]),
			source: ReportSource::Availability,
			head: None,
		}],
	);
}

/// The same package sits in availability at one moment and in the ready queue the next, and a
/// tick reads both; listing it twice would double every package in the monitor's view.
#[test]
fn a_package_listed_in_both_state_entries_is_kept_once() {
	let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

	push_report(&mut reports, &work_report(1, 42), ReportSource::Availability, 42);
	push_report(&mut reports, &work_report(1, 42), ReportSource::ReadyQueue, 42);

	assert_eq!(reports.len(), 1);
	assert_eq!(reports[0].source, ReportSource::Availability);
}

/// The monitor reads which block a package carries out of the work digest, and the shape of
/// that output belongs to the service: a newer service appends the block number to it. Both shapes
/// have to read the same, or the collator would go blind against one version of the service.
#[test]
fn both_shapes_of_the_work_output_yield_the_same_block() {
	let header = chain(1).remove(0);

	for with_number in [false, true] {
		assert_eq!(
			decode_digest_head::<H256>(&work_output(&header, with_number)),
			Some(header.hash()),
			"the shared prefix parses in either shape",
		);
	}
}

/// ...and anything else must come back as "unknown", never as a panic and never as an error
/// that could reach the tick: a service this collator was not built against, a refine that
/// failed, or plain garbage all end up here.
#[test]
fn an_unreadable_work_output_is_simply_unknown() {
	let header = chain(1).remove(0);
	let well_formed = work_output(&header, false);

	for (what, output) in [
		("empty", Vec::new()),
		("garbage", vec![0xff; 64]),
		("truncated head data", well_formed[..well_formed.len() - 40].to_vec()),
		("a vector that is no header", (0u32, vec![1u8, 2, 3], [0u8; 32]).encode()),
	] {
		assert!(
			decode_digest_head::<H256>(&output).is_none(),
			"{what} must not be read as a block",
		);
	}
}

/// A package whose digest cannot be read is still in flight, and that is the fact the monitor
/// exists to show — dropping the report would hide a package from the only pre-accumulation
/// view there is. Same for one whose refinement failed outright.
#[test]
fn a_package_with_an_unreadable_digest_is_still_reported() {
	let header = chain(1).remove(0);
	let mut reports: Vec<InFlightReport<TestHeader>> = Vec::new();

	let output = jam_types::WorkOutput(work_output(&header, true));
	let readable = work_report_with(1, 42, Ok(output));
	push_report(&mut reports, &readable, ReportSource::Availability, 42);
	let unreadable = work_report_with(2, 42, Ok(jam_types::WorkOutput(vec![0xff; 8])));
	push_report(&mut reports, &unreadable, ReportSource::ReadyQueue, 42);
	let failed = work_report_with(3, 42, Err(jam_types::WorkError::Panic));
	push_report(&mut reports, &failed, ReportSource::ReadyQueue, 42);

	assert_eq!(reports.len(), 3);
	assert_eq!(reports[0].head, Some(header.hash()));
	assert_eq!(reports[1].head, None);
	assert_eq!(reports[2].head, None, "a failed refinement names no block");
}

/// The SCALE-encoding of the service's `ParaInfo` entry: compact-length-prefixed head data,
/// no validation code, no pending upgrade, zero balances, not deregistering.
fn para_info_value(head: &[u8]) -> Vec<u8> {
	let mut value = codec::Compact::<u32>(head.len() as u32).encode();
	value.extend(head);
	// `None` validation code, `None` pending upgrade, compact(0) balance, compact(0) used,
	// `false` — five zero bytes after the head.
	value.extend([0u8; 5]);
	value
}

/// A single-key trie proof of `value` under `state_key`, the shape polkajam returns: a large
/// leaf (the value exceeds a leaf's 32-byte payload) plus its preimage.
fn range_proof_of(state_key: StateKey, value: &[u8]) -> (jam_interface::RangeProof, [u8; 32]) {
	let mut node = [0u8; 64];
	node[0] = 0b1100_0000;
	node[1..32].copy_from_slice(&state_key);
	node[32..].copy_from_slice(&parachain_service_core::blake2_256(value));
	let state_root = parachain_service_core::blake2_256(&node);
	let proof = jam_interface::RangeProof {
		nodes: vec![jam_std_common::ProofNode::from(node)],
		values: vec![(
			StorageKey(state_key),
			jam_types::AnyBytes::from(bytes::Bytes::from(value.to_vec())),
		)],
	};
	(proof, state_root)
}

/// A JAM state source serving one fixed range proof, so the state-proof path is exercised
/// without a live node.
struct StubJam {
	proof: jam_interface::RangeProof,
}

#[async_trait::async_trait]
impl JamStateSource for StubJam {
	async fn state_value(
		&self,
		_at: HeaderHash,
		_key: StorageKey,
	) -> jam_interface::Result<Option<Vec<u8>>> {
		unimplemented!()
	}

	async fn state_value_stream(
		&self,
		_key: StorageKey,
		_finalized: bool,
	) -> jam_interface::Result<
		jam_interface::BoxStream<'static, jam_interface::ChainSubUpdate<Option<Vec<u8>>>>,
	> {
		unimplemented!()
	}

	async fn state_proof(
		&self,
		_at: HeaderHash,
		_start_key: StorageKey,
		_end_key: StorageKey,
		_size_limit: u32,
	) -> jam_interface::Result<jam_interface::RangeProof> {
		Ok(self.proof.clone())
	}

	async fn service_value(
		&self,
		_at: HeaderHash,
		_service: ServiceId,
		_key: &[u8],
	) -> jam_interface::Result<Option<Vec<u8>>> {
		unimplemented!()
	}

	async fn service_value_stream(
		&self,
		_service: ServiceId,
		_key: &[u8],
		_finalized: bool,
	) -> jam_interface::Result<
		jam_interface::BoxStream<'static, jam_interface::ChainSubUpdate<Option<Vec<u8>>>>,
	> {
		unimplemented!()
	}
}

/// Every authored block carries exactly one `JAM_PROOF_KEY` entry — the encoded
/// `(anchor_state_root, proof)` of the para head at the anchor — and its digest commits exactly
/// that one entry. The entry decodes, verifies against the anchor root, and reads back the
/// head.
#[tokio::test]
async fn the_state_proof_is_verified_and_carried_as_one_entry() {
	let service_id = 9;
	let para_id = parachain_service_core::types::ParaId::from(TEST_PARA_ID);
	let expected_head = chain(1).remove(0).hash().as_ref().to_vec();
	let value = para_info_value(&expected_head);
	assert!(value.len() > 32, "the head entry is a large leaf, as on the real service");
	let state_key = service_value_state_key(service_id, &para_info_key(para_id));
	let (range_proof, state_root) = range_proof_of(state_key, &value);

	let (proof, proved) = fetch_anchor_state_proof(
		&StubJam { proof: range_proof },
		HeaderHash::default(),
		&StateRootHash::from(state_root),
		service_id,
		TEST_PARA_ID,
	)
	.await
	.expect("the node's own verification of a well-formed proof succeeds");

	assert_eq!(proved, Some(value.clone()), "the proof proves the stored head entry");

	let mut extra_extensions = Extensions::new();
	let additional_data =
		register_jam_state_reader(&mut extra_extensions, state_root, proof.clone());

	assert_eq!(additional_data.len(), 1, "exactly one JAM_PROOF_KEY entry per block");
	let entry = additional_data.get(JAM_PROOF_KEY).expect("the entry is present");

	let (decoded_anchor, decoded_proof) =
		<(parachain_service_core::Hash, StateProof)>::decode(&mut &entry[..])
			.expect("the entry decodes as (anchor_state_root, StateProof)");
	assert_eq!(decoded_anchor, state_root);
	assert_eq!(decoded_proof, proof);
	assert_eq!(
		verify_state_proof(&decoded_proof, &decoded_anchor, &state_key),
		Ok(Some(value.clone())),
		"the carried proof verifies against the anchor root",
	);

	let info = parachain_service_core::ParaInfo::decode(&mut &value[..])
		.expect("the stored head entry is a ParaInfo");
	assert_eq!(
		info.head_data.to_vec(),
		expected_head,
		"the value at the state key is the head hash"
	);

	// The same proof backs the runtime's reads and the digest while the block is built.
	let reader = extra_extensions
		.get_mut(std::any::TypeId::of::<JamStateExt>())
		.expect("the reader extension is registered")
		.downcast_mut::<JamStateExt>()
		.expect("the reader extension is a JamStateExt")
		.0
		.read(&state_key);
	assert_eq!(reader, Some(value.clone()));

	let digest = extra_extensions
		.get_mut(std::any::TypeId::of::<AdditionalDataExt>())
		.expect("the finalizer extension is registered")
		.downcast_mut::<AdditionalDataExt>()
		.expect("the finalizer extension is an AdditionalDataExt")
		.finalize();
	assert_eq!(
		digest,
		hash_commitments(core::iter::once(hash_value(entry))),
		"the digest commits exactly the one carried entry — one DigestItem::AdditionalData",
	);
}

/// A proof the node itself cannot verify must abort the tick — never ship a block JAM would
/// refuse. Both a tampered proof and a well-formed proof under a wrong state root do.
#[tokio::test]
async fn an_unverifiable_state_proof_aborts_the_build() {
	let service_id = 9;
	let head = chain(1).remove(0);
	let value = para_info_value(&head.encode());
	let state_key = service_value_state_key(
		service_id,
		&para_info_key(parachain_service_core::types::ParaId::from(TEST_PARA_ID)),
	);
	let (range_proof, state_root) = range_proof_of(state_key, &value);

	let mut tampered = range_proof.clone();
	tampered.nodes.clear();
	assert!(
		fetch_anchor_state_proof(
			&StubJam { proof: tampered },
			HeaderHash::default(),
			&StateRootHash::from(state_root),
			service_id,
			TEST_PARA_ID,
		)
		.await
		.is_err(),
		"an incomplete proof must fail the tick, not ship",
	);

	assert!(
		fetch_anchor_state_proof(
			&StubJam { proof: range_proof },
			HeaderHash::default(),
			&StateRootHash::from([7u8; 32]),
			service_id,
			TEST_PARA_ID,
		)
		.await
		.is_err(),
		"a proof for another state root must fail the tick",
	);
}
