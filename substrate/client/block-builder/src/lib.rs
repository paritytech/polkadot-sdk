// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate block builder
//!
//! This crate provides the [`BlockBuilder`] utility and the corresponding runtime api
//! [`BlockBuilder`](sp_block_builder::BlockBuilder).
//!
//! The block builder utility is used in the node as an abstraction over the runtime api to
//! initialize a block, to push extrinsics and to finalize a block.

#![warn(missing_docs)]

use codec::Encode;
use sp_api::{
	CallApiAt, Core, DisableProofRecording, EnableProofRecording, RuntimeInstance,
	RuntimeInstanceBuilderStage2, StorageChanges, TransactionOutcome,
};
use sp_blockchain::{ApplyExtrinsicFailed, Error, HeaderBackend};
use sp_runtime::{
	legacy,
	traits::{Block as BlockT, Hash, HashingFor, Header as HeaderT, NumberFor, One},
	Digest, ExtrinsicInclusionMode,
};
use std::marker::PhantomData;

pub use sp_block_builder::BlockBuilder as BlockBuilderApi;

/// A builder for creating an instance of [`BlockBuilder`].
pub struct BlockBuilderBuilder<B, C> {
	call_api_at: C,
	_phantom: PhantomData<B>,
}

impl<B, C> BlockBuilderBuilder<B, C>
where
	B: BlockT,
{
	/// Create a new instance of the builder.
	///
	/// `call_api_at`: Something that implements [`CallApiAt`].
	pub fn new(call_api_at: C) -> Self {
		Self { call_api_at, _phantom: PhantomData }
	}

	/// Specify the parent block to build on top of.
	pub fn on_parent_block(self, parent_block: B::Hash) -> BlockBuilderBuilderStage1<B, C> {
		BlockBuilderBuilderStage1 { call_api_at: self.call_api_at, parent_block }
	}
}

/// The second stage of the [`BlockBuilderBuilder`].
///
/// This type can not be instantiated directly. To get an instance of it
/// [`BlockBuilderBuilder::new`] needs to be used.
pub struct BlockBuilderBuilderStage1<B: BlockT, C> {
	call_api_at: C,
	parent_block: B::Hash,
}

impl<B, C> BlockBuilderBuilderStage1<B, C>
where
	B: BlockT,
{
	/// Fetch the parent block number from the given `header_backend`.
	///
	/// The parent block number is used to initialize the block number of the new block.
	///
	/// Returns an error if there doesn't exist a number for the parent block specified in
	/// [`on_parent_block`](BlockBuilderBuilder::on_parent_block).
	pub fn fetch_parent_block_number<H: HeaderBackend<B>>(
		self,
		header_backend: &H,
	) -> Result<BlockBuilderBuilderStage2<B, C>, Error> {
		let parent_number = header_backend.number(self.parent_block)?.ok_or_else(|| {
			Error::Backend(format!(
				"Could not fetch block number for block: {:?}",
				self.parent_block
			))
		})?;

		Ok(BlockBuilderBuilderStage2 {
			runtime_instance_builder: RuntimeInstance::builder(self.call_api_at, self.parent_block)
				.on_chain_context(),
			inherent_digests: Default::default(),
			parent_block: self.parent_block,
			parent_number,
		})
	}

	/// Provide the block number for the parent block directly.
	///
	/// The parent block is specified in [`on_parent_block`](BlockBuilderBuilder::on_parent_block).
	/// The parent block number is used to initialize the block number of the new block.
	pub fn with_parent_block_number(
		self,
		parent_number: NumberFor<B>,
	) -> BlockBuilderBuilderStage2<B, C> {
		BlockBuilderBuilderStage2 {
			runtime_instance_builder: RuntimeInstance::builder(self.call_api_at, self.parent_block)
				.on_chain_context(),
			inherent_digests: Default::default(),
			parent_block: self.parent_block,
			parent_number,
		}
	}
}

/// The second stage of the [`BlockBuilderBuilder`].
///
/// This type can not be instantiated directly. To get an instance of it
/// [`BlockBuilderBuilder::new`] needs to be used.
pub struct BlockBuilderBuilderStage2<B: BlockT, C, ProofRecording = DisableProofRecording> {
	runtime_instance_builder: RuntimeInstanceBuilderStage2<C, B, ProofRecording>,
	inherent_digests: Digest,
	parent_block: B::Hash,
	parent_number: NumberFor<B>,
}

impl<B: BlockT, C, ProofRecording: sp_api::ProofRecording<B>>
	BlockBuilderBuilderStage2<B, C, ProofRecording>
{
	/// Enable proof recording for the block builder.
	pub fn enable_proof_recording(
		self,
	) -> BlockBuilderBuilderStage2<B, C, EnableProofRecording<B>> {
		BlockBuilderBuilderStage2 {
			runtime_instance_builder: self.runtime_instance_builder.enable_proof_recording(),
			inherent_digests: self.inherent_digests,
			parent_block: self.parent_block,
			parent_number: self.parent_number,
		}
	}

	pub fn with_proof_recording<WProofRecording: sp_api::ProofRecording<B>>(
		self,
	) -> BlockBuilderBuilderStage2<B, C, WProofRecording> {
		BlockBuilderBuilderStage2 {
			runtime_instance_builder: self
				.runtime_instance_builder
				.with_proof_recording::<WProofRecording>(),
			inherent_digests: self.inherent_digests,
			parent_block: self.parent_block,
			parent_number: self.parent_number,
		}
	}

	/// Build the block with the given inherent digests.
	pub fn with_inherent_digests(mut self, inherent_digests: Digest) -> Self {
		self.inherent_digests = inherent_digests;
		self
	}

	/// Create the instance of the [`BlockBuilder`].
	pub fn build(self) -> Result<BlockBuilder<B, C, ProofRecording>, Error>
	where
		C: CallApiAt<B>,
	{
		BlockBuilder::new(
			self.runtime_instance_builder.build(),
			self.parent_block,
			self.parent_number,
			self.inherent_digests,
		)
	}
}

/// A block that was build by [`BlockBuilder`] plus some additional data.
///
/// This additional data includes the `storage_changes`, these changes can be applied to the
/// backend to get the state of the block. Furthermore an optional `proof` is included which
/// can be used to proof that the build block contains the expected data. The `proof` will
/// only be set when proof recording was activated.
pub struct BuiltBlock<Block: BlockT, Proof> {
	/// The actual block that was build.
	pub block: Block,
	/// The changes that need to be applied to the backend to get the state of the build block.
	pub storage_changes: StorageChanges<Block>,
	/// An optional proof that was recorded while building the block.
	pub proof: Proof,
}

impl<Block: BlockT, Proof> BuiltBlock<Block, Proof> {
	/// Convert into the inner values.
	pub fn into_inner(self) -> (Block, StorageChanges<Block>, Proof) {
		(self.block, self.storage_changes, self.proof)
	}
}

/// Utility for building new (valid) blocks from a stream of extrinsics.
pub struct BlockBuilder<Block: BlockT, CallApiAt, ProofRecording> {
	extrinsics: Vec<Block::Extrinsic>,
<<<<<<< HEAD
	runtime_instance: RuntimeInstance<CallApiAt, Block, ProofRecording>,
||||||| 07e55006ad0
	api: ApiRef<'a, C::Api>,
	call_api_at: &'a C,
=======
	api: ApiRef<'a, C::Api>,
	call_api_at: &'a C,
	/// Version of the [`BlockBuilderApi`] runtime API.
>>>>>>> origin/master
	version: u32,
	/// The estimated size of the block header.
	estimated_header_size: usize,
<<<<<<< HEAD
	_phantom: PhantomData<ProofRecording>,
||||||| 07e55006ad0
=======
	extrinsic_inclusion_mode: ExtrinsicInclusionMode,
>>>>>>> origin/master
}

impl<Block, C, ProofRecorder> BlockBuilder<Block, C, ProofRecorder>
where
	Block: BlockT,
	C: sp_api::CallApiAt<Block>,
	ProofRecorder: sp_api::ProofRecording<Block>,
{
	/// Create a new instance of builder based on the given `parent_hash` and `parent_number`.
	///
	/// While proof recording is enabled, all accessed trie nodes are saved.
	/// These recorded trie nodes can be used by a third party to prove the
	/// output of this block builder without having access to the full storage.
	fn new(
	mut runtime_instance: RuntimeInstance<C, Block, ProofRecorder>,
	parent_hash: Block::Hash,
	parent_number: NumberFor<Block>,
	inherent_digests: Digest,
) -> Result<Self, Error> {
	let header = <<Block as BlockT>::Header as HeaderT>::new(
		parent_number + One::one(),
		Default::default(),
		Default::default(),
		parent_hash,
		inherent_digests,
	);

	let estimated_header_size = header.encoded_size();

	Core::<Block>::initialize_block(&mut runtime_instance, &header)?;

<<<<<<< HEAD
	let version = runtime_instance
		.api_version::<dyn BlockBuilderApi<Block>>()?
||||||| 07e55006ad0
	if record_proof {
		api.record_proof();
		let recorder = api
			.proof_recorder()
			.expect("Proof recording is enabled in the line above; qed.");
		api.register_extension(ProofSizeExt::new(recorder));
	}

	api.set_call_context(CallContext::Onchain);

	api.initialize_block(parent_hash, &header)?;

	let version = api
		.api_version::<dyn BlockBuilderApi<Block>>(parent_hash)?
=======
	if record_proof {
		api.record_proof();
		let recorder = api
			.proof_recorder()
			.expect("Proof recording is enabled in the line above; qed.");
		api.register_extension(ProofSizeExt::new(recorder));
	}

	api.set_call_context(CallContext::Onchain);

	let core_version = api
		.api_version::<dyn Core<Block>>(parent_hash)?
		.ok_or_else(|| Error::VersionInvalid("Core".to_string()))?;

	let extrinsic_inclusion_mode = if core_version >= 5 {
		api.initialize_block(parent_hash, &header)?
	} else {
		#[allow(deprecated)]
		api.initialize_block_before_version_5(parent_hash, &header)?;
		ExtrinsicInclusionMode::AllExtrinsics
	};

	let bb_version = api
		.api_version::<dyn BlockBuilderApi<Block>>(parent_hash)?
>>>>>>> origin/master
		.ok_or_else(|| Error::VersionInvalid("BlockBuilderApi".to_string()))?;

	Ok(BlockBuilder {
		extrinsics: Vec::new(),
<<<<<<< HEAD
		runtime_instance,
		version,
||||||| 07e55006ad0
		api,
		version,
=======
		api,
		version: bb_version,
>>>>>>> origin/master
		estimated_header_size,
<<<<<<< HEAD
		_phantom: PhantomData,
||||||| 07e55006ad0
		call_api_at,
=======
		call_api_at,
		extrinsic_inclusion_mode,
>>>>>>> origin/master
	})
}

	/// The extrinsic inclusion mode of the runtime for this block.
	pub fn extrinsic_inclusion_mode(&self) -> ExtrinsicInclusionMode {
	self.extrinsic_inclusion_mode
}
}

impl<Block, CallApiAt, ProofRecording> BlockBuilder<Block, CallApiAt, ProofRecording>
where
	Block: BlockT,
	CallApiAt: sp_api::CallApiAt<Block>,
	ProofRecording: sp_api::ProofRecording<Block>,
{
	/// Push onto the block's list of extrinsics.
	///
	/// This will ensure the extrinsic can be validly executed (by executing it).
	pub fn push(&mut self, xt: <Block as BlockT>::Extrinsic) -> Result<(), Error> {
		let extrinsics = &mut self.extrinsics;
		let version = self.version;

		self.runtime_instance.execute_in_transaction(|api| {
			let res = if version < 6 {
				#[allow(deprecated)]
				BlockBuilderApi::<Block>::apply_extrinsic_before_version_6(api, xt.clone())
					.map(legacy::byte_sized_error::convert_to_latest)
			} else {
				BlockBuilderApi::<Block>::apply_extrinsic(api, xt.clone())
			};

			match res {
				Ok(Ok(_)) => {
					extrinsics.push(xt);
					TransactionOutcome::Commit(Ok(()))
				},
				Ok(Err(tx_validity)) => TransactionOutcome::Rollback(Err(
					ApplyExtrinsicFailed::Validity(tx_validity).into(),
				)),
				Err(e) => TransactionOutcome::Rollback(Err(Error::from(e))),
			}
		})
	}

	/// Consume the builder to build a valid `Block` containing all pushed extrinsics.
	///
	/// Returns the build `Block`, the changes to the storage and an optional `StorageProof`
	/// supplied by `self.api`, combined as [`BuiltBlock`].
	/// The storage proof will be `Some(_)` when proof recording was enabled.
	pub fn build(mut self) -> Result<BuiltBlock<Block, ProofRecording::Proof>, Error> {
		let header: Block::Header =
			BlockBuilderApi::<Block>::finalize_block(&mut self.runtime_instance)?;

		debug_assert_eq!(
			header.extrinsics_root().clone(),
			HashingFor::<Block>::ordered_trie_root(
				self.extrinsics.iter().map(Encode::encode).collect(),
				self.api.version(self.parent_hash)?.extrinsics_root_state_version(),
			),
		);

		let proof = self.runtime_instance.extract_proof();

		let storage_changes = self.runtime_instance.into_storage_changes()?;

		Ok(BuiltBlock {
			block: <Block as BlockT>::new(header, self.extrinsics),
			storage_changes,
			proof,
		})
	}

	/// Create the inherents for the block.
	///
	/// Returns the inherents created by the runtime or an error if something failed.
	pub fn create_inherents(
		&mut self,
		inherent_data: sp_inherents::InherentData,
	) -> Result<Vec<Block::Extrinsic>, Error> {
		self.runtime_instance
			.execute_in_transaction(move |api| {
				// `create_inherents` should not change any state, to ensure this we always rollback
				// the transaction.
				TransactionOutcome::Rollback(BlockBuilderApi::<Block>::inherent_extrinsics(
					api,
					inherent_data,
				))
			})
			.map_err(Into::into)
	}

	/// Estimate the size of the block in the current state.
	///
	/// If `include_proof` is `true`, the estimated size of the storage proof will be added
	/// to the estimation.
	pub fn estimate_block_size(&self, include_proof: bool) -> usize {
		let size = self.estimated_header_size + self.extrinsics.encoded_size();

		if include_proof {
			// size + self.api.proof_recorder().map(|pr| pr.estimate_encoded_size()).unwrap_or(0)
			size
		} else {
			size
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_blockchain::HeaderBackend;
	use sp_core::Blake2Hasher;
	use sp_state_machine::Backend;
	use substrate_test_runtime_client::{
		runtime::ExtrinsicBuilder, DefaultTestClientBuilderExt, TestClientBuilderExt,
	};

	#[test]
	fn block_building_storage_proof_does_not_include_runtime_by_default() {
		let builder = substrate_test_runtime_client::TestClientBuilder::new();
		let client = builder.build();

		let genesis_hash = client.info().best_hash;

		let block = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap()
			.build()
			.unwrap();

		let proof = block.proof;
		let genesis_state_root = client.header(genesis_hash).unwrap().unwrap().state_root;

		let backend =
			sp_state_machine::create_proof_check_backend::<Blake2Hasher>(genesis_state_root, proof)
				.unwrap();

		assert!(backend
			.storage(&sp_core::storage::well_known_keys::CODE)
			.unwrap_err()
			.contains("Database missing expected key"),);
	}

	#[test]
	fn failing_extrinsic_rolls_back_changes_in_storage_proof() {
		let builder = substrate_test_runtime_client::TestClientBuilder::new();
		let client = builder.build();
		let genesis_hash = client.info().best_hash;

		let mut block_builder = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap();

		block_builder.push(ExtrinsicBuilder::new_read_and_panic(8).build()).unwrap_err();

		let block = block_builder.build().unwrap();

		let proof_with_panic = block.proof.encoded_size();

		let mut block_builder = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap();

		block_builder.push(ExtrinsicBuilder::new_read(8).build()).unwrap();

		let block = block_builder.build().unwrap();

		let proof_without_panic = block.proof.encoded_size();

		let block = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap()
			.build()
			.unwrap();

		let proof_empty_block = block.proof.encoded_size();

		// Ensure that we rolled back the changes of the panicked transaction.
		assert!(proof_without_panic > proof_with_panic);
		assert!(proof_without_panic > proof_empty_block);
		assert_eq!(proof_empty_block, proof_with_panic);
	}
}
