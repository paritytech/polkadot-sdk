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

#[frame::pallet]
pub mod pallet {
	use frame::{
		prelude::*,
		traits::{
			fungible::{Inspect, Mutate},
			PollStatus, Polling, SixteenPatriciaMerkleTreeExistenceProof,
			SixteenPatriciaMerkleTreeProver, TransactionExtension, VerifyExistenceProof,
		},
	};
	use polkadot_primitives::HeadData;
	use polkadot_runtime_parachains::paras;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The hardcoded voting power type on AH.
	pub type VotingPowerType = frame_system::AccountInfo<u32, pallet_balances::AccountData<u128>>;

	/// The index of a referenda/poll.
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
		/// Currency interface.
		type Currency: Inspect<<Self as frame_system::Config>::AccountId, Balance = u128>
			+ Mutate<<Self as frame_system::Config>::AccountId>;

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
			who: &T::AccountId,
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

		fn deposit_for_vote(voting_power_claim: u128) -> u128 {
			todo!("A deposit amount that must be present in the origin of `vote` in order to process the transaction.");
		}

		fn ensure_has_deposit(who: &T::AccountId, voting_power_claim: u128) -> DispatchResult {
			let deposit = Self::deposit_for_vote(voting_power_claim);
			ensure!(T::Currency::balance(&who) >= deposit, "InsufficientBalance");
			Ok(())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Vote on a poll.
		///
		/// `origin` can be anyone with enough balance to submit this transaction, and pay for its
		/// pre-dispatch fees. Successful submissions are refunded.
		/// `who` is the account on behalf of whom we are voting.
		/// `voting_power_proof` is the proof that `who` has `voting_power_claim`.
		/// `voting_power_claim` itself is needed to properly prioritize the proof transactions. See
		/// [`PrioritizeByVotingPower`].
		#[pallet::weight(0)]
		#[pallet::call_index(0)]
		pub fn vote(
			origin: OriginFor<T>,
			who: T::AccountId,
			voting_power_proof: SixteenPatriciaMerkleTreeExistenceProof,
			voting_power_claim: u128,
			vote: bool,
			poll_index: PollIndexOf<T>,
		) -> DispatchResultWithPostInfo {
			let frozen_root = Self::frozen_root().ok_or("NotFrozen")?;
			let voting_power = Self::voting_power_of(&who, frozen_root, voting_power_proof)?;
			Self::ensure_has_deposit(&who, voting_power)?;

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
						todo!()
					}
					*is_stalled = false;
					*last_head = head;
					*last_updated = now;
				}
			});

			Default::default()
		}
	}

	/// Prioritize transactions by their claimed voting power.
	///
	/// This extension will not verify the proof of the voting power and "blindly" trust it, knowing
	/// that if the proof is incorrect, [`Pallet::vote`] will slash a proportional deposit from the
	/// sender.
	///
	/// If the voting power proof is invalid
	#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct PrioritizeByVotingPower<T>(PhantomData<T>);

	impl<T: Config> core::fmt::Debug for PrioritizeByVotingPower<T> {
		fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
			write!(f, "PrioritizeByVotingPower")
		}
	}

	impl<T: Config> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
		for PrioritizeByVotingPower<T>
	where
		T: Send + Sync,
	{
		const IDENTIFIER: &'static str = "PrioritizeByVotingPower";
		type Implicit = ();
		type Val = ();
		type Pre = ();

		fn weight(&self, call: &<T as frame_system::Config>::RuntimeCall) -> Weight {
			Default::default()
		}

		fn validate(
			&self,
			origin: frame::traits::DispatchOriginOf<<T as frame_system::Config>::RuntimeCall>,
			call: &<T as frame_system::Config>::RuntimeCall,
			info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
			len: usize,
			self_implicit: Self::Implicit,
			inherited_implication: &impl Encode,
		) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
			todo!();
		}

		fn prepare(
			self,
			val: Self::Val,
			origin: &frame::traits::DispatchOriginOf<<T as frame_system::Config>::RuntimeCall>,
			call: &<T as frame_system::Config>::RuntimeCall,
			info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
			len: usize,
		) -> Result<Self::Pre, TransactionValidityError> {
			todo!();
		}
	}
}

#[cfg(test)]
pub mod mock {}

#[cfg(test)]
pub mod tests {}
