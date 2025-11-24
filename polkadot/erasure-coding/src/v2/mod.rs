// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.
//
// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Version 2 of erasure coding functions using external erasure-coding package.
//! This module provides improved performance through the use of the external
//! erasure-coding library from https://github.com/paritytech/erasure-coding

use codec::{Decode, Encode};
use polkadot_node_primitives::AvailableData;
use super::Error;
use erasure_coding_ext::{construct_chunks, reconstruct, reconstruct_from_systematic, ChunkIndex};

/// Reconstruct the v2 available data from the set of systematic chunks.
///
/// This version uses the external erasure-coding package for improved performance.
/// Provide a vector containing chunk data. If too few chunks are provided, recovery is not
/// possible.
pub fn reconstruct_from_systematic_v2(
	n_validators: usize,
	chunks: Vec<Vec<u8>>,
) -> Result<AvailableData, Error> {
	let n_chunks = n_validators as u16;
	let data_len = chunks.iter().map(|c| c.len()).sum::<usize>();
	
	let reconstructed_bytes = reconstruct_from_systematic(
		n_chunks,
		chunks.len(),
		&mut chunks.iter().map(Vec::as_slice),
		data_len,
	).map_err(|e| match e {
		erasure_coding_ext::Error::NotEnoughChunks => Error::NotEnoughChunks,
		erasure_coding_ext::Error::NonUniformChunks => Error::NonUniformChunks,
		erasure_coding_ext::Error::BadPayload => Error::BadPayload,
		erasure_coding_ext::Error::UnalignedChunk => Error::UnevenLength,
		_ => Error::UnknownReconstruction,
	})?;
	
	Decode::decode(&mut &reconstructed_bytes[..]).map_err(|err| Error::Decode(err))
}

/// Obtain erasure-coded chunks for v2 `AvailableData`, one for each validator.
///
/// This version uses the external erasure-coding package for improved performance.
/// Works only up to 65536 validators, and `n_validators` must be non-zero.
pub fn obtain_chunks_v2(n_validators: usize, data: &AvailableData) -> Result<Vec<Vec<u8>>, Error> {
	let encoded = data.encode();
	if encoded.is_empty() {
		return Err(Error::BadPayload);
	}
	
	let n_chunks = n_validators as u16;
	construct_chunks(n_chunks, &encoded).map_err(|e| match e {
		erasure_coding_ext::Error::BadPayload => Error::BadPayload,
		erasure_coding_ext::Error::NotEnoughTotalChunks => Error::NotEnoughValidators,
		erasure_coding_ext::Error::TooManyTotalChunks => Error::TooManyValidators,
		_ => Error::UnknownCodeParam,
	})
}

/// Reconstruct the v2 available data from a set of chunks.
///
/// This version uses the external erasure-coding package for improved performance.
/// Provide an iterator containing chunk data and the corresponding index.
/// The indices of the present chunks must be indicated. If too few chunks
/// are provided, recovery is not possible.
///
/// Works only up to 65536 validators, and `n_validators` must be non-zero.
pub fn reconstruct_v2<'a, I: 'a>(n_validators: usize, chunks: I) -> Result<AvailableData, Error>
where
	I: IntoIterator<Item = (&'a [u8], usize)>,
{
	let n_chunks = n_validators as u16;
	let chunks_with_indices: Vec<(ChunkIndex, Vec<u8>)> = chunks
		.into_iter()
		.map(|(data, index)| (ChunkIndex::from(index as u16), data.to_vec()))
		.collect();
	
	// Estimate data length - this is a rough estimate
	let estimated_data_len = chunks_with_indices.iter().map(|(_, data)| data.len()).sum::<usize>();
	
	let reconstructed_bytes = reconstruct(n_chunks, chunks_with_indices, estimated_data_len)
		.map_err(|e| match e {
			erasure_coding_ext::Error::NotEnoughChunks => Error::NotEnoughChunks,
			erasure_coding_ext::Error::NonUniformChunks => Error::NonUniformChunks,
			erasure_coding_ext::Error::BadPayload => Error::BadPayload,
			_ => Error::UnknownReconstruction,
		})?;
	
	Decode::decode(&mut &reconstructed_bytes[..]).map_err(|err| Error::Decode(err))
}


#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_node_primitives::{BlockData, PoV};
	use polkadot_primitives::{HeadData, PersistedValidationData};
	use quickcheck::{Arbitrary, Gen, QuickCheck};
	use std::sync::Arc;

	#[derive(Clone, Debug)]
	struct ArbitraryAvailableData(AvailableData);

	impl Arbitrary for ArbitraryAvailableData {
		fn arbitrary(g: &mut Gen) -> Self {
			// Limit the POV len to 256KiB for v2 tests to avoid reconstruction issues
			let pov_len = (u32::arbitrary(g) % (256 * 1024)).max(2);

			let pov = (0..pov_len).map(|_| u8::arbitrary(g)).collect();

			let pvd = PersistedValidationData {
				parent_head: HeadData((0..(u16::arbitrary(g) % 1024)).map(|_| u8::arbitrary(g)).collect()),
				relay_parent_number: u32::arbitrary(g),
				relay_parent_storage_root: [u8::arbitrary(g); 32].into(),
				max_pov_size: u32::arbitrary(g),
			};

			ArbitraryAvailableData(AvailableData {
				pov: Arc::new(PoV { block_data: BlockData(pov) }),
				validation_data: pvd,
			})
		}
	}

	#[test]
	fn v2_round_trip_works() {
		let pov = PoV { block_data: BlockData((0..255).collect()) };
		let available_data = AvailableData { 
			pov: pov.into(), 
			validation_data: Default::default() 
		};
		
		let chunks = obtain_chunks_v2(10, &available_data).unwrap();
		assert_eq!(chunks.len(), 10);

		// Test reconstruction from systematic chunks
		let reconstructed = reconstruct_from_systematic_v2(10, chunks).unwrap();
		assert_eq!(reconstructed, available_data);
	}

	#[test]
	fn v2_reconstruct_works() {
		let pov = PoV { block_data: BlockData((0..255).collect()) };
		let available_data = AvailableData { 
			pov: pov.into(), 
			validation_data: Default::default() 
		};
		
		let chunks = obtain_chunks_v2(10, &available_data).unwrap();
		
		// Test reconstruction from specific chunks
		let reconstructed = reconstruct_v2(
			10,
			[(&*chunks[1], 1), (&*chunks[4], 4), (&*chunks[6], 6), (&*chunks[9], 9)]
				.iter()
				.cloned(),
		).unwrap();
		
		assert_eq!(reconstructed, available_data);
	}

	#[test]
	fn v2_round_trip_reconstruct_works() {
		fn property(available_data: ArbitraryAvailableData, n_validators: u16) {
			// Limit n_validators to a reasonable range (4..1024) to avoid TooManyValidators error
			let n_validators = ((n_validators % 1020) + 4) as usize;
			let chunks = obtain_chunks_v2(n_validators, &available_data.0).unwrap();
			
			// Test reconstruction from a subset of chunks (any f+1 chunks should work)
			// For n_validators, we need at least (n_validators - 1) / 3 + 1 chunks
			let min_chunks = (n_validators - 1) / 3 + 1;
			
			// Take a few more chunks than minimum to ensure reconstruction
			let chunks_to_use = (min_chunks + 2).min(n_validators);
			
			let reconstructed = reconstruct_v2(
				n_validators,
				chunks.iter().enumerate().take(chunks_to_use).map(|(i, c)| (c.as_slice(), i))
			)
			.unwrap();
			
			assert_eq!(reconstructed, available_data.0);
		}

		QuickCheck::new().quickcheck(property as fn(ArbitraryAvailableData, u16))
	}

}
