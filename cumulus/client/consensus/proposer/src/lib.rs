// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! The Cumulus [`Proposer`] is a wrapper around a Substrate [`sp_consensus::Environment`]
//! for creating new parachain blocks.
//!
//! This utility is designed to be composed within any collator consensus algorithm.

use async_trait::async_trait;

use cumulus_primitives_parachain_inherent::ParachainInherentData;
use sp_consensus::{EnableProofRecording, Environment, Proposal, Proposer as SubstrateProposer};
use sp_inherents::InherentData;
use sp_runtime::{traits::Block as BlockT, Digest};
use sp_state_machine::StorageProof;

use std::{fmt::Debug, time::Duration};

/// Errors that can occur when proposing a parachain block.
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct Error {
	inner: anyhow::Error,
}

impl Error {
	/// Create an error tied to the creation of a proposer.
	pub fn proposer_creation(err: impl Into<anyhow::Error>) -> Self {
		Error { inner: err.into().context("Proposer Creation") }
	}

	/// Create an error tied to the proposing logic itself.
	pub fn proposing(err: impl Into<anyhow::Error>) -> Self {
		Error { inner: err.into().context("Proposing") }
	}
}

/// A type alias for easily referring to the type of a proposal produced by a specific
/// [`Proposer`].
pub type ProposalOf<B> = Proposal<B, StorageProof>;

/// An interface for proposers.
#[async_trait]
pub trait ProposerInterface<Block: BlockT> {
	/// Propose a collation using the supplied `InherentData` and the provided
	/// `ParachainInherentData`.
	///
	/// Also specify any required inherent digests, the maximum proposal duration,
	/// and the block size limit in bytes. See the documentation on
	/// [`sp_consensus::Proposer::propose`] for more details on how to interpret these parameters.
	///
	/// The `InherentData` and `Digest` are left deliberately general in order to accommodate
	/// all possible collator selection algorithms or inherent creation mechanisms,
	/// while the `ParachainInherentData` is made explicit so it will be constructed appropriately.
	///
	/// If the `InherentData` passed into this function already has a `ParachainInherentData`,
	/// this should throw an error.
	async fn propose(
		&mut self,
		parent_header: &Block::Header,
		paras_inherent_data: &ParachainInherentData,
		other_inherent_data: InherentData,
		inherent_digests: Digest,
		max_duration: Duration,
		block_size_limit: Option<usize>,
	) -> Result<Option<Proposal<Block, StorageProof>>, Error>;
}

/// A simple wrapper around a Substrate proposer for creating collations.
pub struct Proposer<B, T> {
	inner: T,
	_marker: std::marker::PhantomData<B>,
}

impl<B, T> Proposer<B, T> {
	/// Create a new Cumulus [`Proposer`].
	pub fn new(inner: T) -> Self {
		Proposer { inner, _marker: std::marker::PhantomData }
	}
}

#[async_trait]
impl<B, T> ProposerInterface<B> for Proposer<B, T>
where
	B: sp_runtime::traits::Block,
	T: Environment<B> + Send,
	T::Error: Send + Sync + 'static,
	T::Proposer: SubstrateProposer<B, ProofRecording = EnableProofRecording, Proof = StorageProof>,
	<T::Proposer as SubstrateProposer<B>>::Error: Send + Sync + 'static,
{
	async fn propose(
		&mut self,
		parent_header: &B::Header,
		paras_inherent_data: &ParachainInherentData,
		other_inherent_data: InherentData,
		inherent_digests: Digest,
		max_duration: Duration,
		block_size_limit: Option<usize>,
	) -> Result<Option<Proposal<B, StorageProof>>, Error> {
		let proposer = self
			.inner
			.init(parent_header)
			.await
			.map_err(|e| Error::proposer_creation(anyhow::Error::new(e)))?;

		let mut inherent_data = other_inherent_data;
		inherent_data
			.put_data(
				cumulus_primitives_parachain_inherent::INHERENT_IDENTIFIER,
				&paras_inherent_data,
			)
			.map_err(|e| Error::proposing(anyhow::Error::new(e)))?;

		proposer
			.propose(inherent_data, inherent_digests, max_duration, block_size_limit)
			.await
			.map(Some)
			.map_err(|e| Error::proposing(anyhow::Error::new(e)).into())
	}
}
