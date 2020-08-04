// Copyright 2019 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]

///! The Cumulus runtime to make a runtime a parachain.
use codec::{Decode, Encode};
use sp_runtime::traits::Block as BlockT;
use sp_std::vec::Vec;
use sp_trie::StorageProof;

#[cfg(not(feature = "std"))]
#[doc(hidden)]
pub use sp_std::slice;

#[macro_use]
pub mod validate_block;

/// The parachain block that is created on a collator and validated by a validator.
#[derive(Encode, Decode)]
pub struct ParachainBlockData<B: BlockT> {
	/// The header of the parachain block.
	header: <B as BlockT>::Header,
	/// The extrinsics of the parachain block without the `PolkadotInherent`.
	extrinsics: Vec<<B as BlockT>::Extrinsic>,
	/// The data that is required to emulate the storage accesses executed by all extrinsics.
	storage_proof: StorageProof,
}

impl<B: BlockT> ParachainBlockData<B> {
	pub fn new(
		header: <B as BlockT>::Header,
		extrinsics: Vec<<B as BlockT>::Extrinsic>,
		storage_proof: StorageProof,
	) -> Self {
		Self {
			header,
			extrinsics,
			storage_proof,
		}
	}

	/// Convert `self` into the stored header.
	pub fn into_header(self) -> B::Header {
		self.header
	}

	/// Returns the header.
	pub fn header(&self) -> &B::Header {
		&self.header
	}

	/// Returns the extrinsics.
	pub fn extrinsics(&self) -> &[B::Extrinsic] {
		&self.extrinsics
	}
}
