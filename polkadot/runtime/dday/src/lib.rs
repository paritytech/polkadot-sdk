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
			PollStatus, Polling, SixteenPatriciaMerkleTreeExistenceProof,
			SixteenPatriciaMerkleTreeProver, VerifyExistenceProof,
		},
	};
	use frame_system::pallet_prelude::*;
	use polkadot_primitives::HeadData;
	use polkadot_runtime_parachains::paras;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The hardcoded voting power type on AH.
	pub type VotingPowerType = frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>;

	pub type PollIndexOf<T> = <<T as Config>::Polling as Polling<Tally>>::Index;

	/// The tallying type.
	pub struct Tally {
		/// Total ayes accumulated. Each unit is one DOT.
		pub ayes: u128,
		/// Total nays accumulated. Each unit is one DOT.
		pub nays: u128,
	}

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

		/// The polling aka referenda system.
		type Polling: Polling<Tally>;

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

		fn voting_power_of(
			who: T::AccountId,
			root: T::Hash,
			proof: SixteenPatriciaMerkleTreeExistenceProof,
		) -> Result<u128, DispatchError> {
			ensure!(proof.key == T::VotingPowerKey::get(), "InvalidKey");
			SixteenPatriciaMerkleTreeProver::<<T as frame_system::Config>::Hashing>::verify_proof(
				proof, &root,
			)
			.and_then(|data| {
				<VotingPowerType as Decode>::decode(&mut &*data).map_err(|_| "NotDecode".into())
			})
			.map(|account| account.data.free + account.data.frozen)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn vote(
			origin: OriginFor<T>,
			who: T::AccountId,
			voting_power_proof: SixteenPatriciaMerkleTreeExistenceProof,
			vote: bool,
			poll_index: PollIndexOf<T>,
		) -> DispatchResultWithPostInfo {
			let frozen_root = Self::frozen_root().ok_or("NotFrozen")?;
			let voting_power = Self::voting_power_of(who, frozen_root, voting_power_proof)?;

			T::Polling::try_access_poll(poll_index, |status| match status {
				PollStatus::Ongoing(tally, class) => {
					if vote {
						tally.ayes += voting_power;
					} else {
						tally.nays += voting_power;
					}
					Ok(())
				},
				_ => Err("NotReferenda".into()),
			})?;

			Ok(Pays::No.into())
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
					if *is_stalled {
						// if it was stalled, and now it is not, we need to nullify any ongoing
						// poll.
					}
					*is_stalled = false;
					*last_head = head;
					*last_updated = now;
				}
			});

			Default::default()
		}
	}
}
