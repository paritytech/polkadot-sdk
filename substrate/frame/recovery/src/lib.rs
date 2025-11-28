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
	traits::{Currency, ReservableCurrency, Footprint},
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
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;	
/// The block number type that will be used to measure time.
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
pub type FriendsOf<T> =
	BoundedVec<<T as frame_system::Config>::AccountId, <T as Config>::MaxFriendsPerConfig>;

pub type InheritanceOrder = u16;

/// Configuration for recovering an account.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct FriendGroup<ProvidedBlockNumber, Balance, Friends> {
	/// Minimum relay chain block delay before the account can be recovered.
	///
	/// Uses a provided block number to avoid possible clock skew of parachains.
	pub delay_period: ProvidedBlockNumber,
	/// Slashable deposit that the rescuer needs to reserve.
	pub deposit: Balance,
	/// List of friends that can initiate the recovery process. Always sorted.
	pub friends: Friends,
	/// The number of approving friends needed to recover an account.
	pub friends_needed: u16,
	/// The account that inherited full access to a lost account after successful recovery.
	pub inheritor: AccountId,
	/// The delay since the last approval of an attempt before the attempt can be aborted.
	///
	/// It ensures that a malicious recoverer does not abuse the `abort_attempt` call to doge an
	/// incoming slash from the lost account. They could otherwise monitor the TX pool and abort the
	/// attempt just in time for the slash transaction to fail.
	pub abort_delay: ProvidedBlockNumber,
}
type FriendGroupOf<T> = FriendGroup<ProvidedBlockNumberOf<T>, BalanceOf<T>, FriendsOf<T>>;

type FriendGroupsOf<T> BoundedVec<FriendGroupOf<T>, <T as Config>::MaxConfigsPerAccount>;

/// An active recovery process.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct RecoveryAttempt<ProvidedBlockNumber, Balance, Friend, Vouched, MaxFriendsPerConfig> {
	/// The block number when the recovery process can be completed.
	///
	/// Uses a provided block number to avoid possible clock skew of parachains.
	/// This value is calculated by checking the account config of the lost account at time of recovery initiation.
	pub unlock_at: ProvidedBlockNumber,

	/// Slashable reserve of the `depositor`.
	///
	/// To be either slashed or returned to the `depositor` once the recovery process is closed.
	/// This value is taken from the respective account config upon recovery initiation.
	pub deposit: Balance,

	/// The friends and their vouched status.
	///
	/// A `true` value indicates that they have vouched for the recovery process.
	pub friends: BoundedVec<(Friend, bool), MaxFriendsPerConfig>,

	/// Number of friends needed to recover the account.
	///
	/// This value is copied from the respective account config upon recovery initiation.
	pub friends_needed: u16,

	pub inheritance_order: InheritanceOrder,

	/// The account that inherited full access to a lost account after successful recovery.
	///
	/// This value is copied from the respective account config upon recovery initiation.
	pub inheritor: AccountId,

	pub abortable_at: ProvidedBlockNumber,
}
type RecoveryAttemptOf<T> = RecoveryAttempt<ProvidedBlockNumberOf<T>, BalanceOf<T>, FriendsOf<T>>;
type RecoveryAttemptsOf<T> = BoundedVec<RecoveryAttemptOf<T>, <T as Config>::MaxOngoingRecoveriesPerRecoverer>;

type InheritorsOf<T> = BoundedVec<T::AccountId, <T as Config>::MaxInheritorsPerAccount>;

/// Reason for why a deposit is being held.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Encode,
	Decode,
	DebugNoBound,
	TypeInfo,
	MaxEncodedLen,
	DecodeWithMemTracking,
)]
pub enum DepositKind<AccountId> {
	/// Deposit is held because a recovery configuration has been created.
	RecoveryConfig,
	/// Deposit is held because an active recovery process has been initiated for this account.
	ActiveRecoveryFor(AccountId),
}
pub type DepositKindOf<T> = DepositKind<<T as frame_system::Config>::AccountId>;

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
		type Currency: frame_support::traits::fungible::MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Consideration for holding a non-slashable deposit.
		type Consideration: Consideration<Self::AccountId, Footprint>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of friends per account config.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors.
		#[pallet::constant]
		type MaxFriendsPerConfig: Get<u16>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of configs per account.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors.
		#[pallet::constant]
		type MaxConfigsPerAccount: Get<u16>;

		/// DO NOT REDUCE THIS VALUE. Maximum number of ongoing recoveries per recoverer.
		///
		/// Reducing this value can cause decoding errors in the bounded vectors. This value should generally be be no less than `MaxConfigsPerAccount`.
		#[pallet::constant]
		type MaxOngoingRecoveriesPerRecoverer: Get<u16>;

		type MaxInheritorsPerAccount: Get<u16>;
	}

	/// Events type.
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		LostAccountControlled {
			lost: T::AccountId,
			inheritor: T::AccountId,
			call_hash: H256,
			call_result: DispatchResult,
		},
		FriendGroupsChanged {
			lost: T::AccountId,
			old_friend_groups: FriendGroupsOf<T>,
		},
		AttemptInitiated {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: FriendGroupOF<T>,
		},
		AttemptApproved {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: FriendGroupOF<T>,
			friend: T::AccountId,
		},
		AttemptFinished {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: FriendGroupOF<T>,
		},
		AttemptAborted {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: FriendGroupOF<T>,
		},
		AttemptSlashed {
			lost: T::AccountId,
			recoverer: T::AccountId,
			attempt_index: FriendGroupOF<T>,
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
		/// The caller is not the inheritor of the lost account.
		NotInheritor,
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
			let origin = frame_system::RawOrigin::Signed(lost).into();
			let call_result = call.dispatch(origin);

			Self::deposit_event(Event::<T>::LostAccountControlled {
				lost,
				inheritor,
				call_hash: call.hash(),
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
		pub fn set_friend_groups(
			origin: OriginFor<T>,
			friend_groups: Vec<FriendGroupOf<T>>,
		) -> DispatchResult {
			let lost = ensure_signed(origin)?;

			let current_friend_groups = FriendGroups::<T>::get(&lost).unwrap_or_default();
			let new_friend_groups = friend_groups.try_sane_and_bound()?;

			if new_friend_groups != current_friend_groups {
				FriendGroups::<T>::insert(lost, new_friend_groups);

				Self::deposit_event(Event::<T>::FriendGroupsChanged {
					lost,
					old_friend_groups,
				});
			}

			Ok(())
		}

		/// Attempt to recover a lost account by a friend with the given friend group.
		///
		/// The friend group is passed in as witness to ensure that the recoverer is not operating on stale friend group data and is making wrong assumptions about the delay or deposit amounts.
		// TODO event
		#[pallet::call_index(3)]
		pub fn initiate_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			friend_group: FriendGroupOf<T>,
		) -> DispatchResult {
			let maybe_friend = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;

			let friend_groups = FriendGroups::<T>::get(&lost).ok_or(Error::<T>::NoFriendGroups)?;
			ensure!(friend_groups.contains(&friend_group), Error::<T>::NoFriendGroup);

			// Construct the attempt
			let now = T::BlockNumberProvider::current_block_number();
			let unlock_at = now.checked_add(&friend_group.delay_period).ok_or(Arithmetic)?;
			let mut friends = friend_group.friends.into_iter().map(|f| (f, false)).collect();
			friends.sort();

			let attempt = RecoveryAttempt {
				unlock_at,
				deposit: friend_group.deposit,
				// TODO be smarter here with a bitmask
				friends,
				friends_needed: friend_group.friends_needed,
				abortable_at: now.checked_add(&friend_group.abort_delay).ok_or(Arithmetic)?,
			};

			let mut attempts_by_friend = Attempts::<T>::get(&lost, &maybe_friend);

			
			Ok(())
		}

		#[pallet::call_index(4)]
		pub fn approve_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: FriendGroupOF<T>,
		) -> DispatchResult {
			let maybe_friend = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let recoverer = T::Lookup::lookup(recoverer)?;
			let now = T::BlockNumberProvider::current_block_number();
			
			let mut attempts = Attempts::<T>::get(&lost, &recoverer);
			let mut attempt = attempts.get_mut(&attempt_index).ok_or(Error::<T>::NotAttempt)?;

			// Execute the vote
			// TODO bin search
			let mut vote = attempt.friends.find_mut(|(friend, _)| *friend == maybe_friend).ok_or(Error::<T>::NotFriend)?;
			if vote.1 {
				return Err(Error::<T>::AlreadyVouched.into());
			}
			// TODO event
			vote.1 = true;
			attempt.abortable_at = now.checked_add(&friend_group.abort_delay).ok_or(Arithmetic)?;

			Attempts::<T>::insert(&lost, &recoverer, attempts);

			Ok(())
		}

		#[pallet::call_index(5)]
		pub fn finish_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			recoverer: AccountIdLookupOf<T>,
			attempt_index: FriendGroupOF<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let recoverer = T::Lookup::lookup(recoverer)?;
			

			let now = T::BlockNumberProvider::current_block_number();
			let mut attempts = Attempts::<T>::get(&lost, &recoverer);
			let attempt = attempts.get_mut(&attempt_index).ok_or(Error::<T>::NotAttempt)?;

			// Check if the attempt is now complete
			let vouched = attempt.friends.iter().filter(|(_, v)| *v).count();
			ensure!(vouched >= attempt.friends_needed, Error::<T>::NotEnoughVouches);
			ensure!(now >= attempt.unlock_at, Error::<T>::NotUnlocked);
			// NOTE: We dont need to check the abort delay, since enough friends voted and we dont
			// assume full malicious behavior.

			attempts.remove(&attempt_index);

			// todo event
			match Inheritor::<T>::get(&lost) {
				None => Inheritor::<T>::insert(&lost, (attempt.inheritance_order, recoverer)),
				Some((old_order, _)) if attempt.inheritance_order < old_order => {
					// new recovery has a lower inheritance order, we therefore replace the existing inheritor
					Inheritor::<T>::insert(&lost, (attempt.inheritance_order, recoverer));
				}
				Some(_) => {
					// the existing inheritor stays since an equal or worse inheritor contested
					// TODO event
				}
			}

			Self::write_attempts(&lost, &recoverer, attempts);

			Ok(())
		}

		/// The recoverer or the lost account can abort an attempt at any moment.
		///
		/// This will release the deposit of the attempt back to the recoverer.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::close_recovery(T::MaxFriends::get()))]
		pub fn abort_attempt(
			origin: OriginFor<T>,
			attempt_index: FriendGroupOF<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let rescuer = T::Lookup::lookup(rescuer)?;
			
			let mut attempts = Attempts::<T>::get(&lost, &rescuer);
			let attempt = attempts.get_mut(&attempt_index).ok_or(Error::<T>::NotAttempt)?;

			ensure!(now >= attempt.abortable_at, Error::<T>::NotAbortable);
			// NOTE: It is possible to abort a fully approved attempt, but since we check the abort
			// delay, we ensure that every friend had enough time to call `finish_attempt`.
			attempts.remove(&attempt_index);

			Self::write_attempts(&lost, &rescuer, attempts);

			// TODO currency stuff

			Ok(())
		}

		#[pallet::call_index(7)]
		pub fn slash_attempt(
			origin: OriginFor<T>,
			rescuer: AccountIdLookupOf<T>,
			attempt_index: FriendGroupOF<T>,
		) -> DispatchResult {
			let lost = ensure_signed(origin)?;
			let rescuer = T::Lookup::lookup(rescuer)?;
			let now = T::BlockNumberProvider::current_block_number();

			let mut attempts = Attempts::<T>::get(&lost, &rescuer);
			let attempt = attempts.get_mut(&attempt_index).ok_or(Error::<T>::NotAttempt)?;

			attempts.remove(&attempt_index);
			// TODO slash

			Self::write_attempts(&lost, &rescuer, attempts);

			// TODO currency stuff

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Check that friends list is sorted and has no duplicates.
	fn is_sorted_and_unique(friends: &Vec<T::AccountId>) -> bool {
		friends.windows(2).all(|w| w[0] < w[1])
	}

	fn write_attempts(lost: &T::AccountId, recoverer: &T::AccountId, attempts: RecoveryAttemptsOf<T>) {
		if attempts.is_empty() {
			Attempts::<T>::remove(lost, recoverer);
		} else {
			Attempts::<T>::insert(lost, recoverer, attempts);
		}
	}
}



impl<ProvidedBlockNumber, Balance, Friends: Ord> FriendGroup<ProvidedBlockNumber, Balance, Friends> {
	fn ensure_sane(&self) -> Result<(), DispatchError> {
		ensure!(Self::is_sorted_and_unique(&self.friends), Error::<T>::NotSorted);
		ensure!(self.friends_needed <= self.friends.len(), Error::<T>::NotEnoughFriends);
		
		Ok(())
	}
}

// for friend groups
impl<T: Config> FriendGroups<T> {
	fn try_into_bounded(&self) -> Result<FriendGroupsOf<T>, DispatchError> {
		self.iter().map(|fg| fg.try_into_bounded()).try_into().map_err(|_| Error::<T>::MaxFriendGroups)?;
	}
}
