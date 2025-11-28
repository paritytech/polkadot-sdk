// This file is part of Substrate.

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

//! # Recovery Pallet
//!
//! ## Terminology
//!
//! - `friend`: A befriended account that can vouch for a recovery process.
//! - `lost`: An account that has lost access to its private key and needs to be recovered.
//! - `recoverer`: An account that is trying to recover a lost account.
//! - `recovered`: An account that has been successfully recovered..
//! - `inheritor`: An account that is inheriting access to a lost account after recovery.
//! - `attempt`: An attempt to recover a lost account by a recoverer.
//! - `trust level`: The level of trust that an account has in a friend group.
//!
//! ## Trust Levels
//!
//! The trust level in a friend group can be parametrized with three values:
//! - `deposit`: The amount that a friends of this group needs to reserve to initiate an attempt.
//! - `threshold`: The number of friends that need to approve an attempt.
//! - `delay`: How long an attempt will be delayed before it can succeed.
//! ### Friend Group Order
//!
//! The order of friend groups in the account config. Since there can be only one concurrent
//! inheritor to an account, friends groups get granted exclusive access to the lost account
//! depending on their index in the account config. For example: The *family* group is the most
//! trusted group and therefore the first group in the account config. The *colleagues* group is
//! less trusted and therefore is the second group in the account config. Now, if the *family*
//! recovers the account first, it will inherit indefinite full access to the lost account. The
//! recovery attempt of the *colleagues* will always fail. In the case that the *colleagues* recover
//! the account first, the *family* can take back control by finishing their recovery attempt since
//! their group has a higher order.P

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use frame::{
	prelude::*,
	traits::{Consideration, Footprint, fungible::{Inspect, MutateHold}},
};

pub use pallet::*;
pub use weights::WeightInfo;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<AccountIdFor<T>>>::Balance;	
/// The block number type that will be used to measure time.
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
pub type FriendsOf<T> =
	BoundedVec<<T as frame_system::Config>::AccountId, <T as Config>::MaxFriendsPerConfig>;
pub type HashOf<T> = <T as frame_system::Config>::Hash;

pub type InheritanceOrder = u32;

/// Configuration for recovering an account.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct FriendGroup<ProvidedBlockNumber, AccountId, Balance, Friends> {
	/// Minimum relay chain block delay before the account can be recovered.
	///
	/// Uses a provided block number to avoid possible clock skew of parachains.
	pub delay_period: ProvidedBlockNumber,
	/// Slashable deposit that the rescuer needs to reserve.
	pub deposit: Balance,
	/// List of friends that can initiate the recovery process. Always sorted.
	pub friends: Friends,
	/// The number of approving friends needed to recover an account.
	pub friends_needed: u32,
	/// The account that inherited full access to a lost account after successful recovery.
	pub inheritor: AccountId,
	pub inheritance_order: InheritanceOrder,
	/// The delay since the last approval of an attempt before the attempt can be aborted.
	///
	/// It ensures that a malicious recoverer does not abuse the `abort_attempt` call to doge an
	/// incoming slash from the lost account. They could otherwise monitor the TX pool and abort the
	/// attempt just in time for the slash transaction to fail.
	pub abort_delay: ProvidedBlockNumber,
}
type FriendGroupOf<T> = FriendGroup<ProvidedBlockNumberOf<T>, AccountIdFor<T>, BalanceOf<T>, FriendsOf<T>>;

type FriendGroupsOf<T> = BoundedVec<FriendGroupOf<T>, <T as Config>::MaxConfigsPerAccount>;

/// Bitfield helper for tracking friend votes.
///
/// Uses a vector of u128 values where each bit represents whether a friend at that index has voted.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(MaxEntries))]
pub struct Bitfield<MaxEntries: Get<u32>>(pub BoundedVec<u128, BitfieldLenOf<MaxEntries>>);

pub type BitfieldLenOf<MaxEntries: Get<u32>> = ConstDivCeil<MaxEntries, ConstU32<128>, u32, u32>;

pub struct ConstDivCeil<Dividend, Divisor, R, T>(pub core::marker::PhantomData<(Dividend, Divisor, R, T)>);
impl<Dividend: Get<T>, Divisor: Get<T>, R: AtLeast32BitUnsigned, T: Into<R>> Get<R> for ConstDivCeil<Dividend, Divisor, R, T> {
	fn get() -> R {
		123u32.into()
	}
}

impl<MaxEntries: Get<u32>> Default for Bitfield<MaxEntries> {
	fn default() -> Self {
		Self(vec![0u128; BitfieldLenOf::<MaxEntries>::get() as usize].try_into().defensive().unwrap_or_default()) // todo error
	}
}

impl<MaxEntries: Get<u32>> Bitfield<MaxEntries> {
	/// Set the bit at the given index to true (friend has voted).
	pub fn set_if_not_set(&mut self, index: usize) -> Result<(), ()> {
		let word_index = index / 128;
		let bit_index = index % 128;

		let word = self.0.get_mut(word_index).ok_or(())?;
		if (*word & (1u128 << bit_index)) == 0 {
			*word |= 1u128 << bit_index;
			Ok(())
		} else {
			Err(())
		}
	}

	/// Count the total number of set bits (total votes).
	pub fn count_ones(&self) -> u32 {
		self.0.iter().map(|word| word.count_ones() as u32).sum()
	}
}

pub type ApprovalBitfield<MaxFriends: Get<u32>> = Bitfield<MaxFriends>;
pub type ApprovalBitfieldOf<T> = ApprovalBitfield<<T as Config>::MaxFriendsPerConfig>;

/// An active recovery process.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RecoveryAttempt<ProvidedBlockNumber, FriendGroup, ApprovalBitfield> {
	pub init_block: ProvidedBlockNumber,
	pub last_approval_block: ProvidedBlockNumber,

	/// The friend group snapshot at the time of recovery attempt initiation.
	///
	/// Contains all the parameters (friends, threshold, deposit, inheritor, etc.) at the time
	/// the attempt was created.
	pub friend_group: FriendGroup,

	/// Bitfield tracking which friends have vouched.
	///
	/// Each bit corresponds to a friend in the `friend_group.friends` list by index.
	pub approvals: ApprovalBitfield,
}
type RecoveryAttemptOf<T> = RecoveryAttempt<ProvidedBlockNumberOf<T>, FriendGroupOf<T>, ApprovalBitfieldOf<T>>;
type RecoveryAttemptsOf<T> = BoundedVec<RecoveryAttemptOf<T>, <T as Config>::MaxOngoingRecoveriesPerRecoverer>;

type InheritorsOf<T> = BoundedVec<AccountIdFor<T>, <T as Config>::MaxInheritorsPerAccount>;

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		/// The overarching freeze reason.
		type RuntimeHoldReason: Parameter + Member + MaxEncodedLen + Copy + VariantCount;

		/// Query the block number that will be used to measure time.
		///
		/// Must return monotonically increasing values when called from consecutive blocks.
		/// Can be configured to return either:
		/// - the local block number of the runtime via `frame_system::Pallet`
		/// - a remote block number, eg from the relay chain through `RelaychainDataProvider`
		/// - an arbitrary value through a custom implementation of the trait
		///
		/// There is currently no migration provided to "hot-swap" block number providers and it may
		/// result in undefined behavior when doing so. Parachains are therefore best off setting
		/// this to their local block number provider if they have the pallet already deployed.
		///
		/// Suggested values:
		/// - Solo- and Relay-chains: `frame_system::Pallet`
		/// - Parachains that may produce blocks sparingly or only when needed (on-demand):
		///   - already have the pallet deployed: `frame_system::Pallet`
		///   - are freshly deploying this pallet: `RelaychainDataProvider`
		/// - Parachains with a reliably block production rate (PLO or bulk-coretime):
		///   - already have the pallet deployed: `frame_system::Pallet`
		///   - are freshly deploying this pallet: no strong recommendation. Both local and remote
		///     providers can be used. Relay provider can be a bit better in cases where the
		///     parachain is lagging its block production to avoid clock skew.
		type BlockNumberProvider: BlockNumberProvider;

		/// The currency mechanism.
		type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Consideration for holding a non-slashable deposit.
		type Consideration: Consideration<Self::AccountId, Footprint>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of friends per account config.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors.
		#[pallet::constant]
		type MaxFriendsPerConfig: Get<u32>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of configs per account.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors.
		#[pallet::constant]
		type MaxConfigsPerAccount: Get<u32>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of ongoing recoveries per recoverer.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors. This value should generally be be no less than `MaxConfigsPerAccount`.
		#[pallet::constant]
		type MaxOngoingRecoveriesPerRecoverer: Get<u32>;

		#[pallet::constant]
		type MaxInheritorsPerAccount: Get<u32>;
	}

	/// Events type.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		LostAccountControlled {
			lost: T::AccountId,
			inheritor: T::AccountId,
			call_hash: HashOf<T>,
			call_result: DispatchResult,
		},
		FriendGroupsChanged {
			lost: T::AccountId,
			old_friend_groups: FriendGroupsOf<T>,
		},
		AttemptInitiated {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: u32,
		},
		AttemptApproved {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: u32,
			friend: T::AccountId,
		},
		AttemptFinished {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: u32,
		},
		AttemptAborted {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: u32,
		},
		AttemptSlashed {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: u32,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// This account does not have any friend groups.
		NoFriendGroups,
		/// The caller is not a friend of the lost account.
		NotFriend,
		/// A specific referenced friend group was not found.
		NoFriendGroup,
		/// The referenced recovery attempt was not found.
		NotAttempt,
		/// The friend has already vouched for this attempt.
		AlreadyVouched,
		/// The lost account does not have any inheritor.
		NoInheritor,
		/// The caller is not the inheritor of the lost account.
		NotInheritor,
		/// Not enough friends have vouched for this attempt.
		NotEnoughVouches,
		/// The recovery attempt is not yet unlocked.
		NotUnlocked,
		/// The recovery attempt cannot be aborted yet.
		NotAbortable,
		/// Too many concurrent recovery attempts for this recoverer.
		TooManyAttempts,
	}

	/// The friend groups of an that can initiate and vouch for recovery attempts.
	#[pallet::storage]
	pub type FriendGroups<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		FriendGroupsOf<T>,
		OptionQuery,
	>;

	/// Ongoing recovery attempts of an account indexed by `(lost, recoverer)`.
	///
	/// A *recoverer* can initiate multiple recovery attempts for the same lost account if they are part of multiple account configs. For example: A friend could be part of the *family* group but also the *friends* group. In this case, they can initiate both recovery attempts at once, as long as it are not more than `MaxOngoingRecoveriesPerRecoverer` at a time.
	#[pallet::storage]
	pub type Attempts<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::AccountId,
		RecoveryAttemptsOf<T>,
		OptionQuery,
	>;

	/// The account that inherited full access to a lost account after successful recovery.
	///
	/// NOTE: This could be a multisig or proxy account
	#[pallet::storage]
	pub type Inheritor<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, (InheritanceOrder, T::AccountId)>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// todo bin search
		// todo event
		// todo copy call filters
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn control_lost_account(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let maybe_inheritor = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;

			let inheritor = Inheritor::<T>::get(&lost).map(|(_, inheritor)| inheritor).ok_or(Error::<T>::NoInheritor)?;
			ensure!(maybe_inheritor == inheritor, Error::<T>::NotInheritor);

			// pretend to be the lost account
			let origin = frame_system::RawOrigin::Signed(lost.clone()).into();
			let call_hash = call.using_encoded(&T::Hashing::hash);
			let call_result = call.dispatch(origin).map(|_| ()).map_err(|r| r.error);

			Self::deposit_event(Event::<T>::LostAccountControlled {
				lost,
				inheritor,
				call_hash,
				call_result,
			});

			// NOTE: We ALWAYS return okay if the caller had the permission to control the lost
			// account regardless of the inner call result.
			Ok(())
		}

		// todo event
		/// Set the friend groups of the calling account before it lost access.
		///
		/// This does not impact or cancel any ongoing recovery attempts.
		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn set_friend_groups(
			origin: OriginFor<T>,
			friend_groups: Vec<FriendGroupOf<T>>,
		) -> DispatchResult {
			let lost = ensure_signed(origin)?;

			let current_friend_groups = FriendGroups::<T>::get(&lost).unwrap_or_default();
			let new_friend_groups: FriendGroupsOf<T> = friend_groups
				.try_into()
				.map_err(|_| "Too many friend groups")?;

			if new_friend_groups != current_friend_groups {
				FriendGroups::<T>::insert(&lost, &new_friend_groups);

				Self::deposit_event(Event::<T>::FriendGroupsChanged {
					lost,
					old_friend_groups: current_friend_groups,
				});
			}

			Ok(())
		}

		/// Attempt to recover a lost account by a friend with the given friend group.
		///
		/// The friend group is passed in as witness to ensure that the recoverer is not operating on stale friend group data and is making wrong assumptions about the delay or deposit amounts.
		// TODO event
		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn initiate_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			friend_group: FriendGroupOf<T>,
		) -> DispatchResult {
			let maybe_friend = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;

			let friend_groups = FriendGroups::<T>::get(&lost).ok_or(Error::<T>::NoFriendGroups)?;

			// Find the friend group and its inheritance order
			let inheritance_order = friend_groups
				.iter()
				.position(|fg| fg == &friend_group)
				.ok_or(Error::<T>::NoFriendGroup)? as InheritanceOrder;

			// Construct the attempt
			let now = T::BlockNumberProvider::current_block_number();
			let unlock_at = now.checked_add(&friend_group.delay_period).ok_or(ArithmeticError::Overflow)?;
			let abortable_at = now.checked_add(&friend_group.abort_delay).ok_or(ArithmeticError::Overflow)?;

			let attempt = RecoveryAttempt {
				init_block: now,
				last_approval_block: now,
				friend_group,
				approvals: ApprovalBitfield::default(),
			};

			let mut attempts = Attempts::<T>::get(&lost, &maybe_friend).unwrap_or_default();
			attempts.try_push(attempt).map_err(|_| Error::<T>::TooManyAttempts)?;

			Attempts::<T>::insert(&lost, &maybe_friend, attempts);

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn approve_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: u32,
		) -> DispatchResult {
			let maybe_friend = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let recoverer = T::Lookup::lookup(recoverer)?;
			let now = T::BlockNumberProvider::current_block_number();

			let mut attempts = Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let attempt = attempts.get_mut(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			// Find the friend's index in the friend group
			let friend_index = attempt
				.friend_group
				.friends
				.binary_search(&maybe_friend)
				.map_err(|_| Error::<T>::NotFriend)?;

		    attempt.approvals.set_if_not_set(friend_index as usize).map_err(|_| Error::<T>::AlreadyVouched)?;
			Attempts::<T>::insert(&lost, &recoverer, attempts);

			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn finish_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: u32,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let recoverer = T::Lookup::lookup(recoverer)?;

			let now = T::BlockNumberProvider::current_block_number();
			let mut attempts = Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let attempt = attempts.get(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			// Check if the attempt is now complete
			let approvals = attempt.approvals.count_ones();
			ensure!(approvals >= attempt.friend_group.friends_needed, Error::<T>::NotEnoughVouches);
			let unlock_at = attempt.init_block.checked_add(&attempt.friend_group.delay_period).ok_or(ArithmeticError::Overflow)?;
			ensure!(now >= unlock_at, Error::<T>::NotUnlocked);
			// NOTE: We dont need to check the abort delay, since enough friends voted and we dont
			// assume full malicious behavior.

			let inheritance_order = attempt.friend_group.inheritance_order;
			attempts.remove(attempt_index as usize);

			// todo event
			match Inheritor::<T>::get(&lost) {
				None => Inheritor::<T>::insert(&lost, (inheritance_order, &recoverer)),
				Some((old_order, _)) if inheritance_order < old_order => {
					// new recovery has a lower inheritance order, we therefore replace the existing inheritor
					Inheritor::<T>::insert(&lost, (inheritance_order, &recoverer));
				}
				Some(_) => {
					// the existing inheritor stays since an equal or worse inheritor contested
					// TODO event
				}
			}

			Self::write_attempts(&lost, &recoverer, &attempts);

			Ok(())
		}

		/// The recoverer or the lost account can abort an attempt at any moment.
		///
		/// This will release the deposit of the attempt back to the recoverer.
		#[pallet::call_index(6)]
		#[pallet::weight(0)]
		pub fn abort_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: u32,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let recoverer = T::Lookup::lookup(recoverer)?;
			let now = T::BlockNumberProvider::current_block_number();

			let mut attempts = Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let attempt = attempts.get(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			let abortable_at = attempt.last_approval_block.checked_add(&attempt.friend_group.abort_delay).ok_or(ArithmeticError::Overflow)?;
			ensure!(now >= abortable_at, Error::<T>::NotAbortable);
			// NOTE: It is possible to abort a fully approved attempt, but since we check the abort
			// delay, we ensure that every friend had enough time to call `finish_attempt`.
			attempts.remove(attempt_index as usize);

			Self::write_attempts(&lost, &recoverer, &attempts);

			// TODO currency stuff

			Ok(())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(0)]
		pub fn slash_attempt(
			origin: OriginFor<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: u32,
		) -> DispatchResult {
			let lost = ensure_signed(origin)?;
			let recoverer = T::Lookup::lookup(recoverer)?;

			let mut attempts = Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let _attempt = attempts.get(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			attempts.remove(attempt_index as usize);
			// TODO slash

			Self::write_attempts(&lost, &recoverer, &attempts);

			// TODO currency stuff

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn write_attempts(lost: &T::AccountId, recoverer: &T::AccountId, attempts: &RecoveryAttemptsOf<T>) {
		if attempts.is_empty() {
			Attempts::<T>::remove(lost, recoverer);
		} else {
			Attempts::<T>::insert(lost, recoverer, attempts);
		}
	}
}
