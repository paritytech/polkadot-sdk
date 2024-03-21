// Copyright (C) Parity Technologies (UK) Ltd.
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
use async_trait::async_trait;
use cumulus_primitives_core::relay_chain::BlockId;
use cumulus_relay_chain_inprocess_interface::{check_block_in_chain, BlockCheckStatus};
use cumulus_relay_chain_interface::{
	OverseerHandle, PHeader, ParaId, RelayChainError, RelayChainResult,
};
use cumulus_test_service::runtime::{Block, Hash, Header};
use futures::{executor::block_on, poll, task::Poll, FutureExt, Stream, StreamExt};
use parking_lot::Mutex;
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_primitives::{
	CandidateCommitments, CandidateDescriptor, CollatorPair, CommittedCandidateReceipt,
	Hash as PHash, HeadData, InboundDownwardMessage, InboundHrmpMessage, OccupiedCoreAssumption,
	PersistedValidationData, SessionIndex, SigningContext, ValidationCodeHash, ValidatorId,
};
use polkadot_test_client::{
	Client as PClient, ClientBlockImportExt, DefaultTestClientBuilderExt, FullBackend as PBackend,
	InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sc_client_api::{Backend, BlockchainEvents};
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_core::{Pair, H256};
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::MemoryKeystore, Keystore, KeystorePtr};
use sp_runtime::RuntimeAppPublic;
use sp_state_machine::StorageValue;
use std::{collections::BTreeMap, time::Duration};

fn check_error(error: crate::BoxedError, check_error: impl Fn(&BlockAnnounceError) -> bool) {
	let error = *error
		.downcast::<BlockAnnounceError>()
		.expect("Downcasts error to `ClientError`");
	if !check_error(&error) {
		panic!("Invalid error: {:?}", error);
	}
}

#[derive(Clone)]
struct DummyRelayChainInterface {
	data: Arc<Mutex<ApiData>>,
	relay_client: Arc<PClient>,
	relay_backend: Arc<PBackend>,
}

impl DummyRelayChainInterface {
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

#[async_trait]
impl RelayChainInterface for DummyRelayChainInterface {
	async fn validators(&self, _: PHash) -> RelayChainResult<Vec<ValidatorId>> {
		Ok(self.data.lock().validators.clone())
	}

	async fn best_block_hash(&self) -> RelayChainResult<PHash> {
		Ok(self.relay_backend.blockchain().info().best_hash)
	}
	async fn finalized_block_hash(&self) -> RelayChainResult<PHash> {
		Ok(self.relay_backend.blockchain().info().finalized_hash)
	}

	async fn retrieve_dmq_contents(
		&self,
		_: ParaId,
		_: PHash,
	) -> RelayChainResult<Vec<InboundDownwardMessage>> {
		unimplemented!("Not needed for test")
	}

	async fn retrieve_all_inbound_hrmp_channel_contents(
		&self,
		_: ParaId,
		_: PHash,
	) -> RelayChainResult<BTreeMap<ParaId, Vec<InboundHrmpMessage>>> {
		Ok(BTreeMap::new())
	}

	async fn persisted_validation_data(
		&self,
		_: PHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<PersistedValidationData>> {
		Ok(Some(PersistedValidationData {
			parent_head: HeadData(default_header().encode()),
			..Default::default()
		}))
	}

	async fn validation_code_hash(
		&self,
		_: PHash,
		_: ParaId,
		_: OccupiedCoreAssumption,
	) -> RelayChainResult<Option<ValidationCodeHash>> {
		unimplemented!("Not needed for test")
	}

	async fn candidate_pending_availability(
		&self,
		_: PHash,
		_: ParaId,
	) -> RelayChainResult<Option<CommittedCandidateReceipt>> {
		if self.data.lock().has_pending_availability {
			Ok(Some(CommittedCandidateReceipt {
				descriptor: CandidateDescriptor {
					para_head: polkadot_parachain_primitives::primitives::HeadData(
						default_header().encode(),
					)
					.hash(),
					para_id: 0u32.into(),
					relay_parent: PHash::random(),
					collator: CollatorPair::generate().0.public(),
					persisted_validation_data_hash: PHash::random(),
					pov_hash: PHash::random(),
					erasure_root: PHash::random(),
					signature: sp_core::sr25519::Signature::default().into(),
					validation_code_hash: ValidationCodeHash::from(PHash::random()),
				},
				commitments: CandidateCommitments {
					upward_messages: Default::default(),
					horizontal_messages: Default::default(),
					new_validation_code: None,
					head_data: HeadData(Vec::new()),
					processed_downward_messages: 0,
					hrmp_watermark: 0,
				},
			}))
		} else {
			Ok(None)
		}
	}

	async fn session_index_for_child(&self, _: PHash) -> RelayChainResult<SessionIndex> {
		Ok(0)
	}

	async fn import_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		Ok(Box::pin(
			self.relay_client
				.import_notification_stream()
				.map(|notification| notification.header),
		))
	}

	async fn finality_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		Ok(Box::pin(
			self.relay_client
				.finality_notification_stream()
				.map(|notification| notification.header),
		))
	}

	async fn is_major_syncing(&self) -> RelayChainResult<bool> {
		Ok(false)
	}

	fn overseer_handle(&self) -> RelayChainResult<OverseerHandle> {
		unimplemented!("Not needed for test")
	}

	async fn get_storage_by_key(
		&self,
		_: PHash,
		_: &[u8],
	) -> RelayChainResult<Option<StorageValue>> {
		unimplemented!("Not needed for test")
	}

	async fn prove_read(
		&self,
		_: PHash,
		_: &Vec<Vec<u8>>,
	) -> RelayChainResult<sc_client_api::StorageProof> {
		unimplemented!("Not needed for test")
	}

	async fn wait_for_block(&self, hash: PHash) -> RelayChainResult<()> {
		let mut listener = match check_block_in_chain(
			self.relay_backend.clone(),
			self.relay_client.clone(),
			hash,
		)? {
			BlockCheckStatus::InChain => return Ok(()),
			BlockCheckStatus::Unknown(listener) => listener,
		};

		let mut timeout = futures_timer::Delay::new(Duration::from_secs(10)).fuse();

		loop {
			futures::select! {
				_ = timeout => return Err(RelayChainError::WaitTimeout(hash)),
				evt = listener.next() => match evt {
					Some(evt) if evt.hash == hash => return Ok(()),
					// Not the event we waited on.
					Some(_) => continue,
					None => return Err(RelayChainError::ImportListenerClosed(hash)),
				}
			}
		}
	}

	async fn new_best_notification_stream(
		&self,
	) -> RelayChainResult<Pin<Box<dyn Stream<Item = PHeader> + Send>>> {
		let notifications_stream =
			self.relay_client
				.import_notification_stream()
				.filter_map(|notification| async move {
					if notification.is_new_best {
						Some(notification.header)
					} else {
						None
					}
				});
		Ok(Box::pin(notifications_stream))
	}

	async fn header(&self, block_id: BlockId) -> RelayChainResult<Option<PHeader>> {
		let hash = match block_id {
			BlockId::Hash(hash) => hash,
			BlockId::Number(num) =>
				if let Some(hash) = self.relay_client.hash(num)? {
					hash
				} else {
					return Ok(None)
				},
		};
		let header = self.relay_client.header(hash)?;

		Ok(header)
	}
}

fn make_validator_and_api() -> (
	RequireSecondedInBlockAnnounce<Block, Arc<DummyRelayChainInterface>>,
	Arc<DummyRelayChainInterface>,
) {
	let relay_chain_interface = Arc::new(DummyRelayChainInterface::new());
	(
		RequireSecondedInBlockAnnounce::new(relay_chain_interface.clone(), ParaId::from(56)),
		relay_chain_interface,
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
	api: Arc<DummyRelayChainInterface>,
	validator_index: u32,
) -> (CollationSecondedSignal, Header) {
	let relay_parent = api.relay_client.hash(0).ok().flatten().expect("Genesis hash exists");

	make_gossip_message_and_header(api, relay_parent, validator_index).await
}

async fn make_gossip_message_and_header(
	relay_chain_interface: Arc<DummyRelayChainInterface>,
	relay_parent: H256,
	validator_index: u32,
) -> (CollationSecondedSignal, Header) {
	let keystore: KeystorePtr = Arc::new(MemoryKeystore::new());
	let alice_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let session_index = relay_chain_interface.session_index_for_child(relay_parent).await.unwrap();
	let signing_context = SigningContext { parent_hash: relay_parent, session_index };

	let header = default_header();
	let candidate_receipt = CommittedCandidateReceipt {
		commitments: CandidateCommitments {
			head_data: header.encode().into(),
			..Default::default()
		},
		descriptor: CandidateDescriptor {
			para_id: 0u32.into(),
			relay_parent,
			collator: CollatorPair::generate().0.public(),
			persisted_validation_data_hash: PHash::random(),
			pov_hash: PHash::random(),
			erasure_root: PHash::random(),
			signature: sp_core::sr25519::Signature::default().into(),
			para_head: polkadot_parachain_primitives::primitives::HeadData(header.encode()).hash(),
			validation_code_hash: ValidationCodeHash::from(PHash::random()),
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
	.ok()
	.flatten()
	.expect("Signing statement");

	(CollationSecondedSignal { statement: signed, relay_parent }, header)
}

#[test]
fn valid_if_no_data_and_less_than_best_known_number() {
	let mut validator = make_validator_and_api().0;
	let header = Header { number: 0, ..default_header() };
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
	let header = Header { number: 1, state_root: Hash::random(), ..default_header() };
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
		.expect_err("Should fail on invalid encoded statement");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("Can not decode the `BlockAnnounceData`")
		)
	});
}

#[test]
fn block_announce_data_decoding_should_reject_extra_data() {
	let (mut validator, api) = make_validator_and_api();

	let (signal, header) = block_on(make_gossip_message_and_header_using_genesis(api, 1));
	let mut data = BlockAnnounceData::try_from(&signal).unwrap().encode();
	data.push(0x42);

	let res = block_on(validator.validate(&header, &data)).expect_err("Should return an error ");

	check_error(res, |error| {
		matches!(
			error,
			BlockAnnounceError(x) if x.contains("Input buffer has still data left after decoding!")
		)
	});
}

#[derive(Encode, Decode, Debug)]
struct LegacyBlockAnnounceData {
	receipt: CandidateReceipt,
	statement: UncheckedSigned<CompactStatement>,
}

#[test]
fn legacy_block_announce_data_handling() {
	let (_, api) = make_validator_and_api();

	let (signal, _) = block_on(make_gossip_message_and_header_using_genesis(api, 1));

	let receipt = if let Statement::Seconded(receipt) = signal.statement.payload() {
		receipt.to_plain()
	} else {
		panic!("Invalid")
	};

	let legacy = LegacyBlockAnnounceData {
		receipt: receipt.clone(),
		statement: signal.statement.convert_payload().into(),
	};

	let data = legacy.encode();

	let block_data =
		BlockAnnounceData::decode(&mut &data[..]).expect("Decoding works from legacy works");
	assert_eq!(receipt.descriptor.relay_parent, block_data.relay_parent);

	let data = block_data.encode();
	LegacyBlockAnnounceData::decode(&mut &data[..]).expect("Decoding works");
}

#[test]
fn check_signer_is_legit_validator() {
	let (mut validator, api) = make_validator_and_api();

	let (signal, header) = block_on(make_gossip_message_and_header_using_genesis(api, 1));
	let data = BlockAnnounceData::try_from(&signal).unwrap().encode();

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[test]
fn check_statement_is_correctly_signed() {
	let (mut validator, api) = make_validator_and_api();

	let (signal, header) = block_on(make_gossip_message_and_header_using_genesis(api, 0));

	let mut data = BlockAnnounceData::try_from(&signal).unwrap().encode();

	// The signature comes at the end of the type, so change a bit to make the signature invalid.
	let last = data.len() - 1;
	data[last] = data[last].wrapping_add(1);

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[tokio::test]
async fn check_statement_seconded() {
	let (mut validator, relay_chain_interface) = make_validator_and_api();
	let header = default_header();
	let relay_parent = H256::from_low_u64_be(1);

	let keystore: KeystorePtr = Arc::new(MemoryKeystore::new());
	let alice_public = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let session_index = relay_chain_interface.session_index_for_child(relay_parent).await.unwrap();
	let signing_context = SigningContext { parent_hash: relay_parent, session_index };

	let statement = Statement::Valid(Default::default());

	let signed_statement = SignedFullStatement::sign(
		&keystore,
		statement,
		&signing_context,
		0.into(),
		&alice_public.into(),
	)
	.ok()
	.flatten()
	.expect("Signs statement");

	let data = BlockAnnounceData {
		receipt: CandidateReceipt {
			commitments_hash: PHash::random(),
			descriptor: CandidateDescriptor {
				para_head: HeadData(Vec::new()).hash(),
				para_id: 0u32.into(),
				relay_parent: PHash::random(),
				collator: CollatorPair::generate().0.public(),
				persisted_validation_data_hash: PHash::random(),
				pov_hash: PHash::random(),
				erasure_root: PHash::random(),
				signature: sp_core::sr25519::Signature::default().into(),
				validation_code_hash: ValidationCodeHash::from(PHash::random()),
			},
		},
		statement: signed_statement.convert_payload().into(),
		relay_parent,
	}
	.encode();

	let res = block_on(validator.validate(&header, &data));
	assert_eq!(Validation::Failure { disconnect: true }, res.unwrap());
}

#[test]
fn check_header_match_candidate_receipt_header() {
	let (mut validator, api) = make_validator_and_api();

	let (signal, mut header) = block_on(make_gossip_message_and_header_using_genesis(api, 0));
	let data = BlockAnnounceData::try_from(&signal).unwrap().encode();
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
		let block = client.init_polkadot_block_builder().build().expect("Build new block").block;

		let (signal, header) = make_gossip_message_and_header(api, block.hash(), 0).await;

		let data = BlockAnnounceData::try_from(&signal).unwrap().encode();

		let mut validation = validator.validate(&header, &data);

		// The relay chain block is not available yet, so the first poll should return
		// that the future is still pending.
		assert!(poll!(&mut validation).is_pending());

		client.import(BlockOrigin::Own, block).await.expect("Imports the block");

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

		assert!(matches!(validation.await, Ok(Validation::Success { is_new_best: true })));
	});
}

#[derive(Default)]
struct ApiData {
	validators: Vec<ValidatorId>,
	has_pending_availability: bool,
}
