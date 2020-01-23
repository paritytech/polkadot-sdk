// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Block import helpers.

use sp_runtime::{
	Justification,
	traits::{Block as BlockT, DigestItemFor, Header as HeaderT, NumberFor, HasherFor},
};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::import_queue::{Verifier, CacheKeyId};

/// Block import result.
#[derive(Debug, PartialEq, Eq)]
pub enum ImportResult {
	/// Block imported.
	Imported(ImportedAux),
	/// Already in the blockchain.
	AlreadyInChain,
	/// Block or parent is known to be bad.
	KnownBad,
	/// Block parent is not in the chain.
	UnknownParent,
	/// Parent state is missing.
	MissingState,
}

/// Auxiliary data associated with an imported block result.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportedAux {
	/// Only the header has been imported. Block body verification was skipped.
	pub header_only: bool,
	/// Clear all pending justification requests.
	pub clear_justification_requests: bool,
	/// Request a justification for the given block.
	pub needs_justification: bool,
	/// Received a bad justification.
	pub bad_justification: bool,
	/// Request a finality proof for the given block.
	pub needs_finality_proof: bool,
	/// Whether the block that was imported is the new best block.
	pub is_new_best: bool,
}

impl ImportResult {
	/// Returns default value for `ImportResult::Imported` with
	/// `clear_justification_requests`, `needs_justification`,
	/// `bad_justification` and `needs_finality_proof` set to false.
	pub fn imported(is_new_best: bool) -> ImportResult {
		let mut aux = ImportedAux::default();
		aux.is_new_best = is_new_best;

		ImportResult::Imported(aux)
	}
}

/// Block data origin.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BlockOrigin {
	/// Genesis block built into the client.
	Genesis,
	/// Block is part of the initial sync with the network.
	NetworkInitialSync,
	/// Block was broadcasted on the network.
	NetworkBroadcast,
	/// Block that was received from the network and validated in the consensus process.
	ConsensusBroadcast,
	/// Block that was collated by this node.
	Own,
	/// Block was imported from a file.
	File,
}

/// Fork choice strategy.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ForkChoiceStrategy {
	/// Longest chain fork choice.
	LongestChain,
	/// Custom fork choice rule, where true indicates the new block should be the best block.
	Custom(bool),
}

/// Data required to check validity of a Block.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BlockCheckParams<Block: BlockT> {
	/// Hash of the block that we verify.
	pub hash: Block::Hash,
	/// Block number of the block that we verify.
	pub number: NumberFor<Block>,
	/// Parent hash of the block that we verify.
	pub parent_hash: Block::Hash,
	/// Allow importing the block skipping state verification if parent state is missing.
	pub allow_missing_state: bool,
	/// Re-validate existing block.
	pub import_existing: bool,
}

/// Data required to import a Block.
pub struct BlockImportParams<Block: BlockT, Transaction> {
	/// Origin of the Block
	pub origin: BlockOrigin,
	/// The header, without consensus post-digests applied. This should be in the same
	/// state as it comes out of the runtime.
	///
	/// Consensus engines which alter the header (by adding post-runtime digests)
	/// should strip those off in the initial verification process and pass them
	/// via the `post_digests` field. During block authorship, they should
	/// not be pushed to the header directly.
	///
	/// The reason for this distinction is so the header can be directly
	/// re-executed in a runtime that checks digest equivalence -- the
	/// post-runtime digests are pushed back on after.
	pub header: Block::Header,
	/// Justification provided for this block from the outside.
	pub justification: Option<Justification>,
	/// Digest items that have been added after the runtime for external
	/// work, like a consensus signature.
	pub post_digests: Vec<DigestItemFor<Block>>,
	/// The body of the block.
	pub body: Option<Vec<Block::Extrinsic>>,
	/// The changes to the storage to create the state for the block. If this is `Some(_)`,
	/// the block import will not need to re-execute the block for importing it.
	pub storage_changes: Option<
		sp_state_machine::StorageChanges<Transaction, HasherFor<Block>, NumberFor<Block>>
	>,
	/// Is this block finalized already?
	/// `true` implies instant finality.
	pub finalized: bool,
	/// Intermediate values that are interpreted by block importers. Each block importer,
	/// upon handling a value, removes it from the intermediate list. The final block importer
	/// rejects block import if there are still intermediate values that remain unhandled.
	pub intermediates: HashMap<Vec<u8>, Vec<u8>>,
	/// Auxiliary consensus data produced by the block.
	/// Contains a list of key-value pairs. If values are `None`, the keys
	/// will be deleted.
	pub auxiliary: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	/// Fork choice strategy of this import. This should only be set by a
	/// synchronous import, otherwise it may race against other imports.
	/// `None` indicates that the current verifier or importer cannot yet
	/// determine the fork choice value, and it expects subsequent importer
	/// to modify it. If `None` is passed all the way down to bottom block
	/// importer, the import fails with an `IncompletePipeline` error.
	pub fork_choice: Option<ForkChoiceStrategy>,
	/// Allow importing the block skipping state verification if parent state is missing.
	pub allow_missing_state: bool,
	/// Re-validate existing block.
	pub import_existing: bool,
}

impl<Block: BlockT, Transaction> BlockImportParams<Block, Transaction> {
	/// Deconstruct the justified header into parts.
	pub fn into_inner(self)
		-> (
			BlockOrigin,
			<Block as BlockT>::Header,
			Option<Justification>,
			Vec<DigestItemFor<Block>>,
			Option<Vec<Block::Extrinsic>>,
			Option<sp_state_machine::StorageChanges<Transaction, HasherFor<Block>, NumberFor<Block>>>,
			bool,
			Vec<(Vec<u8>, Option<Vec<u8>>)>,
		) {
		(
			self.origin,
			self.header,
			self.justification,
			self.post_digests,
			self.body,
			self.storage_changes,
			self.finalized,
			self.auxiliary,
		)
	}

	/// Get a handle to full header (with post-digests applied).
	pub fn post_header(&self) -> Cow<Block::Header> {
		if self.post_digests.is_empty() {
			Cow::Borrowed(&self.header)
		} else {
			Cow::Owned({
				let mut hdr = self.header.clone();
				for digest_item in &self.post_digests {
					hdr.digest_mut().push(digest_item.clone());
				}

				hdr
			})
		}
	}

	/// Auxiliary function for "converting" the transaction type.
	///
	/// Actually this just sets `storage_changes` to `None` and makes rustc think that `Self` now
	/// uses a different transaction type.
	pub fn convert_transaction<Transaction2>(self) -> BlockImportParams<Block, Transaction2> {
		BlockImportParams {
			origin: self.origin,
			header: self.header,
			justification: self.justification,
			post_digests: self.post_digests,
			body: self.body,
			storage_changes: None,
			finalized: self.finalized,
			auxiliary: self.auxiliary,
			intermediates: self.intermediates,
			allow_missing_state: self.allow_missing_state,
			fork_choice: self.fork_choice,
			import_existing: self.import_existing,
		}
	}
}

/// Block import trait.
pub trait BlockImport<B: BlockT> {
	/// The error type.
	type Error: std::error::Error + Send + 'static;
	/// The transaction type used by the backend.
	type Transaction;

	/// Check block preconditions.
	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error>;

	/// Import a block.
	///
	/// Cached data can be accessed through the blockchain cache.
	fn import_block(
		&mut self,
		block: BlockImportParams<B, Self::Transaction>,
		cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error>;
}

impl<B: BlockT, Transaction> BlockImport<B> for crate::import_queue::BoxBlockImport<B, Transaction> {
	type Error = crate::error::Error;
	type Transaction = Transaction;

	/// Check block preconditions.
	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		(**self).check_block(block)
	}

	/// Import a block.
	///
	/// Cached data can be accessed through the blockchain cache.
	fn import_block(
		&mut self,
		block: BlockImportParams<B, Transaction>,
		cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		(**self).import_block(block, cache)
	}
}

impl<B: BlockT, T, E: std::error::Error + Send + 'static, Transaction> BlockImport<B> for Arc<T>
	where for<'r> &'r T: BlockImport<B, Error = E, Transaction = Transaction>
{
	type Error = E;
	type Transaction = Transaction;

	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		(&**self).check_block(block)
	}

	fn import_block(
		&mut self,
		block: BlockImportParams<B, Transaction>,
		cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		(&**self).import_block(block, cache)
	}
}

/// Justification import trait
pub trait JustificationImport<B: BlockT> {
	type Error: std::error::Error + Send + 'static;

	/// Called by the import queue when it is started. Returns a list of justifications to request
	/// from the network.
	fn on_start(&mut self) -> Vec<(B::Hash, NumberFor<B>)> { Vec::new() }

	/// Import a Block justification and finalize the given block.
	fn import_justification(
		&mut self,
		hash: B::Hash,
		number: NumberFor<B>,
		justification: Justification,
	) -> Result<(), Self::Error>;
}

/// Finality proof import trait.
pub trait FinalityProofImport<B: BlockT> {
	type Error: std::error::Error + Send + 'static;

	/// Called by the import queue when it is started. Returns a list of finality proofs to request
	/// from the network.
	fn on_start(&mut self) -> Vec<(B::Hash, NumberFor<B>)> { Vec::new() }

	/// Import a Block justification and finalize the given block. Returns finalized block or error.
	fn import_finality_proof(
		&mut self,
		hash: B::Hash,
		number: NumberFor<B>,
		finality_proof: Vec<u8>,
		verifier: &mut dyn Verifier<B>,
	) -> Result<(B::Hash, NumberFor<B>), Self::Error>;
}
