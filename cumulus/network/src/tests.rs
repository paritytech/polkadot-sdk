// Copyright 2020 Parity Technologies (UK) Ltd.
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
use cumulus_test_service::runtime::{Block, Header};
use futures::{executor::block_on, poll, task::Poll};
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_primitives::v1::{
	Block as PBlock, BlockNumber, CandidateCommitments, CandidateDescriptor, CandidateEvent,
	CommittedCandidateReceipt, CoreState, GroupRotationInfo, Hash as PHash, HeadData, Id as ParaId,
	InboundDownwardMessage, InboundHrmpMessage, OccupiedCoreAssumption, ParachainHost,
	PersistedValidationData, SessionIndex, SessionInfo, SigningContext, ValidationCode,
	ValidationData, ValidatorId, ValidatorIndex,
};
use polkadot_test_client::{
	Client as PClient, ClientBlockImportExt, DefaultTestClientBuilderExt, FullBackend as PBackend,
	InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sp_api::{ApiRef, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::{block_validation::BlockAnnounceValidator as _, BlockOrigin};
use sp_core::H256;
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::KeyStore, SyncCryptoStore, SyncCryptoStorePtr};
use sp_runtime::RuntimeAppPublic;
use std::collections::BTreeMap;

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
			..Default::default()
		},
	};
	let statement = Statement::Seconded(candidate_receipt);
	let signed = SignedFullStatement::sign(
		&keystore,
		statement,
		&signing_context,
		validator_index,
		&alice_public.into(),
	)
	.await
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
fn check_statement_is_encoded_correctly() {
	let mut validator = make_validator_and_api().0;
	let header = default_header();
	let res = block_on(validator.validate(&header, &[0x42]))
		.err()
		.expect("Should fail on invalid encoded statement");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("must be a `SignedFullStatement`")
		)
	});
}

#[test]
fn check_signer_is_legit_validator() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, header) = block_on(make_gossip_message_and_header_using_genesis(api, 1));
	let data = signed_statement.encode();

	let res = block_on(validator.validate(&header, &data))
		.err()
		.expect("Should fail on invalid validator");

	assert!(matches!(
		*res.downcast::<BlockAnnounceError>().unwrap(),
		BlockAnnounceError(x) if x.contains("signer is a validator")
	));
}

#[test]
fn check_statement_is_correctly_signed() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, header) = block_on(make_gossip_message_and_header_using_genesis(api, 0));

	let mut data = signed_statement.encode();

	// The signature comes at the end of the type, so change a bit to make the signature invalid.
	let last = data.len() - 1;
	data[last] = data[last].wrapping_add(1);

	let res = block_on(validator.validate(&header, &data))
		.err()
		.expect("Validation should fail if the statement is not signed correctly");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("signature is invalid")
		)
	});
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
		0,
		&alice_public.into(),
	))
	.expect("Signs statement");
	let data = signed_statement.encode();

	let res = block_on(validator.validate(&header, &data))
		.err()
		.expect("validation should fail if not seconded statement");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("must be a `Statement::Seconded`")
		)
	});
}

#[test]
fn check_header_match_candidate_receipt_header() {
	let (mut validator, api) = make_validator_and_api();

	let (signed_statement, mut header) =
		block_on(make_gossip_message_and_header_using_genesis(api, 0));
	let data = signed_statement.encode();
	header.number = 300;

	let res = block_on(validator.validate(&header, &data))
		.err()
		.expect("validation should fail if the header in doesn't match");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("header does not match")
		)
	});
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

		let data = signed_statement.encode();

		let mut validation = validator.validate(&header, &data);

		// The relay chain block is not available yet, so the first poll should return
		// that the future is still pending.
		assert!(poll!(&mut validation).is_pending());

		client
			.import(BlockOrigin::Own, block)
			.expect("Imports the block");

		assert!(matches!(
			poll!(validation),
			Poll::Ready(Ok(Validation::Success { is_new_best: true }))
		));
	});
}

#[derive(Default)]
struct ApiData {
	validators: Vec<ValidatorId>,
}

struct TestApi {
	data: Arc<ApiData>,
	relay_client: Arc<PClient>,
	relay_backend: Arc<PBackend>,
}

impl TestApi {
	fn new() -> Self {
		let builder = TestClientBuilder::new();
		let relay_backend = builder.backend();

		Self {
			data: Arc::new(ApiData {
				validators: vec![Sr25519Keyring::Alice.public().into()],
			}),
			relay_client: Arc::new(builder.build()),
			relay_backend,
		}
	}
}

#[derive(Default)]
struct RuntimeApi {
	data: Arc<ApiData>,
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
		type Error = sp_blockchain::Error;

		fn validators(&self) -> Vec<ValidatorId> {
			self.data.validators.clone()
		}

		fn validator_groups(&self) -> (Vec<Vec<ValidatorIndex>>, GroupRotationInfo<BlockNumber>) {
			(Vec::new(), GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 0, now: 0 })
		}

		fn availability_cores(&self) -> Vec<CoreState<PHash>> {
			Vec::new()
		}

		fn full_validation_data(&self, _: ParaId, _: OccupiedCoreAssumption) -> Option<ValidationData<BlockNumber>> {
			None
		}

		fn persisted_validation_data(&self, _: ParaId, _: OccupiedCoreAssumption) -> Option<PersistedValidationData<BlockNumber>> {
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
			None
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
	}
}
