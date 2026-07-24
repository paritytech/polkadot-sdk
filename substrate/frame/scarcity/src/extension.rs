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

//! Scarcity transaction extension.

use crate::{pallet::*, weights::WeightInfo, Config, Nft};
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	pallet_prelude::{Get, TransactionSource},
	traits::{IsSubType, OriginTrait, UnixTime},
	weights::Weight,
	CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, PartialEqNoBound,
};
use scale_info::TypeInfo;
use sp_io::hashing::twox_64;
use sp_runtime::{
	traits::{
		DispatchInfoOf, Implication, PostDispatchInfoOf, TransactionExtension, ValidateResult,
	},
	transaction_validity::{InvalidTransaction, TransactionValidityError, ValidTransaction},
	DispatchResult,
};

/// The Scarcity authorization requested by this extension.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	EqNoBound,
	CloneNoBound,
	PartialEqNoBound,
	DecodeWithMemTracking,
	DebugNoBound,
)]
pub enum AsScarcityInfo {
	/// Authorize a transfer or burn using the NFT's purse key.
	AsNft,
}

/// An error reported while validating [`AsScarcity`].
#[repr(u8)]
pub enum CustomInvalidity {
	/// The purse-key authorization must begin with a signed origin.
	OriginToAsNftMustBeSigned = 0,
	/// The purse key is temporarily locked after a failed dispatch.
	NftTemporarilyLocked = 1,
	/// The purse key has no NFT to authorize the requested action.
	NoNft = 2,
	/// The transfer destination equals the current purse key.
	TransferToSelf = 3,
	/// The transfer destination already holds an NFT.
	DestinationOccupied = 4,
}

impl From<CustomInvalidity> for TransactionValidityError {
	fn from(value: CustomInvalidity) -> Self {
		InvalidTransaction::Custom(value as u8).into()
	}
}

/// Information carried from validation to preparation.
pub enum Val<T: Config + Send + Sync> {
	NotUsing,
	UsingNft { owner: T::AccountId },
}

/// Information carried from preparation to post-dispatch.
pub enum Pre<T: Config + Send + Sync> {
	NotUsing,
	UsingNft { owner: T::AccountId, nft: Nft },
}

/// Purse-key authorization for Scarcity transfers and burns.
///
/// NFT authorization is deliberately nonce-free. Consumption-on-use makes replays hit
/// `NoNft`; the transaction-pool `provides` tag, mandatory mortality, and the failure lock
/// together bound replay. Callers must send mortal transactions.
#[derive(
	Encode,
	Decode,
	TypeInfo,
	EqNoBound,
	CloneNoBound,
	PartialEqNoBound,
	DefaultNoBound,
	DecodeWithMemTracking,
	DebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct AsScarcity<T: Config + Send + Sync>(Option<AsScarcityInfo>, PhantomData<T>);

impl<T: Config + Send + Sync> AsScarcity<T> {
	/// Create an extension. `None` is the identity extension for ordinary transactions.
	pub fn new(explicit: Option<AsScarcityInfo>) -> Self {
		Self(explicit, PhantomData)
	}

	fn failed_dispatch_lock(previous: Option<LockInfo>) -> LockInfo {
		let retries = previous.map(|lock| lock.retries.saturating_add(1)).unwrap_or(0);
		let multiplier = 2u64.saturating_pow(u32::from(retries).min(63));
		LockInfo {
			retries,
			until: T::UnixTime::now()
				.as_secs()
				.saturating_add(multiplier.saturating_mul(T::LockPeriod::get())),
		}
	}
}

impl<T: Config + Send + Sync> TransactionExtension<<T as frame_system::Config>::RuntimeCall>
	for AsScarcity<T>
{
	const IDENTIFIER: &'static str = "AsScarcity";
	type Implicit = ();
	type Val = Val<T>;
	type Pre = Pre<T>;

	fn weight(&self, call: &<T as frame_system::Config>::RuntimeCall) -> Weight {
		if matches!(self.0, Some(AsScarcityInfo::AsNft)) &&
			matches!(
				call.is_sub_type(),
				Some(Call::<T>::transfer { .. }) | Some(Call::<T>::burn {})
			) {
			T::WeightInfo::as_scarcity_pipeline()
		} else {
			Weight::zero()
		}
	}

	fn validate(
		&self,
		mut origin: <T as frame_system::Config>::RuntimeOrigin,
		call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Implication,
		_source: TransactionSource,
	) -> ValidateResult<Self::Val, <T as frame_system::Config>::RuntimeCall> {
		let transfer_to = match call.is_sub_type() {
			Some(Call::<T>::transfer { to }) => Some(to),
			Some(Call::<T>::burn {}) => None,
			_ => return Ok((ValidTransaction::default(), Val::NotUsing, origin)),
		};
		if !matches!(self.0, Some(AsScarcityInfo::AsNft)) {
			return Ok((ValidTransaction::default(), Val::NotUsing, origin));
		}

		let Some(frame_system::Origin::<T>::Signed(owner)) = origin.as_system_ref() else {
			return Err(CustomInvalidity::OriginToAsNftMustBeSigned.into());
		};
		let owner = owner.clone();
		let now = T::UnixTime::now().as_secs();
		if let Some(lock) = Locked::<T>::get(&owner) {
			if lock.until > now {
				return Err(CustomInvalidity::NftTemporarilyLocked.into());
			}
		}
		let nft = NftsByOwner::<T>::get(&owner).ok_or(CustomInvalidity::NoNft)?;
		if let Some(to) = transfer_to {
			// Pre-validate the destination so ordinary user error is rejected at the pool and
			// never reaches dispatch, where a failure triggers the backoff lock. The
			// dispatch-time checks remain for genuine same-block races. Mirrors coinage's
			// `validate_transfer` pattern. Burns have no destination checks.
			if to == &owner {
				return Err(CustomInvalidity::TransferToSelf.into());
			}
			if NftsByOwner::<T>::contains_key(to) {
				return Err(CustomInvalidity::DestinationOccupied.into());
			}
		}
		let priority = now.saturating_sub(nft.last_moved).min(T::MaxTransferPriority::get());
		let validity = ValidTransaction::with_tag_prefix("Scarcity")
			.and_provides(twox_64(&("scarcity", &owner).encode()))
			.priority(priority)
			.into();
		origin.set_caller_from(Origin::Nft { owner: owner.clone(), nft });
		Ok((validity, Val::UsingNft { owner }, origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		_origin: &<T as frame_system::Config>::RuntimeOrigin,
		_call: &<T as frame_system::Config>::RuntimeCall,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Val::NotUsing => Ok(Pre::NotUsing),
			Val::UsingNft { owner } => {
				let nft = NftsByOwner::<T>::take(&owner).ok_or(CustomInvalidity::NoNft)?;
				Ok(Pre::UsingNft { owner, nft })
			},
		}
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_post_info: &PostDispatchInfoOf<<T as frame_system::Config>::RuntimeCall>,
		_len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		if let Pre::UsingNft { owner, nft } = pre {
			if result.is_err() {
				NftsByOwner::<T>::insert(&owner, nft);
				Locked::<T>::insert(&owner, Self::failed_dispatch_lock(Locked::<T>::get(&owner)));
			} else {
				Locked::<T>::remove(&owner);
			}
		}
		Ok(Weight::zero())
	}
}
