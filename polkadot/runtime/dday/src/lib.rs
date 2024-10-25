// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{
			SixteenPatriciaMerkleTreeExistenceProof, SixteenPatriciaMerkleTreeProver,
			VerifyExistenceProof,
		},
	};
	use frame_system::pallet_prelude::*;
	use polkadot_primitives::HeadData;
	use polkadot_runtime_parachains::paras;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The hardcoded voting power type on AH.
	pub type VotingPowerType = frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>;

	#[pallet::config]
	pub trait Config: frame_system::Config + paras::Config {
		/// The asset hub parachain id.
		type AssetHub: Get<polkadot_primitives::Id>;

		/// The storage key who's value in a frozen AH will determine one's voting power.
		///
		/// This needs to be hardcoded given e.g. the name of the `pallet-balances` in AH.
		///
		/// This is assuming that the voting power ca be determined by just one key. It might be
		/// more complicated than this.
		///
		/// We assume the value stored under this key is known: [`VotingPowerType`].
		type VotingPowerKey: Get<Vec<u8>>;

		/// If the head data of AssetHub is not updated in this many blocks, we assume it is
		/// stalled.
		type StallThreshold: Get<BlockNumberFor<Self>>;
	}

	#[pallet::storage]
	#[pallet::unbounded]
	pub type LastAssetHubHead<T: Config> =
		StorageValue<_, (BlockNumberFor<T>, HeadData, bool), ValueQuery>;

	impl<T: Config> Pallet<T> {
		fn frozen_root() -> Option<T::Hash> {
			match LastAssetHubHead::<T>::get() {
				// TODO: Janky, there should be a better way to express this conversion.
				(_, head, true) => T::Hash::decode(&mut &head.0[head.0.len() - 32..]).ok(),
				_ => None,
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Submit the proof of a user's free balance on AH if it is frozen.
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn frozen_balance_of(
			origin: OriginFor<T>,
			who: T::AccountId,
			proof: SixteenPatriciaMerkleTreeExistenceProof,
		) -> DispatchResult {
			ensure!(proof.key == T::VotingPowerKey::get(), "InvalidKey");
			let root = Self::frozen_root().ok_or("NotFrozen")?;
			let voting_power = SixteenPatriciaMerkleTreeProver::<
				<T as frame_system::Config>::Hashing,
			>::verify_proof(proof, &root)
			.and_then(|data| {
				<VotingPowerType as Decode>::decode(&mut &*data).map_err(|_| "NotDecode".into())
			})
			.map(|account| account.data.free + account.data.frozen)?;

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			let head = paras::Heads::<T>::get(T::AssetHub::get()).unwrap_or_default();

			LastAssetHubHead::<T>::mutate(|(last_updated, last_head, is_stalled)| {
				if head == *last_head {
					// head has not changed
					if *last_updated + T::StallThreshold::get() <= now {
						// head has not changed for too long, mark it as stalled.
						*is_stalled = true;
					}
				} else {
					// head has changed, update it and return okk.
					*last_head = head;
					*last_updated = now;
				}
			});

			Default::default()
		}
	}
}
