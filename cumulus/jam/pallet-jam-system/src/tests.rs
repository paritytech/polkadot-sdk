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

use crate::{mock::*, *};
use alloc::collections::BTreeMap;
use codec::Encode;
use cumulus_jam_state_reader::{JamProofFinalizer, JamProofReader, JamStateExt, JAM_PROOF_KEY};
use cumulus_primitives_core::{ClaimQueueOffset, CoreInfo, CoreSelector};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use parachain_service_core::{self as jam_helpers, log::AccumulateLog, LogEntry, ParachainLog};
use sp_additional_data::{
	hash_commitments, hash_value, AdditionalDataExt, AdditionalDataFinalizer,
};
use sp_core::{storage::well_known_keys::CODE, H256};
use sp_runtime::{DigestItem, DispatchError};

/// The hash of mock block `n`; block `n` builds on `block_hash(n - 1)`, genesis is `block_hash(0)`.
fn block_hash(n: u64) -> H256 {
	H256::repeat_byte(n as u8)
}

/// Run blocks `from..=to`, each on the previous one.
fn run_blocks(from: u64, to: u64) {
	for n in from..=to {
		initialize_block(n, block_hash(n - 1));
	}
}

/// A `ParaInfo` with `head` as head data and the given active and announced code refs.
fn para_info(
	head: &[u8],
	active: Option<([u8; 32], u32)>,
	announced: Option<([u8; 32], u32)>,
) -> ParaInfo {
	use jam_helpers::types::{HeadData, ValidationCodeRef};
	let to_ref = |(hash, len): ([u8; 32], u32)| ValidationCodeRef { hash: hash.into(), len };
	ParaInfo {
		head_data: HeadData::try_from(head.to_vec()).expect("head is shorter than 4 KiB; qed"),
		validation_code: active.map(to_ref),
		announced_upgrade: announced.map(to_ref),
		total_state_balance: 0,
		used_state_balance: 0,
		is_deregistering: false,
	}
}

/// Serve `info` as the mock para's `ParaInfo`.
fn set_para_info(info: &ParaInfo) {
	set_mock_jam_reads([(para_info_state_key().to_vec(), info.encode())].into());
}

/// Serve a `ParaInfo` naming the block with hash `head` as included.
fn set_included(head: H256) {
	set_para_info(&para_info(head.as_bytes(), None, None));
}

/// Serve no `ParaInfo`, as before the para's registration.
fn set_no_para_info() {
	set_mock_jam_reads(BTreeMap::new());
}

fn segment() -> Vec<H256> {
	UnincludedSegment::<Test>::get()
}

// --- `para_info` (ported from parachain-system's `read_included_para_head_*` tests) -----------

#[test]
fn para_info_reads_from_jam_state() {
	let info = para_info(H256::repeat_byte(0xca).as_bytes(), None, None);
	new_test_ext().execute_with(|| {
		set_para_info(&info);
		assert_eq!(JamSystem::para_info(), Some(info));
	});
}

/// A head that is not a 32-byte hash (e.g. an encoded header) names no included block, so the
/// segment drains nothing.
#[test]
fn para_info_non_hash_head_names_no_included_block() {
	new_test_ext().execute_with(|| {
		set_para_info(&para_info(&[0xca, 0xfe, 0x00, 0x01], None, None));
		assert!(JamSystem::para_info().is_some());
		run_blocks(1, 3);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2)]);
	});
}

/// A `ParaInfo` value that does not decode reads as no `ParaInfo`.
#[test]
fn para_info_malformed_value_reads_none() {
	new_test_ext().execute_with(|| {
		set_mock_jam_reads([(para_info_state_key().to_vec(), vec![0xff, 0x00, 0x01])].into());
		assert_eq!(JamSystem::para_info(), None);
	});
}

/// A JAM state trie for tests: builds the Gray-Paper binary-trie nodes for a set of key/value
/// entries and can emit a proof for the whole trie. Independent merklization, pinning the layout
/// against polkajam's own trie.
struct JamTrie {
	nodes: Vec<jam_helpers::ProofNode>,
	/// Values too large to sit inside their leaf.
	values: Vec<(jam_helpers::StateKey, Vec<u8>)>,
	root: jam_helpers::Hash,
}

impl JamTrie {
	fn new(mut entries: Vec<(jam_helpers::StateKey, Vec<u8>)>) -> Self {
		entries.sort_by_key(|(a, _)| *a);
		let values = entries.iter().filter(|(_, value)| value.len() > 32).cloned().collect();
		let mut trie = JamTrie { nodes: Vec::new(), values, root: [0u8; 32] };
		trie.root = trie.hash_subtree(0, &entries);
		trie
	}

	fn hash_subtree(
		&mut self,
		depth: usize,
		entries: &[(jam_helpers::StateKey, Vec<u8>)],
	) -> jam_helpers::Hash {
		match entries {
			[] => [0u8; 32],
			[(key, value)] => self.push(jam_leaf_node(key, value)),
			_ => {
				let (left, right): (Vec<_>, Vec<_>) =
					entries.iter().cloned().partition(|(key, _)| jam_bit_at(key, depth) == 0);
				let left = self.hash_subtree(depth + 1, &left);
				let right = self.hash_subtree(depth + 1, &right);
				self.push(jam_branch_node(&left, &right))
			},
		}
	}

	fn push(&mut self, node: jam_helpers::ProofNode) -> jam_helpers::Hash {
		let hash = jam_helpers::blake2_256(&node);
		self.nodes.push(node);
		hash
	}

	fn proof(&self) -> jam_helpers::StateProof {
		jam_helpers::StateProof { nodes: self.nodes.clone(), values: self.values.clone() }
	}
}

fn jam_bit_at(key: &jam_helpers::StateKey, depth: usize) -> u8 {
	(key[depth / 8] >> (7 - (depth % 8))) & 1
}

fn jam_leaf_node(key: &jam_helpers::StateKey, value: &[u8]) -> jam_helpers::ProofNode {
	let mut node = [0u8; 64];
	node[1..32].copy_from_slice(key);
	if value.len() > 32 {
		node[0] = 0b1100_0000;
		node[32..].copy_from_slice(&jam_helpers::blake2_256(value));
	} else {
		node[0] = 0b1000_0000 | value.len() as u8;
		node[32..32 + value.len()].copy_from_slice(value);
	}
	node
}

fn jam_branch_node(left: &jam_helpers::Hash, right: &jam_helpers::Hash) -> jam_helpers::ProofNode {
	let mut node = [0u8; 64];
	node[..32].copy_from_slice(left);
	node[32..].copy_from_slice(right);
	node[0] &= 0b0111_1111;
	node
}

/// The `(anchor_state_root, proof)` entry a PoV carries under `JAM_PROOF_KEY` for `trie`.
fn carried_entry(trie: &JamTrie) -> Vec<u8> {
	(trie.root, &trie.proof()).encode()
}

/// The reader `jam_validate_block` builds from a carried entry.
fn reader_for(entry: &[u8]) -> JamProofReader {
	let (anchor_root, proof) = <([u8; 32], jam_helpers::StateProof)>::decode(&mut &entry[..])
		.expect("entry decodes as (anchor_state_root, proof)");
	JamProofReader::new(anchor_root, proof)
}

/// The refine read is served from the JAM state proof carried in the PoV, verified against the
/// trusted anchor state root, with no live state access.
#[test]
fn para_info_reads_from_carried_jam_proof() {
	let info = para_info(H256::repeat_byte(0xca).as_bytes(), None, None);
	let trie = JamTrie::new(vec![(para_info_state_key(), info.encode())]);

	let mut ext = new_test_ext();
	ext.register_extension(JamStateExt(Box::new(reader_for(&carried_entry(&trie)))));
	ext.execute_with(|| assert_eq!(JamSystem::para_info(), Some(info)));
}

/// A carried proof missing a node the read needs must panic, never serve `None`: collapsing the
/// verify error to absence would let a collator suppress a present value by omitting proof nodes.
#[test]
#[should_panic(expected = "cannot authenticate the requested key")]
fn para_info_tampered_proof_panics() {
	let encoded = para_info(&[0xca, 0xfe, 0x00, 0x01], None, None).encode();
	let state_key = para_info_state_key();
	let trie = JamTrie::new(vec![(state_key, encoded.clone())]);
	let mut proof = trie.proof();
	proof.nodes.retain(|node| node != &jam_leaf_node(&state_key, &encoded));

	let mut ext = new_test_ext();
	ext.register_extension(JamStateExt(Box::new(JamProofReader::new(trie.root, proof))));
	ext.execute_with(|| {
		let _ = JamSystem::para_info();
	});
}

/// The JAM refine digest fold: a carried `JAM_PROOF_KEY` entry arms both the proof-backed reader
/// (serving `para_info`) and the digest finalizer, and the registry fold recomputes exactly the
/// `DigestItem::AdditionalData` the collator committed at authoring,
/// `hash_commitments([hash_value(entry)])`.
#[test]
fn carried_jam_proof_finalizes_to_authored_digest() {
	let info = para_info(H256::repeat_byte(0xca).as_bytes(), None, None);
	let trie = JamTrie::new(vec![(para_info_state_key(), info.encode())]);
	let entry = carried_entry(&trie);
	let finalizer = JamProofFinalizer { commitment: hash_value(&entry) };

	let mut ext = new_test_ext();
	ext.register_extension(JamStateExt(Box::new(reader_for(&entry))));
	ext.register_extension(AdditionalDataExt(
		[(JAM_PROOF_KEY.to_string(), Box::new(finalizer) as Box<dyn AdditionalDataFinalizer>)]
			.into(),
	));
	ext.execute_with(|| {
		assert_eq!(JamSystem::para_info(), Some(info));
		assert_eq!(
			sp_additional_data::additional_data::finalize(),
			hash_commitments(core::iter::once(hash_value(&entry)))
		);
	});
}

// --- unincluded segment ---------------------------------------------------------------------

/// Without a `ParaInfo` the service has included nothing, so every parent stays.
#[test]
fn absent_para_info_keeps_the_segment() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		SegmentCapacity::set(5);
		run_blocks(1, 4);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2), block_hash(3)]);
	});
}

/// A carried proof that proves the `ParaInfo` key absent (another para's leaf sits on the walk)
/// reads no `ParaInfo` and drains nothing.
#[test]
fn absent_para_info_in_carried_proof_keeps_the_segment() {
	let other_para_key = jam_helpers::service_value_state_key(
		jam_helpers::PARACHAIN_SERVICE_ID,
		&jam_helpers::para_info_key(jam_helpers::types::ParaId::from(PARA_ID + 1)),
	);
	let other = para_info(block_hash(1).as_bytes(), None, None);
	let trie = JamTrie::new(vec![(other_para_key, other.encode())]);

	let mut ext = new_test_ext();
	ext.register_extension(JamStateExt(Box::new(reader_for(&carried_entry(&trie)))));
	ext.execute_with(|| {
		assert_eq!(JamSystem::para_info(), None);
		run_blocks(1, 3);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2)]);
	});
}

/// Block 1 builds on genesis, which is included by definition; from block 2 on every parent
/// enters the segment.
#[test]
fn segment_pushes_parents_from_block_two() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 1);
		assert!(segment().is_empty());
		run_blocks(2, 2);
		assert_eq!(segment(), vec![block_hash(1)]);
	});
}

/// The segment drains up to and including the head the service records as included.
#[test]
fn segment_drains_up_to_and_including_the_included_head() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 3);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2)]);

		set_included(block_hash(1));
		run_blocks(4, 4);
		assert_eq!(segment(), vec![block_hash(2), block_hash(3)]);

		// The parent itself included: only the block being built is unincluded.
		set_included(block_hash(4));
		run_blocks(5, 5);
		assert!(segment().is_empty());
	});
}

/// An included head that is not in the segment, like genesis or a head drained before, drains
/// nothing.
#[test]
fn segment_keeps_entries_when_the_included_head_is_not_in_it() {
	new_test_ext().execute_with(|| {
		set_included(block_hash(0));
		run_blocks(1, 3);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2)]);
	});
}

/// Every block needs its place within the hook's capacity.
#[test]
#[should_panic(expected = "No space left for the block in the unincluded segment")]
fn segment_capacity_overflow_panics() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		SegmentCapacity::set(2);
		run_blocks(1, 2);
		assert_eq!(segment(), vec![block_hash(1)]);
		run_blocks(3, 3);
	});
}

#[test]
fn size_after_is_zero_at_genesis() {
	new_test_ext().execute_with(|| {
		assert_eq!(JamSystem::unincluded_segment_size_after(block_hash(0)), 0);
	});
}

/// A hash in the segment counts the entries after it plus the block of this state.
#[test]
fn size_after_found_hash_counts_later_entries_and_the_block() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		SegmentCapacity::set(5);
		run_blocks(1, 4);
		assert_eq!(segment(), vec![block_hash(1), block_hash(2), block_hash(3)]);

		assert_eq!(JamSystem::unincluded_segment_size_after(block_hash(1)), 3);
		assert_eq!(JamSystem::unincluded_segment_size_after(block_hash(3)), 1);
	});
}

/// A hash not in the segment counts the whole segment plus the block of this state: `len + 1`.
#[test]
fn size_after_missing_hash_counts_len_plus_one() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 1);
		assert_eq!(JamSystem::unincluded_segment_size_after(block_hash(0)), 1);

		run_blocks(2, 3);
		let len = segment().len() as u32;
		assert_eq!(len, 2);
		assert_eq!(JamSystem::unincluded_segment_size_after(block_hash(0)), len + 1);
		assert_eq!(JamSystem::unincluded_segment_size_after(H256::repeat_byte(0xee)), len + 1);
	});
}

// --- block initialization -------------------------------------------------------------------

#[test]
fn on_initialize_hands_the_jam_parent_to_the_consensus_hook() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 2);
		assert_eq!(HookCalls::get(), vec![jam_parent(), jam_parent()]);
	});
}

#[test]
#[should_panic(expected = "a JAM block carries a JamParent digest")]
fn on_initialize_panics_without_jam_parent_digest() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		System::initialize(&1, &block_hash(0), &Default::default());
		JamSystem::on_initialize(1);
	});
}

/// The runtime's `block_weight::Config` round-trips through the pallet, and the next block starts
/// without the mode a block set.
#[test]
fn block_weight_mode_lasts_one_block() {
	use cumulus_pallet_parachain_system::block_weight::BlockWeightMode as Mode;

	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 1);
		let mode = Mode::<Test>::FullCore { context: 1 };
		<Test as block_weight::Config>::set_mode(Some(mode.clone()));
		assert_eq!(<Test as block_weight::Config>::mode(), Some(mode));

		run_blocks(2, 2);
		assert_eq!(<Test as block_weight::Config>::mode(), None);
	});
}

/// `PreviousCoreCount` follows the block's `CoreInfo` digest; a block without one used one core.
#[test]
fn previous_core_count_follows_core_info() {
	new_test_ext().execute_with(|| {
		set_no_para_info();
		run_blocks(1, 1);
		JamSystem::on_finalize(1);
		assert_eq!(JamSystem::previous_core_count(), Some(1));

		run_blocks(2, 2);
		System::deposit_log(
			CumulusDigestItem::CoreInfo(CoreInfo {
				selector: CoreSelector(0),
				claim_queue_offset: ClaimQueueOffset(0),
				number_of_cores: codec::Compact(3),
			})
			.to_digest_item(),
		);
		JamSystem::on_finalize(2);
		assert_eq!(JamSystem::previous_core_count(), Some(3));
	});
}

// --- code upgrade by hash (moved from parachain-system's `jam_upgrade_*` tests) --------------

const CODE_HASH: [u8; 32] = [0xab; 32];
const CODE_LEN: u32 = 42;

fn runtime_environment_updated() -> bool {
	System::digest()
		.logs()
		.iter()
		.any(|d| *d == DigestItem::RuntimeEnvironmentUpdated)
}

/// The service does not know the code yet: announce it, leave `:code` untouched, and keep the
/// local pending state armed.
#[test]
fn jam_upgrade_unseen_request_emits_announcement() {
	new_test_ext().execute_with(|| {
		set_para_info(&para_info(&[0xca, 0xfe], None, None));
		let code_before = sp_io::storage::get(CODE);
		PendingCodeHash::<Test>::put((CODE_HASH, CODE_LEN));

		run_blocks(1, 1);

		assert_eq!(sp_io::storage::get(CODE), code_before, ":code must be unchanged");
		assert!(!runtime_environment_updated());
		assert!(PendingCodeHash::<Test>::exists(), "pending must remain armed");
		assert_eq!(
			PendingUpgradeEmit::<Test>::get(),
			Some((CODE_HASH, CODE_LEN, EmitPhase::Announcement))
		);
		assert_eq!(CodeApplied::get(), 0);
	});
}

/// The service announced the code: the switch is immediate, so write the marker and emit an
/// `Apply` for the service; the next block emits nothing.
#[test]
fn jam_upgrade_announced_request_writes_marker_and_emits_apply() {
	new_test_ext().execute_with(|| {
		set_para_info(&para_info(&[0xca, 0xfe], None, Some((CODE_HASH, CODE_LEN))));
		PendingCodeHash::<Test>::put((CODE_HASH, CODE_LEN));

		run_blocks(1, 1);

		let code = sp_io::storage::get(CODE).expect(":code must be set after apply");
		assert_eq!(code, sp_code_marker::encode_marker(&CODE_HASH).to_vec());
		assert!(
			runtime_environment_updated(),
			"RuntimeEnvironmentUpdated digest must be deposited"
		);
		assert!(!PendingCodeHash::<Test>::exists(), "PendingCodeHash must be cleared");
		assert_eq!(
			PendingUpgradeEmit::<Test>::get(),
			Some((CODE_HASH, CODE_LEN, EmitPhase::Apply))
		);
		assert_eq!(CodeApplied::get(), 1);

		run_blocks(2, 2);
		assert!(!PendingUpgradeEmit::<Test>::exists(), "the emission belongs to block 1");
	});
}

/// The service already runs the code: write the marker, clear the pending state, and emit
/// nothing.
#[test]
fn jam_upgrade_active_code_writes_marker_and_applies() {
	new_test_ext().execute_with(|| {
		set_para_info(&para_info(&[0xca, 0xfe], Some((CODE_HASH, CODE_LEN)), None));
		PendingCodeHash::<Test>::put((CODE_HASH, CODE_LEN));

		run_blocks(1, 1);

		let code = sp_io::storage::get(CODE).expect(":code must be set after apply");
		assert_eq!(code, sp_code_marker::encode_marker(&CODE_HASH).to_vec());
		assert!(
			runtime_environment_updated(),
			"RuntimeEnvironmentUpdated digest must be deposited"
		);
		assert!(!PendingCodeHash::<Test>::exists(), "PendingCodeHash must be cleared");
		assert!(!PendingUpgradeEmit::<Test>::exists(), "an already-applied code emits nothing");
		assert_eq!(CodeApplied::get(), 1);
	});
}

#[test]
fn schedule_code_upgrade_hash_records_one_pending_upgrade() {
	let too_big = parachain_service_core::MAX_VALIDATION_CODE_SIZE + 1;
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		assert_noop!(
			JamSystem::schedule_code_upgrade_hash(RuntimeOrigin::signed(1), CODE_HASH, CODE_LEN),
			DispatchError::BadOrigin
		);
		assert_noop!(
			JamSystem::schedule_code_upgrade_hash(RuntimeOrigin::root(), CODE_HASH, too_big),
			Error::<Test>::TooBig
		);

		assert_ok!(JamSystem::schedule_code_upgrade_hash(
			RuntimeOrigin::root(),
			CODE_HASH,
			CODE_LEN
		));
		assert_eq!(PendingCodeHash::<Test>::get(), Some((CODE_HASH, CODE_LEN)));
		System::assert_last_event(Event::<Test>::ValidationFunctionStored.into());

		assert_noop!(
			JamSystem::schedule_code_upgrade_hash(RuntimeOrigin::root(), [0xcd; 32], CODE_LEN),
			Error::<Test>::OverlappingUpgrades
		);
	});
}

#[test]
fn set_code_rejects_code_bytes() {
	new_test_ext().execute_with(|| {
		assert_eq!(
			<JamSetCode<Test> as frame_system::SetCode<Test>>::set_code(vec![1, 2, 3]),
			Err(Error::<Test>::CodeBytesNotAccepted.into())
		);
	});
}

// --- feedback --------------------------------------------------------------------------------

/// The feedback getters read this para's log out of JAM state.
#[test]
fn feedback_reads_this_paras_log() {
	let id = codec::Compact::<u64>;
	let log: ParachainLog = vec![
		(1, LogEntry::Accumulate { entries: vec![(id(3), AccumulateLog::CoreNotAssignable)] }),
		(2, LogEntry::Accumulate { entries: vec![(id(0), AccumulateLog::TooMuchStateHeld)] }),
	];
	let log_key = jam_helpers::service_value_state_key(
		jam_helpers::PARACHAIN_SERVICE_ID,
		&jam_helpers::para_log_key(jam_helpers::types::ParaId::from(PARA_ID)),
	);

	new_test_ext().execute_with(|| {
		set_no_para_info();
		assert!(JamSystem::rejected_cores().is_empty());
		assert_eq!(JamSystem::latest_feedback(), None);

		set_mock_jam_reads([(log_key.to_vec(), log.encode())].into());
		assert_eq!(JamSystem::rejected_cores(), vec![3]);
		assert_eq!(
			JamSystem::latest_feedback(),
			Some(vec![(id(0), AccumulateLog::TooMuchStateHeld)])
		);
	});
}
