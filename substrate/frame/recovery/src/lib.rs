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
//! Pallet Recovery allows you to have friends or family recover access to your account if you lose
//! your seed phrase or private key.
//!
//! ## Terminology
//!
//! - `lost`: An account that has lost access to its private key and needs to be recovered.
//! - `friend`: A befriended account that can approve a recovery process.
//! - `recoverer`: An account that is trying to recover a lost account.
//! - `recovered`: An account that has been successfully recovered.
//! - `inheritor`: An account that is inheriting access to a lost account after recovery.
//! - `attempt`: An attempt to recover a lost account by a recoverer.
//! - `order`: The level of trust that an account has in a friend group.
//! - `deposit`: The amount that a friends of this group needs to reserve to initiate an attempt.
//! - `threshold`: The number of friends that need to approve an attempt.
//! - `delay`: How long an attempt will be delayed before it can succeed.
//! - `provided block`: The blocks that are *provided* by the `T::BlockNumberProvider`.
//!
//! ## Scenario: Recovering a lost account
//!
//! Story of how the user Alice user loses access and is recovered by her friends.
//!
//! 1. Alice uses the recovery pallet to configure one or more friends groups:
//! 	 - Alice picks a suitable `inheritor` account that will inherit the access to her account for
//!     each friend group. This could be a multisig.
//!  - Alice configures all groups with via `set_friend_groups`.
//! 2. Alice loses access to her account and becomes a `lost` account.
//! 3. Any member (aka `recoverer`) of Alice's friend groups become aware of the situation and
//!    starts a recovery `attempt` via `initiate_attempt`.
//! 4. The friend group self-organizes and one-by-one approve the ongoing attempt via
//!    `approve_attempt`.
//! 5. Exactly `threshold` friends approve the attempt (further approvals will fail since they are
//!    useless).
//! 6. Any account finishes the attempt via `finish_attempt` after at least `delay` blocks since the
//!    initiation have passed.
//! 7. Alice's account is now officially `recovered` and accessible by the `inheritor` account.
//! 8. The `inheritor` may call `control_inherited_account` at any point to transfer Alice's funds
//!    to her new account.
//!
//! ## Scenario: Multiple friend group try to recover an account
//!
//! Alice may have configured multiple friend groups that all try to recover her account at the same
//! time. This can lead to a conflict of which friend group should eventually inherit the access.
//!
//! 1. Alice configures groups *Family* (delay 10d, order 0) and *Friends* (delay 20d, order 1).
//! 1. Day 0: Alice loses access to her account.
//! 1. Day 6: *Friends* initiate a recovery attempt for Alice.
//! 1. Day 15: *Family* finally understands Polkadot and initiates an attempt as well.
//! 1. Day 25: *Family* inherits access to Alice account.
//! 1. Day 26: *Friends* group gets nothing since inheritance order is higher the one from *Family*.
//!
//! In the case above you see how the *Friends* group is now unable to recover Alice account since
//! the *Family* group already did it and has a higher inheritance order. Now, imagine the case that
//! the *Friends* group would have started on day 4 and would have already recovered the account on
//! day 24. Two days later, the *Family* group can take access back and will replace the inheritor
//! account with their own. The *Friends* group had access for two days since they were faster. If
//! Alice account has most balance locked in 28 day staking this would not make a big difference,
//! since only the free balance would be immediately transferable.
//!
//! ## Data Structures
//!
//! The pallet has three storage items, see the in-code docs [`FriendGroups`], [`Attempts`] and
//! [`Inheritor`]. Storage items may contain deposit "tickets" or similar noise and should therefore
//! not be read directly but only through the API.
//!
//! ## API
//!
//! *Reading* data can be done through the view functions:
//! -

use frame::{
	prelude::*,
	traits::{
		fungible::{Inspect, MutateHold},
		Consideration, Footprint,
	},
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
pub type BalanceOf<T> = <<T as Config>::Currency as Inspect<AccountIdFor<T>>>::Balance;
/// The block number type that will be used to measure time.
pub type ProvidedBlockNumberOf<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
pub type FriendsOf<T> =
	BoundedVec<<T as frame_system::Config>::AccountId, <T as Config>::MaxFriendsPerConfig>;
pub type HashOf<T> = <T as frame_system::Config>::Hash;

pub type InheritanceOrder = u32;

/// Configuration for recovering an account.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Encode,
	Decode,
	Default,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	DecodeWithMemTracking,
)]
pub struct FriendGroup<ProvidedBlockNumber, AccountId, Balance, Friends> {
	/// Slashable deposit that the rescuer needs to reserve.
	pub deposit: Balance,
	/// List of friends that can initiate the recovery process. Always sorted.
	pub friends: Friends,
	/// The number of approving friends needed to recover an account.
	pub friends_needed: u32,
	/// The account that inherited full access to a lost account after successful recovery.
	pub inheritor: AccountId,
	/// Minimum time that a recovery attempt must stay active before it can be finished.
	///
	/// Uses a provided block number to avoid possible clock skew of parachains.
	pub inheritance_delay: ProvidedBlockNumber,
	/// Used to resolve inheritance conflicts when multiple friend groups finish a recovery.
	///
	/// Lower order friend groups can replace the inheritor of a higher order group. For example:
	/// You can set your family group as order 0, your friends group as order 1 and co-workers as
	/// group 2. This in combination with the `inheritance_delay` enables you to ensure that the
	/// correct group receives the inheritance.
	pub inheritance_order: InheritanceOrder,
	/// The delay since the last approval of an attempt before the attempt can be aborted.
	///
	/// It ensures that a malicious recoverer does not abuse the `abort_attempt` call to doge an
	/// incoming slash from the lost account. They could otherwise monitor the TX pool and abort
	/// the attempt just in time for the slash transaction to fail. Now instead, the lost account
	/// has at least `abort_delay` provided blocks to slash the attempt.
	pub abort_delay: ProvidedBlockNumber,
}

pub type FriendGroupIndex = u32;

/// A `FriendGroup` for a specific `Config`.
pub type FriendGroupOf<T> =
	FriendGroup<ProvidedBlockNumberOf<T>, AccountIdFor<T>, BalanceOf<T>, FriendsOf<T>>;

pub type FriendGroupsOf<T> = BoundedVec<FriendGroupOf<T>, <T as Config>::MaxConfigsPerAccount>;

/// Bitfield helper for tracking friend votes.
///
/// Uses a vector of u128 values where each bit represents whether a friend at that index has voted.
#[derive(
	CloneNoBound, EqNoBound, PartialEqNoBound, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(MaxEntries))]
pub struct Bitfield<MaxEntries: Get<u32>>(pub BoundedVec<u128, BitfieldLenOf<MaxEntries>>);

pub type BitfieldLenOf<MaxEntries> = ConstDivCeil<MaxEntries, ConstU32<128>, u32, u32>;

pub struct ConstDivCeil<Dividend, Divisor, R, T>(
	pub core::marker::PhantomData<(Dividend, Divisor, R, T)>,
);
impl<Dividend: Get<T>, Divisor: Get<T>, R: AtLeast32BitUnsigned, T: Into<R>> Get<R>
	for ConstDivCeil<Dividend, Divisor, R, T>
{
	fn get() -> R {
		123u32.into()
	}
}

impl<MaxEntries: Get<u32>> Default for Bitfield<MaxEntries> {
	fn default() -> Self {
		Self(
			vec![0u128; BitfieldLenOf::<MaxEntries>::get() as usize]
				.try_into()
				.defensive()
				.unwrap_or_default(),
		) // todo error
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

pub type ApprovalBitfield<MaxFriends> = Bitfield<MaxFriends>;
pub type ApprovalBitfieldOf<T> = ApprovalBitfield<<T as Config>::MaxFriendsPerConfig>;

/// An attempt to recover an account.
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Attempt<ProvidedBlockNumber, ApprovalBitfield> {
	pub friend_group_index: FriendGroupIndex,
	pub init_block: ProvidedBlockNumber,
	pub last_approval_block: ProvidedBlockNumber,
	/// Bitfield tracking which friends approved.
	///
	/// Each bit corresponds to a friend in the `friend_group.friends` list by index.
	pub approvals: ApprovalBitfield,
}

impl<ProvidedBlockNumber, ApprovalBitfield> Attempt<ProvidedBlockNumber, ApprovalBitfield>
where
	ProvidedBlockNumber: CheckedAdd,
{
	/// Calculate the earliest block when the attempt can be aborted.
	///
	/// This is the last approval block plus the abort delay from the friend group. Returns None if
	/// overflow occurs.
	pub fn abortable_at<AccountId, Balance, Friends>(
		&self,
		friend_groups: &[FriendGroup<ProvidedBlockNumber, AccountId, Balance, Friends>],
	) -> Option<ProvidedBlockNumber> {
		let fg = friend_groups.get(self.friend_group_index as usize)?;
		self.last_approval_block.checked_add(&fg.abort_delay)
	}
}

pub type AttemptOf<T> = Attempt<ProvidedBlockNumberOf<T>, ApprovalBitfieldOf<T>>;

/// A `Consideration`-like type that tracks who paid for it.
#[derive(
	Clone,
	Eq,
	PartialEq,
	Encode,
	Decode,
	Default,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	DecodeWithMemTracking,
)]
pub struct IdentifiedConsideration<AccountId, Footprint, C> {
	pub depositor: AccountId,
	pub ticket: C,
	pub _phantom: PhantomData<Footprint>,
}

impl<AccountId: Clone + Eq, Footprint, C: Consideration<AccountId, Footprint>>
	IdentifiedConsideration<AccountId, Footprint, C>
{
	fn new(depositor: &AccountId, fp: Footprint) -> Result<Self, DispatchError> {
		let ticket = Consideration::<AccountId, Footprint>::new(depositor, fp)?;

		Ok(Self { depositor: depositor.clone(), ticket, _phantom: Default::default() })
	}

	fn update(self, new_depositor: &AccountId, fp: Footprint) -> Result<Self, DispatchError> {
		if *new_depositor != self.depositor {
			self.ticket.drop(&self.depositor)?;
		}

		let ticket = Consideration::<AccountId, Footprint>::new(&new_depositor, fp)?;
		Ok(Self { depositor: new_depositor.clone(), ticket, _phantom: Default::default() })
	}

	fn drop(self) -> Result<(), DispatchError> {
		self.ticket.drop(&self.depositor)
	}
}

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching call type.
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ From<frame_system::Call<Self>>;

		/// The overarching freeze reason.
		type RuntimeHoldReason: Parameter + Member + MaxEncodedLen + Copy + VariantCount;

		/// Query the block number that will be used to measure time.
		///
		/// Must return monotonically increasing values when called from consecutive blocks. Can be
		/// configured to return either:
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

		/// Storage consideration for holding friend group configs.
		type FriendGroupsConsideration: Consideration<Self::AccountId, Footprint>;

		type AttemptConsideration: Consideration<Self::AccountId, Footprint>;

		type InheritorConsideration: Consideration<Self::AccountId, Footprint>;

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

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// The friend groups of an that can conduct recovery attempts.
	///
	/// Modifying this storage does not impact ongoing recovery attempts.
	#[pallet::storage]
	pub type FriendGroups<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		(FriendGroupsOf<T>, T::FriendGroupsConsideration),
	>;

	/// Ongoing recovery attempts of a lost account indexed by `(lost, friend_group)`.
	#[pallet::storage]
	pub type Attempt<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		FriendGroupIndex,
		(AttemptOf<T>, T::AttemptConsideration),
	>;

	/// The account that inherited full access to a lost account after successful recovery.
	///
	/// NOTE: This could be a multisig or proxy account
	#[pallet::storage]
	pub type Inheritor<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		(InheritanceOrder, T::AccountId, T::InheritorConsideration),
	>;

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
			friend_group_index: FriendGroupIndex,
			recoverer: T::AccountId,
		},
		AttemptApproved {
			lost: T::AccountId,
			friend_group_index: FriendGroupIndex,
			friend: T::AccountId,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The lost account has ongoing recovery attempts.
		HasOngoingAttempts,
		/// The recovery attempt has already been initiated.
		AlreadyInitiated,
		/// This account does not have any friend groups.
		NoFriendGroups,
		/// A specific referenced friend group was not found.
		NotFriendGroup,
		/// The caller is not a friend of the lost account.
		NotFriend,
		/// The referenced recovery attempt was not found.
		NotAttempt,
		/// This attempt is already fully approved and does not need any more votes.
		AlreadyApproved,
		/// The friend already voted for this attempt.
		AlreadyVoted,
		/// The lost account does not have any inheritor.
		NoInheritor,
		/// The caller is not the inheritor of the lost account.
		NotInheritor,
		/// Not enough friends approved this attempt.
		NotEnoughApprovals,
		/// The recovery attempt is not yet unlocked.
		NotUnlocked,
		/// The recovery attempt cannot be aborted yet.
		NotAbortable,
		/// Too many concurrent recovery attempts for this recoverer.
		TooManyAttempts,
		/// The inheritance delay of this attempt has not yet passed.
		NotYetInheritable,
	}

	#[pallet::view_functions]
	impl<T: Config> Pallet<T> {
		/// The friend groups of an account that can conduct recovery attempts.
		pub fn friend_groups(lost: T::AccountId) -> Vec<FriendGroupOf<T>> {
			FriendGroups::<T>::get(lost).map(|(g, _t)| g.into_inner()).unwrap_or_default()
		}

		/*pub fn attempt(lost: T::AccountId, friend_group_index: u32) -> Option<AttemptOf<T>> {
			Attempts::<T>::get(lost, friend_group_index).map(|(ass, _t)| ass.get(0 as usize).cloned()).flatten()
		}*/

		pub fn provided_block_number() -> ProvidedBlockNumberOf<T> {
			T::BlockNumberProvider::current_block_number()
		}

		pub fn inheritor(lost: T::AccountId) -> Option<T::AccountId> {
			Inheritor::<T>::get(lost).map(|(_, inheritor, _)| inheritor)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		// todo bin search todo event todo copy call filters
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn control_inherited_account(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			let maybe_inheritor = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;

			let inheritor = Inheritor::<T>::get(&lost)
				.map(|(_, inheritor, _ticket)| inheritor)
				.ok_or(Error::<T>::NoInheritor)?;
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

			if Attempt::<T>::iter_prefix(&lost).next().is_some() {
				return Err(Error::<T>::HasOngoingAttempts.into());
			}

			let (old_friend_groups, old_ticket) = match FriendGroups::<T>::get(&lost) {
				Some((g, t)) => (g, Some(t)),
				None => Default::default(),
			};
			let new_friend_groups: FriendGroupsOf<T> =
				friend_groups.try_into().map_err(|_| "Too many friend groups")?;
			let new_footprint = Self::friend_group_footprint(&new_friend_groups);

			let new_ticket = Self::update_ticket(&lost, old_ticket, new_footprint)?;
			FriendGroups::<T>::insert(&lost, (&new_friend_groups, &new_ticket));

			if new_friend_groups != old_friend_groups {
				Self::deposit_event(Event::<T>::FriendGroupsChanged { lost, old_friend_groups });
			}

			Ok(())
		}

		/// Attempt to recover a lost account by a friend with the given friend group.
		///
		/// The friend group is passed in as witness to ensure that the recoverer is not operating
		/// on stale friend group data and is making wrong assumptions about the delay or deposit
		/// amounts.
		// TODO event
		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn initiate_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			friend_group_index: FriendGroupIndex,
		) -> DispatchResult {
			let recoverer = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;

			if Self::attempt_of(&lost, friend_group_index).is_ok() {
				return Err(Error::<T>::AlreadyInitiated.into());
			}

			let friend_group = Self::friend_group_of(&lost, friend_group_index)?;
			ensure!(friend_group.friends.contains(&recoverer), Error::<T>::NotFriend);

			// Construct the attempt
			let now = T::BlockNumberProvider::current_block_number();
			let attempt = AttemptOf::<T> {
				friend_group_index,
				init_block: now,
				last_approval_block: now,
				approvals: ApprovalBitfield::default(),
			};

			let footprint =
				T::AttemptConsideration::new(&recoverer, Self::attempt_footprint(&attempt))?;
			Attempt::<T>::insert(&lost, friend_group_index, (&attempt, &footprint));

			Self::deposit_event(Event::<T>::AttemptInitiated {
				lost,
				friend_group_index,
				recoverer,
			});

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn approve_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			friend_group_index: FriendGroupIndex,
		) -> DispatchResult {
			let friend = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let now = T::BlockNumberProvider::current_block_number();

			let (mut attempt, old_ticket) = Self::attempt_of(&lost, friend_group_index)?;
			let friend_group = Self::friend_group_of(&lost, friend_group_index).defensive()?;

			let friends_voted = attempt.approvals.count_ones();
			ensure!(friends_voted < friend_group.friends_needed, Error::<T>::AlreadyApproved);

			let friend_index = friend_group
				.friends
				.iter()
				.position(|f| f == &friend)
				.ok_or(Error::<T>::NotFriend)?;
			attempt
				.approvals
				.set_if_not_set(friend_index)
				.map_err(|_| Error::<T>::AlreadyVoted)?;

			let footprint = Self::attempt_footprint(&attempt);
			let new_ticket = old_ticket.update(&friend, footprint)?;
			Attempt::<T>::insert(&lost, friend_group_index, (&attempt, &new_ticket));

			Self::deposit_event(Event::<T>::AttemptApproved { lost, friend_group_index, friend });

			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn finish_attempt(
			origin: OriginFor<T>,
			lost: AccountIdLookupOf<T>,
			attempt_index: u32,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;
			let lost = T::Lookup::lookup(lost)?;
			let now = T::BlockNumberProvider::current_block_number();

			let (attempt, attempts_ticket) =
				Attempt::<T>::get(&lost, &attempt_index).ok_or(Error::<T>::NotAttempt)?;

			// AUDIT: attempt_index == friend_group_index
			let friend_group = Self::friend_group_of(&lost, attempt_index).defensive()?;

			// Check if the attempt is now complete
			let approvals = attempt.approvals.count_ones();
			ensure!(
				// We use >= defensively, but it should be at most ==
				approvals >= friend_group.friends_needed,
				Error::<T>::NotEnoughApprovals
			);

			let inheritable_at = attempt
				.init_block
				.checked_add(&friend_group.inheritance_delay)
				.ok_or(ArithmeticError::Overflow)?;
			ensure!(now >= inheritable_at, Error::<T>::NotYetInheritable);
			// NOTE: We dont need to check the abort delay, since enough friends voted and we dont
			// assume fully malicious behavior.

			let inheritor = friend_group.inheritor;
			let inheritance_order = friend_group.inheritance_order;

			// todo event
			match Inheritor::<T>::get(&lost) {
				None => {
					let ticket = Self::inheritor_ticket(&caller)?;
					Inheritor::<T>::insert(&lost, (inheritance_order, &inheritor, ticket))
				},
				// new recovery has a lower inheritance order, we therefore replace the existing
				// inheritor
				Some((old_order, _, ticket)) if inheritance_order < old_order => {
					// We have to update the ticket since we don't know who created it:
					let ticket = ticket.update(&caller, Self::inheritor_footprint())?;
					Inheritor::<T>::insert(&lost, (inheritance_order, &inheritor, ticket));
				},
				Some(_) => {
					// The existing inheritor stays since an equal or worse inheritor contested.
					// We do not treat this as a poke but just do nothing.
				},
			}

			//Self::write_attempts(&lost, &recoverer, &attempts, aticket)?;

			Ok(())
		}

		/*
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

			let (mut attempts, ticket) =
				Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let attempt = attempts.get(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			let abortable_at = attempt.abortable_at().ok_or(ArithmeticError::Overflow)?;
			ensure!(now >= abortable_at, Error::<T>::NotAbortable);
			// NOTE: It is possible to abort a fully approved attempt, but since we check the abort
			// delay, we ensure that every friend had enough time to call `finish_attempt`.
			attempts.remove(attempt_index as usize);

			Self::write_attempts(&lost, &recoverer, &attempts, ticket)?;

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

			let (mut attempts, ticket) =
				Attempts::<T>::get(&lost, &recoverer).ok_or(Error::<T>::NotAttempt)?;
			let _attempt = attempts.get(attempt_index as usize).ok_or(Error::<T>::NotAttempt)?;

			attempts.remove(attempt_index as usize);
			// TODO slash

			Self::write_attempts(&lost, &recoverer, &attempts, ticket)?;

			// TODO currency stuff

			Ok(())
		}*/
	}
}

impl<T: Config> Pallet<T> {
	pub fn friend_group_footprint(friend_groups: &FriendGroupsOf<T>) -> Footprint {
		// TODO think about this. maybe we just use items_count * item_mel
		Footprint::from_encodable(friend_groups)
	}

	pub fn attempt_footprint(attempt: &AttemptOf<T>) -> Footprint {
		// TODO think about this. maybe we just use items_count * item_mel
		Footprint::from_encodable(attempt)
	}

	pub fn inheritor_footprint() -> Footprint {
		Footprint::from_mel::<(InheritanceOrder, T::AccountId)>()
	}

	pub fn inheritor_ticket(
		who: &T::AccountId,
	) -> Result<T::InheritorConsideration, DispatchError> {
		T::InheritorConsideration::new(&who, Self::inheritor_footprint())
	}

	pub fn friend_group_of(
		lost: &T::AccountId,
		friend_group_index: u32,
	) -> Result<FriendGroupOf<T>, Error<T>> {
		let friend_groups = match FriendGroups::<T>::get(lost) {
			Some((g, _t)) => g,
			None => return Err(Error::<T>::NoFriendGroups),
		};
		friend_groups
			.get(friend_group_index as usize)
			.cloned()
			.ok_or(Error::<T>::NotFriendGroup)
	}

	pub fn attempt_of(
		lost: &T::AccountId,
		friend_group_index: u32,
	) -> Result<(AttemptOf<T>, T::AttemptConsideration), Error<T>> {
		pallet::Attempt::<T>::get(lost, friend_group_index).ok_or(Error::<T>::NotAttempt)
	}

	fn update_ticket<C: Consideration<T::AccountId, Footprint>>(
		who: &T::AccountId,
		old_ticket: Option<C>,
		new_footprint: Footprint,
	) -> Result<C, DispatchError> {
		match old_ticket {
			Some(old_ticket) => old_ticket.update(who, new_footprint),
			None => C::new(who, new_footprint),
		}
	}
}
