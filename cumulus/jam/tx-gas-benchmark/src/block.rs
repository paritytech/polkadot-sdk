// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Building parachain blocks and their PoVs the way the JAM collator does, in memory.
//!
//! The runtime blob runs through the node's executor on an in-memory client. Each block gets what
//! the omni-node's JAM builder gives it (`nodes/jam/builder_task.rs`): the refine context, the JAM
//! state reader and additional-data finalizer over a JAM state proof, the Aura slot, `JamParent`
//! and `BlockBundleInfo` digests, the timestamp and mocked parachain inherents, and the Aura seal.
//! The PoV is the V4 `ParachainBlockData` the collator ships as work-item
//! extrinsic 0 (`nodes/jam/package.rs`).
//!
//! Everything is deterministic: JAM slots, timestamps and anchors are derived from the block
//! number and every signature uses a seeded nonce, so two runs build the same bytes.

use crate::jam_state::{Entries, JamState};
use codec::{Decode, Encode};
use cumulus_jam_state_reader::{JamProofFinalizer, JamProofReader, JamStateExt, JAM_PROOF_KEY};
use cumulus_primitives_core::{
	BlockBundleInfo, CumulusDigestItem, ParachainBlockData, SchedulingProof,
};
use jam_types::RefineContext;
use parachain_service_core::blake2_256;
use parachain_template_runtime::{
	apis::RuntimeApi, opaque::Block, RuntimeCall, TxExtension, UncheckedExtrinsic, UNIT,
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use sc_block_builder::BlockBuilderBuilder;
use sc_client_api::in_mem;
use sc_consensus::{BlockImport, BlockImportParams, ForkChoiceStrategy, StateAction};
use sc_pvm_executor::PvmExecutor;
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

/// The host calls the omni-node executes JAM parachain runtimes with
/// (`polkadot-omni-node-lib`'s `JamHostFunctions`).
type HostFunctions = (
	sp_io::jam::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::jam::HostFunctions,
	sp_additional_data::jam::HostFunctions,
	cumulus_jam_state_reader::jam_state::HostFunctions,
);
type Executor = PvmExecutor<HostFunctions>;
type Backend = in_mem::Backend<Block>;
type ParaClient = Client<Backend, LocalCallExecutor<Block, Backend, Executor>, Block, RuntimeApi>;

/// The genesis preset the chain starts from.
const PRESET: &str = sp_genesis_builder::DEV_RUNTIME_PRESET;
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
	/// Size of the Aura authority set and the collator's index in it.
	aura_authorities: u32,
	aura_index: u32,
	collator: Signer,
	sender: Signer,
	nonce: u32,
	genesis_hash: H256,
	spec_version: u32,
	transaction_version: u32,
}

impl Chain {
	/// Start a chain from the runtime's development preset, its blocks authored by Alice.
	pub fn new(runtime: &[u8]) -> Result<Self, String> {
		let mut storage = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<()>::new(runtime)
			.get_storage_for_named_preset(Some(&PRESET.to_string()))?;
		storage.top.insert(CODE.to_vec(), runtime.to_vec());

		let executor = Executor::default();
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
		let aura_index = authorities
			.iter()
			.position(|key| key.clone().into_inner().0 == collator.public)
			.ok_or("Alice is not an Aura authority of the development preset")?
			as u32;

		Ok(Chain {
			client,
			para_id,
			runtime_code: (H256(blake2_256(runtime)), runtime.len() as u32),
			aura_authorities: authorities.len() as u32,
			aura_index,
			collator,
			sender: Signer::new(Sr25519Keyring::Alice, 2),
			nonce: 0,
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
		let timestamp = timestamp(jam_slot);
		let para_slot = Slot::from(timestamp / PARA_SLOT_MILLIS);

		// The JAM state at the anchor the block is built against. The parent is the included
		// head, as if every block was accumulated before the next one is authored.
		let jam_state = JamState::new(Entries {
			para_id: self.para_id,
			included_head: parent_hash.0,
			validation_code: (self.runtime_code.0 .0, self.runtime_code.1),
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
		futures::executor::block_on(
			sp_timestamp::InherentDataProvider::new(timestamp.into())
				.provide_inherent_data(&mut inherent_data),
		)
		.map_err(|error| format!("inherent data: {error}"))?;

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

		// Seal like the collator: Alice's Aura signature over the seal-less header hash.
		let (pre_header, body) = built.block.deconstruct();
		let seal = DigestItem::Seal(
			AURA_ENGINE_ID,
			self.collator.sign(pre_header.hash().as_bytes()).to_vec(),
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
	/// on whose Aura slot is the collator's.
	fn jam_slot(&self, number: u32) -> u32 {
		(FIRST_JAM_SLOT + number..)
			.find(|slot| {
				(timestamp(*slot) / PARA_SLOT_MILLIS) % u64::from(self.aura_authorities) ==
					u64::from(self.aura_index)
			})
			.expect("Aura names every authority once per turn; qed")
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

/// Unix time in milliseconds at the start of `jam_slot`.
fn timestamp(jam_slot: u32) -> u64 {
	(jam_types::JAM_COMMON_ERA + u64::from(jam_slot) * 6) * 1000
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
