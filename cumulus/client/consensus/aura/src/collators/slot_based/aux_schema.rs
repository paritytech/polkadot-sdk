// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use codec::{Decode, Encode};
use sc_client_api::backend::AuxStore;
use sp_blockchain::{Error as ClientError, Result as ClientResult};

const STORAGE_PROOF_RECORDING_VERSION: &[u8] = b"cumulus_aura_storage_proof_recording_version";
const STORAGE_PROOF_RECORDING_CURRENT_VERSION: u32 = 1;

/// The aux storage key used to store the storage proof size recordings for the given block hash.
pub fn storage_proof_recording_key<H: Encode>(block_hash: H) -> Vec<u8> {
	(b"cumulus_aura_storage_proof_recording", block_hash).encode()
}

fn load_decode<B, T>(backend: &B, key: &[u8]) -> ClientResult<Option<T>>
where
	B: AuxStore,
	T: Decode,
{
	let corrupt = |e: codec::Error| {
		ClientError::Backend(format!("Storage proof recording DB is corrupted. Decode error: {}", e))
	};
	match backend.get_aux(key)? {
		None => Ok(None),
		Some(t) => T::decode(&mut &t[..]).map(Some).map_err(corrupt),
	}
}

/// Write the storage proof size recordings of a block to aux storage.
pub(crate) fn write_storage_proof_recording<H: Encode, F, R>(
	block_hash: H,
	recordings: Vec<u32>,
	write_aux: F,
) -> R
where
	F: FnOnce(&[(Vec<u8>, &[u8])]) -> R,
{
	STORAGE_PROOF_RECORDING_CURRENT_VERSION.using_encoded(|version| {
		let key = storage_proof_recording_key(block_hash);
		recordings.using_encoded(|s| {
			write_aux(&[
				(key, s),
				(STORAGE_PROOF_RECORDING_VERSION.to_vec(), version),
			])
		})
	})
}

/// Load the storage proof size recordings associated with a block.
pub fn load_storage_proof_recording<H: Encode, B: AuxStore>(
	backend: &B,
	block_hash: H,
) -> ClientResult<Option<Vec<u32>>> {
	let version = load_decode::<_, u32>(backend, STORAGE_PROOF_RECORDING_VERSION)?;

	match version {
		None => Ok(None),
		Some(STORAGE_PROOF_RECORDING_CURRENT_VERSION) =>
			load_decode(backend, storage_proof_recording_key(block_hash).as_slice()),
		Some(other) =>
			Err(ClientError::Backend(format!("Unsupported storage proof recording DB version: {:?}", other))),
	}
}

/// Prune the storage proof size recordings for a block from aux storage.
pub(crate) fn prune_storage_proof_recording<H: Encode, B: AuxStore>(
	backend: &B,
	block_hash: H,
) -> ClientResult<()> {
	let key = storage_proof_recording_key(block_hash);
	backend.insert_aux(&[], &[key.as_slice()])
}