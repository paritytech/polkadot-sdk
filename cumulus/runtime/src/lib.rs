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

use rstd::{vec::Vec, collections::btree_map::BTreeMap};
use codec::{Encode, Decode};
use runtime_primitives::traits::Block as BlockT;

pub mod validate_block;

type WitnessData = BTreeMap<Vec<u8>, Vec<u8>>;

/// The parachain block that is created on a collator and validated by a validator.
#[derive(Encode, Decode)]
struct ParachainBlock<B: BlockT> {
	extrinsics: Vec<<B as BlockT>::Extrinsic>,
	/// The data that is required to emulate the storage accesses executed by all extrinsics.
	witness_data: WitnessData,
}

impl<B: BlockT> ParachainBlock<B> {
	#[cfg(test)]
	fn new(extrinsics: Vec<<B as BlockT>::Extrinsic>, witness_data: WitnessData) -> Self {
		Self {
			extrinsics,
			witness_data,
		}
	}
}

impl<B: BlockT> Default for ParachainBlock<B> {
	fn default() -> Self {
		Self {
			extrinsics: Vec::default(),
			witness_data: BTreeMap::default(),
		}
	}
}