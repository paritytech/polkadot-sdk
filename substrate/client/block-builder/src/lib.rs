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
	ApiExt, ApiRef, CallApiAt, Core, ProofRecorder, ProvideRuntimeApi, StorageChanges,
	StorageProof, TransactionOutcome,
};
use sp_blockchain::{ApplyExtrinsicFailed, Error, HeaderBackend};
use sp_core::traits::CallContext;
use sp_runtime::{
	legacy,
	traits::{Block as BlockT, Hash, HashingFor, Header as HeaderT, NumberFor, One},
	Digest, ExtrinsicInclusionMode,
};
use sp_state_machine::{backend::AsStateBackend, state_backend::BackendType};
use std::marker::PhantomData;

pub use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_trie::proof_size_extension::ProofSizeExt;

/// A builder for creating an instance of [`BlockBuilder`].
pub struct BlockBuilderBuilder<'a, B, C> {
	call_api_at: &'a C,
	_phantom: PhantomData<B>,
}

impl<'a, B, C> BlockBuilderBuilder<'a, B, C>
where
	B: BlockT,
{
	/// Create a new instance of the builder.
	///
	/// `call_api_at`: Something that implements [`CallApiAt`].
	pub fn new(call_api_at: &'a C) -> Self {
		Self { call_api_at, _phantom: PhantomData }
	}

	/// Specify the parent block to build on top of.
	pub fn on_parent_block(self, parent_block: B::Hash) -> BlockBuilderBuilderStage1<'a, B, C> {
		BlockBuilderBuilderStage1 { call_api_at: self.call_api_at, parent_block }
	}
}

/// The second stage of the [`BlockBuilderBuilder`].
///
/// This type can not be instantiated directly. To get an instance of it
/// [`BlockBuilderBuilder::new`] needs to be used.
pub struct BlockBuilderBuilderStage1<'a, B: BlockT, C> {
	call_api_at: &'a C,
	parent_block: B::Hash,
}

impl<'a, B, C> BlockBuilderBuilderStage1<'a, B, C>
where
	B: BlockT,
{
	/// Fetch the parent block number from the given `header_backend`.
	///
	/// The parent block number is used to initialize the block number of the new block.
	///
	/// Returns an error if the parent block specified in
	/// [`on_parent_block`](BlockBuilderBuilder::on_parent_block) does not exist.
	pub fn fetch_parent_block_number<H: HeaderBackend<B>>(
		self,
		header_backend: &H,
	) -> Result<BlockBuilderBuilderStage2<'a, B, C>, Error> {
		let parent_number = header_backend.number(self.parent_block)?.ok_or_else(|| {
			Error::Backend(format!(
				"Could not fetch block number for block: {:?}",
				self.parent_block
			))
		})?;

		Ok(BlockBuilderBuilderStage2 {
			call_api_at: self.call_api_at,
			proof_recorder: None,
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
	) -> BlockBuilderBuilderStage2<'a, B, C> {
		BlockBuilderBuilderStage2 {
			call_api_at: self.call_api_at,
			proof_recorder: None,
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
pub struct BlockBuilderBuilderStage2<'a, B: BlockT, C> {
	call_api_at: &'a C,
	proof_recorder: Option<ProofRecorder<B>>,
	inherent_digests: Digest,
	parent_block: B::Hash,
	parent_number: NumberFor<B>,
}

impl<'a, B: BlockT, C> BlockBuilderBuilderStage2<'a, B, C> {
	/// Enable proof recording for the block builder.
	pub fn enable_proof_recording(mut self) -> Self
	where
		C: CallApiAt<B>,
	{
		self.proof_recorder = Some(ProofRecorder::<B>::new(self.call_api_at.backend_type()));
		self
	}

	/// Enable/disable proof recording for the block builder.
	pub fn with_proof_recording(mut self, enable: bool) -> Self
	where
		C: CallApiAt<B>,
	{
		self.proof_recorder =
			enable.then(|| ProofRecorder::<B>::new(self.call_api_at.backend_type()));
		self
	}

	/// Enable/disable proof recording for the block builder using the given proof recorder.
	pub fn with_proof_recorder(mut self, proof_recorder: Option<ProofRecorder<B>>) -> Self {
		self.proof_recorder = proof_recorder;
		self
	}

	/// Build the block with the given inherent digests.
	pub fn with_inherent_digests(mut self, inherent_digests: Digest) -> Self {
		self.inherent_digests = inherent_digests;
		self
	}

	/// Create the instance of the [`BlockBuilder`].
	pub fn build(self) -> Result<BlockBuilder<'a, B, C>, Error>
	where
		C: CallApiAt<B> + ProvideRuntimeApi<B>,
		C::Api: BlockBuilderApi<B>,
	{
		BlockBuilder::new(
			self.call_api_at,
			self.parent_block,
			self.parent_number,
			self.proof_recorder,
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
pub struct BuiltBlock<Block: BlockT> {
	/// The actual block that was build.
	pub block: Block,
	/// The changes that need to be applied to the backend to get the state of the build block.
	pub storage_changes: StorageChanges<Block>,
	/// An optional proof that was recorded while building the block.
	pub proof: Option<StorageProof>,
}

impl<Block: BlockT> BuiltBlock<Block> {
	/// Convert into the inner values.
	pub fn into_inner(self) -> (Block, StorageChanges<Block>, Option<StorageProof>) {
		(self.block, self.storage_changes, self.proof)
	}
}

/// Utility for building new (valid) blocks from a stream of extrinsics.
pub struct BlockBuilder<'a, Block: BlockT, C: ProvideRuntimeApi<Block> + 'a> {
	extrinsics: Vec<Block::Extrinsic>,
	api: ApiRef<'a, C::Api>,
	call_api_at: &'a C,
	/// Version of the [`BlockBuilderApi`] runtime API.
	version: u32,
	parent_hash: Block::Hash,
	/// The estimated size of the block header.
	estimated_header_size: usize,
	extrinsic_inclusion_mode: ExtrinsicInclusionMode,
}

impl<'a, Block, C> BlockBuilder<'a, Block, C>
where
	Block: BlockT,
	C: CallApiAt<Block> + ProvideRuntimeApi<Block> + 'a,
	C::Api: BlockBuilderApi<Block>,
{
	/// Create a new instance of builder based on the given `parent_hash` and `parent_number`.
	///
	/// While proof recording is enabled, all accessed trie nodes are saved.
	/// These recorded trie nodes can be used by a third party to prove the
	/// output of this block builder without having access to the full storage.
	fn new(
		call_api_at: &'a C,
		parent_hash: Block::Hash,
		parent_number: NumberFor<Block>,
		proof_recorder: Option<ProofRecorder<Block>>,
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

		let mut api = call_api_at.runtime_api();

		if let Some(recorder) = proof_recorder {
			api.record_proof_with_recorder(recorder.clone());
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
			.ok_or_else(|| Error::VersionInvalid("BlockBuilderApi".to_string()))?;

		Ok(Self {
			parent_hash,
			extrinsics: Vec::new(),
			api,
			version: bb_version,
			estimated_header_size,
			call_api_at,
			extrinsic_inclusion_mode,
		})
	}

	/// The extrinsic inclusion mode of the runtime for this block.
	pub fn extrinsic_inclusion_mode(&self) -> ExtrinsicInclusionMode {
		self.extrinsic_inclusion_mode
	}

	/// Push onto the block's list of extrinsics.
	///
	/// This will ensure the extrinsic can be validly executed (by executing it).
	pub fn push(&mut self, xt: <Block as BlockT>::Extrinsic) -> Result<(), Error> {
		let parent_hash = self.parent_hash;
		let extrinsics = &mut self.extrinsics;
		let version = self.version;

		self.api.execute_in_transaction(|api| {
			let res = if version < 6 {
				#[allow(deprecated)]
				api.apply_extrinsic_before_version_6(parent_hash, xt.clone())
					.map(legacy::byte_sized_error::convert_to_latest)
			} else {
				api.apply_extrinsic(parent_hash, xt.clone())
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
	pub fn build(mut self) -> Result<BuiltBlock<Block>, Error> {
		let header = self.api.finalize_block(self.parent_hash)?;

		debug_assert_eq!(
			header.extrinsics_root().clone(),
			HashingFor::<Block>::ordered_trie_root(
				self.extrinsics.iter().map(Encode::encode).collect(),
				self.api.version(self.parent_hash)?.extrinsics_root_state_version(),
			),
		);

		// TODO: if new code works remove this one.
		// NOTE: trie and nomt proof extraction requires a different order,
		// possibly the trie version could be adapted to nomt requirements.
		// For now just follow two different code path.
		// OLD
		// let (storage_changes, proof) = match self.call_api_at.backend_type() {
		// 	BackendType::Trie => {
		// 		let proof = self.api.extract_proof();
		// 		let state = self.call_api_at.state_at(self.parent_hash)?;
		// 		let storage_changes = self
		// 			.api
		// 			.into_storage_changes(&state, self.parent_hash)
		// 			.map_err(sp_blockchain::Error::StorageChanges)?;
		// 		(storage_changes, proof)
		// 	},
		// 	BackendType::Nomt => {
		// 		// TODO: proof extraction should stay here (order unchanged)
		// 		let proof = self.api.extract_proof();

		// 		let state = self.call_api_at.state_at(self.parent_hash)?;

		// 		// NOTE: right now this seems to be the easiest way to know
		// 		// if proof recording is enabled. If so, toggle the nomt state backend
		// 		// to effectively produce a witness when 'into_storage_changes' is called.
		// 		if let Some(proof_recorder) = self.api.proof_recorder() {
		// 			let nomt_state_backend = state.as_state_backend();
		// 			nomt_state_backend.inject_nomt_recorder(proof_recorder.as_nomt_recorder())
		// 		}

		// 		let storage_changes = self
		// 			.api
		// 			.into_storage_changes(&state, self.parent_hash)
		// 			.map_err(sp_blockchain::Error::StorageChanges)?;

		// 		// let proof = storage_changes.transaction.inner -> NomtBackendTransaction contains
		// 		// the witness

		// 		// TODO: or should it be moved here?
		// 		// let proof = self.api.extract_proof();

		// 		(storage_changes, proof)
		// 	},
		// };

		let proof = self.api.extract_proof();

		let state = self.call_api_at.state_at(self.parent_hash)?;

		// NOTE: This function is expected to find an already computed storage change
		// due to the previous `api.finalize_block` call which internally calls
		// the host function `storage_root`.
		//
		// If something will change in the future than there is the need to inject
		// the proof recorder within nomt because `call_api_at.state_at` doesn't do
		// it on its own.
		let storage_changes = self
			.api
			.into_storage_changes(&state, self.parent_hash)
			.map_err(sp_blockchain::Error::StorageChanges)?;

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
		let parent_hash = self.parent_hash;
		self.api
			.execute_in_transaction(move |api| {
				// `create_inherents` should not change any state, to ensure this we always rollback
				// the transaction.
				TransactionOutcome::Rollback(api.inherent_extrinsics(parent_hash, inherent_data))
			})
			.map_err(|e| Error::Application(Box::new(e)))
	}

	/// Estimate the size of the block in the current state.
	///
	/// If `include_proof` is `true`, the estimated size of the storage proof will be added
	/// to the estimation.
	pub fn estimate_block_size(&self, include_proof: bool) -> usize {
		let size = self.estimated_header_size + self.extrinsics.encoded_size();

		// NOTE: This is not that straightforward.
		// This is a call to `estimate_encoded_size` made from the client, thus something
		// that does not have to be registered. How does this happen if the
		// proof recorder implements `sp_trie::ProofSizeProvider`, which estimates and records
		// the estimation?
		// The same struct implements the *same* method, just not under the trait, and thus
		// if the trait is not imported in this module, the method of the struct is being called
		// and not the one implemented within the trait.
		// If the trait were imported within this module, `use sp_trie::ProofSizeProvider`,
		// then the implementation which records the estimation would be used.
		if include_proof {
			size + self.api.proof_recorder().map(|pr| pr.estimate_encoded_size()).unwrap_or(0)
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

		let proof = block.proof.expect("Proof is build on request");
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

		let proof_with_panic = block.proof.expect("Proof is build on request").encoded_size();

		let mut block_builder = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap();

		block_builder.push(ExtrinsicBuilder::new_read(8).build()).unwrap();

		let block = block_builder.build().unwrap();

		let proof_without_panic = block.proof.expect("Proof is build on request").encoded_size();

		let block = BlockBuilderBuilder::new(&client)
			.on_parent_block(genesis_hash)
			.with_parent_block_number(0)
			.enable_proof_recording()
			.build()
			.unwrap()
			.build()
			.unwrap();

		let proof_empty_block = block.proof.expect("Proof is build on request").encoded_size();

		// Ensure that we rolled back the changes of the panicked transaction.
		assert!(proof_without_panic > proof_with_panic);
		assert!(proof_without_panic > proof_empty_block);
		assert_eq!(proof_empty_block, proof_with_panic);
	}
}
