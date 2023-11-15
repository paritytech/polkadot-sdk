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

//! # Identity Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! A federated naming system, allowing for multiple registrars to be added from a specified origin.
//! Registrars can set a fee to provide identity-verification service. Anyone can put forth a
//! proposed identity for a fixed deposit and ask for review by any number of registrars (paying
//! each of their fees). Registrar judgements are given as an `enum`, allowing for sophisticated,
//! multi-tier opinions.
//!
//! Some judgements are identified as *sticky*, which means they cannot be removed except by
//! complete removal of the identity, or by the registrar. Judgements are allowed to represent a
//! portion of funds that have been reserved for the registrar.
//!
//! A super-user can remove accounts and in doing so, slash the deposit.
//!
//! All accounts may also have a limited number of sub-accounts which may be specified by the owner;
//! by definition, these have equivalent ownership and each has an individual name.
//!
//! The number of registrars should be limited, and the deposit made sufficiently large, to ensure
//! no state-bloat attack is viable.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! #### For general users
//! * `set_identity` - Set the associated identity of an account; a small deposit is reserved if not
//!   already taken.
//! * `clear_identity` - Remove an account's associated identity; the deposit is returned.
//! * `request_judgement` - Request a judgement from a registrar, paying a fee.
//! * `cancel_request` - Cancel the previous request for a judgement.
//!
//! #### For general users with sub-identities
//! * `set_subs` - Set the sub-accounts of an identity.
//! * `add_sub` - Add a sub-identity to an identity.
//! * `remove_sub` - Remove a sub-identity of an identity.
//! * `rename_sub` - Rename a sub-identity of an identity.
//! * `quit_sub` - Remove a sub-identity of an identity (called by the sub-identity).
//!
//! #### For registrars
//! * `set_fee` - Set the fee required to be paid for a judgement to be given by the registrar.
//! * `set_fields` - Set the fields that a registrar cares about in their judgements.
//! * `provide_judgement` - Provide a judgement to an identity.
//!
//! #### For super-users
//! * `add_registrar` - Add a new registrar to the system.
//! * `kill_identity` - Forcibly remove the associated identity; the deposit is lost.
//!
//! [`Call`]: ./enum.Call.html
//! [`Config`]: ./trait.Config.html

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod legacy;
#[cfg(test)]
mod tests;
mod types;
pub mod weights;

use crate::types::{Username, SUFFIX_MAX_LENGTH, USERNAME_MAX_LENGTH};
use codec::Encode;
use frame_support::{
	ensure,
	pallet_prelude::{DispatchError, DispatchResult},
	traits::{BalanceStatus, Currency, Get, OnUnbalanced, ReservableCurrency},
};
pub use pallet::*;
use sp_runtime::{
	traits::{AppendZerosInput, Hash, IdentifyAccount, Saturating, StaticLookup, Verify, Zero},
	MultiSignature,
};
use sp_std::prelude::*;
pub use types::{
	AccountIdentifier, Data, IdentityInformationProvider, Judgement, RegistrarIndex, RegistrarInfo,
	Registration,
};
pub use weights::WeightInfo;

type BalanceOf<T> =
	<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<
	<T as frame_system::Config>::AccountId,
>>::NegativeImbalance;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency trait.
		type Currency: ReservableCurrency<Self::AccountId>;

		/// The amount held on deposit for a registered identity.
		#[pallet::constant]
		type BasicDeposit: Get<BalanceOf<Self>>;

		/// The amount held on deposit per encoded byte for a registered identity.
		#[pallet::constant]
		type ByteDeposit: Get<BalanceOf<Self>>;

		/// The amount held on deposit for a registered subaccount. This should account for the fact
		/// that one storage item's value will increase by the size of an account ID, and there will
		/// be another trie item whose value is the size of an account ID plus 32 bytes.
		#[pallet::constant]
		type SubAccountDeposit: Get<BalanceOf<Self>>;

		/// The maximum number of sub-accounts allowed per identified account.
		#[pallet::constant]
		type MaxSubAccounts: Get<u32>;

		/// Structure holding information about an identity.
		type IdentityInformation: IdentityInformationProvider;

		/// Maxmimum number of registrars allowed in the system. Needed to bound the complexity
		/// of, e.g., updating judgements.
		#[pallet::constant]
		type MaxRegistrars: Get<u32>;

		/// What to do with slashed funds.
		type Slashed: OnUnbalanced<NegativeImbalanceOf<Self>>;

		/// The origin which may forcibly set or remove a name. Root can always do this.
		type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The origin which may add or remove registrars. Root can always do this.
		type RegistrarOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The amount held on deposit for each username.
		///
		/// Guide: 1 storage item, 32 bytes.
		type UsernameDeposit: Get<BalanceOf<Self>>;

		/// The origin which may add or remove username authorities. Root can always do this.
		type UsernameAuthorityOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The number of blocks within which a username authorization must be validated.
		type PreapprovalExpiration: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Information that is pertinent to identify the entity behind an account.
	///
	/// TWOX-NOTE: OK ― `AccountId` is a secure hash.
	//
	// TODO: Consider setting a username limit per account and storing a BoundedVec of all of an
	// account's usernames. Obvious downside is storing a vec that may not be full, but without it,
	// when a user calls `clear_identity` there is no way to clean up all their username accounts
	// without looping through all the map keys. Alternate way would be to introduce a new
	// dispatchable, callable by anyone, that could identify a username with no key in `IdentityOf`
	// and remove the username from storage.
	#[pallet::storage]
	#[pallet::getter(fn identity)]
	pub(super) type IdentityOf<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		(Registration<BalanceOf<T>, T::MaxRegistrars, T::IdentityInformation>, Option<Username>),
		OptionQuery,
	>;

	/// The super-identity of an alternative "sub" identity together with its name, within that
	/// context. If the account is not some other account's sub-identity, then just `None`.
	#[pallet::storage]
	#[pallet::getter(fn super_of)]
	pub(super) type SuperOf<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, (T::AccountId, Data), OptionQuery>;

	/// Alternative "sub" identities of this account.
	///
	/// The first item is the deposit, the second is a vector of the accounts.
	///
	/// TWOX-NOTE: OK ― `AccountId` is a secure hash.
	#[pallet::storage]
	#[pallet::getter(fn subs_of)]
	pub(super) type SubsOf<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		(BalanceOf<T>, BoundedVec<T::AccountId, T::MaxSubAccounts>),
		ValueQuery,
	>;

	/// The set of registrars. Not expected to get very big as can only be added through a
	/// special origin (likely a council motion).
	///
	/// The index into this can be cast to `RegistrarIndex` to get a valid value.
	#[pallet::storage]
	#[pallet::getter(fn registrars)]
	pub(super) type Registrars<T: Config> = StorageValue<
		_,
		BoundedVec<
			Option<
				RegistrarInfo<
					BalanceOf<T>,
					T::AccountId,
					<T::IdentityInformation as IdentityInformationProvider>::FieldsIdentifier,
				>,
			>,
			T::MaxRegistrars,
		>,
		ValueQuery,
	>;

	/// A map of the accounts who are authorized to grant usernames. The value is a byte vector that
	/// represents a suffix appended to the username (e.g. ".wallet"). Suffixes should be unique and
	/// start with a `.`, but it is up to the authority selection process (nominally, governance) to
	/// enforce that.
	#[pallet::storage]
	#[pallet::getter(fn authority)]
	pub(super) type UsernameAuthorities<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		BoundedVec<u8, ConstU32<SUFFIX_MAX_LENGTH>>,
		OptionQuery,
	>;

	/// Reverse lookup from `username` to the `AccountId` that has registered it. The value should
	/// be a key in the `IdentityOf` map. May return `None` in the case that an Identity owner has
	/// not registered a username.
	///
	/// Multiple usernames may map to the same `AccountId`, but `IdentityOf` will only map to one
	/// primary username.
	#[pallet::storage]
	#[pallet::getter(fn username)]
	pub(super) type AccountOfUsername<T: Config> =
		StorageMap<_, Twox64Concat, Username, T::AccountId, OptionQuery>;

	/// Check if a user has pre-approved a username. Used primarily in cases where the `AccountId`
	/// cannot provide a signature because they are a pure proxy, multisig, etc.
	///
	/// First tuple item is the username, second is the deposit held, and third is its expiration.
	#[pallet::storage]
	#[pallet::getter(fn preapproved_usernames)]
	pub(super) type PreapprovedUsernames<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AccountId,
		(Username, BalanceOf<T>, BlockNumberFor<T>),
		OptionQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too many subs-accounts.
		TooManySubAccounts,
		/// Account isn't found.
		NotFound,
		/// Account isn't named.
		NotNamed,
		/// Empty index.
		EmptyIndex,
		/// Fee is changed.
		FeeChanged,
		/// No identity found.
		NoIdentity,
		/// Sticky judgement.
		StickyJudgement,
		/// Judgement given.
		JudgementGiven,
		/// Invalid judgement.
		InvalidJudgement,
		/// The index is invalid.
		InvalidIndex,
		/// The target is invalid.
		InvalidTarget,
		/// Maximum amount of registrars reached. Cannot add any more.
		TooManyRegistrars,
		/// Account ID is already named.
		AlreadyClaimed,
		/// Sender is not a sub-account.
		NotSub,
		/// Sub-account isn't owned by sender.
		NotOwned,
		/// The provided judgement was for a different identity.
		JudgementForDifferentIdentity,
		/// Error that occurs when there is an issue paying for judgement.
		JudgementPaymentFailed,
		/// The provided suffix is too long.
		InvalidSuffix,
		/// The sender does not have permission to issue a username.
		NotUsernameAuthority,
		/// The signature on a username was not valid.
		InvalidSignature,
		/// Setting this username requires a signature, but none was provided.
		RequiresSignature,
		/// The username does not meet the requirements.
		InvalidUsername,
		/// The username is already taken.
		UsernameTaken,
		/// The requested username does not exist.
		NoUsername,
		/// The username cannot be forcefully removed because its preapproval has not expired.
		NotExpired,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A name was set or reset (which will remove all judgements).
		IdentitySet { who: T::AccountId },
		/// A name was cleared, and the given balance returned.
		IdentityCleared { who: T::AccountId, deposit: BalanceOf<T> },
		/// A name was removed and the given balance slashed.
		IdentityKilled { who: T::AccountId, deposit: BalanceOf<T> },
		/// A judgement was asked from a registrar.
		JudgementRequested { who: T::AccountId, registrar_index: RegistrarIndex },
		/// A judgement request was retracted.
		JudgementUnrequested { who: T::AccountId, registrar_index: RegistrarIndex },
		/// A judgement was given by a registrar.
		JudgementGiven { target: T::AccountId, registrar_index: RegistrarIndex },
		/// A registrar was added.
		RegistrarAdded { registrar_index: RegistrarIndex },
		/// A sub-identity was added to an identity and the deposit paid.
		SubIdentityAdded { sub: T::AccountId, main: T::AccountId, deposit: BalanceOf<T> },
		/// A sub-identity was removed from an identity and the deposit freed.
		SubIdentityRemoved { sub: T::AccountId, main: T::AccountId, deposit: BalanceOf<T> },
		/// A sub-identity was cleared, and the given deposit repatriated from the
		/// main identity account to the sub-identity account.
		SubIdentityRevoked { sub: T::AccountId, main: T::AccountId, deposit: BalanceOf<T> },
		// TODO: Add events
	}

	#[pallet::call]
	/// Identity pallet declaration.
	impl<T: Config> Pallet<T> {
		/// Add a registrar to the system.
		///
		/// The dispatch origin for this call must be `T::RegistrarOrigin`.
		///
		/// - `account`: the account of the registrar.
		///
		/// Emits `RegistrarAdded` if successful.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_registrar(T::MaxRegistrars::get()))]
		pub fn add_registrar(
			origin: OriginFor<T>,
			account: AccountIdLookupOf<T>,
		) -> DispatchResultWithPostInfo {
			T::RegistrarOrigin::ensure_origin(origin)?;
			let account = T::Lookup::lookup(account)?;

			let (i, registrar_count) = <Registrars<T>>::try_mutate(
				|registrars| -> Result<(RegistrarIndex, usize), DispatchError> {
					registrars
						.try_push(Some(RegistrarInfo {
							account,
							fee: Zero::zero(),
							fields: Default::default(),
						}))
						.map_err(|_| Error::<T>::TooManyRegistrars)?;
					Ok(((registrars.len() - 1) as RegistrarIndex, registrars.len()))
				},
			)?;

			Self::deposit_event(Event::RegistrarAdded { registrar_index: i });

			Ok(Some(T::WeightInfo::add_registrar(registrar_count as u32)).into())
		}

		/// Set an account's identity information and reserve the appropriate deposit.
		///
		/// If the account already has identity information, the deposit is taken as part payment
		/// for the new deposit.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `info`: The identity information.
		///
		/// Emits `IdentitySet` if successful.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_identity(T::MaxRegistrars::get()))]
		pub fn set_identity(
			origin: OriginFor<T>,
			info: Box<T::IdentityInformation>,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			let (mut id, username) = match <IdentityOf<T>>::get(&sender) {
				Some((mut id, maybe_username)) => (
					{
						// Only keep non-positive judgements.
						id.judgements.retain(|j| j.1.is_sticky());
						id.info = *info;
						id
					},
					maybe_username,
				),
				None => (
					Registration {
						info: *info,
						judgements: BoundedVec::default(),
						deposit: Zero::zero(),
					},
					None,
				),
			};

			let new_deposit = Self::calculate_identity_deposit(&id.info);
			let old_deposit = id.deposit;
			Self::rejig_deposit(&sender, old_deposit, new_deposit)?;

			id.deposit = new_deposit;
			let judgements = id.judgements.len();
			<IdentityOf<T>>::insert(&sender, (id, username));
			Self::deposit_event(Event::IdentitySet { who: sender });

			Ok(Some(T::WeightInfo::set_identity(judgements as u32)).into())
		}

		/// Set the sub-accounts of the sender.
		///
		/// Payment: Any aggregate balance reserved by previous `set_subs` calls will be returned
		/// and an amount `SubAccountDeposit` will be reserved for each item in `subs`.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// identity.
		///
		/// - `subs`: The identity's (new) sub-accounts.
		// TODO: This whole extrinsic screams "not optimized". For example we could
		// filter any overlap between new and old subs, and avoid reading/writing
		// to those values... We could also ideally avoid needing to write to
		// N storage items for N sub accounts. Right now the weight on this function
		// is a large overestimate due to the fact that it could potentially write
		// to 2 x T::MaxSubAccounts::get().
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_subs_old(T::MaxSubAccounts::get())
			.saturating_add(T::WeightInfo::set_subs_new(subs.len() as u32))
		)]
		pub fn set_subs(
			origin: OriginFor<T>,
			subs: Vec<(T::AccountId, Data)>,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;
			ensure!(<IdentityOf<T>>::contains_key(&sender), Error::<T>::NotFound);
			ensure!(
				subs.len() <= T::MaxSubAccounts::get() as usize,
				Error::<T>::TooManySubAccounts
			);

			let (old_deposit, old_ids) = <SubsOf<T>>::get(&sender);
			let new_deposit = Self::subs_deposit(subs.len() as u32);

			let not_other_sub =
				subs.iter().filter_map(|i| SuperOf::<T>::get(&i.0)).all(|i| i.0 == sender);
			ensure!(not_other_sub, Error::<T>::AlreadyClaimed);

			if old_deposit < new_deposit {
				T::Currency::reserve(&sender, new_deposit - old_deposit)?;
			} else if old_deposit > new_deposit {
				let err_amount = T::Currency::unreserve(&sender, old_deposit - new_deposit);
				debug_assert!(err_amount.is_zero());
			}
			// do nothing if they're equal.

			for s in old_ids.iter() {
				<SuperOf<T>>::remove(s);
			}
			let mut ids = BoundedVec::<T::AccountId, T::MaxSubAccounts>::default();
			for (id, name) in subs {
				<SuperOf<T>>::insert(&id, (sender.clone(), name));
				ids.try_push(id).expect("subs length is less than T::MaxSubAccounts; qed");
			}
			let new_subs = ids.len();

			if ids.is_empty() {
				<SubsOf<T>>::remove(&sender);
			} else {
				<SubsOf<T>>::insert(&sender, (new_deposit, ids));
			}

			Ok(Some(
				T::WeightInfo::set_subs_old(old_ids.len() as u32) // P: Real number of old accounts removed.
					// S: New subs added
					.saturating_add(T::WeightInfo::set_subs_new(new_subs as u32)),
			)
			.into())
		}

		/// Clear an account's identity info and all sub-accounts and return all deposits.
		///
		/// Payment: All reserved balances on the account are returned.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// identity.
		///
		/// Emits `IdentityCleared` if successful.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::clear_identity(
			T::MaxRegistrars::get(),
			T::MaxSubAccounts::get(),
		))]
		pub fn clear_identity(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;

			let (subs_deposit, sub_ids) = <SubsOf<T>>::take(&sender);
			let (id, _username) = <IdentityOf<T>>::take(&sender).ok_or(Error::<T>::NotNamed)?;
			let deposit = id.total_deposit().saturating_add(subs_deposit);
			for sub in sub_ids.iter() {
				<SuperOf<T>>::remove(sub);
			}

			let err_amount = T::Currency::unreserve(&sender, deposit);
			debug_assert!(err_amount.is_zero());

			Self::deposit_event(Event::IdentityCleared { who: sender, deposit });

			#[allow(deprecated)]
			Ok(Some(T::WeightInfo::clear_identity(
				id.judgements.len() as u32,
				sub_ids.len() as u32,
			))
			.into())
		}

		/// Request a judgement from a registrar.
		///
		/// Payment: At most `max_fee` will be reserved for payment to the registrar if judgement
		/// given.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a
		/// registered identity.
		///
		/// - `reg_index`: The index of the registrar whose judgement is requested.
		/// - `max_fee`: The maximum fee that may be paid. This should just be auto-populated as:
		///
		/// ```nocompile
		/// Self::registrars().get(reg_index).unwrap().fee
		/// ```
		///
		/// Emits `JudgementRequested` if successful.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::request_judgement(T::MaxRegistrars::get(),))]
		pub fn request_judgement(
			origin: OriginFor<T>,
			#[pallet::compact] reg_index: RegistrarIndex,
			#[pallet::compact] max_fee: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;
			let registrars = <Registrars<T>>::get();
			let registrar = registrars
				.get(reg_index as usize)
				.and_then(Option::as_ref)
				.ok_or(Error::<T>::EmptyIndex)?;
			ensure!(max_fee >= registrar.fee, Error::<T>::FeeChanged);
			let (mut id, username) = <IdentityOf<T>>::get(&sender).ok_or(Error::<T>::NoIdentity)?;

			let item = (reg_index, Judgement::FeePaid(registrar.fee));
			match id.judgements.binary_search_by_key(&reg_index, |x| x.0) {
				Ok(i) =>
					if id.judgements[i].1.is_sticky() {
						return Err(Error::<T>::StickyJudgement.into())
					} else {
						id.judgements[i] = item
					},
				Err(i) =>
					id.judgements.try_insert(i, item).map_err(|_| Error::<T>::TooManyRegistrars)?,
			}

			T::Currency::reserve(&sender, registrar.fee)?;

			let judgements = id.judgements.len();
			<IdentityOf<T>>::insert(&sender, (id, username));

			Self::deposit_event(Event::JudgementRequested {
				who: sender,
				registrar_index: reg_index,
			});

			Ok(Some(T::WeightInfo::request_judgement(judgements as u32)).into())
		}

		/// Cancel a previous request.
		///
		/// Payment: A previously reserved deposit is returned on success.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a
		/// registered identity.
		///
		/// - `reg_index`: The index of the registrar whose judgement is no longer requested.
		///
		/// Emits `JudgementUnrequested` if successful.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::cancel_request(T::MaxRegistrars::get()))]
		pub fn cancel_request(
			origin: OriginFor<T>,
			reg_index: RegistrarIndex,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;
			let (mut id, username) = <IdentityOf<T>>::get(&sender).ok_or(Error::<T>::NoIdentity)?;

			let pos = id
				.judgements
				.binary_search_by_key(&reg_index, |x| x.0)
				.map_err(|_| Error::<T>::NotFound)?;
			let fee = if let Judgement::FeePaid(fee) = id.judgements.remove(pos).1 {
				fee
			} else {
				return Err(Error::<T>::JudgementGiven.into())
			};

			let err_amount = T::Currency::unreserve(&sender, fee);
			debug_assert!(err_amount.is_zero());
			let judgements = id.judgements.len();
			<IdentityOf<T>>::insert(&sender, (id, username));

			Self::deposit_event(Event::JudgementUnrequested {
				who: sender,
				registrar_index: reg_index,
			});

			Ok(Some(T::WeightInfo::cancel_request(judgements as u32)).into())
		}

		/// Set the fee required for a judgement to be requested from a registrar.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must be the account
		/// of the registrar whose index is `index`.
		///
		/// - `index`: the index of the registrar whose fee is to be set.
		/// - `fee`: the new fee.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_fee(T::MaxRegistrars::get()))]
		pub fn set_fee(
			origin: OriginFor<T>,
			#[pallet::compact] index: RegistrarIndex,
			#[pallet::compact] fee: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let registrars = <Registrars<T>>::mutate(|rs| -> Result<usize, DispatchError> {
				rs.get_mut(index as usize)
					.and_then(|x| x.as_mut())
					.and_then(|r| {
						if r.account == who {
							r.fee = fee;
							Some(())
						} else {
							None
						}
					})
					.ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
				Ok(rs.len())
			})?;
			Ok(Some(T::WeightInfo::set_fee(registrars as u32)).into())
		}

		/// Change the account associated with a registrar.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must be the account
		/// of the registrar whose index is `index`.
		///
		/// - `index`: the index of the registrar whose fee is to be set.
		/// - `new`: the new account ID.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_account_id(T::MaxRegistrars::get()))]
		pub fn set_account_id(
			origin: OriginFor<T>,
			#[pallet::compact] index: RegistrarIndex,
			new: AccountIdLookupOf<T>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let new = T::Lookup::lookup(new)?;

			let registrars = <Registrars<T>>::mutate(|rs| -> Result<usize, DispatchError> {
				rs.get_mut(index as usize)
					.and_then(|x| x.as_mut())
					.and_then(|r| {
						if r.account == who {
							r.account = new;
							Some(())
						} else {
							None
						}
					})
					.ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
				Ok(rs.len())
			})?;
			Ok(Some(T::WeightInfo::set_account_id(registrars as u32)).into())
		}

		/// Set the field information for a registrar.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must be the account
		/// of the registrar whose index is `index`.
		///
		/// - `index`: the index of the registrar whose fee is to be set.
		/// - `fields`: the fields that the registrar concerns themselves with.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::set_fields(T::MaxRegistrars::get()))]
		pub fn set_fields(
			origin: OriginFor<T>,
			#[pallet::compact] index: RegistrarIndex,
			fields: <T::IdentityInformation as IdentityInformationProvider>::FieldsIdentifier,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;

			let registrars =
				<Registrars<T>>::mutate(|registrars| -> Result<usize, DispatchError> {
					let registrar = registrars
						.get_mut(index as usize)
						.and_then(|r| r.as_mut())
						.filter(|r| r.account == who)
						.ok_or_else(|| DispatchError::from(Error::<T>::InvalidIndex))?;
					registrar.fields = fields;

					Ok(registrars.len())
				})?;
			Ok(Some(T::WeightInfo::set_fields(registrars as u32)).into())
		}

		/// Provide a judgement for an account's identity.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must be the account
		/// of the registrar whose index is `reg_index`.
		///
		/// - `reg_index`: the index of the registrar whose judgement is being made.
		/// - `target`: the account whose identity the judgement is upon. This must be an account
		///   with a registered identity.
		/// - `judgement`: the judgement of the registrar of index `reg_index` about `target`.
		/// - `identity`: The hash of the [`IdentityInformationProvider`] for that the judgement is
		///   provided.
		///
		/// Emits `JudgementGiven` if successful.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::provide_judgement(T::MaxRegistrars::get()))]
		pub fn provide_judgement(
			origin: OriginFor<T>,
			#[pallet::compact] reg_index: RegistrarIndex,
			target: AccountIdLookupOf<T>,
			judgement: Judgement<BalanceOf<T>>,
			identity: T::Hash,
		) -> DispatchResultWithPostInfo {
			let sender = ensure_signed(origin)?;
			let target = T::Lookup::lookup(target)?;
			ensure!(!judgement.has_deposit(), Error::<T>::InvalidJudgement);
			<Registrars<T>>::get()
				.get(reg_index as usize)
				.and_then(Option::as_ref)
				.filter(|r| r.account == sender)
				.ok_or(Error::<T>::InvalidIndex)?;
			let (mut id, username) =
				<IdentityOf<T>>::get(&target).ok_or(Error::<T>::InvalidTarget)?;

			if T::Hashing::hash_of(&id.info) != identity {
				return Err(Error::<T>::JudgementForDifferentIdentity.into())
			}

			let item = (reg_index, judgement);
			match id.judgements.binary_search_by_key(&reg_index, |x| x.0) {
				Ok(position) => {
					if let Judgement::FeePaid(fee) = id.judgements[position].1 {
						T::Currency::repatriate_reserved(
							&target,
							&sender,
							fee,
							BalanceStatus::Free,
						)
						.map_err(|_| Error::<T>::JudgementPaymentFailed)?;
					}
					id.judgements[position] = item
				},
				Err(position) => id
					.judgements
					.try_insert(position, item)
					.map_err(|_| Error::<T>::TooManyRegistrars)?,
			}

			let judgements = id.judgements.len();
			<IdentityOf<T>>::insert(&target, (id, username));
			Self::deposit_event(Event::JudgementGiven { target, registrar_index: reg_index });

			Ok(Some(T::WeightInfo::provide_judgement(judgements as u32)).into())
		}

		/// Remove an account's identity and sub-account information and slash the deposits.
		///
		/// Payment: Reserved balances from `set_subs` and `set_identity` are slashed and handled by
		/// `Slash`. Verification request deposits are not returned; they should be cancelled
		/// manually using `cancel_request`.
		///
		/// The dispatch origin for this call must match `T::ForceOrigin`.
		///
		/// - `target`: the account whose identity the judgement is upon. This must be an account
		///   with a registered identity.
		///
		/// Emits `IdentityKilled` if successful.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::kill_identity(
			T::MaxRegistrars::get(),
			T::MaxSubAccounts::get(),
		))]
		pub fn kill_identity(
			origin: OriginFor<T>,
			target: AccountIdLookupOf<T>,
		) -> DispatchResultWithPostInfo {
			T::ForceOrigin::ensure_origin(origin)?;

			// Figure out who we're meant to be clearing.
			let target = T::Lookup::lookup(target)?;
			// Grab their deposit (and check that they have one).
			let (subs_deposit, sub_ids) = <SubsOf<T>>::take(&target);
			let (id, _username) = <IdentityOf<T>>::take(&target).ok_or(Error::<T>::NotNamed)?;
			let deposit = id.total_deposit().saturating_add(subs_deposit);
			for sub in sub_ids.iter() {
				<SuperOf<T>>::remove(sub);
			}
			// Slash their deposit from them.
			T::Slashed::on_unbalanced(T::Currency::slash_reserved(&target, deposit).0);

			Self::deposit_event(Event::IdentityKilled { who: target, deposit });

			#[allow(deprecated)]
			Ok(Some(T::WeightInfo::kill_identity(id.judgements.len() as u32, sub_ids.len() as u32))
				.into())
		}

		/// Add the given account to the sender's subs.
		///
		/// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
		/// to the sender.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// sub identity of `sub`.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::add_sub(T::MaxSubAccounts::get()))]
		pub fn add_sub(
			origin: OriginFor<T>,
			sub: AccountIdLookupOf<T>,
			data: Data,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let sub = T::Lookup::lookup(sub)?;
			ensure!(IdentityOf::<T>::contains_key(&sender), Error::<T>::NoIdentity);

			// Check if it's already claimed as sub-identity.
			ensure!(!SuperOf::<T>::contains_key(&sub), Error::<T>::AlreadyClaimed);

			SubsOf::<T>::try_mutate(&sender, |(ref mut subs_deposit, ref mut sub_ids)| {
				// Ensure there is space and that the deposit is paid.
				ensure!(
					sub_ids.len() < T::MaxSubAccounts::get() as usize,
					Error::<T>::TooManySubAccounts
				);
				let deposit = T::SubAccountDeposit::get();
				T::Currency::reserve(&sender, deposit)?;

				SuperOf::<T>::insert(&sub, (sender.clone(), data));
				sub_ids.try_push(sub.clone()).expect("sub ids length checked above; qed");
				*subs_deposit = subs_deposit.saturating_add(deposit);

				Self::deposit_event(Event::SubIdentityAdded { sub, main: sender.clone(), deposit });
				Ok(())
			})
		}

		/// Alter the associated name of the given sub-account.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// sub identity of `sub`.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::rename_sub(T::MaxSubAccounts::get()))]
		pub fn rename_sub(
			origin: OriginFor<T>,
			sub: AccountIdLookupOf<T>,
			data: Data,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let sub = T::Lookup::lookup(sub)?;
			ensure!(IdentityOf::<T>::contains_key(&sender), Error::<T>::NoIdentity);
			ensure!(SuperOf::<T>::get(&sub).map_or(false, |x| x.0 == sender), Error::<T>::NotOwned);
			SuperOf::<T>::insert(&sub, (sender, data));
			Ok(())
		}

		/// Remove the given account from the sender's subs.
		///
		/// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
		/// to the sender.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// sub identity of `sub`.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::remove_sub(T::MaxSubAccounts::get()))]
		pub fn remove_sub(origin: OriginFor<T>, sub: AccountIdLookupOf<T>) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			ensure!(IdentityOf::<T>::contains_key(&sender), Error::<T>::NoIdentity);
			let sub = T::Lookup::lookup(sub)?;
			let (sup, _) = SuperOf::<T>::get(&sub).ok_or(Error::<T>::NotSub)?;
			ensure!(sup == sender, Error::<T>::NotOwned);
			SuperOf::<T>::remove(&sub);
			SubsOf::<T>::mutate(&sup, |(ref mut subs_deposit, ref mut sub_ids)| {
				sub_ids.retain(|x| x != &sub);
				let deposit = T::SubAccountDeposit::get().min(*subs_deposit);
				*subs_deposit -= deposit;
				let err_amount = T::Currency::unreserve(&sender, deposit);
				debug_assert!(err_amount.is_zero());
				Self::deposit_event(Event::SubIdentityRemoved { sub, main: sender, deposit });
			});
			Ok(())
		}

		/// Remove the sender as a sub-account.
		///
		/// Payment: Balance reserved by a previous `set_subs` call for one sub will be repatriated
		/// to the sender (*not* the original depositor).
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must have a registered
		/// super-identity.
		///
		/// NOTE: This should not normally be used, but is provided in the case that the non-
		/// controller of an account is maliciously registered as a sub-account.
		#[pallet::call_index(14)]
		#[pallet::weight(T::WeightInfo::quit_sub(T::MaxSubAccounts::get()))]
		pub fn quit_sub(origin: OriginFor<T>) -> DispatchResult {
			let sender = ensure_signed(origin)?;
			let (sup, _) = SuperOf::<T>::take(&sender).ok_or(Error::<T>::NotSub)?;
			SubsOf::<T>::mutate(&sup, |(ref mut subs_deposit, ref mut sub_ids)| {
				sub_ids.retain(|x| x != &sender);
				let deposit = T::SubAccountDeposit::get().min(*subs_deposit);
				*subs_deposit -= deposit;
				let _ =
					T::Currency::repatriate_reserved(&sup, &sender, deposit, BalanceStatus::Free);
				Self::deposit_event(Event::SubIdentityRevoked {
					sub: sender,
					main: sup.clone(),
					deposit,
				});
			});
			Ok(())
		}

		/// Add an `AccountId` with permission to grant usernames with a given `suffix` appended.
		#[pallet::call_index(15)]
		#[pallet::weight(1)]
		pub fn add_username_authority(
			origin: OriginFor<T>,
			authority: AccountIdLookupOf<T>,
			suffix: Vec<u8>,
		) -> DispatchResult {
			T::UsernameAuthorityOrigin::ensure_origin(origin)?;
			let authority = T::Lookup::lookup(authority)?;
			Self::validate_username(&suffix).map_err(|_| Error::<T>::InvalidSuffix)?;
			let suffix = BoundedVec::<u8, ConstU32<SUFFIX_MAX_LENGTH>>::try_from(suffix)
				.map_err(|_| Error::<T>::InvalidSuffix)?;
			// The authority may already exist, but we don't need to check. They might be changing
			// their suffix, so we just want to overwrite whatever was there.
			UsernameAuthorities::<T>::insert(&authority, suffix);
			Ok(())
		}

		/// Remove `authority` from the username authorities.
		#[pallet::call_index(16)]
		#[pallet::weight(1)]
		pub fn remove_username_authority(
			origin: OriginFor<T>,
			authority: AccountIdLookupOf<T>,
		) -> DispatchResult {
			T::UsernameAuthorityOrigin::ensure_origin(origin)?;
			let authority = T::Lookup::lookup(authority)?;
			UsernameAuthorities::<T>::remove(&authority);
			Ok(())
		}

		/// Set the username for `who`. Must be called by a username authority.
		#[pallet::call_index(17)]
		#[pallet::weight(1)]
		pub fn set_username_for(
			origin: OriginFor<T>,
			who: AccountIdentifier<T::AccountId>,
			username: Vec<u8>,
			signature: Option<MultiSignature>,
		) -> DispatchResult {
			// Ensure origin is a Username Authority.
			let sender = ensure_signed(origin)?;
			let suffix = if let Some(s) = UsernameAuthorities::<T>::get(&sender) {
				s
			} else {
				return Err(Error::<T>::NotUsernameAuthority.into())
			};

			// Verify input length before allocating a Vec with the user's input.
			// `<` instead of `<=` because it needs one element for the point
			// (`username` + `.` + `suffix`)
			ensure!(
				(username.len().saturating_add(suffix.len()) as u32) < USERNAME_MAX_LENGTH,
				Error::<T>::InvalidUsername
			);

			// Ensure that the username only contains allowed characters. We already know the suffix
			// does.
			Self::validate_username(&username)?;

			// Concatenate the username with suffix and cast into a BoundedVec. Should be infallible
			// since we already ensured it is below the max length.
			let mut full_username =
				Vec::with_capacity(username.len().saturating_add(suffix.len()).saturating_add(1));
			full_username.extend(username);
			full_username.extend(b".");
			full_username.extend(suffix);
			let bounded_username =
				Username::try_from(full_username).map_err(|_| Error::<T>::InvalidUsername)?;

			// Usernames must be unique. Ensure it's not taken.
			ensure!(
				!AccountOfUsername::<T>::contains_key(bounded_username.clone()),
				Error::<T>::UsernameTaken
			);

			// Verify `signature` that `who` has signed their `username` (i.e. have requested it
			// themselves).
			let (who, requires_deposit) = match who {
				// Ensure the account has preapproved this username.
				AccountIdentifier::AbstractAccount(a) => {
					ensure!(signature.is_none(), Error::<T>::InvalidSignature);
					if let Some((u, _, _)) = PreapprovedUsernames::<T>::take(&a) {
						// username may have expired, but it's there and hasn't been reaped. we are
						// cleaning the storage and keeping the deposit, so no harm.
						ensure!(
							u.to_vec() == bounded_username.to_vec(),
							Error::<T>::InvalidUsername
						);
					} else {
						return Err(Error::<T>::NoUsername.into())
					}
					(a, false)
					//  ^^^^^
					// the account has already placed a deposit
				},

				// Account has pre-signed an authorization. Verify the signature provided.
				AccountIdentifier::KeyedAccount(ms) => {
					let signature = signature.ok_or(Error::<T>::RequiresSignature)?;
					let valid = bounded_username.to_vec().using_encoded(|encoded| {
						signature.verify(encoded, &ms.clone().into_account())
					});
					ensure!(valid, Error::<T>::InvalidSignature);

					// It's late and I'm frustrated. Obviously do not keep.
					//
					// `MultiSigner` is either 32 byte (ED/SR) or 33 byte (ECDSA) public keys and
					// already implements `into_account` for `type AccountId = AccountId32`. Is it
					// possible (or desirable) to say that the `AccountId` type of the runtime must
					// be able to convert a 32 or 33 byte array to an `AccountId`? Or to just make
					// it fail if it doesn't?
					//
					// TODO: type KeyedAccounts: Convert<MultiSigner, Self::AccountId>;
					use sp_runtime::traits::TrailingZeroInput;
					let dummy = T::AccountId::decode(&mut TrailingZeroInput::new(&[][..]))
						.expect("infinite input; qed");
					(dummy, true)
					//      ^^^^
					// the account must place a deposit
				},
			};

			// Check if they already have a primary. If so, leave it. If not, set it.
			// Likewise, check if they have an identity. If not, give them a minimal one.
			let (reg, primary_username) = match <IdentityOf<T>>::get(&who) {
				// User has an existing Identity and a primary username. Leave it.
				Some((reg, Some(primary))) => (reg, primary),
				// User has an Identity but no primary. Set the new one as primary.
				Some((reg, None)) => (reg, bounded_username.clone()),
				// User does not have an existing Identity. Give them a fresh default one and set
				// their username as primary.
				None => (
					Registration {
						info: T::IdentityInformation::default(),
						judgements: BoundedVec::default(),
						deposit: Zero::zero(),
					},
					bounded_username.clone(),
				),
			};

			// Correct deposit for IdentityInfo
			let new_id_deposit = Self::calculate_identity_deposit(&reg.info);
			Self::rejig_deposit(&who, reg.deposit, new_id_deposit)?;

			// Take deposit for username (if not already taken in preapproval).
			if requires_deposit {
				let deposit = T::UsernameDeposit::get();
				T::Currency::reserve(&who, deposit)?;
			}

			// Enter in identity map.
			IdentityOf::<T>::insert(&who, (reg, Some(primary_username)));
			// Enter in username map.
			AccountOfUsername::<T>::insert(bounded_username, &who);

			Ok(())
		}

		/// Approve a `username` for `origin`. For accounts corresponding to cryptographic key
		/// pairs, this call is optional, as they can just provide a signature on their desired
		/// username for the authority to include in `set_username_for`.
		///
		/// The preapproval call is meant for accounts that do _not_ correspond to a cryptographic
		/// key pair, e.g. a pure proxy or multisig. These accounts can use the preapproval to
		/// register a desired username that an authority can then grant.
		///
		/// Preapproving does not guarantee a claim on the `username`.
		///
		/// The username _must_ include a suffix as a means to designate the approver.
		#[pallet::call_index(18)]
		#[pallet::weight(1)]
		pub fn preapprove_username(origin: OriginFor<T>, username: Vec<u8>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let bounded_username =
				Username::try_from(username).map_err(|_| Error::<T>::InvalidUsername)?;

			let now = frame_system::Pallet::<T>::block_number();
			let expiration = now.saturating_add(T::PreapprovalExpiration::get());

			let deposit = T::UsernameDeposit::get();
			T::Currency::reserve(&who, deposit)?;

			PreapprovedUsernames::<T>::insert(&who, (bounded_username, deposit, expiration));
			Ok(())
		}

		/// Remove a preapproval for a username.
		#[pallet::call_index(19)]
		#[pallet::weight(1)]
		pub fn remove_preapproval(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;
			if let Some((_, deposit, _)) = PreapprovedUsernames::<T>::take(&who) {
				let err_amount = T::Currency::unreserve(&who, deposit);
				debug_assert!(err_amount.is_zero());
				Ok(())
			} else {
				Err(Error::<T>::NoUsername.into())
			}
		}

		/// Remove an expired username preapproval.
		#[pallet::call_index(20)]
		#[pallet::weight(1)]
		pub fn remove_expired_preapproval(
			origin: OriginFor<T>,
			who: AccountIdLookupOf<T>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;
			let who = T::Lookup::lookup(who)?;
			if let Some((_, deposit, expiration)) = PreapprovedUsernames::<T>::take(&who) {
				let now = frame_system::Pallet::<T>::block_number();
				ensure!(now > expiration, Error::<T>::NotExpired);

				let err_amount = T::Currency::unreserve(&who, deposit);
				debug_assert!(err_amount.is_zero());

				Ok(Pays::No.into())
			} else {
				Err(Error::<T>::NoUsername.into())
			}
		}

		/// Set a given username as the primary. The username should include the suffix.
		#[pallet::call_index(21)]
		#[pallet::weight(1)]
		pub fn set_primary_username(origin: OriginFor<T>, username: Vec<u8>) -> DispatchResult {
			// ensure `username` maps to `origin` (i.e. has already been set by an authority)
			let who = ensure_signed(origin)?;
			let bounded_username =
				Username::try_from(username).map_err(|_| Error::<T>::InvalidUsername)?;
			ensure!(
				AccountOfUsername::<T>::get(bounded_username.clone()).is_some(),
				Error::<T>::NoUsername
			);
			// todo: mutate_extant?
			let (registration, _maybe_username) =
				IdentityOf::<T>::get(&who).ok_or(Error::<T>::NoIdentity)?;
			IdentityOf::<T>::insert(&who, (registration, Some(bounded_username)));
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Get the subs of an account.
	pub fn subs(who: &T::AccountId) -> Vec<(T::AccountId, Data)> {
		SubsOf::<T>::get(who)
			.1
			.into_iter()
			.filter_map(|a| SuperOf::<T>::get(&a).map(|x| (a, x.1)))
			.collect()
	}

	/// Calculate the deposit required for a number of `sub` accounts.
	fn subs_deposit(subs: u32) -> BalanceOf<T> {
		T::SubAccountDeposit::get().saturating_mul(<BalanceOf<T>>::from(subs))
	}

	/// Take the `current` deposit that `who` is holding, and update it to a `new` one.
	fn rejig_deposit(
		who: &T::AccountId,
		current: BalanceOf<T>,
		new: BalanceOf<T>,
	) -> DispatchResult {
		if new > current {
			T::Currency::reserve(who, new - current)?;
		} else if new < current {
			let err_amount = T::Currency::unreserve(who, current - new);
			debug_assert!(err_amount.is_zero());
		}
		Ok(())
	}

	/// Check if the account has corresponding identity information by the identity field.
	pub fn has_identity(
		who: &T::AccountId,
		fields: <T::IdentityInformation as IdentityInformationProvider>::FieldsIdentifier,
	) -> bool {
		IdentityOf::<T>::get(who)
			.map_or(false, |(registration, _username)| (registration.info.has_identity(fields)))
	}

	/// Calculate the deposit required for an identity.
	fn calculate_identity_deposit(info: &T::IdentityInformation) -> BalanceOf<T> {
		let bytes = info.encoded_size() as u32;
		let byte_deposit = T::ByteDeposit::get().saturating_mul(<BalanceOf<T>>::from(bytes));
		T::BasicDeposit::get().saturating_add(byte_deposit)
	}

	/// Validate that a username conforms to allowed characters/format.
	fn validate_username(username: &Vec<u8>) -> DispatchResult {
		ensure!(
			username.iter().all(|byte| byte.is_ascii_alphanumeric()),
			Error::<T>::InvalidUsername
		);
		Ok(())
	}

	/// Reap an identity, clearing associated storage items and refunding any deposits. This
	/// function is very similar to (a) `clear_identity`, but called on a `target` account instead
	/// of self; and (b) `kill_identity`, but without imposing a slash.
	///
	/// Parameters:
	/// - `target`: The account for which to reap identity state.
	///
	/// Return type is a tuple of the number of registrars, `IdentityInfo` bytes, and sub accounts,
	/// respectively.
	///
	/// NOTE: This function is here temporarily for migration of Identity info from the Polkadot
	/// Relay Chain into a system parachain. It will be removed after the migration.
	pub fn reap_identity(who: &T::AccountId) -> Result<(u32, u32, u32), DispatchError> {
		// `take` any storage items keyed by `target`
		// identity
		let (id, _maybe_username) = <IdentityOf<T>>::take(&who).ok_or(Error::<T>::NotNamed)?;
		let registrars = id.judgements.len() as u32;
		let encoded_byte_size = id.info.encoded_size() as u32;

		// subs
		let (subs_deposit, sub_ids) = <SubsOf<T>>::take(&who);
		let actual_subs = sub_ids.len() as u32;
		for sub in sub_ids.iter() {
			<SuperOf<T>>::remove(sub);
		}

		// unreserve any deposits
		let deposit = id.total_deposit().saturating_add(subs_deposit);
		let err_amount = T::Currency::unreserve(&who, deposit);
		debug_assert!(err_amount.is_zero());
		Ok((registrars, encoded_byte_size, actual_subs))
	}

	/// Update the deposits held by `target` for its identity info.
	///
	/// Parameters:
	/// - `target`: The account for which to update deposits.
	///
	/// Return type is a tuple of the new Identity and Subs deposits, respectively.
	///
	/// NOTE: This function is here temporarily for migration of Identity info from the Polkadot
	/// Relay Chain into a system parachain. It will be removed after the migration.
	pub fn poke_deposit(
		target: &T::AccountId,
	) -> Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
		// Identity Deposit
		let new_id_deposit = IdentityOf::<T>::try_mutate(
			&target,
			|identity_of| -> Result<BalanceOf<T>, DispatchError> {
				let (reg, _) = identity_of.as_mut().ok_or(Error::<T>::NoIdentity)?;
				// Calculate what deposit should be
				let encoded_byte_size = reg.info.encoded_size() as u32;
				let byte_deposit =
					T::ByteDeposit::get().saturating_mul(<BalanceOf<T>>::from(encoded_byte_size));
				let new_id_deposit = T::BasicDeposit::get().saturating_add(byte_deposit);

				// Update account
				Self::rejig_deposit(&target, reg.deposit, new_id_deposit)?;

				reg.deposit = new_id_deposit;
				Ok(new_id_deposit)
			},
		)?;

		// Subs Deposit
		let new_subs_deposit = SubsOf::<T>::try_mutate(
			&target,
			|(current_subs_deposit, subs_of)| -> Result<BalanceOf<T>, DispatchError> {
				let new_subs_deposit = Self::subs_deposit(subs_of.len() as u32);
				Self::rejig_deposit(&target, *current_subs_deposit, new_subs_deposit)?;
				*current_subs_deposit = new_subs_deposit;
				Ok(new_subs_deposit)
			},
		)?;
		Ok((new_id_deposit, new_subs_deposit))
	}

	/// Set an identity with zero deposit. Only used for benchmarking that involves `rejig_deposit`.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn set_identity_no_deposit(
		who: &T::AccountId,
		info: T::IdentityInformation,
	) -> DispatchResult {
		IdentityOf::<T>::insert(
			&who,
			(
				Registration {
					judgements: Default::default(),
					deposit: Zero::zero(),
					info: info.clone(),
				},
				None::<Username>,
			),
		);
		Ok(())
	}

	/// Set subs with zero deposit. Only used for benchmarking that involves `rejig_deposit`.
	#[cfg(feature = "runtime-benchmarks")]
	pub fn set_sub_no_deposit(who: &T::AccountId, sub: T::AccountId) -> DispatchResult {
		use frame_support::BoundedVec;
		let subs = BoundedVec::<_, T::MaxSubAccounts>::try_from(vec![sub]).unwrap();
		SubsOf::<T>::insert::<
			&T::AccountId,
			(BalanceOf<T>, BoundedVec<T::AccountId, T::MaxSubAccounts>),
		>(&who, (Zero::zero(), subs));
		Ok(())
	}
}
