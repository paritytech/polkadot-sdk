// Copyright 2018 Parity Technologies (UK) Ltd.
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

extern crate substrate_primitives as primitives;
extern crate futures;
extern crate parking_lot;
extern crate sr_version as runtime_version;
extern crate sr_primitives as runtime_primitives;
#[cfg(any(test, feature = "test-helpers"))]
extern crate substrate_test_client as test_client;
extern crate tokio;

extern crate parity_codec as codec;
extern crate parity_codec_derive;

#[macro_use]
extern crate error_chain;
#[macro_use] extern crate log;

use std::sync::Arc;

use runtime_primitives::generic::BlockId;
use runtime_primitives::traits::{AuthorityIdFor, Block};
use futures::prelude::*;

pub mod offline_tracker;
pub mod error;
mod block_import;
pub mod import_queue;
pub mod evaluation;

// block size limit.
const MAX_TRANSACTIONS_SIZE: usize = 4 * 1024 * 1024;

pub use self::error::{Error, ErrorKind};
pub use block_import::{BlockImport, ImportBlock, BlockOrigin, ImportResult, ForkChoiceStrategy};

/// Trait for getting the authorities at a given block.
pub trait Authorities<B: Block> {
	type Error: ::std::error::Error + Send + 'static;	/// Get the authorities at the given block.
	fn authorities(&self, at: &BlockId<B>) -> Result<Vec<AuthorityIdFor<B>>, Self::Error>;
}

/// Environment producer for a Consensus instance. Creates proposer instance and communication streams.
pub trait Environment<B: Block, ConsensusData> {
	/// The proposer type this creates.
	type Proposer: Proposer<B, ConsensusData>;
	/// Error which can occur upon creation.
	type Error: From<Error>;

	/// Initialize the proposal logic on top of a specific header. Provide
	/// the authorities at that header.
	fn init(&self, parent_header: &B::Header, authorities: &[AuthorityIdFor<B>])
		-> Result<Self::Proposer, Self::Error>;
}

/// Logic for a proposer.
///
/// This will encapsulate creation and evaluation of proposals at a specific
/// block.
///
/// Proposers are generic over bits of "consensus data" which are engine-specific.
pub trait Proposer<B: Block, ConsensusData> {
	/// Error type which can occur when proposing or evaluating.
	type Error: From<Error> + ::std::fmt::Debug + 'static;
	/// Future that resolves to a committed proposal.
	type Create: IntoFuture<Item=B,Error=Self::Error>;
	/// Create a proposal.
	fn propose(&self, consensus_data: ConsensusData) -> Self::Create;
}

/// An oracle for when major synchronization work is being undertaken.
///
/// Generally, consensus authoring work isn't undertaken while well behind
/// the head of the chain.
pub trait SyncOracle {
	/// Whether the synchronization service is undergoing major sync.
	/// Returns true if so.
	fn is_major_syncing(&self) -> bool;
}

/// A synchronization oracle for when there is no network.
#[derive(Clone, Copy, Debug)]
pub struct NoNetwork;

impl SyncOracle for NoNetwork {
	fn is_major_syncing(&self) -> bool { false }
}

impl<T: SyncOracle> SyncOracle for Arc<T> {
	fn is_major_syncing(&self) -> bool {
		T::is_major_syncing(&*self)
	}
}
