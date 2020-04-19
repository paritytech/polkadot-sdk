// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! # Randomness Module
//!
//! The Randomness Collective Flip module provides a [`random`](./struct.Module.html#method.random)
//! function that generates low-influence random values based on the block hashes from the previous
//! `81` blocks. Low-influence randomness can be useful when defending against relatively weak
//! adversaries. Using this pallet as a randomness source is advisable primarily in low-security
//! situations like testing. 
//!
//! ## Public Functions
//!
//! See the [`Module`](./struct.Module.html) struct for details of publicly available functions.
//!
//! ## Usage
//!
//! ### Prerequisites
//!
//! Import the Randomness Collective Flip module and derive your module's configuration trait from
//! the system trait.
//!
//! ### Example - Get random seed for the current block
//!
//! ```
//! use frame_support::{decl_module, dispatch, traits::Randomness, weights::{SimpleDispatchInfo, MINIMUM_WEIGHT}};
//!
//! pub trait Trait: frame_system::Trait {}
//!
//! decl_module! {
//! 	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
//! 		#[weight = SimpleDispatchInfo::FixedNormal(MINIMUM_WEIGHT)]
//! 		pub fn random_module_example(origin) -> dispatch::DispatchResult {
//! 			let _random_value = <pallet_randomness_collective_flip::Module<T>>::random(&b"my context"[..]);
//! 			Ok(())
//! 		}
//! 	}
//! }
//! # fn main() { }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

use sp_std::{prelude::*, convert::TryInto};
use sp_runtime::traits::Hash;
use frame_support::{
	decl_module, decl_storage, traits::Randomness,
	weights::{Weight, MINIMUM_WEIGHT}
};
use safe_mix::TripletMix;
use codec::Encode;
use frame_system::Trait;

const RANDOM_MATERIAL_LEN: u32 = 81;

fn block_number_to_index<T: Trait>(block_number: T::BlockNumber) -> usize {
	// on_initialize is called on the first block after genesis
	let index = (block_number - 1.into()) % RANDOM_MATERIAL_LEN.into();
	index.try_into().ok().expect("Something % 81 is always smaller than usize; qed")
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn on_initialize(block_number: T::BlockNumber) -> Weight {
			let parent_hash = <frame_system::Module<T>>::parent_hash();

			<RandomMaterial<T>>::mutate(|ref mut values| if values.len() < RANDOM_MATERIAL_LEN as usize {
				values.push(parent_hash)
			} else {
				let index = block_number_to_index::<T>(block_number);
				values[index] = parent_hash;
			});

			MINIMUM_WEIGHT
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as RandomnessCollectiveFlip {
		/// Series of block headers from the last 81 blocks that acts as random seed material. This
		/// is arranged as a ring buffer with `block_number % 81` being the index into the `Vec` of
		/// the oldest hash.
		RandomMaterial get(fn random_material): Vec<T::Hash>;
	}
}

impl<T: Trait> Randomness<T::Hash> for Module<T> {
	/// This randomness uses a low-influence function, drawing upon the block hashes from the
	/// previous 81 blocks. Its result for any given subject will be known far in advance by anyone
	/// observing the chain. Any block producer has significant influence over their block hashes
	/// bounded only by their computational resources. Our low-influence function reduces the actual
	/// block producer's influence over the randomness, but increases the influence of small
	/// colluding groups of recent block producers.
	///
	/// WARNING: Hashing the result of this function will remove any low-influence properties it has
	/// and mean that all bits of the resulting value are entirely manipulatable by the author of
	/// the parent block, who can determine the value of `parent_hash`.
	fn random(subject: &[u8]) -> T::Hash {
		let block_number = <frame_system::Module<T>>::block_number();
		let index = block_number_to_index::<T>(block_number);

		let hash_series = <RandomMaterial<T>>::get();
		if !hash_series.is_empty() {
			// Always the case after block 1 is initialized.
			hash_series.iter()
				.cycle()
				.skip(index)
				.take(RANDOM_MATERIAL_LEN as usize)
				.enumerate()
				.map(|(i, h)| (i as i8, subject, h).using_encoded(T::Hashing::hash))
				.triplet_mix()
		} else {
			T::Hash::default()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;
	use sp_runtime::{
		Perbill,
		testing::Header,
		traits::{BlakeTwo256, Header as _, IdentityLookup},
	};
	use frame_support::{
		impl_outer_origin, parameter_types, weights::Weight, traits::{Randomness, OnInitialize},
	};

	#[derive(Clone, PartialEq, Eq)]
	pub struct Test;

	impl_outer_origin! {
		pub enum Origin for Test  where system = frame_system {}
	}

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::one();
	}

	impl frame_system::Trait for Test {
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Call = ();
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = ();
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type DbWeight = ();
		type AvailableBlockRatio = AvailableBlockRatio;
		type MaximumBlockLength = MaximumBlockLength;
		type Version = ();
		type ModuleToIndex = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
	}

	type System = frame_system::Module<Test>;
	type CollectiveFlip = Module<Test>;

	fn new_test_ext() -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::default().build_storage::<Test>().unwrap();
		t.into()
	}

	#[test]
	fn test_block_number_to_index() {
		for i in 1 .. 1000 {
			assert_eq!((i - 1) as usize % 81, block_number_to_index::<Test>(i));
		}
	}

	fn setup_blocks(blocks: u64) {
		let mut parent_hash = System::parent_hash();

		for i in 1 .. (blocks + 1) {
			System::initialize(
				&i,
				&parent_hash,
				&Default::default(),
				&Default::default(),
				frame_system::InitKind::Full,
			);
			CollectiveFlip::on_initialize(i);

			let header = System::finalize();
			parent_hash = header.hash();
			System::set_block_number(*header.number());
		}
	}

	#[test]
	fn test_random_material_partial() {
		new_test_ext().execute_with(|| {
			let genesis_hash = System::parent_hash();

			setup_blocks(38);

			let random_material = CollectiveFlip::random_material();

			assert_eq!(random_material.len(), 38);
			assert_eq!(random_material[0], genesis_hash);
		});
	}

	#[test]
	fn test_random_material_filled() {
		new_test_ext().execute_with(|| {
			let genesis_hash = System::parent_hash();

			setup_blocks(81);

			let random_material = CollectiveFlip::random_material();

			assert_eq!(random_material.len(), 81);
			assert_ne!(random_material[0], random_material[1]);
			assert_eq!(random_material[0], genesis_hash);
		});
	}

	#[test]
	fn test_random_material_filled_twice() {
		new_test_ext().execute_with(|| {
			let genesis_hash = System::parent_hash();

			setup_blocks(162);

			let random_material = CollectiveFlip::random_material();

			assert_eq!(random_material.len(), 81);
			assert_ne!(random_material[0], random_material[1]);
			assert_ne!(random_material[0], genesis_hash);
		});
	}

	#[test]
	fn test_random() {
		new_test_ext().execute_with(|| {
			setup_blocks(162);

			assert_eq!(System::block_number(), 162);
			assert_eq!(CollectiveFlip::random_seed(), CollectiveFlip::random_seed());
			assert_ne!(CollectiveFlip::random(b"random_1"), CollectiveFlip::random(b"random_2"));

			let random = CollectiveFlip::random_seed();

			assert_ne!(random, H256::zero());
			assert!(!CollectiveFlip::random_material().contains(&random));
		});
	}
}
