// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for [`cumulus_client_storage_chain_sync::StorageChainBlockImport`].
//!
//! These tests drive the wrapper through its public [`BlockImport::import_block`] surface
//! against a hand-rolled mock client/runtime API and a recording inner `BlockImport` that
//! captures the `BlockImportParams` it receives.
//!
//! Scope: case A (incoming `StorageChanges`) and the `should_intercept` short-circuits.
//! Case B requires a runtime API instance with executable overlay support and is covered by
//! crate-level checks rather than this mock-only harness.

use mock::{case_a_params, make_harness, params_with_origin, prefetched_attached, renew_op};
use rstest::rstest;
use sc_consensus::{BlockImport, ImportResult};
use sp_consensus::BlockOrigin;
use sp_runtime::OpaqueExtrinsic;
use sp_transaction_storage_proof::{ContentHash, HashingAlgorithm, IndexedTransactionInfo};

#[rstest]
#[case::warp_sync(BlockOrigin::WarpSync, None)]
#[case::gap_sync(BlockOrigin::GapSync, Some(Vec::new()))]
#[case::body_none(BlockOrigin::NetworkBroadcast, None)]
#[tokio::test]
async fn import_passes_through(
	#[case] origin: BlockOrigin,
	#[case] body: Option<Vec<OpaqueExtrinsic>>,
) {
	let h = make_harness();
	let params = params_with_origin(origin, 1, body);
	let result = h.wrapper.import_block(params).await.expect("import_block");
	assert!(matches!(result, ImportResult::Imported(_)));
	let captured = h.captured.lock().unwrap();
	assert_eq!(captured.len(), 1);
	assert!(!prefetched_attached(&captured[0]));
}

fn info(
	content_hash: ContentHash,
	size: u32,
	hashing: HashingAlgorithm,
	extrinsic_index: u32,
) -> IndexedTransactionInfo {
	IndexedTransactionInfo {
		content_hash,
		size,
		hashing,
		cid_codec: sc_network::bitswap::RAW_CODEC,
		extrinsic_index,
	}
}

#[tokio::test]
async fn import_case_a_no_renews_attaches_nothing() {
	let h = make_harness();
	let params = case_a_params(1, Vec::new());
	let result = h.wrapper.import_block(params).await.expect("import_block");
	assert!(matches!(result, ImportResult::Imported(_)));
	let captured = h.captured.lock().unwrap();
	assert_eq!(captured.len(), 1);
	assert!(!prefetched_attached(&captured[0]));
}

#[rstest]
#[case::blake2b(
	b"renew-blob-payload".as_slice(),
	sp_transaction_storage_proof::HashingAlgorithm::Blake2b256,
)]
#[case::sha2(
	b"sha2-renew-blob-payload".as_slice(),
	sp_transaction_storage_proof::HashingAlgorithm::Sha2_256,
)]
#[case::keccak(
	b"keccak-renew-blob-payload".as_slice(),
	sp_transaction_storage_proof::HashingAlgorithm::Keccak256,
)]
#[tokio::test]
async fn import_case_a_attaches_prefetched(
	#[case] bytes: &[u8],
	#[case] algorithm: sp_transaction_storage_proof::HashingAlgorithm,
) {
	let h = make_harness();
	let content_hash: ContentHash = algorithm.hash(bytes);
	h.api
		.set_indexed(1, vec![info(content_hash, bytes.len() as u32, algorithm, u32::MAX)]);
	h.network.insert(content_hash, bytes.to_vec());

	let params = case_a_params(1, vec![renew_op(content_hash, 0)]);
	let result = h.wrapper.import_block(params).await.expect("import_block");
	assert!(matches!(result, ImportResult::Imported(_)));

	let captured = h.captured.lock().unwrap();
	assert_eq!(captured.len(), 1);
	let payload = &captured[0].prefetched_indexed_transactions;
	assert_eq!(payload.len(), 1);
	assert_eq!(payload[0].0, content_hash);
	assert_eq!(payload[0].1, bytes);
	assert_eq!(h.api.call_api_at_count(), 1);
	assert!(h.api.overlay_marker_seen());
}

#[tokio::test]
async fn import_case_a_skips_already_present_hash() {
	let h = make_harness();
	let bytes = b"already-on-disk".to_vec();
	let content_hash: ContentHash = HashingAlgorithm::Blake2b256.hash(&bytes);
	h.api.set_indexed(
		1,
		vec![info(content_hash, bytes.len() as u32, HashingAlgorithm::Blake2b256, u32::MAX)],
	);
	h.api.insert_indexed_transaction(content_hash, bytes);

	let params = case_a_params(1, vec![renew_op(content_hash, 0)]);
	let result = h.wrapper.import_block(params).await.expect("import_block");
	assert!(matches!(result, ImportResult::Imported(_)));

	let captured = h.captured.lock().unwrap();
	assert_eq!(captured.len(), 1);
	assert!(!prefetched_attached(&captured[0]));
}

#[tokio::test]
async fn import_case_a_errors_when_fetcher_partial() {
	let h = make_harness();
	let content_hash: ContentHash = [0x33u8; 32];
	h.api
		.set_indexed(1, vec![info(content_hash, 32, HashingAlgorithm::Blake2b256, u32::MAX)]);
	let params = case_a_params(1, vec![renew_op(content_hash, 0)]);
	let err = h
		.wrapper
		.import_block(params)
		.await
		.expect_err("fetcher should yield zero bytes and the wrapper should error");
	let msg = format!("{err}");
	assert!(msg.contains("bitswap fetch"), "unexpected error message: {msg}",);
	assert!(h.captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn import_case_b_executes_once_and_indexes_on_same_overlay() {
	let bytes = b"case-b-renew-blob".to_vec();
	let h = mock::make_case_b_harness(bytes.clone());
	let params = mock::case_b_params(1);
	let result = h.wrapper.import_block(params).await.expect("import_block");
	assert!(matches!(result, ImportResult::Imported(_)));

	assert_eq!(h.api.execute_block_count(), 1);
	assert_eq!(h.api.indexed_transactions_count(), 1);
	assert!(h.api.overlay_marker_seen_by_indexed_transactions());

	let captured = h.captured.lock().unwrap();
	assert_eq!(captured.len(), 1);
	let changes = captured[0]
		.state_action
		.as_storage_changes()
		.expect("Case B forwards generated storage changes");
	assert_eq!(changes.transaction_index_changes.len(), 1);
	let sp_state_machine::IndexOperation::Renew { extrinsic, hash } =
		&changes.transaction_index_changes[0]
	else {
		panic!("expected Case B renew operation");
	};
	assert_eq!((*extrinsic, hash.as_slice()), (0, h.content_hash.as_slice()));
	assert!(
		changes
			.main_storage_changes
			.iter()
			.any(|(key, value)| key == mock::CASE_B_MARKER_KEY &&
				value.as_deref() == Some(mock::CASE_B_MARKER_VALUE)),
		"execute_block overlay marker must be forwarded",
	);
	assert!(
		!changes
			.main_storage_changes
			.iter()
			.any(|(key, _)| key == mock::CASE_B_ROLLBACK_MARKER_KEY),
		"indexed_transactions rollback marker must not leak into forwarded changes",
	);

	let payload = &captured[0].prefetched_indexed_transactions;
	assert_eq!(payload, &vec![(h.content_hash, bytes)]);
}

mod mock {
	use async_trait::async_trait;
	use cid::{Cid, Version as CidVersion};
	use codec::{Decode, Encode};
	use cumulus_client_storage_chain_sync::{
		BitswapPeerSource, IndexedTransactionFetcher, NetworkHandle, StorageChainBlockImport,
		SyncingHandle,
	};
	use futures::channel::oneshot;

	use sc_consensus::{
		BlockCheckParams, BlockImport, BlockImportParams, ImportResult, ImportedAux, StateAction,
		StorageChanges as ConsensusStorageChanges,
	};
	use sc_network::{
		bitswap::{test_helpers::schema as bitswap_schema, RAW_CODEC},
		request_responses::{IfDisconnected, RequestFailure},
		types::ProtocolName,
		NetworkRequest, PeerId,
	};
	use sp_api::{mock_impl_runtime_apis, ApiError, ConstructRuntimeApi};
	use sp_consensus::{BlockOrigin, Error as ConsensusError};
	use sp_core::H256;
	use sp_runtime::{
		generic,
		traits::{BlakeTwo256, Block as BlockT, Header as _},
		Digest, Justifications, OpaqueExtrinsic,
	};
	use sp_state_machine::{InMemoryBackend, IndexOperation, OverlayedChanges, StorageChanges};
	use sp_transaction_storage_proof::{
		runtime_api::TransactionStorageApi, ContentHash, IndexedTransactionInfo,
	};
	use std::{
		collections::HashMap,
		sync::{Arc, Mutex, OnceLock},
	};

	pub(super) type TestBlock = generic::Block<generic::Header<u32, BlakeTwo256>, OpaqueExtrinsic>;
	type TestHeader = generic::Header<u32, BlakeTwo256>;
	type Block = TestBlock;

	const OVERLAY_MARKER_KEY: &[u8] = b"storage-chain-sync-overlay-marker";
	const OVERLAY_MARKER_VALUE: &[u8] = b"visible";
	pub(super) const CASE_B_MARKER_KEY: &[u8] = b"storage-chain-sync-case-b-marker";
	pub(super) const CASE_B_MARKER_VALUE: &[u8] = b"visible-after-execute-block";
	pub(super) const CASE_B_ROLLBACK_MARKER_KEY: &[u8] = b"storage-chain-sync-case-b-rollback";

	#[derive(Default, Clone)]
	struct MockApiInner {
		indexed_at_block_number: HashMap<u32, Vec<IndexedTransactionInfo>>,
		indexed_transactions: HashMap<H256, Vec<u8>>,
		call_api_at_count: usize,
		overlay_marker_seen: bool,
	}

	#[derive(Clone)]
	pub(super) struct MockApiClient {
		inner: Arc<Mutex<MockApiInner>>,
	}

	impl Default for MockApiClient {
		fn default() -> Self {
			Self { inner: Arc::new(Mutex::new(MockApiInner::default())) }
		}
	}

	impl MockApiClient {
		pub(super) fn set_indexed(&self, block_number: u32, infos: Vec<IndexedTransactionInfo>) {
			self.inner.lock().unwrap().indexed_at_block_number.insert(block_number, infos);
		}

		pub(super) fn insert_indexed_transaction(&self, hash: ContentHash, data: Vec<u8>) {
			self.inner.lock().unwrap().indexed_transactions.insert(H256::from(hash), data);
		}

		pub(super) fn call_api_at_count(&self) -> usize {
			self.inner.lock().unwrap().call_api_at_count
		}

		pub(super) fn overlay_marker_seen(&self) -> bool {
			self.inner.lock().unwrap().overlay_marker_seen
		}
	}

	mock_impl_runtime_apis! {
		impl TransactionStorageApi<Block> for MockApiClient {
			fn retention_period(&self) -> u32 {
				0
			}

			fn indexed_transactions(&self, block: u32) -> Vec<IndexedTransactionInfo> {
				self.inner
					.lock()
					.unwrap()
					.indexed_at_block_number
					.get(&block)
					.cloned()
					.unwrap_or_default()
			}
		}
	}

	impl sp_api::ProvideRuntimeApi<TestBlock> for MockApiClient {
		type Api = MockApiClient;
		fn runtime_api(&self) -> sp_api::ApiRef<'_, Self::Api> {
			self.clone().into()
		}
	}

	impl sp_api::CallApiAt<TestBlock> for MockApiClient {
		type StateBackend =
			sp_state_machine::InMemoryBackend<sp_runtime::traits::HashingFor<TestBlock>>;

		fn call_api_at(
			&self,
			params: sp_api::CallApiAtParams<TestBlock>,
		) -> Result<Vec<u8>, ApiError> {
			assert_eq!(
				params.function, "TransactionStorageApi_indexed_transactions",
				"unexpected runtime API function",
			);
			let (block_number,): (u32,) = Decode::decode(&mut &params.arguments[..])
				.expect("encoded indexed_transactions argument must decode");
			let overlay_marker_seen = matches!(
				params.overlayed_changes.borrow_mut().storage(OVERLAY_MARKER_KEY),
				Some(Some(value)) if value == OVERLAY_MARKER_VALUE
			);
			let mut inner = self.inner.lock().unwrap();
			inner.call_api_at_count += 1;
			inner.overlay_marker_seen |= overlay_marker_seen;
			Ok(inner
				.indexed_at_block_number
				.get(&block_number)
				.cloned()
				.unwrap_or_default()
				.encode())
		}

		fn runtime_version_at(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
			_call_context: sp_core::traits::CallContext,
		) -> Result<sp_version::RuntimeVersion, ApiError> {
			unreachable!("not used by the wrapper in these tests")
		}

		fn state_at(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
		) -> Result<Self::StateBackend, ApiError> {
			unreachable!("only the case B execute path queries this; case B is out of scope")
		}

		fn initialize_extensions(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
			_extensions: &mut sp_externalities::Extensions,
		) -> Result<(), ApiError> {
			Ok(())
		}
	}

	impl sc_client_api::BlockBackend<TestBlock> for MockApiClient {
		fn block_body(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<<TestBlock as BlockT>::Extrinsic>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_indexed_body(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<Vec<u8>>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_indexed_hashes(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<H256>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<generic::SignedBlock<TestBlock>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_status(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<sp_consensus::BlockStatus> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn justifications(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Justifications>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_hash(
			&self,
			_number: sp_runtime::traits::NumberFor<TestBlock>,
		) -> sp_blockchain::Result<Option<<TestBlock as BlockT>::Hash>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<Option<Vec<u8>>> {
			Ok(self.inner.lock().unwrap().indexed_transactions.get(&hash).cloned())
		}

		fn has_indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<bool> {
			Ok(self.inner.lock().unwrap().indexed_transactions.contains_key(&hash))
		}

		fn requires_full_sync(&self) -> bool {
			false
		}
	}

	pub(super) struct TestInner {
		captured: Arc<Mutex<Vec<BlockImportParams<TestBlock>>>>,
	}

	impl TestInner {
		fn recording() -> Self {
			Self { captured: Arc::new(Mutex::new(Vec::new())) }
		}
	}

	#[async_trait]
	impl BlockImport<TestBlock> for TestInner {
		type Error = ConsensusError;

		async fn check_block(
			&self,
			_block: BlockCheckParams<TestBlock>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::imported(true))
		}

		async fn import_block(
			&self,
			block: BlockImportParams<TestBlock>,
		) -> Result<ImportResult, Self::Error> {
			self.captured.lock().unwrap().push(block);
			Ok(ImportResult::Imported(ImportedAux::default()))
		}
	}

	#[derive(Default)]
	pub(super) struct MockNetworkRequest {
		responses: Mutex<HashMap<ContentHash, Vec<u8>>>,
	}

	impl MockNetworkRequest {
		pub(super) fn insert(&self, hash: ContentHash, data: Vec<u8>) {
			self.responses.lock().unwrap().insert(hash, data);
		}
	}

	#[async_trait]
	impl NetworkRequest for MockNetworkRequest {
		async fn request(
			&self,
			_target: PeerId,
			_protocol: ProtocolName,
			request: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			_connect: IfDisconnected,
		) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
			use prost::Message as _;
			let message = bitswap_schema::Message::decode(&*request)
				.expect("MockNetworkRequest received malformed bitswap request");
			let responses = self.responses.lock().unwrap();
			let mut payload = Vec::new();
			let mut block_presences = Vec::new();
			for entry in message.wantlist.unwrap_or_default().entries {
				let Ok(cid) = Cid::read_bytes(entry.block.as_slice()) else { continue };
				let digest: Option<ContentHash> = cid.hash().digest().try_into().ok();
				match digest.and_then(|d| responses.get(&d).cloned()) {
					Some(data) => payload.push(bitswap_schema::message::Block {
						prefix: raw_prefix_for(&cid),
						data,
					}),
					None => block_presences.push(bitswap_schema::message::BlockPresence {
						cid: entry.block,
						r#type: bitswap_schema::message::BlockPresenceType::DontHave as i32,
					}),
				}
			}
			let response =
				bitswap_schema::Message { payload, block_presences, ..Default::default() };
			Ok((response.encode_to_vec(), ProtocolName::from("/ipfs/bitswap/1.2.0")))
		}

		fn start_request(
			&self,
			_target: PeerId,
			_protocol: ProtocolName,
			_request: Vec<u8>,
			_fallback_request: Option<(Vec<u8>, ProtocolName)>,
			_tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
			_connect: IfDisconnected,
		) {
			unreachable!("the bitswap client uses async request(), never start_request()")
		}
	}

	fn raw_prefix_for(cid: &Cid) -> Vec<u8> {
		sc_network::bitswap::test_helpers::Prefix {
			version: CidVersion::V1,
			codec: RAW_CODEC,
			mh_type: cid.hash().code(),
			mh_len: 32,
		}
		.to_bytes()
	}

	struct MockBitswapPeerSource {
		peers: Vec<PeerId>,
	}

	#[async_trait]
	impl BitswapPeerSource for MockBitswapPeerSource {
		async fn current_peers(&self) -> Result<Vec<PeerId>, oneshot::Canceled> {
			Ok(self.peers.clone())
		}
	}

	#[allow(dead_code)]
	struct Runtime;

	sp_api::impl_runtime_apis! {
		impl sp_api::Core<Block> for Runtime {
			fn version() -> sp_version::RuntimeVersion {
				case_b_runtime_version()
			}

			fn execute_block(_block: <Block as BlockT>::LazyBlock) {}

			fn initialize_block(_header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
				sp_runtime::ExtrinsicInclusionMode::AllExtrinsics
			}
		}

		impl sp_transaction_storage_proof::runtime_api::TransactionStorageApi<Block> for Runtime {
			fn retention_period() -> u32 {
				0
			}

			fn indexed_transactions(_block: u32) -> Vec<IndexedTransactionInfo> {
				Vec::new()
			}
		}
	}

	fn case_b_runtime_version() -> sp_version::RuntimeVersion {
		sp_version::RuntimeVersion {
			spec_name: "storage-chain-sync-test".into(),
			impl_name: "storage-chain-sync-test".into(),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: RUNTIME_API_VERSIONS,
			transaction_version: 1,
			system_version: 1,
		}
	}

	#[derive(Default)]
	struct CaseBInner {
		indexed_transactions: HashMap<H256, Vec<u8>>,
		execute_block_count: usize,
		indexed_transactions_count: usize,
		overlay_marker_seen_by_indexed_transactions: bool,
	}

	pub(super) struct CaseBClient {
		inner: Arc<Mutex<CaseBInner>>,
		content_hash: ContentHash,
		info: IndexedTransactionInfo,
	}

	impl CaseBClient {
		fn new(content_hash: ContentHash, data: Vec<u8>) -> Self {
			let info = IndexedTransactionInfo {
				content_hash,
				size: data.len() as u32,
				hashing: sp_transaction_storage_proof::HashingAlgorithm::Blake2b256,
				cid_codec: RAW_CODEC,
				extrinsic_index: u32::MAX,
			};
			Self { inner: Arc::new(Mutex::new(CaseBInner::default())), content_hash, info }
		}

		pub(super) fn execute_block_count(&self) -> usize {
			self.inner.lock().unwrap().execute_block_count
		}

		pub(super) fn indexed_transactions_count(&self) -> usize {
			self.inner.lock().unwrap().indexed_transactions_count
		}

		pub(super) fn overlay_marker_seen_by_indexed_transactions(&self) -> bool {
			self.inner.lock().unwrap().overlay_marker_seen_by_indexed_transactions
		}
	}

	impl sp_api::ProvideRuntimeApi<TestBlock> for CaseBClient {
		type Api = RuntimeApiImpl<TestBlock, CaseBClient>;

		fn runtime_api(&self) -> sp_api::ApiRef<'_, Self::Api> {
			RuntimeApi::construct_runtime_api(self)
		}
	}

	impl sp_api::CallApiAt<TestBlock> for CaseBClient {
		type StateBackend = InMemoryBackend<sp_runtime::traits::HashingFor<TestBlock>>;

		fn call_api_at(
			&self,
			params: sp_api::CallApiAtParams<TestBlock>,
		) -> Result<Vec<u8>, ApiError> {
			match params.function {
				"Core_execute_block" => {
					self.inner.lock().unwrap().execute_block_count += 1;
					let mut overlay = params.overlayed_changes.borrow_mut();
					overlay.set_storage(
						CASE_B_MARKER_KEY.to_vec(),
						Some(CASE_B_MARKER_VALUE.to_vec()),
					);
					overlay.add_transaction_index(IndexOperation::Renew {
						extrinsic: 0,
						hash: self.content_hash.to_vec(),
					});
					Ok(().encode())
				},
				"TransactionStorageApi_indexed_transactions" => {
					let overlay_marker_seen = {
						let mut overlay = params.overlayed_changes.borrow_mut();
						let seen = matches!(
							overlay.storage(CASE_B_MARKER_KEY),
							Some(Some(value)) if value == CASE_B_MARKER_VALUE
						);
						overlay.set_storage(
							CASE_B_ROLLBACK_MARKER_KEY.to_vec(),
							Some(b"must-be-rolled-back".to_vec()),
						);
						seen
					};
					let mut inner = self.inner.lock().unwrap();
					inner.indexed_transactions_count += 1;
					inner.overlay_marker_seen_by_indexed_transactions |= overlay_marker_seen;
					Ok(vec![self.info.clone()].encode())
				},
				other => panic!("unexpected runtime API function: {other}"),
			}
		}

		fn runtime_version_at(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
			_call_context: sp_core::traits::CallContext,
		) -> Result<sp_version::RuntimeVersion, ApiError> {
			Ok(case_b_runtime_version())
		}

		fn state_at(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
		) -> Result<Self::StateBackend, ApiError> {
			Ok(InMemoryBackend::default())
		}

		fn initialize_extensions(
			&self,
			_at: <TestBlock as sp_runtime::traits::Block>::Hash,
			_extensions: &mut sp_externalities::Extensions,
		) -> Result<(), ApiError> {
			Ok(())
		}
	}

	impl sc_client_api::BlockBackend<TestBlock> for CaseBClient {
		fn block_body(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<<TestBlock as BlockT>::Extrinsic>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_indexed_body(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<Vec<u8>>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_indexed_hashes(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Vec<H256>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<generic::SignedBlock<TestBlock>>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_status(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<sp_consensus::BlockStatus> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn justifications(
			&self,
			_hash: <TestBlock as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<Justifications>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn block_hash(
			&self,
			_number: sp_runtime::traits::NumberFor<TestBlock>,
		) -> sp_blockchain::Result<Option<<TestBlock as BlockT>::Hash>> {
			unreachable!("StorageChainBlockImport tests only query indexed transaction presence")
		}

		fn indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<Option<Vec<u8>>> {
			Ok(self.inner.lock().unwrap().indexed_transactions.get(&hash).cloned())
		}

		fn has_indexed_transaction(&self, hash: H256) -> sp_blockchain::Result<bool> {
			Ok(self.inner.lock().unwrap().indexed_transactions.contains_key(&hash))
		}

		fn requires_full_sync(&self) -> bool {
			false
		}
	}

	pub(super) struct Harness {
		pub(super) wrapper: StorageChainBlockImport<TestBlock, TestInner, MockApiClient>,
		pub(super) api: Arc<MockApiClient>,
		pub(super) captured: Arc<Mutex<Vec<BlockImportParams<TestBlock>>>>,
		pub(super) network: Arc<MockNetworkRequest>,
	}

	pub(super) fn make_harness() -> Harness {
		let api = Arc::new(MockApiClient::default());
		let network: Arc<MockNetworkRequest> = Arc::new(MockNetworkRequest::default());
		let inner = TestInner::recording();
		let captured = inner.captured.clone();

		let network_handle: NetworkHandle = Arc::new(OnceLock::new());
		let syncing_handle: SyncingHandle = Arc::new(OnceLock::new());
		let _ = network_handle.set(network.clone() as Arc<dyn NetworkRequest + Send + Sync>);
		let _ = syncing_handle
			.set(Arc::new(MockBitswapPeerSource { peers: vec![PeerId::random()] })
				as Arc<dyn BitswapPeerSource + Send + Sync>);

		let fetcher = IndexedTransactionFetcher::<TestBlock>::new(network_handle, syncing_handle);
		let wrapper = StorageChainBlockImport::new(inner, api.clone(), fetcher);

		Harness { wrapper, api, captured, network }
	}

	pub(super) struct CaseBHarness {
		pub(super) wrapper: StorageChainBlockImport<TestBlock, TestInner, CaseBClient>,
		pub(super) api: Arc<CaseBClient>,
		pub(super) captured: Arc<Mutex<Vec<BlockImportParams<TestBlock>>>>,
		pub(super) content_hash: ContentHash,
	}

	pub(super) fn make_case_b_harness(data: Vec<u8>) -> CaseBHarness {
		let content_hash = sp_transaction_storage_proof::HashingAlgorithm::Blake2b256.hash(&data);
		let api = Arc::new(CaseBClient::new(content_hash, data.clone()));
		let network: Arc<MockNetworkRequest> = Arc::new(MockNetworkRequest::default());
		network.insert(content_hash, data);
		let inner = TestInner::recording();
		let captured = inner.captured.clone();

		let network_handle: NetworkHandle = Arc::new(OnceLock::new());
		let syncing_handle: SyncingHandle = Arc::new(OnceLock::new());
		let _ = network_handle.set(network as Arc<dyn NetworkRequest + Send + Sync>);
		let _ = syncing_handle
			.set(Arc::new(MockBitswapPeerSource { peers: vec![PeerId::random()] })
				as Arc<dyn BitswapPeerSource + Send + Sync>);

		let fetcher = IndexedTransactionFetcher::<TestBlock>::new(network_handle, syncing_handle);
		let wrapper = StorageChainBlockImport::new(inner, api.clone(), fetcher);

		CaseBHarness { wrapper, api, captured, content_hash }
	}

	pub(super) fn case_b_params(number: u32) -> BlockImportParams<TestBlock> {
		let header = TestHeader::new(
			number,
			H256::zero(),
			case_b_state_root(),
			H256::zero(),
			Digest::default(),
		);
		let mut params = BlockImportParams::new(BlockOrigin::NetworkBroadcast, header);
		params.body = Some(vec![OpaqueExtrinsic::from_blob(b"case-b-renew-call".to_vec())]);
		params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::Custom(true));
		params
	}

	fn case_b_state_root() -> H256 {
		let backend = InMemoryBackend::<sp_runtime::traits::HashingFor<TestBlock>>::default();
		let mut overlay = OverlayedChanges::<sp_runtime::traits::HashingFor<TestBlock>>::default();
		overlay.set_storage(CASE_B_MARKER_KEY.to_vec(), Some(CASE_B_MARKER_VALUE.to_vec()));
		overlay.storage_root(&backend, case_b_runtime_version().state_version()).0
	}

	fn test_header(number: u32, parent: H256) -> TestHeader {
		TestHeader::new(number, H256::zero(), H256::zero(), parent, Digest::default())
	}

	pub(super) fn params_with_origin(
		origin: BlockOrigin,
		number: u32,
		body: Option<Vec<OpaqueExtrinsic>>,
	) -> BlockImportParams<TestBlock> {
		let header = test_header(number, H256::zero());
		let mut params = BlockImportParams::new(origin, header);
		params.body = body;
		params.fork_choice = Some(sc_consensus::ForkChoiceStrategy::Custom(true));
		params
	}

	fn empty_storage_changes() -> StorageChanges<sp_runtime::traits::HashingFor<TestBlock>> {
		StorageChanges::default()
	}

	pub(super) fn renew_op(hash: ContentHash, extrinsic_index: u32) -> IndexOperation {
		IndexOperation::Renew { extrinsic: extrinsic_index, hash: hash.to_vec() }
	}

	pub(super) fn case_a_params(
		number: u32,
		renews: Vec<IndexOperation>,
	) -> BlockImportParams<TestBlock> {
		let mut params = params_with_origin(
			BlockOrigin::NetworkBroadcast,
			number,
			Some(vec![OpaqueExtrinsic::from_blob(b"renew-call".to_vec())]),
		);
		let mut changes = empty_storage_changes();
		changes
			.main_storage_changes
			.push((OVERLAY_MARKER_KEY.to_vec(), Some(OVERLAY_MARKER_VALUE.to_vec())));
		changes.transaction_index_changes = renews;
		params.state_action = StateAction::ApplyChanges(ConsensusStorageChanges::Changes(changes));
		params
	}

	pub(super) fn prefetched_attached(params: &BlockImportParams<TestBlock>) -> bool {
		!params.prefetched_indexed_transactions.is_empty()
	}
}
