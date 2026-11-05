// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Building parachain blocks and their PoVs the way the JAM collator does, in memory.
//!
//! The runtime blob runs through the node's executor on an in-memory client. Each block gets what
//! the omni-node's JAM builder gives it (`nodes/jam/builder_task.rs`): the refine context, the JAM
//! state reader and additional-data finalizer over a JAM state proof, the Aura slot, `JamParent`
//! and `BlockBundleInfo` digests, the timestamp and mocked parachain inherents, and the JAM block
//! authorization seal. The PoV is the V4 `ParachainBlockData` the collator ships as work-item
//! extrinsic 0 (`nodes/jam/package.rs`).
//!
//! Everything is deterministic: JAM slots, timestamps and anchors are derived from the block
//! number and every signature uses a seeded nonce, so two runs build the same bytes.

use crate::jam_state::{Entries, JamState};
use codec::{Decode, Encode};
use cumulus_client_parachain_inherent::MockValidationDataInherentDataProvider;
use cumulus_jam_state_reader::{JamProofFinalizer, JamProofReader, JamStateExt, JAM_PROOF_KEY};
use cumulus_primitives_core::{
	relay_chain::HeadData, BlockBundleInfo, CumulusDigestItem, ParachainBlockData, SchedulingProof,
};
use jam_types::RefineContext;
use parachain_authorizer::{
	aura::{build_collator_proofs, AuthConfig, AuthToken, CollatorKey},
	block::signable_block_hash,
};
use parachain_service_core::{
	authorizer::{authorizer_hash, AuthConfigBlob, Authorizer, AuthorizerHash},
	blake2_256,
	block_authorization::{BlockAuthorization, BLOCK_AUTHORIZATION_ENGINE_ID},
	PARACHAIN_SERVICE_ID,
};
use parachain_template_runtime::{
	apis::RuntimeApi, opaque::Block, RuntimeCall, TxExtension, UncheckedExtrinsic, UNIT,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::in_mem;
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy, StateAction};
use sc_executor::WasmExecutor;
use sc_executor_polkavm::JamRefineContextExt;
use sc_service::{client::Client, ClientConfig, GenesisBlockBuilder, LocalCallExecutor};
use sp_additional_data::{hash_value, AdditionalData, AdditionalDataExt, AdditionalDataFinalizer};
use sp_api::{ProofRecorder, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_consensus_aura::{sr25519::AuthorityId as AuraId, AuraApi, Slot, AURA_ENGINE_ID};
use sp_core::{storage::well_known_keys::CODE, Pair, H256};
use sp_externalities::Extensions;
use sp_inherents::{InherentData, InherentDataProvider};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{
	generic::{Era, SignedPayload},
	traits::{BlakeTwo256, Block as BlockT, Header as HeaderT},
	Digest, DigestItem, MultiAddress, MultiSignature, OpaqueExtrinsic,
};
use sp_trie::proof_size_extension::ProofSizeExt;
use std::sync::Arc;

/// The host functions the omni-node executes parachain runtimes with
/// (`polkadot-omni-node-lib`'s `ParachainHostFunctions`).
type HostFunctions = (
	cumulus_client_service::ParachainHostFunctions,
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_jam_state_reader::jam_state::HostFunctions,
	cumulus_jam_state_reader::jam_upward::HostFunctions,
);
/// The same set without `sp_io`'s, which the genesis config builder adds itself.
type ExtraHostFunctions = (
	cumulus_client_service::storage_proof_size::HostFunctions,
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_jam_state_reader::jam_state::HostFunctions,
	cumulus_jam_state_reader::jam_upward::HostFunctions,
);
type Executor = WasmExecutor<HostFunctions>;
type Backend = in_mem::Backend<Block>;
type ParaClient = Client<Backend, LocalCallExecutor<Block, Backend, Executor>, Block, RuntimeApi>;

/// The genesis preset the chain starts from.
const PRESET: &str = sp_genesis_builder::DEV_RUNTIME_PRESET;
/// Milliseconds per JAM timeslot.
const JAM_SLOT_MILLIS: u64 = 6000;
/// Milliseconds per parachain slot, the template's `SLOT_DURATION`.
const PARA_SLOT_MILLIS: u64 = 6000;
/// JAM slot of the first block. The chain advances by one slot per block.
const FIRST_JAM_SLOT: u32 = 1_000;

/// A built block and everything a refine of it needs.
pub struct BuiltBlock {
	pub number: u32,
	pub hash: H256,
	pub parent_hash: H256,
	pub transactions: u32,
	/// The SCALE-encoded `ParachainBlockData::V4`, work-item extrinsic 0.
	pub pov: Vec<u8>,
	/// The JAM-encoded refine context.
	pub refine_context: Vec<u8>,
	import: BlockImportParams<Block>,
}

/// An in-memory parachain whose blocks one collator authors.
pub struct Chain {
	client: Arc<ParaClient>,
	para_id: u32,
	runtime_code: (H256, u32),
	authorizer: AuthorizerSetup,
	collator: Signer,
	sender: Signer,
	nonce: u32,
	/// Nonces for the mocked relay chain's BABE seals.
	relay_rng: ChaCha20Rng,
	genesis_hash: H256,
	spec_version: u32,
	transaction_version: u32,
}

/// The JAM authorizer the collator authorizes its blocks under.
struct AuthorizerSetup {
	code: Vec<u8>,
	code_hash: [u8; 32],
	config: Vec<u8>,
	hash: AuthorizerHash,
	auth_config: AuthConfig,
	own_index: u32,
	own_proof: Vec<H256>,
}

impl Chain {
	/// Start a chain from the runtime's development preset, its blocks authored by Alice under
	/// the authorizer `authorizer_code`.
	pub fn new(runtime: &[u8], authorizer_code: &[u8]) -> Result<Self, String> {
		let mut storage =
			sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<ExtraHostFunctions>::new(runtime)
				.get_storage_for_named_preset(Some(&PRESET.to_string()))?;
		storage.top.insert(CODE.to_vec(), runtime.to_vec());

		let executor = Executor::builder().with_allow_missing_host_functions(true).build();
		let backend = Arc::new(Backend::new());
		let genesis_block_builder =
			GenesisBlockBuilder::new_with_storage(storage, true, backend.clone(), executor.clone())
				.map_err(|error| format!("genesis: {error}"))?;
		let client = Arc::new(
			sc_service::client::new_with_backend::<_, _, Block, _, RuntimeApi>(
				backend,
				executor,
				genesis_block_builder,
				Box::new(sp_core::testing::TaskExecutor::new()),
				None,
				None,
				ClientConfig::default(),
			)
			.map_err(|error| format!("client: {error}"))?,
		);

		let genesis_hash = client.info().genesis_hash;
		let version = client
			.runtime_version_at(
				genesis_hash,
				sp_core::traits::CallContext::Onchain { import: false },
			)
			.map_err(|error| format!("runtime version: {error}"))?;
		let para_id = parachain_template_runtime::PARACHAIN_ID;
		let authorities: Vec<AuraId> = client
			.runtime_api()
			.authorities(genesis_hash)
			.map_err(|error| format!("aura authorities: {error}"))?;
		let collator = Signer::new(Sr25519Keyring::Alice, 1);
		let collators: Vec<CollatorKey> =
			authorities.iter().map(|key| key.clone().into_inner().0).collect();
		let own_index = collators
			.iter()
			.position(|key| *key == collator.public)
			.ok_or("Alice is not an Aura authority of the development preset")?
			as u32;
		let (collator_set_root, proofs) = build_collator_proofs(&collators)
			.map_err(|error| format!("collator set: {error:?}"))?;
		let auth_config = AuthConfig {
			para_ids: vec![para_id.into()],
			parachain_service: PARACHAIN_SERVICE_ID,
			collator_set_root,
			collator_set_size: collators.len() as u32,
			slot_duration: 1,
		};
		let code_hash = blake2_256(authorizer_code);
		let config = auth_config.encode();
		let hash = authorizer_hash(&Authorizer {
			code_hash: code_hash.into(),
			config: AuthConfigBlob(config.clone()),
		});

		Ok(Chain {
			client,
			para_id,
			runtime_code: (H256(blake2_256(runtime)), runtime.len() as u32),
			authorizer: AuthorizerSetup {
				code: authorizer_code.to_vec(),
				code_hash,
				config,
				hash,
				auth_config,
				own_index,
				own_proof: proofs[own_index as usize].clone(),
			},
			collator,
			sender: Signer::new(Sr25519Keyring::Alice, 2),
			nonce: 0,
			relay_rng: ChaCha20Rng::seed_from_u64(3),
			genesis_hash,
			spec_version: version.spec_version,
			transaction_version: version.transaction_version,
		})
	}

	/// Build a block with `transactions` `transfer_keep_alive`s from Alice to Bob on top of the
	/// best block. The block is not imported.
	pub fn build(&mut self, transactions: u32) -> Result<BuiltBlock, String> {
		let info = self.client.info();
		let parent_hash = info.best_hash;
		let parent_header = self
			.client
			.header(parent_hash)
			.map_err(|error| error.to_string())?
			.ok_or("the best block has no header")?;
		let number = info.best_number + 1;

		let jam_slot = self.jam_slot(number);
		let timestamp = (jam_types::JAM_COMMON_ERA + u64::from(jam_slot) * 6) * 1000;
		let para_slot = Slot::from(timestamp / PARA_SLOT_MILLIS);
		let relay_slot = timestamp / JAM_SLOT_MILLIS;

		// The JAM state at the anchor the block is built against. The parent is the included
		// head, as if every block was accumulated before the next one is authored.
		let jam_state = JamState::new(Entries {
			para_id: self.para_id,
			included_head: parent_hash.0,
			validation_code: (self.runtime_code.0 .0, self.runtime_code.1),
			authorizer_hash: self.authorizer.hash,
			authorizer_code: &self.authorizer.code,
		});
		let anchor = blake2_256(&(b"anchor", jam_slot).encode());
		let lookup_anchor = blake2_256(&(b"lookup_anchor", jam_slot).encode());
		let context = RefineContext {
			anchor: anchor.into(),
			anchor_slot: jam_slot,
			state_root: jam_state.root.into(),
			beefy_root: Default::default(),
			lookup_anchor: lookup_anchor.into(),
			lookup_anchor_slot: jam_slot,
			lookup_anchor_state_root: jam_state.root.into(),
			prerequisites: Default::default(),
		};
		let refine_context = jam_codec::Encode::encode(&context);

		let recorder = ProofRecorder::<Block>::default();
		let mut extensions = Extensions::new();
		extensions.register(ProofSizeExt::new(recorder.clone()));
		extensions.register(JamRefineContextExt(refine_context.clone()));
		let jam_proof = (jam_state.root, &jam_state.proof).encode();
		extensions.register(JamStateExt(Box::new(Arc::new(JamProofReader::new(
			jam_state.root,
			jam_state.proof.clone(),
		)))));
		extensions.register(AdditionalDataExt(
			[(
				JAM_PROOF_KEY.to_string(),
				Box::new(JamProofFinalizer { commitment: hash_value(&jam_proof) })
					as Box<dyn AdditionalDataFinalizer>,
			)]
			.into(),
		));
		let additional_data: AdditionalData =
			core::iter::once((JAM_PROOF_KEY.to_string(), jam_proof)).collect();

		let digest = Digest {
			logs: vec![
				DigestItem::PreRuntime(AURA_ENGINE_ID, para_slot.encode()),
				CumulusDigestItem::JamParent {
					anchor: anchor.into(),
					anchor_slot: jam_slot,
					lookup_anchor: lookup_anchor.into(),
					lookup_anchor_slot: jam_slot,
				}
				.to_digest_item(),
				BlockBundleInfo { index: 0, is_last: true }.to_digest_item(),
			],
		};

		let mut inherent_data = InherentData::new();
		let mocked_parachain = MockValidationDataInherentDataProvider::<()> {
			current_para_block: number,
			para_id: self.para_id.into(),
			current_para_block_head: Some(HeadData(parent_header.encode())),
			relay_blocks_per_para_block: (PARA_SLOT_MILLIS / JAM_SLOT_MILLIS) as u32,
			relay_offset: (relay_slot as u32).saturating_sub(number),
			relay_parent_offset: 0,
			para_blocks_per_relay_epoch: 10,
			upgrade_go_ahead: None,
			..Default::default()
		};
		futures::executor::block_on(async {
			sp_timestamp::InherentDataProvider::new(timestamp.into())
				.provide_inherent_data(&mut inherent_data)
				.await?;
			mocked_parachain.provide_inherent_data(&mut inherent_data).await
		})
		.map_err(|error| format!("inherent data: {error}"))?;
		normalize_relay_parent_seals(&mut inherent_data, &mut self.relay_rng)?;

		let transfers: Vec<_> = (self.nonce..self.nonce + transactions)
			.map(|nonce| self.transfer(nonce))
			.collect();
		let mut builder = BlockBuilderBuilder::new(&*self.client)
			.on_parent_block(parent_hash)
			.with_parent_block_number(info.best_number)
			.with_proof_recorder(Some(recorder.clone()))
			.with_inherent_digests(digest)
			.with_extra_extensions(extensions)
			.build()
			.map_err(|error| format!("block builder: {error}"))?;
		for inherent in builder
			.create_inherents(inherent_data)
			.map_err(|error| format!("inherents: {error}"))?
		{
			builder.push(inherent).map_err(|error| format!("inherent: {error}"))?;
		}
		for (nonce, transfer) in (self.nonce..).zip(transfers) {
			builder
				.push(transfer)
				.map_err(|error| format!("transaction {nonce}: {error}"))?;
		}
		let built = builder.build().map_err(|error| format!("building: {error}"))?;
		let proof = recorder
			.drain_storage_proof()
			.into_compact_proof::<BlakeTwo256>(*parent_header.state_root())
			.map_err(|error| format!("compact proof: {error}"))?;

		// Seal like the collator: the authorizer's `BlockAuthorization` of the seal-less header.
		let (pre_header, body) = built.block.deconstruct();
		let pre_hash = pre_header.hash();
		let token = AuthToken {
			proof: self.authorizer.own_proof.clone(),
			key: self.collator.public,
			signature: self.collator.sign(signable_block_hash(&pre_hash.0).as_bytes()),
		};
		let seal = DigestItem::Seal(
			BLOCK_AUTHORIZATION_ENGINE_ID,
			BlockAuthorization {
				auth_code_host: PARACHAIN_SERVICE_ID,
				code_hash: self.authorizer.code_hash,
				config: self.authorizer.config.clone(),
				token: token.encode(),
			}
			.encode(),
		);
		let mut header = pre_header.clone();
		header.digest_mut().push(seal.clone());
		let hash = header.hash();
		let pov = ParachainBlockData::<Block>::new_with_parent_header(
			vec![Block::new(header, body.clone())],
			proof,
			SchedulingProof::empty(),
			vec![Some(additional_data)],
			parent_header.encode(),
			Vec::new(),
		)
		.encode();

		let mut import = BlockImportParams::new(BlockOrigin::Own, pre_header);
		import.post_digests.push(seal);
		import.body = Some(body);
		import.state_action =
			StateAction::ApplyChanges(sc_consensus::StorageChanges::Changes(built.storage_changes));
		import.fork_choice = Some(ForkChoiceStrategy::LongestChain);

		Ok(BuiltBlock { number, hash, parent_hash, transactions, pov, refine_context, import })
	}

	/// Import `block` as the new best block.
	pub fn import(&mut self, block: BuiltBlock) -> Result<(), String> {
		futures::executor::block_on((&*self.client).import_block(block.import))
			.map_err(|error| format!("import: {error}"))?;
		self.nonce += block.transactions;
		Ok(())
	}

	/// The JAM slot block `number` is authored in: the first slot from `FIRST_JAM_SLOT + number`
	/// on whose round-robin turn is the collator's.
	fn jam_slot(&self, number: u32) -> u32 {
		(FIRST_JAM_SLOT + number..)
			.find(|slot| {
				matches!(
					self.authorizer.auth_config.expected_collator_index(*slot),
					Ok(index) if index == self.authorizer.own_index
				)
			})
			.expect("the round-robin names every collator once per turn; qed")
	}

	/// A signed `transfer_keep_alive` of one unit from Alice to Bob, immortal.
	fn transfer(&mut self, nonce: u32) -> OpaqueExtrinsic {
		let call = RuntimeCall::Balances(pallet_balances::Call::transfer_keep_alive {
			dest: MultiAddress::Id(Sr25519Keyring::Bob.to_account_id()),
			value: UNIT,
		});
		let extension: TxExtension = cumulus_pallet_weight_reclaim::StorageWeightReclaim::new((
			frame_system::AuthorizeCall::new(),
			frame_system::CheckNonZeroSender::new(),
			frame_system::CheckSpecVersion::new(),
			frame_system::CheckTxVersion::new(),
			frame_system::CheckGenesis::new(),
			frame_system::CheckEra::from(Era::Immortal),
			frame_system::CheckNonce::from(nonce),
			frame_system::CheckWeight::new(),
			pallet_transaction_payment::ChargeTransactionPayment::from(0),
			frame_metadata_hash_extension::CheckMetadataHash::new(false),
		));
		let implicit = (
			(),
			(),
			self.spec_version,
			self.transaction_version,
			self.genesis_hash,
			self.genesis_hash,
			(),
			(),
			(),
			None,
		);
		let payload = SignedPayload::<RuntimeCall, TxExtension>::from_raw(
			call.clone(),
			extension.clone(),
			implicit,
		);
		let signature = payload.using_encoded(|payload| self.sender.sign(payload));
		let extrinsic = UncheckedExtrinsic::new_signed(
			call,
			MultiAddress::Id(Sr25519Keyring::Alice.to_account_id()),
			MultiSignature::Sr25519(sp_core::sr25519::Signature::from_raw(signature)),
			extension,
		);
		OpaqueExtrinsic::decode(&mut &extrinsic.encode()[..])
			.expect("an encoded extrinsic decodes as an opaque one; qed")
	}
}

/// Re-seal the relay-parent descendants of the mocked parachain inherent with reproducible
/// signatures.
///
/// The mock BABE-seals them with `sp_core`'s randomized sr25519 signing, which would make every
/// block, and through the block hashes everything after it, differ from run to run. Each header is
/// re-signed by the same authority (the mock's `AuthorityPair::from_seed(&[index; 32])`) over the
/// same pre-hash, so the seals stay valid.
fn normalize_relay_parent_seals(
	inherent_data: &mut InherentData,
	rng: &mut ChaCha20Rng,
) -> Result<(), String> {
	use cumulus_primitives_parachain_inherent::{ParachainInherentData, INHERENT_IDENTIFIER};
	use sp_consensus_babe::{digests::CompatibleDigestItem, AuthoritySignature};

	let mut data: ParachainInherentData = inherent_data
		.get_data(&INHERENT_IDENTIFIER)
		.map_err(|error| format!("parachain inherent: {error}"))?
		.ok_or("the mocked parachain inherent is missing")?;
	let mut parent_hash = None;
	for header in &mut data.relay_parent_descendants {
		if let Some(parent_hash) = parent_hash {
			header.parent_hash = parent_hash;
		}
		header.digest.logs.retain(|item| item.as_babe_seal().is_none());
		let index = header
			.digest
			.logs
			.iter()
			.find_map(|item| item.as_babe_pre_digest())
			.ok_or("a mocked relay header has no BABE pre-digest")?
			.authority_index();
		let keypair = schnorrkel::MiniSecretKey::from_bytes(&[index as u8; 32])
			.expect("32 bytes are a mini secret key; qed")
			.expand_to_keypair(schnorrkel::ExpansionMode::Ed25519);
		let signature = sign(&keypair, rng, header.hash().as_bytes());
		header.digest.push(<DigestItem as CompatibleDigestItem>::babe_seal(
			AuthoritySignature::from(sp_core::sr25519::Signature::from_raw(signature)),
		));
		parent_hash = Some(header.hash());
	}
	inherent_data.replace_data(INHERENT_IDENTIFIER, &data);
	Ok(())
}

/// Sign `message` in the `substrate` signing context, like `sp_core`'s sr25519, with a nonce
/// from `rng`.
fn sign(keypair: &schnorrkel::Keypair, rng: &mut ChaCha20Rng, message: &[u8]) -> [u8; 64] {
	let transcript = schnorrkel::signing_context(b"substrate").bytes(message);
	keypair.sign(schnorrkel::context::attach_rng(transcript, rng)).to_bytes()
}

/// An sr25519 signer with a seeded nonce source, so its signatures are reproducible. Valid
/// sr25519 signatures in the `substrate` signing context, like `sp_core`'s.
struct Signer {
	keypair: schnorrkel::Keypair,
	public: [u8; 32],
	rng: ChaCha20Rng,
}

impl Signer {
	fn new(keyring: Sr25519Keyring, seed: u64) -> Self {
		let pair = keyring.pair();
		let keypair = schnorrkel::SecretKey::from_bytes(&pair.to_raw_vec())
			.expect("an sr25519 pair's raw secret is a schnorrkel secret key; qed")
			.to_keypair();
		let public = keypair.public.to_bytes();
		assert_eq!(public, pair.public().0, "the schnorrkel key is the keyring's key");
		Signer { keypair, public, rng: ChaCha20Rng::seed_from_u64(seed) }
	}

	fn sign(&mut self, message: &[u8]) -> [u8; 64] {
		sign(&self.keypair, &mut self.rng, message)
	}
}
