// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{
	bundle_decode::decode_bundle,
	state_machine::{
		decode_reported_head, ImportBlocksSink, JamWorkPackageRecovery, PendingReport,
		RecoveredBlock,
	},
	RecoveryDelayRange, RecoveryQueue,
};
use codec::Encode;
use cumulus_jam_interface::{
	BlockDesc, HeaderHash, JamChainSource, JamStateSource, ServiceId, WorkPackageHash, WorkReport,
	WorkReportHash,
};
use cumulus_primitives_core::{ParachainBlockData, SchedulingProof};
use jam_std_common::{build_encoded_bundle, hash_raw};
use jam_types::{
	Authorization, Authorizer, CodeHash, Encode as _, ExtrinsicSpec, RefineContext, WorkDigest,
	WorkItem, WorkOutput, WorkPackage, WorkPayload,
};
use parachain_service_core::{candidate::ParachainCandidate, types::ValidationCodeHash};
use sp_consensus::BlockStatus;
use sp_runtime::{
	testing::{Block, Header as TestHeader, MockCallU64, TestXt},
	traits::Block as BlockT,
};
use sp_trie::CompactProof;
use std::{sync::Arc, time::Duration};

type TestExtrinsic = TestXt<MockCallU64, ()>;
type TestBlock = Block<TestExtrinsic>;
type TestHash = <TestBlock as BlockT>::Hash;

// ── existing baseline tests ────────────────────────────────────────────────────

#[test]
fn delay_range_within_bounds() {
	let range =
		RecoveryDelayRange { min: Duration::from_millis(100), max: Duration::from_millis(500) };
	for _ in 0..100 {
		let d = range.duration();
		assert!(d >= range.min && d <= range.max);
	}
}

#[tokio::test]
async fn queue_push_then_next_returns_hash() {
	let range = RecoveryDelayRange { min: Duration::ZERO, max: Duration::ZERO };
	let mut queue = RecoveryQueue::new(range);
	let hash = WorkReportHash::from([1u8; 32]);
	queue.push_recovery(hash);
	assert_eq!(queue.next_recovery().await, hash);
}

#[tokio::test]
async fn queue_preserves_fifo_order() {
	let range = RecoveryDelayRange { min: Duration::ZERO, max: Duration::ZERO };
	let mut queue = RecoveryQueue::new(range);
	let hash_a = WorkReportHash::from([1u8; 32]);
	let hash_b = WorkReportHash::from([2u8; 32]);
	let hash_c = WorkReportHash::from([3u8; 32]);
	queue.push_recovery(hash_a);
	queue.push_recovery(hash_b);
	queue.push_recovery(hash_c);
	assert_eq!(queue.next_recovery().await, hash_a);
	assert_eq!(queue.next_recovery().await, hash_b);
	assert_eq!(queue.next_recovery().await, hash_c);
}

// ── test infrastructure ────────────────────────────────────────────────────────

struct MockImportSink {
	imported: Arc<std::sync::Mutex<Vec<TestHash>>>,
}

impl ImportBlocksSink<TestBlock> for MockImportSink {
	fn import_blocks(&mut self, blocks: Vec<RecoveredBlock<TestBlock>>) {
		self.imported.lock().unwrap().extend(blocks.into_iter().map(|b| b.hash));
	}
}

/// Stub `JamChainSource` that panics on every call — tests never reach the chain source
/// because they insert pending reports and trigger recovery directly.
struct StubChainSource;

#[async_trait::async_trait]
impl JamChainSource for StubChainSource {
	async fn best_block(&self) -> cumulus_jam_interface::Result<BlockDesc> {
		unimplemented!("stub")
	}
	async fn finalized_block(&self) -> cumulus_jam_interface::Result<BlockDesc> {
		unimplemented!("stub")
	}
	async fn best_block_stream(
		&self,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::BoxStream<'static, BlockDesc>> {
		unimplemented!("stub")
	}
	async fn finalized_block_stream(
		&self,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::BoxStream<'static, BlockDesc>> {
		unimplemented!("stub")
	}
	async fn parent(&self, _: HeaderHash) -> cumulus_jam_interface::Result<BlockDesc> {
		unimplemented!("stub")
	}
	async fn header(&self, _: HeaderHash) -> cumulus_jam_interface::Result<Vec<u8>> {
		unimplemented!("stub")
	}
	async fn state_root(
		&self,
		_: HeaderHash,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::StateRootHash> {
		unimplemented!("stub")
	}
	async fn beefy_root(
		&self,
		_: HeaderHash,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::MmrPeakHash> {
		unimplemented!("stub")
	}
	async fn parameters(
		&self,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::VersionedParameters> {
		unimplemented!("stub")
	}
}

/// Stub `JamStateSource` — same reasoning as `StubChainSource`.
struct StubStateSource;

#[async_trait::async_trait]
impl JamStateSource for StubStateSource {
	async fn state_value(
		&self,
		_: HeaderHash,
		_: cumulus_jam_interface::StorageKey,
	) -> cumulus_jam_interface::Result<Option<Vec<u8>>> {
		unimplemented!("stub")
	}
	async fn state_value_stream(
		&self,
		_: cumulus_jam_interface::StorageKey,
		_: bool,
	) -> cumulus_jam_interface::Result<
		cumulus_jam_interface::BoxStream<
			'static,
			cumulus_jam_interface::ChainSubUpdate<Option<Vec<u8>>>,
		>,
	> {
		unimplemented!("stub")
	}
	async fn state_proof(
		&self,
		_: HeaderHash,
		_: cumulus_jam_interface::StorageKey,
		_: cumulus_jam_interface::StorageKey,
		_: u32,
	) -> cumulus_jam_interface::Result<cumulus_jam_interface::RangeProof> {
		unimplemented!("stub")
	}
	async fn service_value(
		&self,
		_: HeaderHash,
		_: cumulus_jam_interface::ServiceId,
		_: &[u8],
	) -> cumulus_jam_interface::Result<Option<Vec<u8>>> {
		unimplemented!("stub")
	}
	async fn service_value_stream(
		&self,
		_: cumulus_jam_interface::ServiceId,
		_: &[u8],
		_: bool,
	) -> cumulus_jam_interface::Result<
		cumulus_jam_interface::BoxStream<
			'static,
			cumulus_jam_interface::ChainSubUpdate<Option<Vec<u8>>>,
		>,
	> {
		unimplemented!("stub")
	}
}

fn make_test_sm(
	imported: Arc<std::sync::Mutex<Vec<TestHash>>>,
) -> JamWorkPackageRecovery<TestBlock> {
	make_test_sm_with_recorder(imported, Box::new(|_, _| Ok(())))
}

fn make_test_sm_with_recorder(
	imported: Arc<std::sync::Mutex<Vec<TestHash>>>,
	on_recovered: Box<dyn Fn(TestHash, WorkPackageHash) -> Result<(), String> + Send + Sync>,
) -> JamWorkPackageRecovery<TestBlock> {
	JamWorkPackageRecovery::new(
		RecoveryDelayRange { min: Duration::ZERO, max: Duration::ZERO },
		Box::new(MockImportSink { imported }),
		on_recovered,
		Box::new(StubChainSource),
		Box::new(StubStateSource),
		SERVICE_ID,
		PARA_ID,
	)
}

const SERVICE_ID: ServiceId = 7;
const PARA_ID: u32 = 2000;

/// Insert a pending report directly into the state machine — the test equivalent of what
/// `discover_work_reports` does at runtime.
fn insert_pending(sm: &mut JamWorkPackageRecovery<TestBlock>, report_hash: WorkReportHash) {
	sm.outstanding
		.insert(report_hash, PendingReport { assurance_epoch: 0, block_hash: Default::default() });
	sm.recovery_queue.push_recovery(report_hash);
}

/// The refine output the parachain service produces for a block of `para_id` with `head`:
/// `ParachainWorkDigest::Ok { para_id, validation_code, parent_head_hash, head_data, .. }`.
fn refine_output(para_id: u32, head: TestHash) -> Vec<u8> {
	// Upward messages and the lookup-anchor slot follow the head; the decoder ignores them.
	Encode::encode(&(0u8, para_id, [1u8; 32], [2u8; 32], head.encode(), Vec::<u8>::new(), 5u32))
}

/// A report as JAM availability holds it, refined by `service` with `output`.
fn report_with(byte: u8, service: ServiceId, output: Vec<u8>) -> WorkReport {
	let digest = WorkDigest {
		service,
		code_hash: Default::default(),
		payload_hash: Default::default(),
		accumulate_gas: 0,
		result: Ok(WorkOutput(output)),
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
		context: make_refine_context(),
		core_index: 0,
		authorizer_hash: Default::default(),
		auth_gas_used: 0,
		auth_output: Default::default(),
		sr_lookup: Default::default(),
		results: vec![digest].try_into().expect("a single digest always fits; qed"),
	}
}

/// Collect the `(block_hash, work_package_hash)` pairs the recovery engine reports.
fn hash_recorder(
	recorded: Arc<std::sync::Mutex<Vec<(TestHash, WorkPackageHash)>>>,
) -> Box<dyn Fn(TestHash, WorkPackageHash) -> Result<(), String> + Send + Sync> {
	Box::new(move |block_hash, wp_hash| {
		recorded.lock().expect("mutex not poisoned; qed").push((block_hash, wp_hash));
		Ok(())
	})
}

fn make_refine_context() -> RefineContext {
	RefineContext {
		anchor: Default::default(),
		anchor_slot: 0,
		state_root: Default::default(),
		beefy_root: Default::default(),
		lookup_anchor: Default::default(),
		lookup_anchor_slot: 0,
		lookup_anchor_state_root: Default::default(),
		prerequisites: Default::default(),
	}
}

/// Build the PoV the collator produces: a SCALE-encoded `ParachainBlockData::V4` carrying the
/// parent header and the per-block additional data.
fn make_pov(
	blocks: Vec<TestBlock>,
	additional_data: Vec<Option<sp_additional_data::AdditionalData>>,
	parent_header: TestHeader,
) -> Vec<u8> {
	ParachainBlockData::new_with_parent_header(
		blocks,
		CompactProof { encoded_nodes: vec![] },
		SchedulingProof::empty(),
		additional_data,
		parent_header.encode(),
		Vec::new(),
	)
	.encode()
}

/// Assemble a work package whose first item carries `extrinsics`, bundle `extrinsic_data` after
/// it, and hand back both the bundle bytes and the package.
///
/// The bundle is `package ‖ extrinsic data ‖ import segments ‖ import proofs`
/// (`jam_std_common::build_encoded_bundle`). The payload only carries the candidate's
/// validation-code hash; the PoV travels as work-item extrinsic 0, exactly as the collator sends
/// it.
fn build_bundle(extrinsics: Vec<ExtrinsicSpec>, extrinsic_data: &[u8]) -> (Vec<u8>, WorkPackage) {
	let payload = ParachainCandidate { validation_code: ValidationCodeHash([0u8; 32]) }.encode();

	let work_item = WorkItem {
		service: 0,
		code_hash: CodeHash::zero(),
		refine_gas_limit: 0,
		accumulate_gas_limit: 0,
		export_count: 0,
		payload: WorkPayload(payload),
		import_segments: Default::default(),
		extrinsics: extrinsics.try_into().expect("extrinsic specs always fit"),
	};

	let package = WorkPackage {
		authorization: Authorization::default(),
		auth_code_host: 0,
		authorizer: Authorizer::any(),
		context: make_refine_context(),
		items: vec![work_item].try_into().expect("one item always fits"),
	};

	let (_, bundle) = build_encoded_bundle(&package, [extrinsic_data], &[vec![]]);
	(bundle, package)
}

/// Build SCALE-encoded bundle bytes from blocks with a spec matching the PoV.
///
/// Returns the package alongside its encoding so a test can derive the expected hash from the
/// same bytes the recovery engine sees.
fn make_bundle_and_package(
	blocks: Vec<TestBlock>,
	additional_data: Vec<Option<sp_additional_data::AdditionalData>>,
	parent_header: TestHeader,
) -> (Vec<u8>, WorkPackage) {
	let pov = make_pov(blocks, additional_data, parent_header);
	let spec = ExtrinsicSpec { hash: hash_raw(&pov).into(), len: pov.len() as u32 };
	build_bundle(vec![spec], &pov)
}

/// Bundle bytes only; [`make_bundle_and_package`] also hands back the package for hashing.
fn make_bundle_bytes(
	blocks: Vec<TestBlock>,
	additional_data: Vec<Option<sp_additional_data::AdditionalData>>,
	parent_header: TestHeader,
) -> Vec<u8> {
	make_bundle_and_package(blocks, additional_data, parent_header).0
}

// ── decode-chain tests (1-4) ───────────────────────────────────────────────────

/// Round-trip: encode a block the way the collator does and assert it comes back.
/// This is the wire-contract proof — if this passes, the decode chain is correct.
#[test]
fn bundle_round_trip_decodes_correct_blocks() {
	let parent_header = TestHeader::new_from_number(0);
	let block = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let bundle = make_bundle_bytes(vec![block.clone()], vec![None], parent_header);
	let decoded = decode_bundle::<TestBlock>(&bundle).expect("round-trip must succeed");
	assert_eq!(decoded.len(), 1);
	assert_eq!(decoded[0].0, block);
}

/// An empty `items` list produces an error, not a panic.
#[test]
fn empty_items_returns_error_no_panic() {
	let package = WorkPackage {
		authorization: Authorization::default(),
		auth_code_host: 0,
		authorizer: Authorizer::any(),
		context: make_refine_context(),
		items: Default::default(),
	};
	let result = decode_bundle::<TestBlock>(&package.encode());
	assert!(result.is_err(), "empty items must be an error, not a panic");
}

/// Garbage/truncated bytes at every decode stage produce an error, not a panic.
#[test]
fn truncated_bytes_returns_error_no_panic() {
	for garbage in [&b""[..], &[0xff_u8; 10], &[0xde, 0xad, 0xbe, 0xef]] {
		assert!(decode_bundle::<TestBlock>(garbage).is_err(), "garbage must be an error");
	}
}

/// Multi-block `ParachainBlockData` → all blocks recovered in submission order.
#[test]
fn multi_block_pov_recovers_all_in_order() {
	let parent_header = TestHeader::new_from_number(0);
	let block_a = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let block_b = TestBlock { header: TestHeader::new_from_number(2), extrinsics: vec![] };
	let bundle =
		make_bundle_bytes(vec![block_a.clone(), block_b.clone()], vec![None, None], parent_header);
	let decoded = decode_bundle::<TestBlock>(&bundle).expect("multi-block decode");
	assert_eq!(decoded.len(), 2);
	assert_eq!(decoded[0].0, block_a);
	assert_eq!(decoded[1].0, block_b);
}

/// A work item with no extrinsic spec yields an error, not a panic.
#[test]
fn missing_extrinsic_spec_returns_error() {
	let (bundle, _) = build_bundle(Vec::new(), &[]);
	assert!(decode_bundle::<TestBlock>(&bundle).is_err(), "no extrinsic spec must be an error");
}

/// Extrinsic bytes that do not hash to their spec are rejected.
#[test]
fn extrinsic_hash_mismatch_returns_error() {
	let pov = make_pov(
		vec![TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] }],
		vec![None],
		TestHeader::new_from_number(0),
	);
	let wrong_spec = ExtrinsicSpec { hash: [0xff; 32].into(), len: pov.len() as u32 };
	let (bundle, _) = build_bundle(vec![wrong_spec], &pov);
	assert!(decode_bundle::<TestBlock>(&bundle).is_err(), "hash mismatch must be an error");
}

/// A bundle that ends before its declared extrinsic length is rejected, not panicked on.
#[test]
fn short_bundle_returns_error() {
	let parent_header = TestHeader::new_from_number(0);
	let block = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let (bundle, _) = make_bundle_and_package(vec![block], vec![None], parent_header);
	let truncated = &bundle[..bundle.len() - 1];
	assert!(decode_bundle::<TestBlock>(truncated).is_err(), "short bundle must be an error");
}

// ── state-machine tests (5-8) ──────────────────────────────────────────────────

/// Recovery success with known parent → block forwarded to the import sink.
#[test]
fn recovery_success_imports_block() {
	let parent_header = TestHeader::new_from_number(0);
	let block = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let bundle = make_bundle_bytes(vec![block.clone()], vec![None], parent_header);

	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm(imported.clone());
	let hash = WorkReportHash::from([1u8; 32]);

	insert_pending(&mut sm, hash);
	sm.handle_recovered_inner(hash, Ok(Some(bundle)), |_| BlockStatus::InChainWithState);

	let guard = imported.lock().unwrap();
	assert_eq!(guard.len(), 1, "exactly one block must be imported");
	assert_eq!(guard[0], block.hash());
}

/// A `None` result re-queues the candidate once; a second `None` drops it permanently.
#[test]
fn recovery_failure_retries_once_then_drops() {
	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm(imported.clone());
	let hash = WorkReportHash::from([2u8; 32]);

	insert_pending(&mut sm, hash);

	// First failure: enters retry set, stays in outstanding.
	sm.handle_recovered_inner(hash, Ok(None), |_| BlockStatus::Unknown);
	assert!(sm.reports_in_retry.contains(&hash), "must enter retry after first failure");
	assert!(sm.outstanding.contains_key(&hash), "must stay in outstanding during retry");

	// Second failure: dropped entirely.
	sm.handle_recovered_inner(hash, Ok(None), |_| BlockStatus::Unknown);
	assert!(!sm.reports_in_retry.contains(&hash), "removed from retry after second failure");
	assert!(!sm.outstanding.contains_key(&hash), "removed from outstanding after drop");
	assert!(imported.lock().unwrap().is_empty(), "nothing imported after drop");
}

/// Child waits in `waiting_for_parent`; imports as soon as the parent block arrives.
#[test]
fn parent_ordering_child_waits_then_imports_when_parent_arrives() {
	let parent_header = TestHeader::new_from_number(0);
	let parent_hash = parent_header.hash();

	// Construct a child block that explicitly names parent_header as its parent.
	let mut child_header = TestHeader::new_from_number(1);
	child_header.parent_hash = parent_hash;
	let block = TestBlock { header: child_header, extrinsics: vec![] };
	let bundle = make_bundle_bytes(vec![block.clone()], vec![None], parent_header);

	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm(imported.clone());
	let hash = WorkReportHash::from([3u8; 32]);

	insert_pending(&mut sm, hash);
	// Parent unknown → deferred.
	sm.handle_recovered_inner(hash, Ok(Some(bundle)), |_| BlockStatus::Unknown);
	assert!(
		sm.waiting_for_parent.contains_key(&parent_hash),
		"block must be deferred to waiting_for_parent"
	);
	assert!(imported.lock().unwrap().is_empty(), "nothing imported before parent arrives");

	// Parent arrives → block imported.
	sm.handle_imported(parent_hash);
	let guard = imported.lock().unwrap();
	assert_eq!(guard.len(), 1, "block must be imported after parent arrives");
	assert_eq!(guard[0], block.hash());
}

/// A refine output of our para yields its id and head; one that is not an `Ok` digest does not.
#[test]
fn reported_head_decodes_the_parachain_digest() {
	let head = TestHash::repeat_byte(0xab);
	assert_eq!(
		decode_reported_head::<TestHash>(&refine_output(PARA_ID, head)),
		Some((PARA_ID, head))
	);

	let mut err_digest = refine_output(PARA_ID, head);
	err_digest[0] = 1;
	assert_eq!(decode_reported_head::<TestHash>(&err_digest), None);
	assert_eq!(decode_reported_head::<TestHash>(&[0u8; 10]), None);
}

/// Only a report of our para whose block is unknown is scheduled, keyed by the report's own
/// hash — the key JAM's `recoverBundle` looks the report up by.
#[test]
fn observe_report_schedules_only_unknown_blocks_of_our_para() {
	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm(imported);
	let unknown = TestHash::repeat_byte(1);
	let known = TestHash::repeat_byte(2);
	let status = |hash: TestHash| {
		if hash == known {
			BlockStatus::InChainWithState
		} else {
			BlockStatus::Unknown
		}
	};

	let ours = report_with(1, SERVICE_ID, refine_output(PARA_ID, unknown));
	let already_known = report_with(2, SERVICE_ID, refine_output(PARA_ID, known));
	let other_para = report_with(3, SERVICE_ID, refine_output(PARA_ID + 1, unknown));
	let other_service = report_with(4, SERVICE_ID + 1, refine_output(PARA_ID, unknown));
	for report in [&ours, &already_known, &other_para, &other_service] {
		sm.observe_report(report, 3, &status);
	}
	// Seen again in the next JAM block: not scheduled twice.
	sm.observe_report(&ours, 3, &status);

	assert_eq!(sm.outstanding.len(), 1, "only our unknown block is scheduled");
	let pending = sm.outstanding.get(&ours.hash()).expect("keyed by the report hash");
	assert_eq!(pending.block_hash, unknown);
	assert_eq!(pending.assurance_epoch, 3);
}

// ── recovered-hash recording tests (9-10) ──────────────────────────────────────

/// A recovered bundle holds the author's own signed package, so its hash is recorded under the
/// hash of the block it carries. The expected hash is recomputed here from the same package
/// bytes with blake2-256 — the function the author used — not read back from the engine.
#[test]
fn recovery_records_hash_in_ledger() {
	let parent_header = TestHeader::new_from_number(0);
	let block = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let (bundle, package) = make_bundle_and_package(vec![block.clone()], vec![None], parent_header);
	let expected_wp_hash = WorkPackageHash::from(sp_crypto_hashing::blake2_256(&package.encode()));

	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let recorded = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm_with_recorder(imported, hash_recorder(recorded.clone()));
	let hash = WorkReportHash::from([6u8; 32]);

	insert_pending(&mut sm, hash);
	sm.handle_recovered_inner(hash, Ok(Some(bundle)), |_| BlockStatus::InChainWithState);

	let guard = recorded.lock().expect("mutex not poisoned; qed");
	assert_eq!(guard.len(), 1, "one recovered block must yield exactly one ledger entry");
	assert_eq!(guard[0], (block.hash(), expected_wp_hash));
}

/// No package bytes means no ledger entry: neither a bundle the DA layer has not produced yet
/// nor an undecodable one may leave a phantom hash behind.
#[test]
fn recovery_without_package_bytes_records_nothing() {
	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let recorded = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm_with_recorder(imported, hash_recorder(recorded.clone()));

	let missing = WorkReportHash::from([7u8; 32]);
	insert_pending(&mut sm, missing);
	// Not in the DA layer yet: one retry, then dropped.
	sm.handle_recovered_inner(missing, Ok(None), |_| BlockStatus::Unknown);
	sm.handle_recovered_inner(missing, Ok(None), |_| BlockStatus::Unknown);

	let garbage = WorkReportHash::from([8u8; 32]);
	insert_pending(&mut sm, garbage);
	sm.handle_recovered_inner(garbage, Ok(Some(vec![0xffu8; 10])), |_| BlockStatus::Unknown);

	assert!(
		recorded.lock().expect("mutex not poisoned; qed").is_empty(),
		"a recovery without package bytes must not record anything"
	);
}

/// A failed ledger write must not abort recovery: the engine reports the failure, still imports
/// the recovered block, and keeps running.
#[test]
fn recovery_continues_when_recording_fails() {
	let parent_header = TestHeader::new_from_number(0);
	let block = TestBlock { header: TestHeader::new_from_number(1), extrinsics: vec![] };
	let bundle = make_bundle_bytes(vec![block.clone()], vec![None], parent_header);

	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let attempts = Arc::new(std::sync::Mutex::new(0usize));
	let attempts_for_recorder = attempts.clone();
	let mut sm = make_test_sm_with_recorder(
		imported.clone(),
		Box::new(move |_, _| {
			*attempts_for_recorder.lock().expect("mutex not poisoned; qed") += 1;
			Err("ledger unavailable".to_string())
		}),
	);
	let hash = WorkReportHash::from([9u8; 32]);

	insert_pending(&mut sm, hash);
	sm.handle_recovered_inner(hash, Ok(Some(bundle)), |_| BlockStatus::InChainWithState);

	assert_eq!(
		*attempts.lock().expect("mutex not poisoned; qed"),
		1,
		"the engine must attempt the ledger write"
	);
	let guard = imported.lock().expect("mutex not poisoned; qed");
	assert_eq!(guard.len(), 1, "the block must be imported despite the failed write");
	assert_eq!(guard[0], block.hash());
}

/// When a parent block is imported from outside the recovery engine (e.g. by the node's import
/// pipeline), `handle_imported` drains every child that was waiting for that parent and imports
/// them in sequence. This is the JAM equivalent of pov-recovery's import-while-recovering
/// scenario: an unrelated import unblocks a pending recovery chain.
#[test]
fn import_drains_waiting_children() {
	let parent_header = TestHeader::new_from_number(0);
	let parent_hash = parent_header.hash();

	let mut child_header = TestHeader::new_from_number(1);
	child_header.parent_hash = parent_hash;
	let child = TestBlock { header: child_header, extrinsics: vec![] };
	let bundle = make_bundle_bytes(vec![child.clone()], vec![None], parent_header);

	let imported = Arc::new(std::sync::Mutex::new(vec![]));
	let mut sm = make_test_sm(imported.clone());

	let hash = WorkReportHash::from([10u8; 32]);
	insert_pending(&mut sm, hash);

	// Recovery returns a block whose parent is unknown — deferred.
	sm.handle_recovered_inner(hash, Ok(Some(bundle)), |_| BlockStatus::Unknown);
	assert!(sm.waiting_for_parent.contains_key(&parent_hash), "child must wait for unknown parent");
	assert_eq!(imported.lock().expect("mutex not poisoned; qed").len(), 0, "nothing imported yet");

	// Parent arrives via import notification — drains the waiting child.
	sm.handle_imported(parent_hash);

	let guard = imported.lock().expect("mutex not poisoned; qed");
	assert_eq!(guard.len(), 1, "deferred child must be imported after parent arrives");
	assert_eq!(guard[0], child.hash(), "imported block must be the deferred child");
	assert!(!sm.waiting_for_parent.contains_key(&parent_hash), "waiting entry must be drained");
}
