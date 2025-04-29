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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod weights;
pub use weights::WeightInfo;

extern crate alloc;
use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	dispatch_context::with_context,
	pallet_prelude::*,
	traits::{
		tokens::{ConversionFromAssetBalance, Pay, PaymentStatus},
		EnsureOrigin, Get, OnUnbalanced, ReservableCurrency,
	},
};
use frame_system::pallet_prelude::{
	ensure_signed, BlockNumberFor as SystemBlockNumberFor, OriginFor,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AccountIdConversion, BadOrigin, BlockNumberProvider, Saturating, StaticLookup, TryConvert,
		Zero,
	},
	Permill, RuntimeDebug,
};

type BalanceOf<T, I = ()> = pallet_treasury::BalanceOf<T, I>;
type BeneficiaryLookupOf<T, I = ()> = pallet_treasury::BeneficiaryLookupOf<T, I>;
/// An index of a bounty. Just a `u32`.
pub type BountyIndex = u32;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// A bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct Bounty<AccountId, Balance, BlockNumber, AssetKind, PaymentId, Beneficiary> {
	/// The account proposing it.
	pub proposer: AccountId,
	/// The kind of asset this bounty is rewarded in.
	pub asset_kind: AssetKind,
	/// The (total) amount of the `asset_kind` that should be paid if the bounty is rewarded.
	pub value: Balance,
	/// The curator fee in the `asset_kind`. Included in value.
	pub fee: Balance,
	/// The deposit of curator.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	pub curator_deposit: Balance,
	/// The amount held on deposit (reserved) for making this proposal.
	///
	/// The asset class determined by the [`pallet_treasury::Config::Currency`].
	pub bond: Balance,
	/// The status of this bounty.
	pub status: BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary>,
}

/// The status of a bounty proposal.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum BountyStatus<AccountId, BlockNumber, PaymentId, Beneficiary> {
	/// The bounty is funded and waiting for curator assignment.
	Funded,
	/// A curator has been proposed. Waiting for acceptance from the curator.
	CuratorProposed {
		/// The assigned curator of this bounty.
		curator: AccountId,
	},
	/// The bounty is active and waiting to be awarded.
	Active {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account/location used as a fee destination.
		curator_stash: Beneficiary,
		/// An update from the curator is due by this block, else they are considered inactive.
		update_due: BlockNumber,
	},
	/// The bounty is awarded and waiting to released after a delay.
	PendingPayout {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator's stash account/location used as a fee destination.
		curator_stash: Beneficiary,
		/// The beneficiary of the bounty.
		beneficiary: Beneficiary,
        // TODO: Remove since BountyDeposityPayoutDelay it is set to 0 (https://github.com/polkadot-fellows/runtimes/blob/43a8f2373129db30709e46ea8bc2baa72c782852/relay/polkadot/src/lib.rs#L794)
        /// When the bounty can be claimed.
		// unlock_at: BlockNumber,
	},
	/// The bounty is approved with a curator and waiting to confirm the funds allocation.
	ApprovedWithCurator {
		/// The assigned curator of this bounty.
		curator: AccountId,
		/// The status of the bounty amount transfer from the source (e.g. Treasury) to
		/// the bounty account.
		///
		/// Once `check_payment_status` confirms the payment succeeded, the bounty will transition
		/// to [`BountyStatus::CuratorProposed`].
		payment_status: PaymentState<PaymentId>,
	},
	/// The bounty payout has been attempted.
	///
	/// The transfers to both the curator stash and the beneficiary have been initiated.
	/// You can call `process_payment` to retry one or both payments, and `check_payment_status`
	/// to advance each payment’s state. Once `check_payment_status` confirms both payments
	/// succeeded, the bounty is finalized and removed from storage.
	PayoutAttempted {
		/// The curator of this bounty.
		curator: AccountId,
		/// The curator stash account/location with the payout status.
		curator_stash: (Beneficiary, PaymentState<PaymentId>),
		/// The beneficiary stash account/location with the payout status.
		beneficiary: (Beneficiary, PaymentState<PaymentId>),
	},
	/// The bounty is closed, and the funds are being refunded to the original source (e.g.,
	/// Treasury). Once `check_payment_status` confirms the payment succeeded, the bounty is
	/// finalized and removed from storage.
	RefundAttempted {
		/// The curator of this bounty.
		curator: Option<AccountId>,
		/// The refund status.
		///
		/// Once the refund is successful, the bounty is removed from the storage.
		payment_status: PaymentState<PaymentId>,
	},
}

/// The state of payments associated with each bounty and its `BountyStatus`.
///
/// When a payment is initiated using `Paymaster::pay`, the payment enters in a pending state,
/// thus supporting asynchronous payments. Calling `check_payment_status` updates the payment state
/// and advances the bounty lifecycle. The `process_payment` can be called to retry a payment in
/// `Failed` or `Pending` state.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub enum PaymentState<Id> {
	/// Pending claim.
	Pending,
	/// Payment attempted with a payment identifier.
	Attempted { id: Id },
	/// Payment failed.
	Failed,
	/// Payment succeeded.
	Succeeded,
}
impl<Id: Clone> PaymentState<Id> {
	/// Used to check if payment can be retried.
	pub fn is_pending_or_failed(&self) -> bool {
		matches!(self, PaymentState::Pending | PaymentState::Failed)
	}

	/// If a payment has been initialized, returns its identifier, which is used to check its
	/// status.
	pub fn get_attempt_id(&self) -> Option<Id> {
		match self {
			PaymentState::Attempted { id } => Some(id.clone()),
			_ => None,
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_treasury::Config<I> {
        /// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
    }

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        // TODO: Create a bounty, initiating the funding from the treasury to the
        // bounty account. Combine `pallet_bounties` `propose_bounty` and `approve_bounty` calls.
        #[pallet::call_index(0)]
        #[pallet::weight(<T as Config<I>>::WeightInfo::propose_bounty(description.len() as u32))]
        pub fn create_bounty(
            origin: OriginFor<T>,
            asset_kind: Box<T::AssetKind>,
            #[pallet::compact] value: BalanceOf<T, I>,
            description: Vec<u8>,
        ) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `propose_curator` call.
        #[pallet::call_index(1)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::propose_curator())]
        pub fn propose_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `unassign_curator` call.
        #[pallet::call_index(2)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::unassign_curator())]
        pub fn unassign_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `accept_curator` call.
        #[pallet::call_index(3)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::accept_curator())]
        pub fn accept_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			stash: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `award_bounty` call.
        #[pallet::call_index(4)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::award_bounty())]
		pub fn award_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			beneficiary: BeneficiaryLookupOf<T, I>,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `claim_bounty` call.
        #[pallet::call_index(5)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::claim_bounty())]
		pub fn claim_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `close_bounty` call.
        #[pallet::call_index(6)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::close_bounty_proposed()
			.max(<T as Config<I>>::WeightInfo::close_bounty_active()))]
		pub fn close_bounty(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `extend_bounty_expiry` call.
        #[pallet::call_index(7)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::extend_bounty_expiry())]
		pub fn extend_bounty_expiry(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			_remark: Vec<u8>,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Similar as `pallet_bounties` `approve_bounty_with_curator` call.
        #[pallet::call_index(8)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn create_bounty_with_curator(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
			curator: AccountIdLookupOf<T>,
			#[pallet::compact] fee: BalanceOf<T, I>,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `process_payment` call in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/bounties/src/lib.rs#L1212.
        #[pallet::call_index(9)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn process_payment(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
            Ok(())
        }

        // TODO: Same as `pallet_bounties` `check_payment_status` call in https://github.com/paritytech/polkadot-sdk/blob/252f3953247c7e9b9776c63cdeee35b4d51e9b24/substrate/frame/bounties/src/lib.rs#L1323C10-L1323C30.
        #[pallet::call_index(10)]
		#[pallet::weight(<T as Config<I>>::WeightInfo::approve_bounty_with_curator())]
		pub fn check_payment_status(
			origin: OriginFor<T>,
			#[pallet::compact] bounty_id: BountyIndex,
		) -> DispatchResult {
            Ok(())
        }
    }
}