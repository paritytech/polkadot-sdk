// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate Consensus Common.

// Substrate Demo is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate Consensus Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate Consensus Common.  If not, see <http://www.gnu.org/licenses/>.

//! Common utilities for building and using consensus engines in substrate.
//!
//! Much of this crate is _unstable_ and thus the API is likely to undergo
//! change. Implementors of traits should not rely on the interfaces to remain
//! the same.

// This provides "unused" building blocks to other crates
#![allow(dead_code)]

// our error-chain could potentially blow up otherwise
#![recursion_limit="128"]

#[macro_use] extern crate log;

use std::sync::Arc;
use std::time::Duration;

use sr_primitives::traits::{Block as BlockT, DigestFor};
use futures::prelude::*;
pub use inherents::InherentData;

pub mod block_validation;
pub mod offline_tracker;
pub mod error;
pub mod block_import;
mod select_chain;
pub mod import_queue;
pub mod evaluation;

// block size limit.
const MAX_BLOCK_SIZE: usize = 4 * 1024 * 1024 + 512;

pub use self::error::Error;
pub use block_import::{
	BlockImport, BlockOrigin, ForkChoiceStrategy, ImportedAux, BlockImportParams, BlockCheckParams, ImportResult,
	JustificationImport, FinalityProofImport,
};
pub use select_chain::SelectChain;

/// Block status.
#[derive(Debug, PartialEq, Eq)]
pub enum BlockStatus {
	/// Added to the import queue.
	Queued,
	/// Already in the blockchain and the state is available.
	InChainWithState,
	/// In the blockchain, but the state is not available.
	InChainPruned,
	/// Block or parent is known to be bad.
	KnownBad,
	/// Not in the queue or the blockchain.
	Unknown,
}

/// Environment producer for a Consensus instance. Creates proposer instance and communication streams.
pub trait Environment<B: BlockT> {
	/// The proposer type this creates.
	type Proposer: Proposer<B>;
	/// Error which can occur upon creation.
	type Error: From<Error>;

	/// Initialize the proposal logic on top of a specific header. Provide
	/// the authorities at that header.
	fn init(&mut self, parent_header: &B::Header)
		-> Result<Self::Proposer, Self::Error>;
}

/// Logic for a proposer.
///
/// This will encapsulate creation and evaluation of proposals at a specific
/// block.
///
/// Proposers are generic over bits of "consensus data" which are engine-specific.
pub trait Proposer<B: BlockT> {
	/// Error type which can occur when proposing or evaluating.
	type Error: From<Error> + ::std::fmt::Debug + 'static;
	/// Future that resolves to a committed proposal.
	type Create: Future<Output = Result<B, Self::Error>>;
	/// Create a proposal.
	fn propose(
		&mut self,
		inherent_data: InherentData,
		inherent_digests: DigestFor<B>,
		max_duration: Duration,
	) -> Self::Create;
}

/// An oracle for when major synchronization work is being undertaken.
///
/// Generally, consensus authoring work isn't undertaken while well behind
/// the head of the chain.
pub trait SyncOracle {
	/// Whether the synchronization service is undergoing major sync.
	/// Returns true if so.
	fn is_major_syncing(&mut self) -> bool;
	/// Whether the synchronization service is offline.
	/// Returns true if so.
	fn is_offline(&mut self) -> bool;
}

/// A synchronization oracle for when there is no network.
#[derive(Clone, Copy, Debug)]
pub struct NoNetwork;

impl SyncOracle for NoNetwork {
	fn is_major_syncing(&mut self) -> bool { false }
	fn is_offline(&mut self) -> bool { false }
}

impl<T> SyncOracle for Arc<T>
where T: ?Sized, for<'r> &'r T: SyncOracle
{
	fn is_major_syncing(&mut self) -> bool {
		<&T>::is_major_syncing(&mut &**self)
	}
	fn is_offline(&mut self) -> bool {
		<&T>::is_offline(&mut &**self)
	}
}
