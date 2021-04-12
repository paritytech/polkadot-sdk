// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use cumulus_test_service::runtime::{Block, Header, Hash};
use futures::{executor::block_on, poll, task::Poll};
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_primitives::v1::{
	Block as PBlock, BlockNumber, CandidateCommitments, CandidateDescriptor, CandidateEvent,
	CommittedCandidateReceipt, CoreState, GroupRotationInfo, Hash as PHash, HeadData, Id as ParaId,
	InboundDownwardMessage, InboundHrmpMessage, OccupiedCoreAssumption, ParachainHost,
	PersistedValidationData, SessionIndex, SessionInfo, SigningContext, ValidationCode,
	ValidatorId, ValidatorIndex,
};
use polkadot_test_client::{
	Client as PClient, ClientBlockImportExt, DefaultTestClientBuilderExt, FullBackend as PBackend,
	InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_core::H256;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::KeyStore, SyncCryptoStore, SyncCryptoStorePtr};
use sp_runtime::RuntimeAppPublic;
use std::collections::BTreeMap;
use parking_lot::Mutex;

fn check_error(error: crate::BoxedError, check_error: impl Fn(&BlockAnnounceError) -> bool) {
	let error = *error
		.downcast::<BlockAnnounceError>()
		.expect("Downcasts error to `ClientError`");
	if !check_error(&error) {
		panic!("Invalid error: {:?}", error);
	}
}

#[derive(Clone)]
struct DummyCollatorNetwork;

impl SyncOracle for DummyCollatorNetwork {
	fn is_major_syncing(&mut self) -> bool {
		false
	}

	fn is_offline(&mut self) -> bool {
		unimplemented!("Not required in tests")
	}
}

fn make_validator_and_api() -> (
	BlockAnnounceValidator<Block, TestApi, PBackend, PClient>,
	Arc<TestApi>,
) {
	let api = Arc::new(TestApi::new());

	(
		BlockAnnounceValidator::new(
			api.clone(),
			ParaId::from(56),
			Box::new(DummyCollatorNetwork),
			api.relay_backend.clone(),
			api.relay_client.clone(),
		),
		api,
	)
}

fn default_header() -> Header {
	Header {
		number: 1,
		digest: Default::default(),
		extrinsics_root: Default::default(),
		parent_hash: Default::default(),
		state_root: Default::default(),
	}
}

/// Same as [`make_gossip_message_and_header`], but using the genesis header as relay parent.
async fn make_gossip_message_and_header_using_genesis(
	api: Arc<TestApi>,
	validator_index: u32,
) -> (SignedFullStatement, Header) {
	let relay_parent = api
		.relay_client
		.hash(0)
		.ok()
		.flatten()
		.expect("Genesis hash exists");

	make_gossip_message_and_header(api, relay_parent, validator_index).await
}

async fn make_gossip_message_and_header(
	api: Arc<TestApi>,
	relay_parent: H256,
	validator_index: u32,
) -> (SignedFullStatement, Header) {
	let keystore: SyncCryptoStorePtr = Arc::new(KeyStore::new());
	let alice_public = SyncCryptoStore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let session_index = api
		.runtime_api()
		.session_index_for_child(&BlockId::Hash(relay_parent))
		.unwrap();
	let signing_context = SigningContext {
		parent_hash: relay_parent,
		session_index,
	};

	let header = default_header();
	let candidate_receipt = CommittedCandidateReceipt {
		commitments: CandidateCommitments {
			head_data: header.encode().into(),
			..Default::default()
		},
		descriptor: CandidateDescriptor {
			relay_parent,
			para_head: polkadot_parachain::primitives::HeadData(header.encode()).hash(),
			..Default::default()
		},
	};
	let statement = Statement::Seconded(candidate_receipt);
	let signed = SignedFullStatement::sign(
		&keystore,
		statement,
		&signing_context,
		validator_index.into(),
		&alice_public.into(),
	)
	.await
	.ok()
	.flatten()
	.expect("Signing statement");

	(signed, header)
}

#[test]
fn valid_if_no_data_and_less_than_best_known_number() {
	let mut validator = make_validator_and_api().0;
	let header = Header {
		number: 0,
		..default_header()
	};
	let res = block_on(validator.validate(&header, &[]));

	assert_eq!(
		res.unwrap(),
		Validation::Success { is_new_best: false },
		"validating without data with block number < best known number is always a success",
	);
}

#[test]
fn invalid_if_no_data_exceeds_best_known_number() {
	let mut validator = make_validator_and_api().0;
	let header = Header {
		number: 1,
		state_root: Hash::random(),
		..default_header()
	};
	let res = block_on(validator.validate(&header, &[]));

	assert_eq!(
		res.unwrap(),
		Validation::Failure { disconnect: false },
		"validation fails if no justification and block number >= best known number",
	);
}

#[test]
fn valid_if_no_data_and_block_matches_best_known_block() {
	let mut validator = make_validator_and_api().0;
	let res = block_on(validator.validate(&default_header(), &[]));

	assert_eq!(
		res.unwrap(),
		Validation::Success { is_new_best: true },
		"validation is successful when the block hash matches the best known block",
	);
}

#[test]
fn check_statement_is_encoded_correctly() {
	let mut validator = make_validator_and_api().0;
	let header = default_header();
	let res = block_on(validator.validate(&header, &[0x42]))
		.err()
		.expect("Should fail on invalid encoded statement");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("Can not decode the `BlockAnnounceData`")
		)
	});
}

#[test]
fn check_signer_is_legit_validator() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, header) = block_on(make_gossip_message_and_header_using_genesis(api, 1));
	let data = BlockAnnounceData::try_from(signed_statement)
		.unwrap()
		.encode();

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[test]
fn check_statement_is_correctly_signed() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, header) = block_on(make_gossip_message_and_header_using_genesis(api, 0));

	let mut data = BlockAnnounceData::try_from(signed_statement)
		.unwrap()
		.encode();

	// The signature comes at the end of the type, so change a bit to make the signature invalid.
	let last = data.len() - 1;
	data[last] = data[last].wrapping_add(1);

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[test]
fn check_statement_seconded() {
	let (mut validator, api) = make_validator_and_api();
	let header = default_header();
	let relay_parent = H256::from_low_u64_be(1);

	let keystore: SyncCryptoStorePtr = Arc::new(KeyStore::new());
	let alice_public = SyncCryptoStore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let session_index = api
		.runtime_api()
		.session_index_for_child(&BlockId::Hash(relay_parent))
		.unwrap();
	let signing_context = SigningContext {
		parent_hash: relay_parent,
		session_index,
	};

	let statement = Statement::Valid(Default::default());

	let signed_statement = block_on(SignedFullStatement::sign(
		&keystore,
		statement,
		&signing_context,
		0.into(),
		&alice_public.into(),
	))
	.ok()
	.flatten()
	.expect("Signs statement");

	let data = BlockAnnounceData {
		receipt: Default::default(),
		statement: signed_statement.convert_payload(),
	}
	.encode();

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[test]
fn check_header_match_candidate_receipt_header() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, mut header) =
		block_on(make_gossip_message_and_header_using_genesis(api, 0));
	let data = BlockAnnounceData::try_from(signed_statement)
		.unwrap()
		.encode();
	header.number = 300;

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

/// Test that ensures that we postpone the block announce verification until
/// a relay chain block is imported. This is important for when we receive a
/// block announcement before we have imported the associated relay chain block
/// which can happen on slow nodes or nodes with a slow network connection.
#[test]
fn relay_parent_not_imported_when_block_announce_is_processed() {
	block_on(async move {
		let (mut validator, api) = make_validator_and_api();

		let mut client = api.relay_client.clone();
		let block = client
			.init_polkadot_block_builder()
			.build()
			.expect("Build new block")
			.block;

		let (signed_statement, header) = make_gossip_message_and_header(api, block.hash(), 0).await;

		let data = BlockAnnounceData::try_from(signed_statement)
			.unwrap()
			.encode();

		let mut validation = validator.validate(&header, &data);

		// The relay chain block is not available yet, so the first poll should return
		// that the future is still pending.
		assert!(poll!(&mut validation).is_pending());

		client
			.import(BlockOrigin::Own, block)
			.await
			.expect("Imports the block");

		assert!(matches!(
			poll!(validation),
			Poll::Ready(Ok(Validation::Success { is_new_best: true }))
		));
	});
}

/// Ensures that when we receive a block announcement without a statement included, while the block
/// is not yet included by the node checking the announcement, but the node is already backed.
#[test]
fn block_announced_without_statement_and_block_only_backed() {
	block_on(async move {
		let (mut validator, api) = make_validator_and_api();
		api.data.lock().has_pending_availability = true;

		let header = default_header();

		let validation = validator.validate(&header, &[]);

		assert!(matches!(
			validation.await,
			Ok(Validation::Success { is_new_best: true })
		));
	});
}

#[derive(Default)]
struct ApiData {
	validators: Vec<ValidatorId>,
	has_pending_availability: bool,
}

struct TestApi {
	data: Arc<Mutex<ApiData>>,
	relay_client: Arc<PClient>,
	relay_backend: Arc<PBackend>,
}

impl TestApi {
	fn new() -> Self {
		let builder = TestClientBuilder::new();
		let relay_backend = builder.backend();

		Self {
			data: Arc::new(Mutex::new(ApiData {
				validators: vec![Sr25519Keyring::Alice.public().into()],
				has_pending_availability: false,
			})),
			relay_client: Arc::new(builder.build()),
			relay_backend,
		}
	}
}

#[derive(Default)]
struct RuntimeApi {
	data: Arc<Mutex<ApiData>>,
}

impl ProvideRuntimeApi<PBlock> for TestApi {
	type Api = RuntimeApi;

	fn runtime_api<'a>(&'a self) -> ApiRef<'a, Self::Api> {
		RuntimeApi {
			data: self.data.clone(),
		}
		.into()
	}
}

sp_api::mock_impl_runtime_apis! {
	impl ParachainHost<PBlock> for RuntimeApi {
		fn validators(&self) -> Vec<ValidatorId> {
			self.data.lock().validators.clone()
		}

		fn validator_groups(&self) -> (Vec<Vec<ValidatorIndex>>, GroupRotationInfo<BlockNumber>) {
			(Vec::new(), GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 0, now: 0 })
		}

		fn availability_cores(&self) -> Vec<CoreState<PHash>> {
			Vec::new()
		}

		fn persisted_validation_data(
			&self,
			_: ParaId,
			_: OccupiedCoreAssumption,
		) -> Option<PersistedValidationData<PHash, BlockNumber>> {
			Some(PersistedValidationData {
				parent_head: HeadData(default_header().encode()),
				..Default::default()
			})
		}

		fn session_index_for_child(&self) -> SessionIndex {
			0
		}

		fn validation_code(&self, _: ParaId, _: OccupiedCoreAssumption) -> Option<ValidationCode> {
			None
		}

		fn candidate_pending_availability(&self, _: ParaId) -> Option<CommittedCandidateReceipt<PHash>> {
			if self.data.lock().has_pending_availability {
				Some(CommittedCandidateReceipt {
					descriptor: CandidateDescriptor {
						para_head: polkadot_parachain::primitives::HeadData(
							default_header().encode(),
						).hash(),
						..Default::default()
					},
					..Default::default()
				})
			} else {
				None
			}
		}

		fn candidate_events(&self) -> Vec<CandidateEvent<PHash>> {
			Vec::new()
		}

		fn session_info(_: SessionIndex) -> Option<SessionInfo> {
			None
		}

		fn check_validation_outputs(_: ParaId, _: CandidateCommitments) -> bool {
			false
		}

		fn dmq_contents(_: ParaId) -> Vec<InboundDownwardMessage<BlockNumber>> {
			Vec::new()
		}

		fn historical_validation_code(_: ParaId, _: BlockNumber) -> Option<ValidationCode> {
			None
		}

		fn inbound_hrmp_channels_contents(
			_: ParaId,
		) -> BTreeMap<ParaId, Vec<InboundHrmpMessage<BlockNumber>>> {
			BTreeMap::new()
		}

		fn validation_code_by_hash(_: PHash) -> Option<ValidationCode> {
			None
		}
	}
}
