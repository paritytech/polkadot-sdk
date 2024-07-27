// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Tools for parachain head proof verification.

use crate::{Config, GrandpaPalletOf, RelayBlockHash, RelayBlockHasher};
use bp_header_chain::{HeaderChain, HeaderChainError};
use bp_parachains::parachain_head_storage_key_at_source;
use bp_polkadot_core::parachains::{ParaHead, ParaId};
use bp_runtime::{RawStorageProof, StorageProofChecker, StorageProofError};
use codec::Decode;
use frame_support::traits::Get;

/// Abstraction over storage proof manipulation, hiding implementation details of actual storage
/// proofs.
pub trait StorageProofAdapter<T: Config<I>, I: 'static> {
	/// Read and decode optional value from the proof.
	fn read_and_decode_optional_value<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<Option<D>, StorageProofError>;

	/// Checks if each key was read.
	fn ensure_no_unused_keys(self) -> Result<(), StorageProofError>;

	/// Read parachain head from storage proof.
	fn read_parachain_head(
		&mut self,
		parachain: ParaId,
	) -> Result<Option<ParaHead>, StorageProofError> {
		let parachain_head_key =
			parachain_head_storage_key_at_source(T::ParasPalletName::get(), parachain);
		self.read_and_decode_optional_value(&parachain_head_key)
	}
}

/// Actual storage proof adapter for parachain proofs.
pub type ParachainsStorageProofAdapter<T, I> = RawStorageProofAdapter<T, I>;

/// A `StorageProofAdapter` implementation for raw storage proofs.
pub struct RawStorageProofAdapter<T: Config<I>, I: 'static> {
	storage: StorageProofChecker<RelayBlockHasher>,
	_dummy: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> RawStorageProofAdapter<T, I> {
	/// Try to create a new instance of `RawStorageProofAdapter`.
	pub fn try_new_with_verified_storage_proof(
		relay_block_hash: RelayBlockHash,
		storage_proof: RawStorageProof,
	) -> Result<Self, HeaderChainError> {
		GrandpaPalletOf::<T, I>::verify_storage_proof(relay_block_hash, storage_proof)
			.map(|storage| RawStorageProofAdapter::<T, I> { storage, _dummy: Default::default() })
	}
}

impl<T: Config<I>, I: 'static> StorageProofAdapter<T, I> for RawStorageProofAdapter<T, I> {
	fn read_and_decode_optional_value<D: Decode>(
		&mut self,
		key: &impl AsRef<[u8]>,
	) -> Result<Option<D>, StorageProofError> {
		self.storage.read_and_decode_opt_value(key.as_ref())
	}

	fn ensure_no_unused_keys(self) -> Result<(), StorageProofError> {
		self.storage.ensure_no_unused_nodes()
	}
}
